use crate::{Brain, BrainEvent, BrainEventSink};
use agent_core::{Agent, LlmAgent, ReActAgent, Session, SessionBuilder};
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: String,
    pub to: String,
    pub content: String,
}

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

impl BrainEventSink for BrainToBotSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                let parsed = parse_brain_output(&text, &self.bot_name);
                match parsed {
                    Ok((to, content)) => {
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
                    Err(error) => {
                        // Log the error with output details for debugging
                        eprintln!("[Bot::{}] Output parsing failed. Raw output:\n{}\nError: {}",
                            self.bot_name, text, error);
                        self.inner.borrow_mut().sink.emit(BotEvent::Error { error });
                    }
                }
            }
            BrainEvent::Error { error } => {
                self.inner.borrow_mut().sink.emit(BotEvent::Error { error });
            }
        }
    }
}

struct Inner {
    sink: Box<dyn BotEventSink>,
}

pub struct Bot {
    name: String,
    brain: Brain,
    deep_brain_session: Rc<Session>,

    // Keep alive for BrainToBotSink.
    _inner: Rc<RefCell<Inner>>,

    // Queue for potential future correlation/ordering; not used for routing.
    _inbox: Rc<RefCell<VecDeque<Envelope>>>,
}

/// BotDeepThinkTool - DeepThink tool that uses Bot's Deep Brain Session
struct BotDeepThinkTool {
    deep_brain_session: Rc<Session>,
}

impl BotDeepThinkTool {
    fn new(deep_brain_session: Rc<Session>) -> Self {
        Self { deep_brain_session }
    }
}

#[async_trait::async_trait(?Send)]
impl agent_core::tools::Tool for BotDeepThinkTool {
    fn spec(&self) -> &agent_core::tools::ToolSpec {
        use agent_core::tools::*;
        static SPEC: std::sync::OnceLock<ToolSpec> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| ToolSpec {
            id: "deep-think".to_string(),
            description: "Deep reasoning agent for multi-step tasks".to_string(),
            functions: vec![FunctionSpec {
                name: "deep-think".to_string(),
                description: "Delegate any task requiring multiple steps, analysis, or planning to the deep reasoning agent. \
                             It can use tools, think through problems, and provide comprehensive answers. \
                             Use for: analysis, review, debugging, calculation, research, multi-file operations."
                    .to_string(),
                parameters: ObjectSpec {
                    properties: vec![PropertySpec {
                        name: "task".to_string(),
                        ty: TypeSpec::String(StringSpec::default()),
                    }],
                    required: vec!["task".to_string()],
                    additional_properties: false,
                },
            }],
        })
    }

    async fn invoke(
        &self,
        _ctx: &agent_core::AgentContext<'_>,
        function_name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        match function_name {
            "deep-think" => {
                let task = args
                    .get("task")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'task' argument"))?;

                // Create context from Deep Brain's Session (not Main Brain's context)
                // Deep Brain has clean Session without Bot protocol prompts
                let deep_history: Box<dyn agent_core::History> = Box::new(agent_core::InMemoryHistory::new());
                let deep_ctx = agent_core::AgentContextBuilder::from_session(self.deep_brain_session.as_ref())
                    .set_history(deep_history)
                    .build()?;

                // Append task after context is created
                deep_ctx.history()
                    .append(&deep_ctx, agent_core::llm::ChatMessage::user_text(task))
                    .await?;

                // Create and run ReActAgent
                eprintln!("[Bot::DeepBrain] Starting deep reasoning for task");
                let react_agent = ReActAgent::new().with_logging(true);
                if let Err(e) = react_agent.run(&deep_ctx).await {
                    eprintln!("[Bot::DeepBrain] Failed: {}", e);
                    return Err(anyhow::anyhow!(
                        "Deep thinking failed: {}. This might be due to API rate limits or quota restrictions. Try again in a moment.",
                        e
                    ));
                }

                eprintln!("[Bot::DeepBrain] Completed successfully");

                // Extract the final answer from the isolated history
                let messages = deep_ctx.history().get_all(&deep_ctx).await?;
                let last_assistant = messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, agent_core::llm::ChatRole::Assistant))
                    .ok_or_else(|| anyhow::anyhow!("no answer from deep brain"))?;

                match &last_assistant.content {
                    agent_core::llm::ChatContent::Text(text) => {
                        // Extract content after [answer] prefix if present
                        let answer = if let Some(content) = text.strip_prefix("[answer]") {
                            content.trim().to_string()
                        } else {
                            text.clone()
                        };

                        eprintln!("[Bot::DeepBrain] Final answer (length: {} chars):\n{}",
                            answer.len(),
                            if answer.len() > 500 {
                                format!("{}...", &answer[..500])
                            } else {
                                answer.clone()
                            });

                        Ok(answer)
                    }
                    _ => anyhow::bail!("unexpected content type from deep brain"),
                }
            }
            _ => anyhow::bail!("unknown function: {}", function_name),
        }
    }
}

