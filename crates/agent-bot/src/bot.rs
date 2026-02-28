use crate::{Brain, BrainConfig, BrainEvent, BrainEventSink, GoalState, GoalTool, MemoryState, MemoryTool};
use agent_core::{Agent, LlmAgent, ReActAgent, Session, SessionBuilder};
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use std::sync::OnceLock;

/// Safely truncate a string to a maximum number of characters (not bytes)
fn truncate_str(s: &str, max_chars: usize) -> String {
    // Single-pass: collect up to max_chars+1 to detect overflow without
    // iterating the entire string twice.
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", truncated)
    } else {
        // We consumed the whole string in one pass; just return it.
        s.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Envelope {
    pub from: String,
    pub to: String,
    pub content: String,
}

#[must_use]
pub enum BotEvent {
    OutputMessage { message: Envelope },
    Error { error: anyhow::Error },
}

pub trait BotEventSink {
    fn emit(&mut self, event: BotEvent);
}

struct BrainToBotSink {
    bot_name: String,
    inner: Rc<RefCell<Inner>>,
}

struct WorkBrainSink {
    bot_name: String,
    inner: Rc<RefCell<Inner>>,
}

/// Sink for Introspection Brain - sends results back to Conversation Brain
struct IntrospectionBrainSink {
    bot_name: String,
    inner: Rc<RefCell<Inner>>,
}

impl BrainEventSink for IntrospectionBrainSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                // Send result back to conversation brain as observation.
                // Clone the Rc and drop the outer borrow before borrowing conversation_brain,
                // to avoid a nested RefCell borrow panic.
                let conv_brain_rc = Rc::clone(&self.inner.borrow().conversation_brain);
                if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
                    conv_brain.push_input(format!("Introspection brain result:\n{}", text));
                }
            }
            BrainEvent::Error { error } => {
                // Send error back to conversation brain.
                // Clone the Rc and drop the outer borrow before borrowing conversation_brain.
                let conv_brain_rc = Rc::clone(&self.inner.borrow().conversation_brain);
                if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
                    conv_brain.push_input(format!("Introspection brain error: {}", error));
                }
            }
        }
    }
}

impl BrainEventSink for WorkBrainSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                // Send result back to conversation brain as observation.
                // Clone the Rc and drop the outer borrow before borrowing conversation_brain,
                // to avoid a nested RefCell borrow panic.
                let conv_brain_rc = Rc::clone(&self.inner.borrow().conversation_brain);
                if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
                    conv_brain.push_input(format!("Work brain result:\n{}", text));
                }
            }
            BrainEvent::Error { error } => {
                // Send error back to conversation brain.
                // Clone the Rc and drop the outer borrow before borrowing conversation_brain.
                let conv_brain_rc = Rc::clone(&self.inner.borrow().conversation_brain);
                if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
                    conv_brain.push_input(format!("Work brain error: {}", error));
                }
            }
        }
    }
}

impl BrainToBotSink {
    /// Route a parsed message to the work brain, introspection brain, or emit it externally.
    fn route_message(&self, to: String, content: String) {
        tracing::debug!(
            "[Bot::{}] Parsed message to '{}': {}",
            self.bot_name, to, truncate_str(&content, 100)
        );

        if to == "work-brain" {
            self.route_to_work_brain(content);
        } else if to == "introspection-brain" {
            self.route_to_introspection_brain(content);
        } else {
            self.emit_output(to, content);
        }
    }

    /// Forward a task to the work brain.
    fn route_to_work_brain(&self, content: String) {
        tracing::debug!("[Bot::{}] Routing to work-brain", self.bot_name);
        // Clone the Rc and drop the outer borrow before borrowing work_brain,
        // to avoid a nested RefCell borrow panic.
        let work_brain_rc = Rc::clone(&self.inner.borrow().work_brain);
        if let Some(work_brain) = work_brain_rc.borrow().as_ref() {
            work_brain.push_input(content);
            tracing::debug!("[Bot::{}] Pushed input to work-brain", self.bot_name);
        } else {
            tracing::error!("[Bot::{}] Work brain not initialized", self.bot_name);
        }
    }

