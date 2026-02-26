use agent_bot::{BotConfig, Team, TeamConfig, TeamEvent, TeamEventSink};
use agent_core::{tools::DebugTool, Agent, AgentContext, LocalSpawner, RuntimeBuilder};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc};

struct TokioLocalSpawner;

impl LocalSpawner for TokioLocalSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

/// Agent that does nothing (for testing)
struct NoOpAgent;

#[async_trait::async_trait(?Send)]
impl Agent for NoOpAgent {
    async fn run(&self, _ctx: &AgentContext<'_>) -> Result<()> {
        Ok(())
    }
}

struct CollectSink {
    events: Rc<RefCell<Vec<TeamEvent>>>,
}

impl TeamEventSink for CollectSink {
    fn emit(&mut self, event: TeamEvent) {
        let cloned_event = match event {
            TeamEvent::UserMessage { content } => TeamEvent::UserMessage { content },
            TeamEvent::BotCreated { name } => TeamEvent::BotCreated { name },
            TeamEvent::Error { error } => TeamEvent::Error {
                error: anyhow::anyhow!("{}", error),
            },
        };
        self.events.borrow_mut().push(cloned_event);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn team_with_default_config() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = CollectSink {
                events: Rc::clone(&events),
            };

            // Create team with default config
            let config = TeamConfig::new();

            let team = Team::new_with_config(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                Box::new(NoOpAgent),
                sink,
                config,
            )
            .unwrap();

            assert_eq!(team.bot_count(), 1);
            assert_eq!(team.leader_name(), "LeaderBot");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_with_default_bot_config() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = CollectSink {
                events: Rc::clone(&events),
            };

            // Configure default bot config with tools
            let default_bot_config = BotConfig::new()
                .add_tool(|| Box::new(DebugTool::new()))
                .add_tool(|| Box::new(agent_core::tools::FileTool::new()));

            let config = TeamConfig::new().with_default_bot_config(default_bot_config);

            let team = Team::new_with_config(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                Box::new(NoOpAgent),
                sink,
                config,
            )
            .unwrap();

            // Create worker bots - should inherit default config
            team.create_bot("Worker1", Box::new(NoOpAgent)).unwrap();
            team.create_bot("Worker2", Box::new(NoOpAgent)).unwrap();

            assert_eq!(team.bot_count(), 3);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_with_separate_leader_config() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = CollectSink {
                events: Rc::clone(&events),
            };

            // Default config for workers
            let default_bot_config = BotConfig::new().add_tool(|| Box::new(DebugTool::new()));

            // Special config for leader
            let leader_config = BotConfig::new()
                .with_model("gpt-4")
                .add_tool(|| Box::new(DebugTool::new()))
                .add_tool(|| Box::new(agent_core::tools::FileTool::new()))
                .add_system_prompt("You are the leader bot.".to_string());

            let config = TeamConfig::new()
                .with_default_bot_config(default_bot_config)
                .with_leader_config(leader_config);

            let team = Team::new_with_config(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                Box::new(NoOpAgent),
                sink,
                config,
            )
            .unwrap();

            // Create workers with default config
            team.create_bot("Worker1", Box::new(NoOpAgent)).unwrap();

            assert_eq!(team.bot_count(), 2);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_create_bot_with_custom_config() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = CollectSink {
                events: Rc::clone(&events),
            };

            let default_bot_config = BotConfig::new().add_tool(|| Box::new(DebugTool::new()));

            let config = TeamConfig::new().with_default_bot_config(default_bot_config);

            let team = Team::new_with_config(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                Box::new(NoOpAgent),
                sink,
                config,
            )
            .unwrap();

            // Create bot with custom config (specialized)
            let specialized_config = BotConfig::new()
                .with_model("gpt-3.5-turbo")
                .add_tool(|| Box::new(agent_core::tools::FileTool::new()))
                .add_system_prompt("You are a specialized bot.".to_string());

            team.create_bot_with_config("SpecializedBot", Box::new(NoOpAgent), specialized_config)
                .unwrap();

            assert_eq!(team.bot_count(), 2);

            let bots = team.list_bots();
            assert!(bots.contains(&"LeaderBot".to_string()));
            assert!(bots.contains(&"SpecializedBot".to_string()));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn bot_config_builder_pattern() {
    let config = BotConfig::new()
        .with_model("gpt-4")
        .add_tool(|| Box::new(DebugTool::new()))
        .add_tool(|| Box::new(agent_core::tools::FileTool::new()))
        .add_system_prompt("System prompt 1".to_string())
        .add_system_prompt("System prompt 2".to_string());

    assert_eq!(config.default_model, "gpt-4");
    assert_eq!(config.tool_constructors.len(), 2);
    assert_eq!(config.system_prompt_segments.len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn team_backward_compatibility() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let runtime = Rc::new(
                RuntimeBuilder::new()
                    .set_local_spawner(Rc::new(TokioLocalSpawner))
                    .build(),
            );

            let events = Rc::new(RefCell::new(Vec::new()));
            let sink = CollectSink {
                events: Rc::clone(&events),
            };

            // Old API should still work
            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                Box::new(NoOpAgent),
                sink,
            )
            .unwrap();

            assert_eq!(team.bot_count(), 1);

            // Old create_bot API should still work
            team.create_bot("Worker1", Box::new(NoOpAgent)).unwrap();

            assert_eq!(team.bot_count(), 2);
        })
        .await;
}
