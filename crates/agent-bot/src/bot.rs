use crate::{Brain, BrainEvent, BrainEventSink, GoalState, GoalTool};
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

struct WorkBrainSink {
    bot_name: String,
    conversation_brain: Rc<RefCell<Option<Brain>>>,
}

impl BrainEventSink for WorkBrainSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                eprintln!("[Bot::{}::WorkBrain] Result: {}", self.bot_name,
                    if text.len() > 200 { format!("{}...", &text[..200]) } else { text.clone() });

                // Send result back to conversation brain as observation
                if let Some(conv_brain) = self.conversation_brain.borrow().as_ref() {
                    conv_brain.push_input(format!("Work brain result:\n{}", text));
                } else {
                    eprintln!("[Bot::{}::WorkBrain] Conversation brain not available", self.bot_name);
                }
            }
            BrainEvent::Error { error } => {
                eprintln!("[Bot::{}::WorkBrain] Error: {}", self.bot_name, error);

                // Send error back to conversation brain
                if let Some(conv_brain) = self.conversation_brain.borrow().as_ref() {
                    conv_brain.push_input(format!("Work brain error: {}", error));
                } else {
                    eprintln!("[Bot::{}::WorkBrain] Conversation brain not available to report error", self.bot_name);
                }
            }
        }
    }
}

