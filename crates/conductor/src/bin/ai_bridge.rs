//! Host-side AI bridge for the bare-metal RayOS kernel.
//!
//! Runs QEMU with `-serial stdio`, watches the serial output for lines like:
//!   `RAYOS_INPUT:<id>:<user text>`
//! Then generates an AI response and writes it back to the guest via serial as:
//!   `AI:<id>:<response chunk>` (one or more lines)
//!   `AI_END:<id>`
//!
//! Build/run:
//!   cd conductor
//!   cargo run --features ai --bin ai_bridge -- qemu-system-x86_64 ... -serial stdio ...

use anyhow::{anyhow, Context, Result};
use rayos_conductor::{
    ConductorConfig, EntropyMonitor, OuroborosEngine, Priority, Task, TaskId, TaskOrchestrator,
    TaskPayload, TaskStatus,
};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use rayos_cortex::LLMConnector;
use rayos_intent::{IntentConfig, IntentEngine};

use tokio::sync::{mpsc, oneshot, Mutex};

#[cfg(feature = "ai_ollama")]
use serde::Deserialize;

#[cfg(feature = "ai_ollama")]
use tokio::sync::OnceCell;

#[derive(Debug)]
enum OrchCmd {
    Submit {
        task: Task,
        reply: oneshot::Sender<Result<TaskId>>,
    },
    GetStatus {
        id: TaskId,
        reply: oneshot::Sender<Option<TaskStatus>>,
    },
}

#[cfg(feature = "ai_ollama")]
#[derive(Clone, Debug)]
struct OllamaConfig {
    url: String,
    model: String,
}

#[cfg(feature = "ai_ollama")]
static OLLAMA_CFG: OnceCell<Option<OllamaConfig>> = OnceCell::const_new();

#[derive(Clone)]
struct OrchClient {
    tx: mpsc::UnboundedSender<OrchCmd>,
}

impl OrchClient {
    async fn submit(&self, task: Task) -> Result<TaskId> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(OrchCmd::Submit {
                task,
                reply: reply_tx,
            })
            .map_err(|_| anyhow!("orchestrator thread not running"))?;
        let task_id = reply_rx
            .await
            .map_err(|_| anyhow!("orchestrator thread dropped response"))??;
        Ok(task_id)
    }

    async fn get_status(&self, id: TaskId) -> Option<TaskStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(OrchCmd::GetStatus { id, reply: reply_tx }).is_err() {
            return None;
        }
        reply_rx.await.ok().flatten()
    }
}

fn spawn_orchestrator_thread() -> Result<OrchClient> {
    let (tx, mut rx) = mpsc::unbounded_channel::<OrchCmd>();

    // Init acknowledgement so we fail fast if the orchestrator can't start.
    let (ack_tx, ack_rx) = std::sync::mpsc::channel::<Result<()>>();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ack_tx.send(Err(anyhow!("failed to build tokio runtime: {e}")));
                return;
            }
        };

        rt.block_on(async move {
            let config = ConductorConfig {
                worker_threads: 2,
                enable_gpu: false,
                ..Default::default()
            };

            let monitor = Arc::new(EntropyMonitor::new(
                config.latency_threshold_ms,
                config.dream_threshold_secs,
            ));

            let ouroboros = Arc::new(OuroborosEngine::new());
            ouroboros.set_enabled(config.enable_ouroboros);

            let orchestrator = match TaskOrchestrator::new(config, monitor, ouroboros) {
                Ok(o) => {
                    let _ = ack_tx.send(Ok(()));
                    Arc::new(o)
                }
                Err(e) => {
                    let _ = ack_tx.send(Err(e));
                    return;
                }
            };

            // Keep the worker loop running.
            {
                let orchestrator = orchestrator.clone();
                tokio::spawn(async move {
                    if let Err(e) = orchestrator.start().await {
                        log::error!("orchestrator stopped: {e:?}");
                    }
                });
            }

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    OrchCmd::Submit { task, reply } => {
                        let r = orchestrator.submit(task);
                        let _ = reply.send(r);
                    }
                    OrchCmd::GetStatus { id, reply } => {
                        let _ = reply.send(orchestrator.get_status(id));
                    }
                }
            }

            orchestrator.shutdown();
        });
    });

    // Wait a bit for init; fail fast if it didn't start.
    ack_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| anyhow!("orchestrator failed to start (timeout)"))??;

    Ok(OrchClient { tx })
}