    /// Trigger the introspection brain to perform self-observation.
    fn route_to_introspection_brain(&self, content: String) {
        tracing::info!("[Bot::{}] Routing to introspection-brain: {}", self.bot_name, truncate_str(&content, 100));
        // Clone the Rc and drop the outer borrow before borrowing introspection_brain,
        // to avoid a nested RefCell borrow panic.
        let introspection_brain_rc = Rc::clone(&self.inner.borrow().introspection_brain);
        if let Some(introspection_brain) = introspection_brain_rc.borrow().as_ref() {
            introspection_brain.push_input(content);
            tracing::info!("[Bot::{}] Introspection brain triggered", self.bot_name);
        } else {
            tracing::error!("[Bot::{}] Introspection brain not initialized", self.bot_name);
        }
    }

    /// Emit an outbound message to an external recipient.
    fn emit_output(&self, to: String, content: String) {
        let message = Envelope {
            from: self.bot_name.clone(),
            to,
            content,
        };
        self.inner
            .borrow_mut()
            .sink
            .emit(BotEvent::OutputMessage { message });
    }

    /// Emit an error event to the bot sink.
    fn emit_error(&self, error: anyhow::Error) {
        self.inner.borrow_mut().sink.emit(BotEvent::Error { error });
    }
}

impl BrainEventSink for BrainToBotSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                tracing::debug!("[Bot::{}] Conversation brain output:\n{}", self.bot_name, text);
                match parse_brain_output(&text, &self.bot_name) {
                    Ok(messages) => {
                        for (to, content) in messages {
                            self.route_message(to, content);
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            "[Bot::{}] Output parsing failed. Raw output:\n{}\nError: {}",
                            self.bot_name, text, error
                        );
                        self.emit_error(error);
                    }
                }
            }
            BrainEvent::Error { error } => {
                self.emit_error(error);
            }
        }
    }
}

struct Inner {
    sink: Box<dyn BotEventSink>,
    conversation_brain: Rc<RefCell<Option<Box<Brain>>>>,
    work_brain: Rc<RefCell<Option<Box<Brain>>>>,
    introspection_brain: Rc<RefCell<Option<Box<Brain>>>>,
    goal_state: GoalState,
    memory_state: MemoryState,
    knowledge_base: Rc<crate::KnowledgeBase>,
}

pub struct Bot {
    name: String,
    goal_state: GoalState,
    memory_state: MemoryState,
    knowledge_base: Rc<crate::KnowledgeBase>,

    // Keep alive for BrainToBotSink and routing.
    _inner: Rc<RefCell<Inner>>,
}