impl Bot {
    /// Creates a new Bot with Main Brain and Deep Brain.
    ///
    /// The Bot manages two agents:
    /// - Main Brain: LlmAgent that handles external communication with Bot protocol
    /// - Deep Brain: ReActAgent for complex reasoning tasks (accessible via deep-think tool)
    ///
    /// # Arguments
    /// * `runtime` - Shared runtime
    /// * `name` - Bot name
    /// * `model` - Model to use (e.g., "gpt-4o")
    /// * `tool_constructors` - Tool constructors (called twice: once for Main Brain, once for Deep Brain)
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

        // Bot protocol prompt (only for Main Brain)
        let bot_protocol = format!(
            "You are @{name}. You MUST follow this output format in EVERY response:\n\n\
            ═══════════════════════════════════════════════════════\n\
            ⚠️  CRITICAL OUTPUT RULE (NEVER SKIP THIS):\n\
            Every response MUST include: @recipient: message\n\
            ═══════════════════════════════════════════════════════\n\n\
            INPUT FORMAT: @sender: message\n\
            OUTPUT FORMAT: @sender: your reply\n\n\
            MESSAGE FORMAT RULES:\n\
            1. Each message line starts with @recipient: at LINE START\n\
            2. Content can span multiple lines until next @recipient:\n\
            3. Only @ at LINE START indicates a new message\n\
            4. End with a newline\n\n\
            THINKING (Optional):\n\
            You can output thinking with @self: before your reply.\n\
            @self messages will be filtered out automatically.\n\n\
            ✓ CORRECT:\n\
            Input:  @alice: Hello\n\
            Output: @alice: Hi there!\n\n\
            Input:  @bob: What's 2+2?\n\
            Output: @self: Let me calculate...\n\
            @bob: 2+2 equals 4.\n\n\
            Input:  @user: Complex question\n\
            Output: @self: Need to think about this step by step.\n\
            @self: First, I should analyze X.\n\
            @self: Then consider Y.\n\
            @user: Here's my answer after thinking...\n\n\
            ✗ WRONG (These will FAIL):\n\
            Output: Hello!                    ← MISSING @recipient:\n\
            Output: Let me think...           ← MISSING @recipient:\n\n\
            AFTER USING TOOLS:\n\
            Still output with @recipient: prefix!\n\
            Example:\n\
            Input: @user: read file.txt\n\
            [you use file-read tool]\n\
            Output: @user: The file contains...\n\n\
            TOOL STRATEGY:\n\
            - Complex tasks (2+ steps): Use deep-think tool\n\
            - Simple tasks (1 step): Use direct tools\n"
        );

        // Create Deep Brain Session (clean, no Bot protocol, only has tools)
        let mut deep_brain_builder = SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model(model.clone())
            .add_tool(Box::new(agent_core::tools::DebugTool::new()));

        // Add tools to Deep Brain
        for constructor in tool_constructors.borrow().iter() {
            deep_brain_builder = deep_brain_builder.add_tool(constructor());
        }

        let deep_brain_session = Rc::new(deep_brain_builder.build()?);

        // Setup DataStore and create dir_node for this bot
        let dir_node = if let Some(data_store) = runtime.data_store() {
            let store = Rc::new(agent_core::DataStore::new(data_store.root().to_path_buf()));
            let bot_dir = store.root_dir().subdir(&name);
            Some(bot_dir)
        } else {
            None
        };

