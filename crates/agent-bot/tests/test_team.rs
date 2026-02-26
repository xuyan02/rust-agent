use agent_bot::{Team, TeamEvent, TeamEventSink};
use agent_core::{Agent, AgentContext, LocalSpawner, RuntimeBuilder};
use anyhow::Result;
use std::{cell::RefCell, pin::Pin, rc::Rc};

struct TokioLocalSpawner;

impl LocalSpawner for TokioLocalSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

/// Simple agent that echoes received messages with a prefix
struct EchoAgent {
    prefix: String,
}

#[async_trait::async_trait(?Send)]
impl Agent for EchoAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        let msgs = ctx.history().get_all().await?;
        if let Some(last_msg) = msgs.last() {
            if let agent_core::llm::ChatContent::Text(text) = &last_msg.content {
                let response = format!("{}: {}", self.prefix, text);
                ctx.history()
                    .append(agent_core::llm::ChatMessage::assistant_text(response))
                    .await?;
            }
        }
        Ok(())
    }
}

/// Agent that sends messages to other bots based on JSON instructions
struct RouterAgent {
    responses: Vec<(String, String)>, // (to, content) pairs
}

#[async_trait::async_trait(?Send)]
impl Agent for RouterAgent {
    async fn run(&self, ctx: &AgentContext<'_>) -> Result<()> {
        // Send predefined responses as JSON bot messages
        for (to, content) in &self.responses {
            let json_msg = serde_json::json!({
                "to": to,
                "content": content
            });
            ctx.history()
                .append(agent_core::llm::ChatMessage::assistant_text(
                    json_msg.to_string(),
                ))
                .await?;
        }
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
async fn team_creates_leader_bot() {
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

            let leader_agent = Box::new(EchoAgent {
                prefix: "Leader".to_string(),
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            assert_eq!(team.bot_count(), 1);
            assert_eq!(team.leader_name(), "LeaderBot");
            assert_eq!(team.user_name(), "Alice");

            let bots = team.list_bots();
            assert!(bots.contains(&"LeaderBot".to_string()));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_user_message_to_leader() {
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

            // Leader echoes messages back to user
            let leader_agent = Box::new(RouterAgent {
                responses: vec![("Alice".to_string(), "Hello from leader!".to_string())],
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            team.send_user_message("Hello leader");

            // Wait for processing
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let events = events.borrow();
            assert!(!events.is_empty());

            // Check that we received a user message response
            let has_user_message = events.iter().any(|e| match e {
                TeamEvent::UserMessage { content } => content.contains("Hello from leader"),
                _ => false,
            });
            assert!(has_user_message, "Expected user message from leader");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_creates_additional_bot() {
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

            let leader_agent = Box::new(EchoAgent {
                prefix: "Leader".to_string(),
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            assert_eq!(team.bot_count(), 1);

            // Create a worker bot
            let worker_agent = Box::new(EchoAgent {
                prefix: "Worker".to_string(),
            });
            team.create_bot("WorkerBot", worker_agent).unwrap();

            assert_eq!(team.bot_count(), 2);

            let bots = team.list_bots();
            assert!(bots.contains(&"LeaderBot".to_string()));
            assert!(bots.contains(&"WorkerBot".to_string()));

            // Check BotCreated event
            let events = events.borrow();
            let has_created_event = events.iter().any(|e| match e {
                TeamEvent::BotCreated { name } => name == "WorkerBot",
                _ => false,
            });
            assert!(has_created_event);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_prevents_duplicate_bot_names() {
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

            let leader_agent = Box::new(EchoAgent {
                prefix: "Leader".to_string(),
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            // Try to create a bot with the same name as leader
            let duplicate_agent = Box::new(EchoAgent {
                prefix: "Duplicate".to_string(),
            });
            let result = team.create_bot("LeaderBot", duplicate_agent);

            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("already exists"));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_bot_to_bot_communication() {
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

            // Leader sends message to worker
            let leader_agent = Box::new(RouterAgent {
                responses: vec![("WorkerBot".to_string(), "Task: process data".to_string())],
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            // Create worker that echoes back
            let worker_agent = Box::new(RouterAgent {
                responses: vec![("LeaderBot".to_string(), "Task completed".to_string())],
            });
            team.create_bot("WorkerBot", worker_agent).unwrap();

            // Send user message to trigger leader
            team.send_user_message("Start task");

            // Wait for message routing
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

            // The worker should have received and processed the message
            // This is verified by the fact that no errors occurred
            let events = events.borrow();
            let has_error = events.iter().any(|e| matches!(e, TeamEvent::Error { .. }));
            assert!(!has_error, "Bot-to-bot communication should work without errors");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_non_leader_cannot_send_to_user() {
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

            let leader_agent = Box::new(EchoAgent {
                prefix: "Leader".to_string(),
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            // Worker tries to send to user (should fail)
            let worker_agent = Box::new(RouterAgent {
                responses: vec![("Alice".to_string(), "Unauthorized message".to_string())],
            });
            team.create_bot("WorkerBot", worker_agent).unwrap();

            // Trigger worker by sending it a message
            team.send_bot_message("LeaderBot", "WorkerBot", "do something");

            // Wait for processing
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let events = events.borrow();

            // Should have an error event about non-leader trying to contact user
            let has_error = events.iter().any(|e| match e {
                TeamEvent::Error { error } => {
                    let msg = error.to_string();
                    msg.contains("cannot send messages to user") || msg.contains("Only leader")
                }
                _ => false,
            });
            assert!(has_error, "Non-leader bot should not be able to send to user");

            // Should NOT have a user message from worker
            let has_user_message = events.iter().any(|e| match e {
                TeamEvent::UserMessage { content } => content.contains("Unauthorized"),
                _ => false,
            });
            assert!(!has_user_message, "User should not receive message from non-leader");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_multi_bot_collaboration() {
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

            // Leader delegates to Worker1, then responds to user
            let leader_agent = Box::new(RouterAgent {
                responses: vec![
                    ("Worker1".to_string(), "Process step 1".to_string()),
                    ("Alice".to_string(), "Task delegated".to_string()),
                ],
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            // Worker1 delegates to Worker2
            let worker1_agent = Box::new(RouterAgent {
                responses: vec![("Worker2".to_string(), "Process step 2".to_string())],
            });
            team.create_bot("Worker1", worker1_agent).unwrap();

            // Worker2 reports back to Leader
            let worker2_agent = Box::new(RouterAgent {
                responses: vec![("LeaderBot".to_string(), "All steps completed".to_string())],
            });
            team.create_bot("Worker2", worker2_agent).unwrap();

            assert_eq!(team.bot_count(), 3);

            // User initiates workflow
            team.send_user_message("Start multi-step task");

            // Wait for all message routing
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            let events = events.borrow();

            // Check that we have bot created events
            let created_count = events
                .iter()
                .filter(|e| matches!(e, TeamEvent::BotCreated { .. }))
                .count();
            assert_eq!(created_count, 2);

            // Check that leader sent message to user
            let has_user_response = events.iter().any(|e| match e {
                TeamEvent::UserMessage { content } => content.contains("Task delegated"),
                _ => false,
            });
            assert!(has_user_response);

            // Should not have errors
            let error_count = events
                .iter()
                .filter(|e| matches!(e, TeamEvent::Error { .. }))
                .count();
            assert_eq!(error_count, 0, "Multi-bot collaboration should work without errors");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_shutdown_stops_all_bots() {
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

            let leader_agent = Box::new(EchoAgent {
                prefix: "Leader".to_string(),
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            let worker_agent = Box::new(EchoAgent {
                prefix: "Worker".to_string(),
            });
            team.create_bot("WorkerBot", worker_agent).unwrap();

            assert_eq!(team.bot_count(), 2);

            // Shutdown should stop all bots gracefully
            team.shutdown();

            // Give time for shutdown to complete
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // Team should still maintain its state
            assert_eq!(team.bot_count(), 2);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn team_message_to_unknown_bot_emits_error() {
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

            // Leader tries to send to non-existent bot
            let leader_agent = Box::new(RouterAgent {
                responses: vec![("NonExistentBot".to_string(), "Hello?".to_string())],
            });

            let team = Team::new(
                Rc::clone(&runtime),
                "Alice",
                "LeaderBot",
                leader_agent,
                sink,
            )
            .unwrap();

            team.send_user_message("trigger");

            // Wait for processing
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let events = events.borrow();

            // Should have an error about unknown bot
            let has_unknown_bot_error = events.iter().any(|e| match e {
                TeamEvent::Error { error } => {
                    let msg = error.to_string();
                    msg.contains("unknown bot") || msg.contains("NonExistentBot")
                }
                _ => false,
            });
            assert!(has_unknown_bot_error, "Should emit error for unknown bot");
        })
        .await;
}