impl Bot {
    /// Creates a new Bot with Conversation Brain and Work Brain.
    ///
    /// The Bot manages two brains:
    /// - Conversation Brain: LlmAgent that handles external communication and task coordination
    /// - Work Brain: ReActAgent that executes complex tasks assigned by Conversation Brain
    ///
    /// # Arguments
    /// * `runtime` - Shared runtime
    /// * `name` - Bot name
    /// * `model` - Model to use (e.g., "gpt-4o")
    /// * `tool_constructors` - Tool constructors for Work Brain tools
    /// * `sink` - Event sink for Bot events
    pub fn new(
        runtime: Rc<agent_core::Runtime>,
        name: impl Into<String>,
        model: impl Into<String>,
        tool_constructors: Rc<RefCell<Vec<Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>>>>,
        sink: impl BotEventSink + 'static,
    ) -> Result<Self> {
        let name = name.into();
        let model = model.into();
        anyhow::ensure!(!name.trim().is_empty(), "bot name must be non-empty");

        // Setup DataStore (required)
        let data_store = runtime
            .data_store()
            .context("Bot requires Runtime with DataStore")?;

        let store = Rc::new(agent_core::DataStore::new(data_store.root().to_path_buf()));
        let bot_dir = store.root_dir().subdir(&name);

        // Create goal/memory state with DataNode
        let goal_state = GoalState::new(bot_dir.node("goal"));
        let memory_state = MemoryState::new(bot_dir.node("memory"));

        // Create separate directories for work brain and conversation brain histories
        let work_dir = bot_dir.subdir("work");
        let conv_dir = bot_dir.subdir("conv");
        let introspection_dir = bot_dir.subdir("introspection");

        // Create Knowledge Base
        let knowledge_dir = bot_dir.full_path().join("knowledge");
        let knowledge_base = Rc::new(crate::KnowledgeBase::new(knowledge_dir));

        // Work Brain prompt and session
        let work_brain_prompt = Self::build_work_brain_prompt(&name);

        // Create Work Brain Session (has GoalTool and MemoryTool)
        let mut work_brain_builder = SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model(model.clone())
            .add_tool(Box::new(agent_core::tools::DebugTool::new()))
            .add_tool(Box::new(GoalTool::new(goal_state.clone())))
            .add_tool(Box::new(MemoryTool::new(memory_state.clone())))
            .add_system_prompt_segment(Box::new(agent_core::StaticSystemPromptSegment::new(work_brain_prompt)))
            .add_system_prompt_segment(Box::new(crate::GoalSegment::new(goal_state.clone())))
            .add_system_prompt_segment(Box::new(crate::MemorySegment::new(memory_state.clone())))
            .set_history(Box::new(agent_core::PersistentHistory::new(Rc::clone(&work_dir))));
            // NOTE: BotPromptSegment not added to Work Brain - it needs freedom to think/act in detail

        // Add tools to Work Brain
        for constructor in tool_constructors.borrow().iter() {
            work_brain_builder = work_brain_builder.add_tool(constructor());
        }

        let work_brain_session = work_brain_builder.build()?;

        // Conversation Brain prompt and session
        let bot_protocol = Self::build_conversation_brain_prompt(&name);

        // Create Conversation Brain Session with GoalTool, MemoryTool and PersistentHistory
        let conversation_brain_session = SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model(model.clone())
            .add_tool(Box::new(agent_core::tools::DebugTool::new()))
            .add_tool(Box::new(GoalTool::new(goal_state.clone())))
            .add_tool(Box::new(MemoryTool::new(memory_state.clone())))
            .add_system_prompt_segment(Box::new(agent_core::StaticSystemPromptSegment::new(bot_protocol)))
            .add_system_prompt_segment(Box::new(crate::BotPromptSegment::new(goal_state.clone(), memory_state.clone())))
            .set_history(Box::new(agent_core::PersistentHistory::new(conv_dir.clone())))
            .build()?;

        // Introspection Brain prompt and session
        let introspection_prompt = include_str!("../prompts/introspection_brain.md");

        // Create Introspection Brain Session - background worker for knowledge curation and memory compression
        let introspection_brain_session = SessionBuilder::new(runtime)
            .set_default_model(model)
            .add_tool(Box::new(crate::KnowledgeTool::new(Rc::clone(&knowledge_base))))
            .add_tool(Box::new(crate::HistoryTool::new(conv_dir, work_dir)))
            .add_tool(Box::new(MemoryTool::new(memory_state.clone())))
            .add_system_prompt_segment(Box::new(agent_core::StaticSystemPromptSegment::new(introspection_prompt.to_string())))
            .set_history(Box::new(agent_core::PersistentHistory::new(introspection_dir)))
            .build()?;

        Self::new_with_sessions(
            conversation_brain_session,
            work_brain_session,
            introspection_brain_session,
            name,
            goal_state,
            memory_state,
            knowledge_base,
            sink,
        )
    }