        // Create Main Brain Session with DeepThinkTool and PersistentHistory
        let mut main_brain_builder = SessionBuilder::new(runtime)
            .set_default_model(model)
            .add_tool(Box::new(agent_core::tools::DebugTool::new()))
            .add_tool(Box::new(BotDeepThinkTool::new(Rc::clone(&deep_brain_session))));

        // Add tools to Main Brain
        for constructor in tool_constructors.borrow().iter() {
            main_brain_builder = main_brain_builder.add_tool(constructor());
        }

        // Set dir_node for persistent storage
        if let Some(dir_node) = dir_node {
            main_brain_builder = main_brain_builder.set_dir_node(dir_node);
        }

        // Use PersistentHistory for Main Brain with compression enabled
        main_brain_builder = main_brain_builder
            .set_history(Box::new(
                agent_core::PersistentHistory::new()
            ));

        let main_brain_session = main_brain_builder.build()?;

        Self::new_with_sessions(main_brain_session, name, bot_protocol, deep_brain_session, sink)
    }

    fn new_with_sessions(
        main_brain_session: Session,
        name: impl Into<String>,
        bot_protocol: String,
        deep_brain_session: Rc<Session>,
        sink: impl BotEventSink + 'static,
    ) -> Result<Self> {
        let name = name.into();

        let inner = Rc::new(RefCell::new(Inner {
            sink: Box::new(sink),
        }));

        // Create Main Brain (LlmAgent) with Bot protocol prompt
        let main_brain = Box::new(LlmAgent::new()) as Box<dyn Agent>;

        let brain = Brain::new_with_system_prompts(
            main_brain_session,
            main_brain,
            BrainToBotSink {
                bot_name: name.clone(),
                inner: Rc::clone(&inner),
            },
            vec![bot_protocol],
        )?;

        Ok(Self {
            name,
            brain,
            deep_brain_session,
            _inner: inner,
            _inbox: Rc::new(RefCell::new(VecDeque::new())),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn push(&self, msg: Envelope) {
        // to is always this bot.
        debug_assert_eq!(msg.to, self.name);

        self._inbox.borrow_mut().push_back(msg.clone());

        let line = format!("@{}: {}", msg.from, msg.content);
        self.brain.push_input(line);
    }

    pub fn shutdown(&self) {
        self.brain.shutdown();
    }
}

/// Structured message format for bot protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BotMessage {
    to: String,
    content: String,
}

fn parse_brain_output(s: &str, bot_name: &str) -> Result<(String, String)> {
    // Try JSON parsing first (preferred format).
    if let Ok((to, content)) = try_parse_json(s) {
        return Ok((to, content));
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

fn extract_json_from_markdown(s: &str) -> Option<String> {
    // Match ```json or ``` followed by JSON content.
    // (?s) enables DOTALL mode where . matches newlines.
    let re = regex::Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n```").ok()?;
    re.captures(s)
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

fn parse_text_protocol(s: &str, bot_name: &str) -> Result<(String, String)> {
    let trimmed = s.trim();
    anyhow::ensure!(
        !trimmed.is_empty(),
        "invalid brain output (empty text)"
    );

    // Parse all @recipient: content messages
    // Format: @recipient: content (starts at line beginning)
    // Only line-start @ indicates a new message
    let lines: Vec<&str> = trimmed.lines().collect();
    let mut messages: Vec<(String, String)> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_start();

        // Check if this line starts with @
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

            // Collect content: rest of current line + all following lines until next @
            let mut content_parts = vec![right.trim()];
            i += 1;

            while i < lines.len() && !lines[i].trim_start().starts_with('@') {
                content_parts.push(lines[i]);
                i += 1;
            }

            let content = content_parts.join("\n").trim().to_string();

            if !content.is_empty() {
                messages.push((recipient.to_string(), content));
            }
        } else {
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

    // Return the last valid message
    let (to, content) = valid_messages.into_iter().last().unwrap();
    Ok((to, content))
}
