use crate::{Bot, BotEvent, BotEventSink, Envelope};
use agent_core::Runtime;
use anyhow::Result;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Events emitted by the Team.
#[derive(Debug)]
pub enum TeamEvent {
    /// Message from leader to user.
    UserMessage { content: String },
    /// A new bot was created.
    BotCreated { name: String },
    /// Error occurred.
    Error { error: anyhow::Error },
}

pub trait TeamEventSink {
    fn emit(&mut self, event: TeamEvent);
}

/// Type alias for tool constructor functions.
pub type ToolConstructor = Box<dyn Fn() -> Box<dyn agent_core::tools::Tool>>;

/// Configuration for a single Bot.
pub struct BotConfig {
    /// Default model to use for this bot.
    pub default_model: String,

    /// Tool constructors for this bot (wrapped in Rc<RefCell<>> for interior mutability).
    pub tool_constructors: Rc<RefCell<Vec<ToolConstructor>>>,

    /// System prompt segments for this bot.
    pub system_prompt_segments: Vec<String>,
}

impl BotConfig {
    /// Creates a new BotConfig with default settings.
    pub fn new() -> Self {
        Self {
            default_model: "gpt-4o".to_string(),
            tool_constructors: Rc::new(RefCell::new(vec![])),
            system_prompt_segments: vec![],
        }
    }

    /// Sets the default model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Adds a tool constructor.
    pub fn add_tool<F>(self, constructor: F) -> Self
    where
        F: Fn() -> Box<dyn agent_core::tools::Tool> + 'static,
    {
        self.tool_constructors.borrow_mut().push(Box::new(constructor));
        self
    }

    /// Adds a system prompt segment.
    pub fn add_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt_segments.push(prompt.into());
        self
    }
}

impl Default for BotConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for Team behavior.
pub struct TeamConfig {
    /// Default configuration for all bots in the team.
    pub default_bot_config: BotConfig,

    /// Optional configuration specifically for the leader bot.
    /// If None, uses default_bot_config.
    pub leader_config: Option<BotConfig>,
}

impl TeamConfig {
    /// Creates a new TeamConfig with default settings.
    pub fn new() -> Self {
        Self {
            default_bot_config: BotConfig::new(),
            leader_config: None,
        }
    }

    /// Sets the default bot configuration for all bots.
    pub fn with_default_bot_config(mut self, config: BotConfig) -> Self {
        self.default_bot_config = config;
        self
    }

    /// Sets a specific configuration for the leader bot.
    pub fn with_leader_config(mut self, config: BotConfig) -> Self {
        self.leader_config = Some(config);
        self
    }

    /// Gets the configuration to use for the leader.
    fn get_leader_config(&self) -> &BotConfig {
        self.leader_config.as_ref().unwrap_or(&self.default_bot_config)
    }

    /// Gets the configuration to use for worker bots.
    fn get_worker_config(&self) -> &BotConfig {
        &self.default_bot_config
    }
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self::new()
    }
}

struct Inner {
    runtime: Rc<Runtime>,
    user_name: String,
    leader_name: String,
    bots: HashMap<String, Bot>,
    sink: Box<dyn TeamEventSink>,
    config: TeamConfig,
}

/// A team of bots that can collaborate together.
/// One bot is designated as the leader who can create other bots and communicate with the user.
pub struct Team {
    inner: Rc<RefCell<Inner>>,
}

impl Team {
    /// Creates a new Team with a leader bot using default configuration.
    ///
    /// # Arguments
    /// * `runtime` - Shared runtime for all bots
    /// * `user_name` - Name of the user interacting with the team
    /// * `leader_name` - Name for the leader bot
    /// * `sink` - Event sink for team events
    pub fn new(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        sink: impl TeamEventSink + 'static,
    ) -> Result<Self> {
        Self::new_with_config(runtime, user_name, leader_name, sink, TeamConfig::new())
    }