#[tokio::main]
async fn main() -> Result<()> {
    // Program args are the QEMU command + args.
    // (Cargo uses `--` to separate its args from ours.)
    let qemu_cmd: Vec<String> = std::env::args().skip(1).collect();
    if qemu_cmd.is_empty() {
        return Err(anyhow!(
            "usage: ai_bridge <qemu-system-*> ... -serial stdio ... (must use -serial stdio)"
        ));
    }

    let mut has_serial_stdio = false;
    for w in qemu_cmd.windows(2) {
        if w[0] == "-serial" && w[1] == "stdio" {
            has_serial_stdio = true;
            break;
        }
    }
    if !has_serial_stdio {
        return Err(anyhow!(
            "ai_bridge requires QEMU args to include `-serial stdio` (bridge replies over stdio)"
        ));
    }

    let mut child = Command::new(&qemu_cmd[0])
        .args(&qemu_cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn {}", qemu_cmd[0]))?;

    let qemu_stdin = Arc::new(Mutex::new(
        child.stdin.take().context("qemu stdin unavailable")?,
    ));
    let mut qemu_stdout = child.stdout.take().context("qemu stdout unavailable")?;

    // Start a real Conductor orchestrator so "Action: queue ..." becomes real work.
    let orchestrator = spawn_orchestrator_thread()?;

    // Init AI components.
    let intent_engine = IntentEngine::new(IntentConfig::default());
    let llm = LLMConnector::new().await?;

    // Stream processing.
    let mut buf = [0u8; 4096];
    let mut line_buf: Vec<u8> = Vec::with_capacity(8192);

    loop {
        let n = qemu_stdout.read(&mut buf)?;
        if n == 0 {
            break;
        }

        // Mirror QEMU serial output to our terminal.
        std::io::stdout().write_all(&buf[..n])?;
        std::io::stdout().flush()?;

        for &b in &buf[..n] {
            if b == b'\r' {
                continue;
            }
            if b == b'\n' {
                handle_line(&line_buf, &intent_engine, &llm, &orchestrator, &qemu_stdin).await?;
                line_buf.clear();
            } else {
                if line_buf.len() < 16 * 1024 {
                    line_buf.push(b);
                }
            }
        }
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("qemu exited with status: {}", status));
    }

    Ok(())
}

async fn handle_line(
    line: &[u8],
    intent: &IntentEngine,
    llm: &LLMConnector,
    orchestrator: &OrchClient,
    qemu_stdin: &Arc<Mutex<std::process::ChildStdin>>,
) -> Result<()> {
    const TAG: &[u8] = b"RAYOS_INPUT:";
    if line.len() < TAG.len() || &line[..TAG.len()] != TAG {
        return Ok(());
    }

    let user_bytes = &line[TAG.len()..];
    let (msg_id, user_text) = parse_tagged_input(user_bytes);
    if user_text.is_empty() {
        return Ok(());
    }

    eprintln!("[ai_bridge] input id={msg_id} text={user_text}");

    // Short-circuit: time/date queries should never become repo searches.
    // The guest already supports time awareness, but in host-bridge mode we can answer
    // directly from the host clock even if no LLM backend is configured.
    if looks_like_time_query(&user_text) {
        let now = chrono::Utc::now();
        let reply = format!(
            "UTC: {} {}",
            now.format("%Y-%m-%d %a"),
            now.format("%H:%M:%S")
        );

        let mut w = qemu_stdin.lock().await;
        send_ai_chunks(&mut *w, msg_id, &reply)?;
        eprintln!("[ai_bridge] send: AI_END:{msg_id}");
        write!(&mut *w, "AI_END:{}\n", msg_id)?;
        w.flush()?;
        return Ok(());
    }

    // Parse intent (for extra context), but only run tasks when the user explicitly asks.
    // This avoids turning normal questions into repo searches or fake "compute" tasks.
    let parsed = intent.parse(&user_text);

    // If intent parsing says it needs clarification and the user appears to be issuing a command,
    // ask a direct question. For normal chat, let the LLM/template handle it.
    if parsed.needs_clarification && looks_like_explicit_task_request(&user_text).is_some() {
        if let Some(q) = extract_clarifying_question(&parsed.intent.command) {
            let mut w = qemu_stdin.lock().await;
            send_ai_chunks(&mut *w, msg_id, q)?;
            eprintln!("[ai_bridge] send: AI_END:{msg_id}");
            write!(&mut *w, "AI_END:{}\n", msg_id)?;
            w.flush()?;
            return Ok(());
        }
    }

    if let Some(task) = task_from_explicit_request(&user_text) {
        run_task_to_completion(msg_id, task, orchestrator, qemu_stdin).await?;
        return Ok(());
    }

    // Default: conversational reply.
    let mut response = generate_reply(user_text.clone(), &parsed, llm)
        .await
        .unwrap_or_else(|_| "OK.".to_string());

    // Keep it single-line ASCII-ish so the kernel renderer can display it.
    response = response.replace('\n', " ");
    // IMPORTANT: kernel treats each AI:<id>:... line as a *separate* response.
    // `send_ai_chunks` wraps at 72 chars, so cap to one line to avoid the Response
    // line showing only the last chunk.
    response = truncate_chars(response, 72);

    eprintln!("[ai_bridge] reply: {response}");

    // Send back to the guest over serial (correlated to msg_id).
    // Kernel watches for `AI:<id>:` lines and replaces "(thinking...)".
    {
        let mut w = qemu_stdin.lock().await;
        send_ai_chunks(&mut *w, msg_id, &response)?;
        eprintln!("[ai_bridge] send: AI_END:{msg_id}");
        write!(&mut *w, "AI_END:{}\n", msg_id)?;
        w.flush()?;
    }

    Ok(())
}

