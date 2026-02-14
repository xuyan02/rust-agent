use crate::support::console::Console;
use crate::{Agent, AgentContext, make_user_message};
use anyhow::Result;

pub trait RunnerConsole {
    fn print_line(&mut self, s: &str);
}

pub struct RunnerConsoleAdapter<'a> {
    console: &'a mut dyn Console,
}

impl<'a> RunnerConsoleAdapter<'a> {
    pub fn new(console: &'a mut dyn Console) -> Self {
        Self { console }
    }
}

impl<'a> RunnerConsole for RunnerConsoleAdapter<'a> {
    fn print_line(&mut self, s: &str) {
        let _ = self.console.print(s);
        let _ = self.console.print("\n");
    }
}

pub struct AgentRunner<A: Agent> {
    agent: A,
}

impl<A: Agent> AgentRunner<A> {
    pub fn new(agent: A) -> Self {
        Self { agent }
    }

    pub async fn run_line(
        &mut self,
        ctx: &AgentContext<'_>,
        console: &mut dyn RunnerConsole,
        line: String,
    ) -> Result<()> {
        let _ = ctx.history().append(make_user_message(line)).await;
        self.agent.run(ctx).await?;

        // Print latest assistant text, even if the most recent entries are tool call/results.
        let all = ctx.history().get_all().await?;
        for m in all.iter().rev() {
            if m.role != crate::llm::ChatRole::Assistant {
                continue;
            }
            if let crate::llm::ChatContent::Text(t) = &m.content {
                console.print_line(t);
                break;
            }
        }

        Ok(())
    }
}