    /// Creates a new Team with a leader bot using custom configuration.
    ///
    /// # Arguments
    /// * `runtime` - Shared runtime for all bots
    /// * `user_name` - Name of the user interacting with the team
    /// * `leader_name` - Name for the leader bot
    /// * `sink` - Event sink for team events
    /// * `config` - Team configuration including common tools
    pub fn new_with_config(
        runtime: Rc<Runtime>,
        user_name: impl Into<String>,
        leader_name: impl Into<String>,
        sink: impl TeamEventSink + 'static,
        config: TeamConfig,
    ) -> Result<Self> {
        let user_name = user_name.into();
        let leader_name = leader_name.into();

        let inner = Rc::new(RefCell::new(Inner {
            runtime: Rc::clone(&runtime),
            user_name: user_name.clone(),
            leader_name: leader_name.clone(),
            bots: HashMap::new(),
            sink: Box::new(sink),
            config,
        }));

        let team = Team {
            inner: Rc::clone(&inner),
        };

        // Create leader bot with TeamBotSink
        let leader_sink = TeamBotSink {
            inner: Rc::clone(&inner),
        };

        // Get leader config and tool constructors reference
        let (model, tool_constructors) = {
            let inner_borrow = inner.borrow();
            let leader_config = inner_borrow.config.get_leader_config();
            (leader_config.default_model.clone(), Rc::clone(&leader_config.tool_constructors))
        };

        // Bot creates Main Brain and Deep Brain internally
        let leader = Bot::new(
            Rc::clone(&runtime),
            leader_name.clone(),
            model,
            tool_constructors,
            leader_sink,
        )?;

        inner.borrow_mut().bots.insert(leader_name, leader);

        Ok(team)
    }

    /// Sends a message from the user to the leader.
    pub fn send_user_message(&self, content: impl Into<String>) {
        let inner = self.inner.borrow();
        let envelope = Envelope {
            from: inner.user_name.clone(),
            to: inner.leader_name.clone(),
            content: content.into(),
        };

        if let Some(leader) = inner.bots.get(&inner.leader_name) {
            leader.push(envelope);
        }
    }

    /// Creates a new bot and adds it to the team.
    /// The bot will use the default bot configuration from TeamConfig.
    pub fn create_bot(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        let mut inner = self.inner.borrow_mut();

        // Check if bot already exists
        if inner.bots.contains_key(&name) {
            anyhow::bail!("Bot with name '{}' already exists", name);
        }

        // Get worker config and tool constructors
        let runtime = Rc::clone(&inner.runtime);
        let worker_config = inner.config.get_worker_config();
        let model = worker_config.default_model.clone();
        let tool_constructors = Rc::clone(&worker_config.tool_constructors);

        // Create bot with TeamBotSink
        let bot_sink = TeamBotSink {
            inner: Rc::clone(&self.inner),
        };

        let bot = Bot::new(runtime, name.clone(), model, tool_constructors, bot_sink)?;

        inner.bots.insert(name.clone(), bot);

        // Emit bot created event
        inner.sink.emit(TeamEvent::BotCreated { name });

        Ok(())
    }

    /// Creates a new bot with a custom configuration.
    ///
    /// # Arguments
    /// * `name` - Name for the new bot
    /// * `bot_config` - Custom configuration for this specific bot
    ///
    /// # Example
    /// ```ignore
    /// let custom_config = BotConfig::new()
    ///     .with_model("gpt-4")
    ///     .add_tool(|| Box::new(DataTool::new()))
    ///     .add_tool(|| Box::new(PlotTool::new()));
    ///
    /// team.create_bot_with_config(
    ///     "DataAnalyzer",
    ///     custom_config,
    /// )?;
    /// ```
    pub fn create_bot_with_config(
        &self,
        name: impl Into<String>,
        bot_config: BotConfig,
    ) -> Result<()> {
        let name = name.into();
        let mut inner = self.inner.borrow_mut();

        // Check if bot already exists
        if inner.bots.contains_key(&name) {
            anyhow::bail!("Bot with name '{}' already exists", name);
        }

        // Get tool constructors from custom config
        let runtime = Rc::clone(&inner.runtime);
        let model = bot_config.default_model;
        let tool_constructors = bot_config.tool_constructors;

        // Create bot with TeamBotSink
        let bot_sink = TeamBotSink {
            inner: Rc::clone(&self.inner),
        };

        let bot = Bot::new(runtime, name.clone(), model, tool_constructors, bot_sink)?;

        inner.bots.insert(name.clone(), bot);

        // Emit bot created event
        inner.sink.emit(TeamEvent::BotCreated { name });

        Ok(())
    }