async fn run_task_to_completion(
    msg_id: u32,
    task: Task,
    orchestrator: &OrchClient,
    qemu_stdin: &Arc<Mutex<std::process::ChildStdin>>,
) -> Result<()> {
    let completion_label = completion_label(&task.payload);
    let task_id = orchestrator.submit(task).await?;

    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let Some(status) = orchestrator.get_status(task_id).await else {
            break;
        };

        match status {
            TaskStatus::Completed { duration: _, result } => {
                let mut w = qemu_stdin.lock().await;
                let msg = match result {
                    Some(result) => {
                        // Search already returns a nice single-line summary.
                        // Other tasks return short status strings.
                        result
                    }
                    None if !completion_label.is_empty() => completion_label,
                    None => "OK".to_string(),
                };

                let msg = truncate_chars(msg.replace('\n', " "), 72);
                send_ai_chunks(&mut *w, msg_id, &msg)?;
                eprintln!("[ai_bridge] send: AI_END:{msg_id}");
                write!(&mut *w, "AI_END:{}\n", msg_id)?;
                w.flush()?;
                break;
            }
            TaskStatus::Failed { error } => {
                let mut w = qemu_stdin.lock().await;
                let msg = truncate_chars(format!("Error: {}", error).replace('\n', " "), 72);
                send_ai_chunks(&mut *w, msg_id, &msg)?;
                eprintln!("[ai_bridge] send: AI_END:{msg_id}");
                write!(&mut *w, "AI_END:{}\n", msg_id)?;
                w.flush()?;
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn looks_like_explicit_task_request(user_text: &str) -> Option<&'static str> {
    let q = user_text.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }

    // Keep this intentionally strict: tasks should be explicit.
    if q.starts_with("search ") || q.starts_with("find ") {
        return Some("search");
    }
    if q.starts_with("index ") {
        return Some("index");
    }
    if q.starts_with("optimize") {
        return Some("optimize");
    }
    if q.starts_with("maint ") || q.starts_with("maintenance") {
        return Some("maintenance");
    }

    None
}

fn task_from_explicit_request(user_text: &str) -> Option<Task> {
    let q = user_text.trim();
    if q.is_empty() {
        return None;
    }

    let q_lower = q.to_lowercase();

    if q_lower.starts_with("search ") {
        let query = q["search ".len()..].trim();
        if query.is_empty() {
            return None;
        }
        return Some(Task::new(
            Priority::High,
            TaskPayload::Search {
                query: query.to_string(),
                limit: 25,
            },
        ));
    }

    if q_lower.starts_with("find ") {
        let query = q["find ".len()..].trim();
        if query.is_empty() {
            return None;
        }
        return Some(Task::new(
            Priority::High,
            TaskPayload::Search {
                query: query.to_string(),
                limit: 25,
            },
        ));
    }

    if let Some(_rest) = q_lower.strip_prefix("index ") {
        let path = q["index ".len()..].trim();
        if path.is_empty() {
            return None;
        }
        return Some(Task::new(
            Priority::High,
            TaskPayload::IndexFile { path: path.into() },
        ));
    }

    if q_lower == "optimize system" || q_lower == "optimize:system" {
        return Some(Task::new(
            Priority::Dream,
            TaskPayload::Optimize {
                target: rayos_conductor::OptimizationTarget::System,
            },
        ));
    }

    None
}

fn looks_like_time_query(s: &str) -> bool {
    let q = s.trim().to_lowercase();
    if q.is_empty() {
        return false;
    }
    q.contains("time is it")
        || q.contains("what time")
        || q.contains("current time")
        || q.contains("date is")
        || q.contains("what date")
        || q.contains("today")
        || q.contains("weekday")
        || q.contains("day of week")
}

fn truncate_chars(s: String, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if s.chars().count() <= max_chars {
        return s;
    }
    s.chars().take(max_chars).collect()
}

fn short_task_id(id: TaskId) -> String {
    let s = id.0.to_string();
    s.chars().take(8).collect()
}

fn describe_payload(payload: &TaskPayload) -> String {
    match payload {
        TaskPayload::Search { query, limit } => format!("Search \"{}\" (limit {})", query, limit),
        TaskPayload::IndexFile { path } => format!("IndexFile {}", path.display()),
        TaskPayload::Compute {
            name,
            estimated_duration,
        } => format!("Compute {} ({}ms)", name, estimated_duration.as_millis()),
        TaskPayload::Optimize { target } => format!("Optimize {:?}", target),
        TaskPayload::Maintenance { task_type } => format!("Maintenance {:?}", task_type),
    }
}

fn completion_label(payload: &TaskPayload) -> String {
    match payload {
        TaskPayload::Search { .. } => "Search results".to_string(),
        TaskPayload::IndexFile { path } => format!("Index complete: {}", path.display()),
        TaskPayload::Optimize { target } => format!("Optimize complete: {:?}", target),
        TaskPayload::Maintenance { task_type } => format!("Maintenance complete: {:?}", task_type),
        TaskPayload::Compute { name, .. } => format!("Done: {}", name),
    }
}

fn map_priority(priority: rayos_intent::Priority) -> Priority {
    match priority {
        rayos_intent::Priority::Realtime => Priority::Critical,
        rayos_intent::Priority::Interactive => Priority::High,
        rayos_intent::Priority::Normal => Priority::Normal,
        rayos_intent::Priority::Low => Priority::Low,
        rayos_intent::Priority::Idle => Priority::Dream,
    }
}

fn task_from_intent(cmd: &rayos_intent::Command, priority: rayos_intent::Priority) -> Option<Task> {
    use rayos_intent::Command;

    let p = map_priority(priority);

    let payload = match cmd {
        Command::Query { query, .. } => {
            if is_conversational_query(query) {
                return None;
            }
            TaskPayload::Search {
                query: query.clone(),
                limit: 25,
            }
        }
        Command::Create { object_type, properties } => {
            if object_type.to_lowercase().contains("file") {
                if let Some(name) = properties.get("name") {
                    TaskPayload::IndexFile { path: name.into() }
                } else {
                    TaskPayload::Compute {
                        name: format!("intent:create:{object_type}"),
                        estimated_duration: Duration::from_millis(150),
                    }
                }
            } else {
                TaskPayload::Compute {
                    name: format!("intent:create:{object_type}"),
                    estimated_duration: Duration::from_millis(150),
                }
            }
        }
        Command::Modify { .. } => TaskPayload::Compute {
            name: "intent:modify".to_string(),
            estimated_duration: Duration::from_millis(250),
        },
        Command::Delete { .. } => TaskPayload::Maintenance {
            task_type: rayos_conductor::MaintenanceType::GarbageCollection,
        },
        Command::Navigate { destination } => TaskPayload::Compute {
            name: format!("intent:navigate:{destination}"),
            estimated_duration: Duration::from_millis(50),
        },
        Command::Execute { action, args } => {
            if is_non_actionable_exec(action) {
                return None;
            }
            TaskPayload::Compute {
                name: if args.is_empty() {
                    format!("intent:exec:{action}")
                } else {
                    format!("intent:exec:{action} {}", args.join(" "))
                },
                estimated_duration: Duration::from_millis(500),
            }
        }
        Command::Configure { component, .. } => TaskPayload::Maintenance {
            task_type: match component.to_lowercase().as_str() {
                "cache" => rayos_conductor::MaintenanceType::CacheFlush,
                "metrics" => rayos_conductor::MaintenanceType::MetricsExport,
                _ => rayos_conductor::MaintenanceType::CacheFlush,
            },
        },
        Command::Sequence { steps } => TaskPayload::Compute {
            name: format!("intent:sequence:{}", steps.len()),
            estimated_duration: Duration::from_millis(750),
        },
        Command::Ambiguous { .. } => return None,
    };

    Some(Task::new(p, payload))
}

fn is_conversational_query(query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }

    q.contains("are you")
        || q.contains("who are you")
        || q.contains("what are you")
        || q.contains("what are you doing")
        || looks_like_time_query(&q)
        || q.contains("status")
        || q.contains("help")
}