    fn new_with_sessions(
        conversation_brain_session: Session,
        work_brain_session: Session,
        introspection_brain_session: Session,
        name: impl Into<String>,
        goal_state: GoalState,
        memory_state: MemoryState,
        knowledge_base: Rc<crate::KnowledgeBase>,
        sink: impl BotEventSink + 'static,
    ) -> Result<Self> {
        let name = name.into();

        // Create Inner early with empty brains
        let conversation_brain_ref = Rc::new(RefCell::new(None));
        let work_brain_ref = Rc::new(RefCell::new(None));
        let introspection_brain_ref = Rc::new(RefCell::new(None));

        let inner = Rc::new(RefCell::new(Inner {
            sink: Box::new(sink),
            conversation_brain: Rc::clone(&conversation_brain_ref),
            work_brain: Rc::clone(&work_brain_ref),
            introspection_brain: Rc::clone(&introspection_brain_ref),
            goal_state: goal_state.clone(),
            memory_state: memory_state.clone(),
            knowledge_base: Rc::clone(&knowledge_base),
        }));

        // Create Work Brain (ReActAgent) with longer timeout for complex tasks
        let work_brain_agent = Box::new(ReActAgent::new().with_logging(true)) as Box<dyn Agent>;
        let work_brain_config = BrainConfig::new()
            .with_timeout(std::time::Duration::from_secs(30 * 60)); // 30 minutes
        let work_brain = Brain::new_with_config(
            "work-brain",
            work_brain_session,
            work_brain_agent,
            WorkBrainSink {
                bot_name: name.clone(),
                inner: Rc::clone(&inner),
            },
            work_brain_config,
        )?;

        // Store work_brain
        *work_brain_ref.borrow_mut() = Some(Box::new(work_brain));

        // Create Conversation Brain (LlmAgent) with Bot protocol prompt
        let conversation_brain_agent = Box::new(LlmAgent::new()) as Box<dyn Agent>;
        let conversation_brain = Brain::new(
            "conversation-brain",
            conversation_brain_session,
            conversation_brain_agent,
            BrainToBotSink {
                bot_name: name.clone(),
                inner: Rc::clone(&inner),
            },
        )?;

        // Store conversation_brain
        *conversation_brain_ref.borrow_mut() = Some(Box::new(conversation_brain));

        // Create Introspection Brain (ReActAgent) - background worker with reasoning
        let introspection_brain_agent = Box::new(ReActAgent::new().with_logging(true)) as Box<dyn Agent>;
        let introspection_brain_config = BrainConfig::new()
            .with_timeout(std::time::Duration::from_secs(10 * 60)); // 10 minutes
        let introspection_brain = Brain::new_with_config(
            "introspection-brain",
            introspection_brain_session,
            introspection_brain_agent,
            IntrospectionBrainSink {
                bot_name: name.clone(),
                inner: Rc::clone(&inner),
            },
            introspection_brain_config,
        )?;

        // Store introspection_brain
        *introspection_brain_ref.borrow_mut() = Some(Box::new(introspection_brain));

        Ok(Self {
            name,
            goal_state,
            memory_state,
            knowledge_base,
            _inner: inner,
        })
    }

    fn build_work_brain_prompt(_bot_name: &str) -> String {
        include_str!("../prompts/work_brain.md").to_string()
    }

    fn build_conversation_brain_prompt(_bot_name: &str) -> String {
        include_str!("../prompts/conversation_brain.md").to_string()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn push(&self, msg: Envelope) {
        // to is always this bot.
        debug_assert_eq!(msg.to, self.name);

        let line = format!("@{}: {}", msg.from, msg.content);

        // Clone the Rc and drop the outer borrow before borrowing the inner RefCell,
        // following the same pattern used in WorkBrainSink/BrainToBotSink.
        let conv_brain_rc = Rc::clone(&self._inner.borrow().conversation_brain);
        if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
            conv_brain.push_input(line);
        } else {
            tracing::error!("[Bot::{}] Conversation brain not initialized", self.name);
        }
    }

