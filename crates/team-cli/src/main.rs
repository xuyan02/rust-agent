use agent_bot::{BotConfig, Team, TeamConfig, TeamEvent, TeamEventSink};
use agent_core::{
    tools::{FileTool, ShellTool},
    LocalSpawner, RuntimeBuilder,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{pin::Pin, rc::Rc, sync::mpsc, time::Duration};
use tokio::io::AsyncBufReadExt;

const USAGE: &str = "team-cli [--user <name>] [--leader <name>] [--cfg <path>] [--timeout-ms <n>]

Interactive CLI to test agent_bot::Team for multi-bot collaboration.

Defaults:
  --user Alice
  --leader LeaderBot
  --cfg ./.agent/agent.yaml
  --timeout-ms (no timeout)

The leader bot can create other bots by outputting JSON:
  {\"to\": \"<new_bot_name>\", \"content\": \"@create_bot <bot_name>\"}

Bots communicate with each other via JSON messages:
  {\"to\": \"<target_bot>\", \"content\": \"<message>\"}
";

// Note: Leader system prompt could be added in the future to guide leader behavior.
// For now, the Bot's built-in routing protocol prompt is used.
// Future enhancement: Add Team::new_with_custom_session to allow custom prompts.

#[derive(Debug, Clone, Deserialize)]
struct OpenAiCfg {
    base_url: String,
    api_key: String,

    #[serde(default)]
    model_provider_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentCfg {
    #[allow(dead_code)]
    model: Option<String>,
    openai: Option<OpenAiCfg>,
}

async fn load_cfg(path: impl AsRef<std::path::Path>) -> Result<AgentCfg> {
    let path = path.as_ref();
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("failed to read config: {}", path.display()))?;
    let cfg: AgentCfg = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
    Ok(cfg)
}

struct TokioSpawner;

impl LocalSpawner for TokioSpawner {
    fn spawn_local(&self, fut: Pin<Box<dyn std::future::Future<Output = ()>>>) {
        tokio::task::spawn_local(fut);
    }
}

struct ChannelSink {
    tx: mpsc::Sender<TeamEvent>,
}

impl TeamEventSink for ChannelSink {
    fn emit(&mut self, event: TeamEvent) {
        let _ = self.tx.send(event);
    }
}

fn print_usage() {
    print!("{USAGE}");
}

fn main() -> Result<()> {
    // Initialize tracing for logging
    // Set RUST_LOG environment variable to control log level
    // Example: RUST_LOG=info or RUST_LOG=debug
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let mut user_name = "Alice".to_string();
    let mut leader_name = "LeaderBot".to_string();
    let mut cfg_path: std::path::PathBuf = std::path::PathBuf::from(".agent").join("agent.yaml");
    let mut timeout_ms: Option<u64> = None; // No timeout by default

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--user" => {
                user_name = args.next().context("--user requires a value")?;
            }
            "--leader" => {
                leader_name = args.next().context("--leader requires a value")?;
            }
            "--cfg" => {
                cfg_path = std::path::PathBuf::from(args.next().context("--cfg requires a value")?);
            }
            "--timeout-ms" => {
                let val: u64 = args
                    .next()
                    .context("--timeout-ms requires a value")?
                    .parse()
                    .context("--timeout-ms must be an integer")?;
                timeout_ms = Some(val);
            }
            _ => {
                eprintln!("error: unknown arg: {arg}");
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let cfg = rt.block_on(load_cfg(&cfg_path))?;
    let model = cfg
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("missing model in {}", cfg_path.display()))?;
    let openai = cfg
        .openai
        .ok_or_else(|| anyhow::anyhow!("missing openai config in {}", cfg_path.display()))?;

    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        // Set data_store_root for Bot persistent history
        let data_store_root = std::env::current_dir()
            .context("failed to get current directory")?
            .join(".agent");

        let runtime = Rc::new(
            RuntimeBuilder::new()
                .set_local_spawner(Rc::new(TokioSpawner))
                .set_data_store_root(data_store_root)
                .add_llm_provider(Box::new(agent_core::llm::OpenAiProvider::new(
                    agent_core::llm::OpenAiProviderConfig {
                        base_url: openai.base_url,
                        api_key: openai.api_key,
                        model_provider_id: openai.model_provider_id,
                    },
                )))
                .build(),
        );

        let (tx, rx) = mpsc::channel();

        // Configure default bot config with common tools
        // These tools will be available to all bots (leader and workers)
        // Bot creates Main Brain and Deep Brain internally

        eprintln!("[team-cli] Using model from config: {}", model);

        let default_bot_config = BotConfig::new()
            .with_model(model)  // Use model from config file
            .add_tool(|| Box::new(FileTool::new()))
            .add_tool(|| Box::new(ShellTool::new()));

        let team_config = TeamConfig::new()
            .with_default_bot_config(default_bot_config);

        let team = Team::new_with_config(
            Rc::clone(&runtime),
            user_name.clone(),
            leader_name.clone(),
            ChannelSink { tx },
            team_config,
        )?;

        eprintln!("=== Team CLI Ready ===");
        eprintln!("User: {}", user_name);
        eprintln!("Leader: {}", leader_name);
        eprintln!("Type messages and press enter. Type 'exit' to quit.");
        eprintln!("Type 'status' to see team status.");
        eprintln!();

        let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        let mut deadline: Option<std::time::Instant> = None;

        loop {
            tokio::select! {
                line = stdin.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let trimmed = line.trim().to_string();

                    if trimmed == "exit" {
                        team.shutdown();
                        break;
                    }

                    if trimmed == "status" {
                        eprintln!("\n=== Team Status ===");
                        eprintln!("Total bots: {}", team.bot_count());
                        eprintln!("Bots: {:?}", team.list_bots());
                        eprintln!();
                        continue;
                    }

                    if trimmed.is_empty() {
                        continue;
                    }

                    eprintln!("[You -> {}]: {}", leader_name, trimmed);
                    team.send_user_message(trimmed);
                    // Reset deadline for this request (only if timeout is configured)
                    deadline = timeout_ms.map(|ms| std::time::Instant::now() + Duration::from_millis(ms));
                }

                _ = tokio::task::yield_now() => {
                    match rx.try_recv() {
                        Ok(TeamEvent::UserMessage { content }) => {
                            println!("\n[{} -> You]: {}\n", leader_name, content);
                            // Clear deadline after receiving response
                            deadline = None;
                        }
                        Ok(TeamEvent::BotCreated { name }) => {
                            eprintln!("\n✓ New bot created: {}", name);
                            eprintln!("  Total bots: {}\n", team.bot_count());
                        }
                        Ok(TeamEvent::Error { error }) => {
                            eprintln!("\n✗ Team error: {}\n", error);
                            // Clear deadline after error
                            deadline = None;
                        }
                        Err(mpsc::TryRecvError::Empty) => {
                            // Check timeout only if waiting for a response
                            if let Some(dl) = deadline {
                                if std::time::Instant::now() >= dl {
                                    team.shutdown();
                                    anyhow::bail!("timed out waiting for team response");
                                }
                            }
                        }
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }
            }
        }

        eprintln!("\nShutting down team...");
        team.shutdown();

        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}
