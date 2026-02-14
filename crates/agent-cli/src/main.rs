use anyhow::Result;

mod app;
mod config;

const USAGE: &str = "agent_cli [--input <text>]\n\nRuns an agent.\n- Loads config from <cwd>/.agent/agent.yaml\n\nModes:\n  (default) interactive console\n  --input <text> single-shot mode (no stdin watch)\n";

fn print_usage() {
    print!("{USAGE}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut input: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--input" => {
                let v = match args.next() {
                    Some(v) => v,
                    None => {
                        eprintln!("error: --input requires a value");
                        std::process::exit(2);
                    }
                };
                input = Some(v);
            }
            _ => {
                eprintln!("error: unknown arg: {arg}");
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }

    app::run(app::Args { input }).await
}