    /// Manually trigger introspection brain to curate knowledge and compress memory
    pub fn trigger_introspection(&self) {
        tracing::info!("[Bot::{}] Triggering introspection brain", self.name);

        let introspection_brain_rc = Rc::clone(&self._inner.borrow().introspection_brain);
        if let Some(brain) = introspection_brain_rc.borrow().as_ref() {
            brain.push_input("Perform introspection: review histories, extract knowledge, and compress memory if needed.".to_string());
        } else {
            tracing::error!("[Bot::{}] Introspection brain not initialized", self.name);
        }
    }

    /// Check if memory should trigger introspection (> 8000 tokens)
    pub fn should_trigger_introspection(&self) -> bool {
        let token_count = self.memory_state.count_tokens();
        token_count > 8000
    }

    /// Check memory size and trigger introspection if needed
    pub fn check_and_trigger_introspection(&self) {
        if self.should_trigger_introspection() {
            tracing::info!(
                "[Bot::{}] Memory exceeds 8000 tokens, triggering introspection",
                self.name
            );
            self.trigger_introspection();
        }
    }

    pub fn shutdown(&self) {
        // Clone the Rc handles and drop the outer borrow before borrowing
        // the inner RefCells, to avoid nested borrow panics.
        let conv_brain_rc = Rc::clone(&self._inner.borrow().conversation_brain);
        let work_brain_rc = Rc::clone(&self._inner.borrow().work_brain);
        let introspection_brain_rc = Rc::clone(&self._inner.borrow().introspection_brain);
        if let Some(conv_brain) = conv_brain_rc.borrow().as_ref() {
            conv_brain.shutdown();
        }
        if let Some(work_brain) = work_brain_rc.borrow().as_ref() {
            work_brain.shutdown();
        }
        if let Some(introspection_brain) = introspection_brain_rc.borrow().as_ref() {
            introspection_brain.shutdown();
        }
    }
}

