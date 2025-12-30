use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{sleep, Duration};

use crate::types::Intent;

/// Minimal QEMU HMP monitor link that injects keyboard input via `sendkey`.
///
/// This matches the behavior of `qemu-sendtext.py` but is implemented in Rust so
/// Cortex can directly talk to a running QEMU guest.
pub struct QemuMonitorLink {
    sock_path: PathBuf,
}

impl QemuMonitorLink {
    pub fn new(sock_path: impl Into<PathBuf>) -> Self {
        Self {
            sock_path: sock_path.into(),
        }
    }

    pub fn sock_path(&self) -> &Path {
        &self.sock_path
    }

    pub async fn send_shell_line(&self, text: &str) -> Result<()> {
        // Inject as normal typed text in the guest shell.
        self.send_text_via_sendkey(text, true).await
    }

    pub async fn send_cortex_line(&self, cortex_line: &str) -> Result<()> {
        // Kernel-bare exposes `:cortex <CORTEX:...>` passthrough.
        let cmd = format!(":cortex {}", cortex_line);
        self.send_shell_line(&cmd).await
    }

    pub async fn send_intent(&self, intent: &Intent) -> Result<()> {
        let mut line = String::from("CORTEX:INTENT ");
        match intent {
            Intent::Select { target } => {
                line.push_str("kind=select ");
                line.push_str("target=");
                line.push_str(&sanitize_ascii(target));
            }
            Intent::Move { source, destination } => {
                line.push_str("kind=move ");
                line.push_str("src=");
                line.push_str(&sanitize_ascii(source));
                line.push(' ');
                line.push_str("dst=");
                line.push_str(&sanitize_ascii(destination));
            }
            Intent::Delete { target } => {
                line.push_str("kind=delete ");
                line.push_str("target=");
                line.push_str(&sanitize_ascii(target));
            }
            Intent::Create { object_type } => {
                line.push_str("kind=create ");
                line.push_str("target=");
                line.push_str(&sanitize_ascii(object_type));
            }
            Intent::Break => {
                line.push_str("kind=break");
            }
            Intent::Idle => {
                line.push_str("kind=idle");
            }
        }
        self.send_cortex_line(&line).await
    }

    async fn send_text_via_sendkey(&self, text: &str, enter: bool) -> Result<()> {
        let mut s = UnixStream::connect(&self.sock_path)
            .await
            .with_context(|| format!("connect to QEMU monitor: {}", self.sock_path.display()))?;

        // Drain banner/prompt (best-effort).
        let mut tmp = [0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_millis(200), s.read(&mut tmp)).await;

        for cmd in to_sendkey_cmds(text, enter) {
            s.write_all(cmd.as_bytes()).await?;
            s.write_all(b"\r\n").await?;
            s.flush().await?;
            sleep(Duration::from_millis(30)).await;

            // Drain output (best-effort).
            let _ = tokio::time::timeout(Duration::from_millis(50), s.read(&mut tmp)).await;
        }

        Ok(())
    }
}

fn sanitize_ascii(s: &str) -> String {
    // Keep protocol payload conservative: printable ASCII, replace spaces with underscores.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == ' ' {
            out.push('_');
            continue;
        }
        if ch >= '!' && ch <= '~' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

fn char_to_key(ch: char) -> Result<&'static str> {
    // Subset of the US layout mapping (mirrors qemu-sendtext.py).
    Ok(match ch {
        ' ' => "spc",
        '\n' => "ret",
        '-' => "minus",
        '=' => "equal",
        ';' => "semicolon",
        '\'' => "apostrophe",
        ',' => "comma",
        '.' => "dot",
        '/' => "slash",
        '\\' => "backslash",
        '[' => "leftbracket",
        ']' => "rightbracket",

        '_' => "shift-minus",
        '+' => "shift-equal",
        ':' => "shift-semicolon",
        '?' => "shift-slash",
        '!' => "shift-1",
        '@' => "shift-2",
        '#' => "shift-3",
        '$' => "shift-4",
        '%' => "shift-5",
        '^' => "shift-6",
        '&' => "shift-7",
        '*' => "shift-8",
        '(' => "shift-9",
        ')' => "shift-0",
        '"' => "shift-apostrophe",
        '<' => "shift-comma",
        '>' => "shift-dot",
        '{' => "shift-leftbracket",
        '}' => "shift-rightbracket",

        '0'..='9' => match ch {
            '0' => "0",
            '1' => "1",
            '2' => "2",
            '3' => "3",
            '4' => "4",
            '5' => "5",
            '6' => "6",
            '7' => "7",
            '8' => "8",
            '9' => "9",
            _ => unreachable!(),
        },
        'a'..='z' => match ch {
            'a' => "a",
            'b' => "b",
            'c' => "c",
            'd' => "d",
            'e' => "e",
            'f' => "f",
            'g' => "g",
            'h' => "h",
            'i' => "i",
            'j' => "j",
            'k' => "k",
            'l' => "l",
            'm' => "m",
            'n' => "n",
            'o' => "o",
            'p' => "p",
            'q' => "q",
            'r' => "r",
            's' => "s",
            't' => "t",
            'u' => "u",
            'v' => "v",
            'w' => "w",
            'x' => "x",
            'y' => "y",
            'z' => "z",
            _ => unreachable!(),
        },
        'A'..='Z' => {
            // QEMU uses shift-<lower>
            return Ok(match ch {
                'A' => "shift-a",
                'B' => "shift-b",
                'C' => "shift-c",
                'D' => "shift-d",
                'E' => "shift-e",
                'F' => "shift-f",
                'G' => "shift-g",
                'H' => "shift-h",
                'I' => "shift-i",
                'J' => "shift-j",
                'K' => "shift-k",
                'L' => "shift-l",
                'M' => "shift-m",
                'N' => "shift-n",
                'O' => "shift-o",
                'P' => "shift-p",
                'Q' => "shift-q",
                'R' => "shift-r",
                'S' => "shift-s",
                'T' => "shift-t",
                'U' => "shift-u",
                'V' => "shift-v",
                'W' => "shift-w",
                'X' => "shift-x",
                'Y' => "shift-y",
                'Z' => "shift-z",
                _ => unreachable!(),
            });
        }
        _ => anyhow::bail!("unsupported character for sendkey: {:?}", ch),
    })
}

fn to_sendkey_cmds(text: &str, enter: bool) -> Vec<String> {
    let mut cmds = Vec::with_capacity(text.len() + 1);
    for ch in text.chars() {
        if let Ok(key) = char_to_key(ch) {
            if key == "ret" {
                cmds.push("sendkey ret".to_string());
            } else {
                cmds.push(format!("sendkey {}", key));
            }
        }
    }
    if enter {
        cmds.push("sendkey ret".to_string());
    }
    cmds
}