fn is_non_actionable_exec(action: &str) -> bool {
    let a = normalize_exec_action(action);
    matches!(
        a.as_str(),
        "chat" |
        "hi" | "hello" | "hey" | "yo" | "sup" | "hiya" | "howdy" | "ping" |
        "thanks" | "thank" | "thx" | "ty" |
        "bye" | "goodbye" | "cya" | "exit" | "quit"
    )
}

fn normalize_exec_action(action: &str) -> String {
    action
        .trim()
        .to_lowercase()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn extract_clarifying_question(cmd: &rayos_intent::Command) -> Option<&str> {
    match cmd {
        rayos_intent::Command::Ambiguous { question, .. } => Some(question.as_str()),
        _ => None,
    }
}

async fn generate_reply(user_text: String, parsed: &rayos_intent::ParseResult, llm: &LLMConnector) -> Result<String> {
    // Keep a tiny host-side history (per-process). This helps *real* LLMs maintain context.
    // We use thread-local storage to avoid `static mut`.
    thread_local! {
        static HISTORY: RefCell<VecDeque<(String, String)>> = RefCell::new(VecDeque::with_capacity(8));
    }

    let system_prompt = "You are RayOS. Be concise, correct, and ask a clarifying question when needed.";

    // Always handle basic conversational openers locally so we behave consistently
    // regardless of whether an external LLM backend is configured.
    let user_norm = normalize_user_text(&user_text);
    if looks_like_time_query(&user_norm) {
        let now = chrono::Utc::now();
        return Ok(format!(
            "UTC: {} {}",
            now.format("%Y-%m-%d %a"),
            now.format("%H:%M:%S")
        ));
    }
    if is_greeting(&user_norm) {
        return Ok("Hi. I'm online. Try: search TERM, index FILE, optimize system.".to_string());
    }
    if is_thanks(&user_norm) {
        return Ok("You're welcome. What's next?".to_string());
    }
    if is_farewell(&user_norm) {
        return Ok("OK. Standing by.".to_string());
    }
    if looks_like_user_is_upset(&user_norm) {
        return Ok("I hear you. Tell me what you expected to happen, and what actually happened.".to_string());
    }
    if let Some(ans) = try_small_arithmetic(&user_norm) {
        return Ok(ans);
    }

    #[cfg(feature = "ai_ollama")]
    {
        // Prefer a real LLM backend when available.
        // If RAYOS_OLLAMA_MODEL isn't set, try to auto-select the first installed model.
        let cfg = OLLAMA_CFG
            .get_or_init(|| async {
                let url = std::env::var("RAYOS_OLLAMA_URL")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string());

                if let Ok(model) = std::env::var("RAYOS_OLLAMA_MODEL") {
                    let model = model.trim().to_string();
                    if !model.is_empty() {
                        eprintln!("[ai_bridge] ollama enabled: url={url} model={model}");
                        return Some(OllamaConfig { url, model });
                    }
                }

                match resolve_ollama_model(&url).await {
                    Ok(Some(model)) => {
                        eprintln!("[ai_bridge] ollama auto-selected model: url={url} model={model}");
                        Some(OllamaConfig { url, model })
                    }
                    Ok(None) => {
                        eprintln!("[ai_bridge] ollama not available (no models found); using template replies");
                        None
                    }
                    Err(e) => {
                        eprintln!("[ai_bridge] ollama not reachable ({e}); using template replies");
                        None
                    }
                }
            })
            .await
            .clone();

        if let Some(cfg) = cfg {
            let history_snapshot = HISTORY.with(|h| h.borrow().iter().cloned().collect::<VecDeque<_>>());
            let reply = ollama_chat(
                &cfg.url,
                &cfg.model,
                system_prompt,
                &history_snapshot,
                &user_text,
                parsed,
            )
            .await;

            if let Ok(reply) = reply {
                HISTORY.with(|h| {
                    let mut h = h.borrow_mut();
                    h.push_back((user_text.clone(), reply.clone()));
                    while h.len() > 8 {
                        h.pop_front();
                    }
                });
                return Ok(reply);
            }
        }
    }

    // Fallback: cortex template response (still improved prompt with parsed intent).
    let prompt = format!(
        "System: {system_prompt}\n\nUser: {user_text}\n\nParsed intent command: {:?}\n\nReply as RayOS:",
        parsed.intent.command
    );

    let reply = llm.generate(&prompt).await?;
    HISTORY.with(|h| {
        let mut h = h.borrow_mut();
        h.push_back((user_text, reply.clone()));
        while h.len() > 8 { h.pop_front(); }
    });
    Ok(reply)
}