    /// Sends a message from one bot to another.
    /// This is for external orchestration if needed.
    pub fn send_bot_message(&self, from: impl Into<String>, to: impl Into<String>, content: impl Into<String>) {
        let from = from.into();
        let to = to.into();
        let inner = self.inner.borrow();

        if let Some(target_bot) = inner.bots.get(&to) {
            target_bot.push(Envelope {
                from,
                to,
                content: content.into(),
            });
        }
    }

    /// Shuts down all bots in the team.
    pub fn shutdown(&self) {
        let inner = self.inner.borrow();
        for bot in inner.bots.values() {
            bot.shutdown();
        }
    }

    /// Returns the number of bots in the team.
    pub fn bot_count(&self) -> usize {
        self.inner.borrow().bots.len()
    }

    /// Returns the leader name.
    pub fn leader_name(&self) -> String {
        self.inner.borrow().leader_name.clone()
    }

    /// Returns the user name.
    pub fn user_name(&self) -> String {
        self.inner.borrow().user_name.clone()
    }

    /// Lists all bot names in the team.
    pub fn list_bots(&self) -> Vec<String> {
        self.inner.borrow().bots.keys().cloned().collect()
    }
}

/// Bot event sink that routes messages within the team.
struct TeamBotSink {
    inner: Rc<RefCell<Inner>>,
}

impl BotEventSink for TeamBotSink {
    fn emit(&mut self, event: BotEvent) {
        match event {
            BotEvent::OutputMessage { message } => {
                let mut inner = self.inner.borrow_mut();

                // Route message based on recipient
                // Check both configured user_name and common aliases like "user"
                let is_user_message = message.to == inner.user_name
                    || message.to.eq_ignore_ascii_case("user");

                if is_user_message {
                    // Message to user - only leader can send to user
                    if message.from == inner.leader_name {
                        inner.sink.emit(TeamEvent::UserMessage {
                            content: message.content,
                        });
                    } else {
                        // Non-leader bots cannot send directly to user
                        let leader_name = inner.leader_name.clone();
                        inner.sink.emit(TeamEvent::Error {
                            error: anyhow::anyhow!(
                                "Bot '{}' cannot send messages to user directly. \
                                 Only leader '{}' can communicate with the user.",
                                message.from,
                                leader_name
                            ),
                        });
                    }
                } else {
                    // Message to another bot
                    // Check if target exists first
                    let target_exists = inner.bots.contains_key(&message.to);

                    if target_exists {
                        // Clone the envelope before dropping the borrow
                        let envelope = message.clone();
                        drop(inner); // Drop borrow before pushing

                        // Re-borrow to get the target bot and push
                        let inner = self.inner.borrow();
                        if let Some(target_bot) = inner.bots.get(&envelope.to) {
                            target_bot.push(envelope);
                        }
                    } else {
                        inner.sink.emit(TeamEvent::Error {
                            error: anyhow::anyhow!(
                                "Bot '{}' tried to send message to unknown bot '{}'",
                                message.from,
                                message.to
                            ),
                        });
                    }
                }
            }
            BotEvent::Error { error } => {
                let mut inner = self.inner.borrow_mut();
                inner.sink.emit(TeamEvent::Error { error });
            }
        }
    }
}

impl Drop for Team {
    fn drop(&mut self) {
        self.shutdown();
    }
}