impl Drop for Bot {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Structured message format for bot protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BotMessage {
    to: String,
    content: String,
}

fn parse_brain_output(s: &str, bot_name: &str) -> Result<Vec<(String, String)>> {
    // Try JSON parsing first (preferred format).
    if let Ok((to, content)) = try_parse_json(s) {
        return Ok(vec![(to, content)]);
    }

    // Fall back to legacy text protocol for backward compatibility.
    parse_text_protocol(s, bot_name)
}

fn try_parse_json(s: &str) -> Result<(String, String)> {
    let trimmed = s.trim();

    // Try direct JSON parsing.
    if let Ok(msg) = serde_json::from_str::<BotMessage>(trimmed) {
        validate_bot_message(&msg)?;
        return Ok((msg.to, msg.content));
    }

    // Try extracting JSON from markdown code block: ```json\n{...}\n```
    if let Some(json_str) = extract_json_from_markdown(trimmed) {
        let msg = serde_json::from_str::<BotMessage>(&json_str)
            .context("failed to parse JSON from markdown block")?;
        validate_bot_message(&msg)?;
        return Ok((msg.to, msg.content));
    }

    anyhow::bail!("not valid JSON format")
}

fn markdown_json_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n```")
            .expect("invalid markdown JSON regex")
    })
}

fn extract_json_from_markdown(s: &str) -> Option<String> {
    // Match ```json or ``` followed by JSON content.
    // (?s) enables DOTALL mode where . matches newlines.
    markdown_json_regex()
        .captures(s)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn validate_bot_message(msg: &BotMessage) -> Result<()> {
    anyhow::ensure!(!msg.to.trim().is_empty(), "bot message 'to' field is empty");
    anyhow::ensure!(
        !msg.content.trim().is_empty(),
        "bot message 'content' field is empty"
    );
    Ok(())
}

fn parse_text_protocol(s: &str, bot_name: &str) -> Result<Vec<(String, String)>> {
    let trimmed = s.trim();
    anyhow::ensure!(
        !trimmed.is_empty(),
        "invalid brain output (empty text)"
    );

    // Parse all @recipient: content messages
    // Format: @recipient: content (starts at line beginning, NO leading spaces)
    // STRICT RULE: Only @ at the ABSOLUTE start of a line (column 0) indicates a new message
    // Middle @ or indented @ are part of content, not message delimiters
    let lines: Vec<&str> = trimmed.lines().collect();
    let mut messages: Vec<(String, String)> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // STRICT: Check if this line starts with @ at position 0 (no trim_start!)
        if line.starts_with('@') {
            // Parse @recipient: content
            let Some((left, right)) = line.split_once(':') else {
                anyhow::bail!(
                    "invalid brain output (line missing ':' after '@'). Line: {}",
                    line
                );
            };

            let recipient = left.trim_start_matches('@').trim();
            anyhow::ensure!(
                !recipient.is_empty(),
                "invalid brain output (empty recipient)"
            );

            // Collect content: rest of current line + all following lines until next @ at line start
            let mut content_parts = vec![right.trim()];
            i += 1;

            // Continue until we see @ at the ABSOLUTE start of a line (no trim_start!)
            while i < lines.len() && !lines[i].starts_with('@') {
                content_parts.push(lines[i]);
                i += 1;
            }

            let content = content_parts.join("\n").trim().to_string();

            if content.is_empty() {
                tracing::warn!(
                    "parse_text_protocol: dropping message to '{}' with empty content",
                    recipient
                );
            } else {
                messages.push((recipient.to_string(), content));
            }
        } else {
            // Lines before the first @recipient are preamble; warn so they are not
            // silently swallowed.
            tracing::warn!(
                "parse_text_protocol: ignoring non-@recipient line: {:?}",
                line
            );
            i += 1;
        }
    }

    anyhow::ensure!(
        !messages.is_empty(),
        "invalid brain output (no valid @recipient: message found)"
    );

    // Filter out messages to self (either @self or @bot_name)
    let valid_messages: Vec<_> = messages
        .into_iter()
        .filter(|(to, _)| to != "self" && to != bot_name)
        .collect();

    anyhow::ensure!(
        !valid_messages.is_empty(),
        "invalid brain output (all messages are to self, no output message)"
    );

    // Return all valid messages
    Ok(valid_messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_json_from_markdown ──

    #[test]
    fn extract_json_from_markdown_basic() {
        let input = "```json\n{\"to\":\"alice\",\"content\":\"hi\"}\n```";
        let result = extract_json_from_markdown(input);
        assert_eq!(result, Some("{\"to\":\"alice\",\"content\":\"hi\"}".to_string()));
    }

    #[test]
    fn extract_json_from_markdown_no_json_tag() {
        let input = "```\n{\"to\":\"bob\",\"content\":\"hey\"}\n```";
        let result = extract_json_from_markdown(input);
        assert_eq!(result, Some("{\"to\":\"bob\",\"content\":\"hey\"}".to_string()));
    }

    #[test]
    fn extract_json_from_markdown_no_fences() {
        let input = "{\"to\":\"carol\",\"content\":\"plain\"}";
        assert_eq!(extract_json_from_markdown(input), None);
    }

    #[test]
    fn extract_json_from_markdown_multiline() {
        let input = "```json\n{\n  \"to\": \"dave\",\n  \"content\": \"line1\\nline2\"\n}\n```";
        let result = extract_json_from_markdown(input).unwrap();
        assert!(result.contains("\"to\": \"dave\""));
    }

    // ── validate_bot_message ──

    #[test]
    fn validate_bot_message_ok() {
        let msg = BotMessage {
            to: "alice".to_string(),
            content: "hello".to_string(),
        };
        assert!(validate_bot_message(&msg).is_ok());
    }

    #[test]
    fn validate_bot_message_empty_to() {
        let msg = BotMessage {
            to: "  ".to_string(),
            content: "hello".to_string(),
        };
        assert!(validate_bot_message(&msg).is_err());
    }