fn looks_like_user_is_upset(s: &str) -> bool {
    let q = s.trim().to_lowercase();
    q.contains("you suck")
        || q.contains("this sucks")
        || q.contains("terrible")
        || q.contains("awful")
        || q.contains("useless")
}

fn try_small_arithmetic(s: &str) -> Option<String> {
    // Extremely small, strict parser for patterns like:
    //   "4+4", "4 plus 4", "12 * 3", "10 / 2"
    let q = s.trim();
    if q.is_empty() {
        return None;
    }

    // Normalize common words.
    let mut t = q.to_lowercase();
    t = t.replace("plus", "+");
    t = t.replace("minus", "-");
    t = t.replace("times", "*");
    t = t.replace("x", "*");
    t = t.replace("divided by", "/");

    // Remove extra words.
    for w in ["what is", "whats", "calculate", "compute", "equals", "=", "?"] {
        t = t.replace(w, " ");
    }

    let t = t.trim();
    // Tokenize on whitespace; also handle tight forms like 4+4 by inserting spaces.
    let mut spaced = String::with_capacity(t.len() + 8);
    for ch in t.chars() {
        if matches!(ch, '+' | '-' | '*' | '/') {
            spaced.push(' ');
            spaced.push(ch);
            spaced.push(' ');
        } else {
            spaced.push(ch);
        }
    }

    let parts: Vec<&str> = spaced.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }

    let a: i64 = parts[0].parse().ok()?;
    let op = parts[1];
    let b: i64 = parts[2].parse().ok()?;

    let result = match op {
        "+" => Some(a + b),
        "-" => Some(a - b),
        "*" => Some(a * b),
        "/" => {
            if b == 0 {
                None
            } else {
                Some(a / b)
            }
        }
        _ => None,
    }?;

    Some(result.to_string())
}