impl BrainEventSink for BrainToBotSink {
    fn emit(&mut self, event: BrainEvent) {
        match event {
            BrainEvent::OutputText { text } => {
                let parsed = parse_brain_output(&text, &self.bot_name);
                match parsed {
                    Ok((to, content)) => {
                        // Check if this is a message to work-brain
                        if to == "work-brain" {
                            // Route to work brain
                            let mut inner = self.inner.borrow_mut();
                            if let Some(work_brain) = &inner.work_brain {
                                eprintln!("[Bot::{}] Routing message to work-brain", self.bot_name);
                                work_brain.push_input(content);
                            } else {
                                eprintln!("[Bot::{}] Work brain not available", self.bot_name);
                                inner.sink.emit(BotEvent::Error {
                                    error: anyhow::anyhow!("work-brain is not available"),
                                });
                            }
                        } else {
                            // Regular message - output to external
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
    work_brain: Option<Brain>,
    goal_state: GoalState,
}

pub struct Bot {
    name: String,
    conversation_brain: Brain,
    work_brain: Brain,
    goal_state: GoalState,

    // Keep alive for BrainToBotSink.
    _inner: Rc<RefCell<Inner>>,

    // Queue for potential future correlation/ordering; not used for routing.
    _inbox: Rc<RefCell<VecDeque<Envelope>>>,
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

        // Create shared goal state
        let goal_state = GoalState::new();

        // Setup DataStore and create dir_node for this bot
        let dir_node = if let Some(data_store) = runtime.data_store() {
            let store = Rc::new(agent_core::DataStore::new(data_store.root().to_path_buf()));
            let bot_dir = store.root_dir().subdir(&name);
            Some(bot_dir)
        } else {
            None
        };

        // Create Work Brain Session (clean, no Bot protocol, only has tools)
        let mut work_brain_builder = SessionBuilder::new(Rc::clone(&runtime))
            .set_default_model(model.clone())
            .add_tool(Box::new(agent_core::tools::DebugTool::new()));

        // Add tools to Work Brain
        for constructor in tool_constructors.borrow().iter() {
            work_brain_builder = work_brain_builder.add_tool(constructor());
        }

        let work_brain_session = work_brain_builder.build()?;

        // Create Conversation Brain Session with GoalTool and PersistentHistory
        let mut conversation_brain_builder = SessionBuilder::new(runtime)
            .set_default_model(model)
            .add_tool(Box::new(agent_core::tools::DebugTool::new()))
            .add_tool(Box::new(GoalTool::new(goal_state.clone())));

        // Set dir_node for persistent storage
        if let Some(dir_node) = dir_node {
            conversation_brain_builder = conversation_brain_builder.set_dir_node(dir_node);
        }

        // Use PersistentHistory for Conversation Brain with compression enabled
        conversation_brain_builder = conversation_brain_builder
            .set_history(Box::new(
                agent_core::PersistentHistory::new()
            ));

        let conversation_brain_session = conversation_brain_builder.build()?;

        Self::new_with_sessions(
            conversation_brain_session,
            work_brain_session,
            name,
            goal_state,
            sink,
        )
    }

    fn new_with_sessions(
        conversation_brain_session: Session,
        work_brain_session: Session,
        name: impl Into<String>,
        goal_state: GoalState,
        sink: impl BotEventSink + 'static,
    ) -> Result<Self> {
        let name = name.into();

        // Create holder for conversation brain reference (used by Work Brain sink)
        let conversation_brain_ref = Rc::new(RefCell::new(None));

        // Create Work Brain (ReActAgent)
        let work_brain_agent = Box::new(ReActAgent::new().with_logging(true)) as Box<dyn Agent>;
        let work_brain = Brain::new(
            work_brain_session,
            work_brain_agent,
            WorkBrainSink {
                bot_name: name.clone(),
                conversation_brain: Rc::clone(&conversation_brain_ref),
            },
        )?;

        // Create Inner with work_brain reference
        let inner = Rc::new(RefCell::new(Inner {
            sink: Box::new(sink),
            work_brain: Some(work_brain.clone()),
            goal_state: goal_state.clone(),
        }));

        // Bot protocol prompt for Conversation Brain
        let bot_protocol = Self::build_conversation_brain_prompt(&name, &goal_state);

        // Create Conversation Brain (LlmAgent) with Bot protocol prompt
        let conversation_brain_agent = Box::new(LlmAgent::new()) as Box<dyn Agent>;
        let conversation_brain = Brain::new_with_system_prompts(
            conversation_brain_session,
            conversation_brain_agent,
            BrainToBotSink {
                bot_name: name.clone(),
                inner: Rc::clone(&inner),
            },
            vec![bot_protocol],
        )?;

        // Store conversation_brain reference for Work Brain sink
        *conversation_brain_ref.borrow_mut() = Some(conversation_brain.clone());

        Ok(Self {
            name,
            conversation_brain: conversation_brain.clone(),
            work_brain,
            goal_state,
            _inner: inner,
            _inbox: Rc::new(RefCell::new(VecDeque::new())),
        })
    }

    fn build_conversation_brain_prompt(bot_name: &str, _goal_state: &GoalState) -> String {
        format!(
            "You are @{bot_name}. You are the Conversation Brain responsible for external communication and task coordination.\n\n\
            GOAL MANAGEMENT:\n\
            - Use 'set-goal' tool to define your current objective\n\
            - Use 'get-goal' tool to check the current goal\n\
            - The goal guides both you and the work brain\n\
            - Update the goal as tasks evolve\n\n\
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
            WORK DELEGATION:\n\
            For complex tasks requiring multiple steps or deep analysis:\n\
            - Send tasks to @work-brain: with clear instructions\n\
            - Work brain will use tools and reasoning to complete the task\n\
            - You'll receive results as 'Work brain result:' input\n\
            - Then respond to the original sender with the results\n\n\
            Example:\n\
            Input:  @user: Analyze the codebase structure\n\
            Output: @work-brain: Analyze the codebase structure. List all modules and their purposes.\n\
            [Work brain completes task...]\n\
            Input:  Work brain result: [analysis results]\n\
            Output: @user: Here's the codebase analysis: [results]\n\n\
            ✓ CORRECT:\n\
            Output: @alice: Hi there!\n\
            Output: @work-brain: Please analyze file.rs\n\n\
            ✗ WRONG (These will FAIL):\n\
            Output: Hello!                    ← MISSING @recipient:\n\
            Output: Let me think...           ← MISSING @recipient:\n"
        )
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn push(&self, msg: Envelope) {
        // to is always this bot.
        debug_assert_eq!(msg.to, self.name);

        self._inbox.borrow_mut().push_back(msg.clone());

        let line = format!("@{}: {}", msg.from, msg.content);
        self.conversation_brain.push_input(line);
    }

    pub fn shutdown(&self) {
        self.conversation_brain.shutdown();
        self.work_brain.shutdown();
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