    #[test]
    fn validate_bot_message_empty_content() {
        let msg = BotMessage {
            to: "alice".to_string(),
            content: "   ".to_string(),
        };
        assert!(validate_bot_message(&msg).is_err());
    }

    // ── try_parse_json ──

    #[test]
    fn try_parse_json_direct() {
        let input = r#"{"to":"alice","content":"hi"}"#;
        let (to, content) = try_parse_json(input).unwrap();
        assert_eq!(to, "alice");
        assert_eq!(content, "hi");
    }

    #[test]
    fn try_parse_json_in_markdown() {
        let input = "Sure, here is the response:\n```json\n{\"to\":\"bob\",\"content\":\"hello\"}\n```";
        let (to, content) = try_parse_json(input).unwrap();
        assert_eq!(to, "bob");
        assert_eq!(content, "hello");
    }

    #[test]
    fn try_parse_json_garbage() {
        assert!(try_parse_json("not json at all").is_err());
    }

    // ── parse_text_protocol ──

    #[test]
    fn parse_text_protocol_single_message() {
        let input = "@alice: hello world";
        let messages = parse_text_protocol(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "hello world");
    }

    #[test]
    fn parse_text_protocol_multiline_content() {
        let input = "@alice: line1\nline2\nline3";
        let messages = parse_text_protocol(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "line1\nline2\nline3");
    }

    #[test]
    fn parse_text_protocol_multiple_recipients() {
        // Should return all non-self messages
        let input = "@alice: first msg\n@bob: second msg";
        let messages = parse_text_protocol(input, "mybot").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "first msg");
        assert_eq!(messages[1].0, "bob");
        assert_eq!(messages[1].1, "second msg");
    }

    #[test]
    fn parse_text_protocol_filters_self() {
        let input = "@self: internal note\n@alice: real message";
        let messages = parse_text_protocol(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "real message");
    }

    #[test]
    fn parse_text_protocol_filters_bot_name() {
        let input = "@mybot: note to myself\n@alice: outgoing";
        let messages = parse_text_protocol(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "outgoing");
    }

    #[test]
    fn parse_text_protocol_all_self_messages_errors() {
        let input = "@self: thinking out loud";
        assert!(parse_text_protocol(input, "mybot").is_err());
    }

    #[test]
    fn parse_text_protocol_empty_input_errors() {
        assert!(parse_text_protocol("", "mybot").is_err());
    }

    #[test]
    fn parse_text_protocol_no_recipient_errors() {
        let input = "just some random text without @ prefix";
        assert!(parse_text_protocol(input, "mybot").is_err());
    }

    #[test]
    fn parse_text_protocol_missing_colon_errors() {
        let input = "@alice hello without colon";
        assert!(parse_text_protocol(input, "mybot").is_err());
    }

    #[test]
    fn parse_text_protocol_empty_recipient_errors() {
        let input = "@: no recipient";
        assert!(parse_text_protocol(input, "mybot").is_err());
    }

    // ── parse_brain_output (integration of both paths) ──

    #[test]
    fn parse_brain_output_prefers_json() {
        let input = r#"{"to":"alice","content":"json wins"}"#;
        let messages = parse_brain_output(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "json wins");
    }

    #[test]
    fn parse_brain_output_falls_back_to_text() {
        let input = "@alice: text protocol fallback";
        let messages = parse_brain_output(input, "mybot").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "alice");
        assert_eq!(messages[0].1, "text protocol fallback");
    }

    // ── truncate_str ──

    #[test]
    fn truncate_str_short() {
        assert_eq!(truncate_str("hi", 10), "hi");
    }

    #[test]
    fn truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn truncate_str_long() {
        let result = truncate_str("hello world", 5);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn truncate_str_unicode() {
        // Ensure we count characters, not bytes
        let s = "héllo"; // 5 chars but more than 5 bytes (é is 2 bytes)
        assert_eq!(truncate_str(s, 5), "héllo");
    }
}
