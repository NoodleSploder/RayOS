//! Intent Parser - Natural Language → TaskStruct Translation
//!
//! Translates user input (text + visual context) into structured intents.
//! Operates in two modes: simulated heuristic mode and optional LLM mode.

use crate::types::*;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// Intent parser that handles NL → command translation
pub struct IntentParser {
    config: IntentConfig,
    patterns: CommandPatterns,
    context_window: Vec<Intent>,  // Recent intents for context
    max_context_size: usize,
}

/// Regex patterns for command recognition
struct CommandPatterns {
    create: Vec<Regex>,
    modify: Vec<Regex>,
    delete: Vec<Regex>,
    query: Vec<Regex>,
    navigate: Vec<Regex>,
    execute: Vec<Regex>,
}

impl Default for CommandPatterns {
    fn default() -> Self {
        Self {
            create: vec![
                Regex::new(r"(?i)create|make|new|add").unwrap(),
                Regex::new(r"(?i)generate|spawn|build").unwrap(),
            ],
            modify: vec![
                Regex::new(r"(?i)modify|change|edit|update|alter").unwrap(),
                Regex::new(r"(?i)rename|move|refactor|optimize").unwrap(),
            ],
            delete: vec![
                Regex::new(r"(?i)delete|remove|destroy|erase").unwrap(),
            ],
            query: vec![
                Regex::new(r"(?i)find|search|look for|show me|list").unwrap(),
                Regex::new(r"(?i)what|where|which|how many").unwrap(),
            ],
            navigate: vec![
                Regex::new(r"(?i)go to|navigate to|open|switch to").unwrap(),
            ],
            execute: vec![
                Regex::new(r"(?i)run|execute|start|launch").unwrap(),
            ],
        }
    }
}

impl IntentParser {
    /// Create new intent parser
    pub fn new(config: IntentConfig) -> Self {
        Self {
            config,
            patterns: CommandPatterns::default(),
            context_window: Vec::new(),
            max_context_size: 10,
        }
    }

    /// Parse user input into intent
    pub fn parse(&mut self, input: &str, context: Context) -> ParseResult {
        let input = input.trim().to_lowercase();

        // Try pattern matching first (simulated mode)
        if let Some(command) = self.parse_with_patterns(&input, &context) {
            let intent = Intent {
                id: IntentId::new(),
                command,
                context: context.clone(),
                confidence: self.calculate_confidence(&input),
                timestamp: Instant::now(),
            };

            // Add to context window
            self.add_to_context(intent.clone());

            return ParseResult {
                intent,
                alternatives: vec![],
                needs_clarification: false,
            };
        }

        // Fallback: try LLM if enabled
        if self.config.enable_llm {
            // LLM parsing integration with fallback
            match self.llm_parse(&input, context.clone()) {
                Ok(result) => return result,
                Err(e) => {
                    log::warn!("LLM parsing failed: {}, using fallback", e);
                    return self.create_ambiguous_result(&input, context);
                }
            }
        }

        // Final fallback: generic execute command
        let intent = Intent {
            id: IntentId::new(),
            command: Command::Execute {
                action: input.to_string(),
                args: vec![],
            },
            context,
            confidence: 0.3,  // Low confidence
            timestamp: Instant::now(),
        };

        // Add to context window
        self.add_to_context(intent.clone());

        ParseResult {
            intent,
            alternatives: vec![],
            needs_clarification: true,
        }
    }

    /// Parse with regex patterns (simulated mode)
    fn parse_with_patterns(&self, input: &str, context: &Context) -> Option<Command> {
        // CREATE patterns
        if self.patterns.create.iter().any(|re| re.is_match(input)) {
            return self.parse_create(input);
        }

        // MODIFY patterns
        if self.patterns.modify.iter().any(|re| re.is_match(input)) {
            return self.parse_modify(input, context);
        }

        // DELETE patterns
        if self.patterns.delete.iter().any(|re| re.is_match(input)) {
            return self.parse_delete(input, context);
        }

        // QUERY patterns
        if self.patterns.query.iter().any(|re| re.is_match(input)) {
            return self.parse_query(input);
        }

        // NAVIGATE patterns
        if self.patterns.navigate.iter().any(|re| re.is_match(input)) {
            return self.parse_navigate(input);
        }

        // EXECUTE patterns
        if self.patterns.execute.iter().any(|re| re.is_match(input)) {
            return self.parse_execute(input);
        }

        None
    }