#[cfg(feature = "ai_ollama")]
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[cfg(feature = "ai_ollama")]
#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[cfg(feature = "ai_ollama")]
async fn resolve_ollama_model(base_url: &str) -> Result<Option<String>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let tags: OllamaTagsResponse = resp.json().await?;
    let name = tags.models.into_iter().next().map(|m| m.name).unwrap_or_default();
    let name = name.trim().to_string();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

fn normalize_user_text(s: &str) -> String {
    let lower = s.to_lowercase();
    let trimmed = lower
        .trim_matches(|c: char| !(c.is_alphanumeric() || c.is_whitespace()))
        .trim();
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_greeting(s: &str) -> bool {
    let first = s.split_whitespace().next().unwrap_or("");
    matches!(first, "hi" | "hello" | "hey" | "yo" | "sup" | "hiya" | "howdy" | "ping")
}

fn is_thanks(s: &str) -> bool {
    if s == "thank you" {
        return true;
    }
    let first = s.split_whitespace().next().unwrap_or("");
    matches!(first, "thanks" | "thank" | "thx" | "ty")
}

fn is_farewell(s: &str) -> bool {
    if s == "see you" {
        return true;
    }
    let first = s.split_whitespace().next().unwrap_or("");
    matches!(first, "bye" | "goodbye" | "cya" | "exit" | "quit")
}

#[cfg(feature = "ai_ollama")]
async fn ollama_chat(
    base_url: &str,
    model: &str,
    system_prompt: &str,
    history: &VecDeque<(String, String)>,
    user_text: &str,
    parsed: &rayos_intent::ParseResult,
) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Msg<'a> {
        role: &'a str,
        content: String,
    }

    #[derive(serde::Serialize)]
    struct Req {
        model: String,
        stream: bool,
        messages: Vec<Msg<'static>>,
    }

    #[derive(serde::Deserialize)]
    struct Resp {
        message: RespMsg,
    }

    #[derive(serde::Deserialize)]
    struct RespMsg {
        content: String,
    }

    let mut messages: Vec<Msg<'static>> = Vec::new();
    messages.push(Msg { role: "system", content: system_prompt.to_string() });

    for (u, a) in history.iter() {
        messages.push(Msg { role: "user", content: u.clone() });
        messages.push(Msg { role: "assistant", content: a.clone() });
    }

    // Include parsed intent as additional context.
    messages.push(Msg {
        role: "system",
        content: format!("Parsed intent command (structural hint): {:?}", parsed.intent.command),
    });
    messages.push(Msg { role: "user", content: user_text.to_string() });

    let req = Req {
        model: model.to_string(),
        stream: false,
        messages,
    };

    let url = format!("{}/api/chat", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp: Resp = client
        .post(url)
        .json(&req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(resp.message.content)
}

fn parse_tagged_input(buf: &[u8]) -> (u32, String) {
    // Expected: "<id>:<text>". Fall back to id=0 and whole buffer as text.
    let mut colon = None;
    for (i, &b) in buf.iter().enumerate() {
        if b == b':' {
            colon = Some(i);
            break;
        }
        // Allow leading spaces.
        if i == 0 && b == b' ' {
            continue;
        }
        if b < b'0' || b > b'9' {
            // Not an id prefix.
            colon = None;
            break;
        }
    }

    if let Some(i) = colon {
        let id = parse_u32_decimal(&buf[..i]).unwrap_or(0);
        let text = String::from_utf8_lossy(&buf[(i + 1)..]).trim().to_string();
        (id, text)
    } else {
        (0, String::from_utf8_lossy(buf).trim().to_string())
    }
}

fn parse_u32_decimal(buf: &[u8]) -> Option<u32> {
    let mut v: u32 = 0;
    let mut any = false;
    for &b in buf {
        if b == b' ' {
            continue;
        }
        if b < b'0' || b > b'9' {
            return None;
        }
        any = true;
        v = v.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    if any { Some(v) } else { None }
}

fn send_ai_chunks(w: &mut impl Write, msg_id: u32, text: &str) -> Result<()> {
    // Keep each line short so kernel UI (and serial buffers) stay happy.
    // We split on whitespace and cap at ~72 chars.
    let clean = text.replace('\n', " ");
    let mut line = String::new();

    for word in clean.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
            continue;
        }

        if line.len() + 1 + word.len() > 72 {
            eprintln!("[ai_bridge] send: AI:{msg_id}:{line}");
            write!(w, "AI:{}:{}\n", msg_id, line)?;
            line.clear();
            line.push_str(word);
        } else {
            line.push(' ');
            line.push_str(word);
        }
    }

    if !line.is_empty() {
        eprintln!("[ai_bridge] send: AI:{msg_id}:{line}");
        write!(w, "AI:{}:{}\n", msg_id, line)?;
    }

    Ok(())
}