    /// Parse CREATE command
    fn parse_create(&self, input: &str) -> Option<Command> {
        let words: Vec<&str> = input.split_whitespace().collect();

        if words.len() < 2 {
            return None;
        }

        // Extract object type (word after create/make/new)
        let object_type = if let Some(pos) = words.iter().position(|&w| {
            w == "create" || w == "make" || w == "new"
        }) {
            if pos + 1 < words.len() {
                words[pos + 1].to_string()
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Extract properties from remaining words
        let mut properties = HashMap::new();
        properties.insert("source".to_string(), input.to_string());

        // Look for "named X" or "called X"
        if let Some(name_pos) = words.iter().position(|&w| w == "named" || w == "called") {
            if name_pos + 1 < words.len() {
                properties.insert("name".to_string(), words[name_pos + 1].to_string());
            }
        }

        Some(Command::Create {
            object_type,
            properties,
        })
    }

    /// Parse MODIFY command
    fn parse_modify(&self, input: &str, context: &Context) -> Option<Command> {
        let words: Vec<&str> = input.split_whitespace().collect();

        // Detect deictic references ("that", "this", "it")
        let target = if input.contains("that") || input.contains("this") || input.contains("it") {
            // Use gaze context
            Target::Deictic {
                gaze_position: context.gaze.as_ref().map(|g| g.position),
                object_id: context.gaze.as_ref().and_then(|g| g.focused_object.clone()),
            }
        } else {
            // Try to extract filename or path
            if let Some(name) = words.iter().find(|w| w.contains('.')) {
                Target::Direct {
                    path: PathBuf::from(name),
                }
            } else {
                return None;
            }
        };

        // Determine operation
        let operation = if input.contains("rename") {
            if let Some(new_name) = self.extract_quoted_string(input) {
                Operation::Rename { new_name }
            } else {
                return None;
            }
        } else if input.contains("move") {
            if let Some(dest) = self.extract_quoted_string(input) {
                Operation::Move {
                    destination: PathBuf::from(dest),
                }
            } else {
                return None;
            }
        } else if input.contains("optimize") {
            Operation::Optimize
        } else if input.contains("refactor") {
            Operation::Refactor
        } else {
            // Generic edit
            Operation::Custom {
                operation: "edit".to_string(),
                params: HashMap::new(),
            }
        };

        Some(Command::Modify { target, operation })
    }

    /// Parse DELETE command
    fn parse_delete(&self, input: &str, context: &Context) -> Option<Command> {
        let target = if input.contains("that") || input.contains("this") || input.contains("it") {
            Target::Deictic {
                gaze_position: context.gaze.as_ref().map(|g| g.position),
                object_id: context.gaze.as_ref().and_then(|g| g.focused_object.clone()),
            }
        } else {
            // Extract filename
            let words: Vec<&str> = input.split_whitespace().collect();
            if let Some(name) = words.iter().find(|w| w.contains('.')) {
                Target::Direct {
                    path: PathBuf::from(name),
                }
            } else {
                return None;
            }
        };

        Some(Command::Delete { target })
    }

    /// Parse QUERY command
    fn parse_query(&self, input: &str) -> Option<Command> {
        // Extract query by removing command keywords
        let query = input
            .replace("find", "")
            .replace("search", "")
            .replace("look for", "")
            .replace("show me", "")
            .replace("list", "")
            .trim()
            .to_string();

        if query.is_empty() {
            return None;
        }

        // Parse filters (simplified)
        let mut filters = Vec::new();

        // Look for file type filters
        if let Some(ext) = self.extract_extension(&query) {
            filters.push(Filter {
                field: "extension".to_string(),
                operator: FilterOperator::Equals,
                value: ext,
            });
        }

        Some(Command::Query { query, filters })
    }

    /// Parse NAVIGATE command
    fn parse_navigate(&self, input: &str) -> Option<Command> {
        let words: Vec<&str> = input.split_whitespace().collect();

        // Extract destination (word after "to")
        let destination = if let Some(pos) = words.iter().position(|&w| w == "to") {
            if pos + 1 < words.len() {
                words[pos + 1..].join(" ")
            } else {
                return None;
            }
        } else {
            // Take last word
            words.last()?.to_string()
        };

        Some(Command::Navigate { destination })
    }

    /// Parse EXECUTE command
    fn parse_execute(&self, input: &str) -> Option<Command> {
        let words: Vec<&str> = input.split_whitespace().collect();

        if words.len() < 2 {
            return None;
        }

        // First word after run/execute is the action
        let action_pos = words.iter().position(|&w| {
            w == "run" || w == "execute" || w == "start" || w == "launch"
        })?;

        if action_pos + 1 >= words.len() {
            return None;
        }

        let action = words[action_pos + 1].to_string();
        let args = words[action_pos + 2..].iter().map(|s| s.to_string()).collect();

        Some(Command::Execute { action, args })
    }

    /// Calculate confidence score
    fn calculate_confidence(&self, input: &str) -> f32 {
        let words = input.split_whitespace().count();

        // More words generally mean more context and higher confidence
        // (up to a point)
        let word_score = (words as f32 / 10.0).min(1.0);

        // Check for deictic references (slightly lower confidence)
        let deictic_penalty = if input.contains("that") || input.contains("this") {
            0.1
        } else {
            0.0
        };

        (0.7 + word_score * 0.3 - deictic_penalty).clamp(0.3, 1.0)
    }

    /// Add intent to context window
    fn add_to_context(&mut self, intent: Intent) {
        self.context_window.push(intent);

        // Keep only recent intents
        if self.context_window.len() > self.max_context_size {
            self.context_window.remove(0);
        }
    }

    /// Create ambiguous result requiring clarification
    fn create_ambiguous_result(&self, input: &str, context: Context) -> ParseResult {
        let possibilities = vec![
            Command::Execute {
                action: input.to_string(),
                args: vec![],
            },
            Command::Query {
                query: input.to_string(),
                filters: vec![],
            },
        ];

        let intent = Intent {
            id: IntentId::new(),
            command: Command::Ambiguous {
                possibilities: possibilities.clone(),
                question: format!("What did you mean by '{}'?", input),
            },
            context,
            confidence: 0.4,
            timestamp: Instant::now(),
        };

        ParseResult {
            intent,
            alternatives: vec![],
            needs_clarification: true,
        }
    }

    /// Parse using LLM when pattern matching fails
    fn llm_parse(&mut self, input: &str, context: Context) -> anyhow::Result<ParseResult> {
        // In a real implementation, this would call an LLM API
        // For now, simulate with enhanced semantic pattern matching

        let input_lower = input.to_lowercase();

        // Try to infer command type from semantic meaning
        let command = if input_lower.contains("make") || input_lower.contains("build") || input_lower.contains("generate") {
            Command::Create {
                object_type: "item".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("description".to_string(), input.to_string());
                    props
                },
            }
        } else if input_lower.contains("change") || input_lower.contains("update") || input_lower.contains("edit") {
            Command::Modify {
                target: Target::Direct { path: PathBuf::from(input.to_string()) },
                operation: Operation::Custom {
                    operation: "edit".to_string(),
                    params: HashMap::new()
                },
            }
        } else if input_lower.contains("remove") || input_lower.contains("delete") || input_lower.contains("erase") {
            Command::Delete {
                target: Target::Direct { path: PathBuf::from(input.to_string()) },
            }
        } else if input_lower.contains("show") || input_lower.contains("display") || input_lower.contains("list") {
            Command::Query {
                query: input.to_string(),
                filters: vec![],
            }
        } else {
            // Default to execute
            Command::Execute {
                action: input.to_string(),
                args: vec![],
            }
        };

        let intent = Intent {
            id: IntentId::new(),
            command,
            context,
            confidence: 0.6,  // Medium confidence from LLM
            timestamp: Instant::now(),
        };

        self.add_to_context(intent.clone());

        Ok(ParseResult {
            intent,
            alternatives: vec![],
            needs_clarification: false,
        })
    }

    /// Extract quoted string from input
    fn extract_quoted_string(&self, input: &str) -> Option<String> {
        let re = Regex::new(r#""([^"]+)"|'([^']+)'"#).unwrap();
        re.captures(input)
            .and_then(|caps| caps.get(1).or_else(|| caps.get(2)))
            .map(|m| m.as_str().to_string())
    }

    /// Extract file extension from query
    fn extract_extension(&self, query: &str) -> Option<String> {
        let words: Vec<&str> = query.split_whitespace().collect();

        // Look for ".ext" or "ext files"
        for word in words {
            if word.starts_with('.') && word.len() > 1 {
                return Some(word[1..].to_string());
            }
            if word.ends_with("files") && word.len() > 5 {
                let ext = &word[..word.len() - 5];
                if !ext.is_empty() {
                    return Some(ext.to_string());
                }
            }
        }

        None
    }

    /// Get recent context for debugging
    pub fn get_context_window(&self) -> &[Intent] {
        &self.context_window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_context() -> Context {
        Context {
            gaze: Some(GazeContext {
                position: (100.0, 200.0),
                focused_object: Some("file_123".to_string()),
                timestamp: Instant::now(),
            }),
            audio: None,
            visual_objects: vec![],
            application: Some("vscode".to_string()),
            filesystem: None,
            system: SystemContext {
                cpu_usage: 50.0,
                memory_usage: 60.0,
                active_tasks: 10,
            },
        }
    }

    #[test]
    fn test_parse_create() {
        let mut parser = IntentParser::new(IntentConfig::default());
        let result = parser.parse("create file named test.rs", make_context());

        assert!(matches!(result.intent.command, Command::Create { .. }));
        assert!(result.intent.confidence > 0.7);
    }

    #[test]
    fn test_parse_query() {
        let mut parser = IntentParser::new(IntentConfig::default());
        let result = parser.parse("find all rust files", make_context());

        assert!(matches!(result.intent.command, Command::Query { .. }));
    }

    #[test]
    fn test_parse_modify_deictic() {
        let mut parser = IntentParser::new(IntentConfig::default());
        let result = parser.parse("optimize that", make_context());

        if let Command::Modify { target, .. } = result.intent.command {
            assert!(matches!(target, Target::Deictic { .. }));
        } else {
            panic!("Expected Modify command, got: {:?}", result.intent.command);
        }
    }

    #[test]
    fn test_parse_delete() {
        let mut parser = IntentParser::new(IntentConfig::default());
        let result = parser.parse("delete test.rs", make_context());

        assert!(matches!(result.intent.command, Command::Delete { .. }));
    }

    #[test]
    fn test_parse_navigate() {
        let mut parser = IntentParser::new(IntentConfig::default());
        let result = parser.parse("go to home directory", make_context());

        if let Command::Navigate { destination } = result.intent.command {
            assert!(destination.contains("home"));
        } else {
            panic!("Expected Navigate command");
        }
    }

    #[test]
    fn test_context_window() {
        let mut parser = IntentParser::new(IntentConfig::default());

        for i in 0..15 {
            parser.parse(&format!("query number {}", i), make_context());
        }

        assert_eq!(parser.get_context_window().len(), 10);  // max_context_size
    }
}
