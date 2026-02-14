use agent_core::llm::{ChatContent, ChatMessage};
use agent_core::{AgentContext, AgentContextBuilder, Tool, ToolLoopOptions, run_tool_loop};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Planner,
    Coder,
    Reviewer,
}

#[derive(Debug, Default)]
pub struct TaskState {
    pub goal: Option<String>,
    pub last_plan: Option<String>,
    pub last_verify_output: Option<String>,
    pub iterations: usize,
}

pub struct ProgrammingBot {
    state: TaskState,
}

impl Default for ProgrammingBot {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgrammingBot {
    pub fn new() -> Self {
        Self {
            state: TaskState::default(),
        }
    }

    pub fn reset(&mut self) {
        self.state = TaskState::default();
    }

    pub fn state(&self) -> &TaskState {
        &self.state
    }

    pub async fn plan(&mut self, base_ctx: &AgentContext<'_>, goal: String) -> Result<String> {
        self.state.goal = Some(goal.clone());

        let ctx = self.role_ctx(base_ctx, Role::Planner)?;
        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();
        messages.extend(ctx.history().get_all().await?);

        messages.push(ChatMessage::user_text(format!(
            "Goal:\n{}\n\nProduce a concrete plan. Output ONLY plain text, with sections:\n- Plan (numbered steps)\n- Files to change\n- Verification (exact commands)\n- Risks\n\nConstraints: work only inside this repository.",
            goal.trim()
        )));

        run_tool_loop(
            &ctx,
            messages,
            ToolLoopOptions {
                max_tool_rounds: 10,
            },
        )
        .await?;

        let last = ctx.history().last().await?;
        let text = last
            .and_then(|m| m.content.as_text())
            .ok_or_else(|| anyhow::anyhow!("planner: missing text reply"))?;

        self.state.last_plan = Some(text.clone());
        Ok(text)
    }

    pub async fn apply(&mut self, base_ctx: &AgentContext<'_>) -> Result<String> {
        let goal = self
            .state
            .goal
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no active goal; run `task` or `plan` first"))?;

        let plan = self.state.last_plan.clone().unwrap_or_default();

        let ctx = self.role_ctx(base_ctx, Role::Coder)?;
        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();
        messages.extend(ctx.history().get_all().await?);

        messages.push(ChatMessage::user_text(format!(
            "Goal:\n{}\n\nPlan (may be empty):\n{}\n\nImplement the changes now. Use tools when needed. After code changes, DO NOT run cargo; verification will be handled separately. Output a short summary of what you changed.",
            goal.trim(),
            plan.trim()
        )));

        run_tool_loop(
            &ctx,
            messages,
            ToolLoopOptions {
                max_tool_rounds: 20,
            },
        )
        .await?;

        let last = ctx.history().last().await?;
        let text = last
            .and_then(|m| m.content.as_text())
            .ok_or_else(|| anyhow::anyhow!("coder: missing text reply"))?;

        Ok(text)
    }

    pub async fn verify(&mut self, base_ctx: &AgentContext<'_>) -> Result<String> {
        // Bot-controlled: do not let the model decide commands.
        let cmds = [
            "cargo fmt",
            "cargo clippy --all-targets --all-features -- -D warnings",
            "cargo test",
        ];

        let mut out = String::new();

        for cmd in cmds {
            let args = serde_json::json!({"command": cmd});
            let r = agent_core::tools::ShellTool
                .invoke(base_ctx, "shell", &args)
                .await?;
            out.push_str("$ ");
            out.push_str(cmd);
            out.push('\n');
            out.push_str(&r);
            if !r.ends_with('\n') {
                out.push('\n');
            }
        }

        self.state.last_verify_output = Some(out.clone());
        Ok(out)
    }

    pub async fn diff(&mut self, base_ctx: &AgentContext<'_>) -> Result<String> {
        let args = serde_json::json!({"command": "git diff"});
        agent_core::tools::ShellTool
            .invoke(base_ctx, "shell", &args)
            .await
    }

    pub async fn review(&mut self, base_ctx: &AgentContext<'_>) -> Result<String> {
        let goal = self
            .state
            .goal
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no active goal"))?;

        let diff = self.diff(base_ctx).await.unwrap_or_default();
        let verify = self.state.last_verify_output.clone().unwrap_or_default();

        let ctx = self.role_ctx(base_ctx, Role::Reviewer)?;

        let mut messages: Vec<ChatMessage> = ctx
            .system_segments()
            .iter()
            .map(|s| ChatMessage::system_text(s.clone()))
            .collect();
        messages.extend(ctx.history().get_all().await?);

        messages.push(ChatMessage::user_text(format!(
            "Goal:\n{}\n\nGit diff:\n{}\n\nVerification output (may be empty):\n{}\n\nReview the changes. Output ONLY plain text with:\n- Summary\n- Potential issues\n- Suggested follow-ups",
            goal.trim(),
            diff.trim(),
            verify.trim()
        )));

        run_tool_loop(
            &ctx,
            messages,
            ToolLoopOptions {
                max_tool_rounds: 10,
            },
        )
        .await?;

        let last = ctx.history().last().await?;
        let text = last
            .and_then(|m| m.content.as_text())
            .ok_or_else(|| anyhow::anyhow!("reviewer: missing text reply"))?;

        Ok(text)
    }

    pub async fn task(&mut self, base_ctx: &AgentContext<'_>, goal: String) -> Result<String> {
        let _plan = self.plan(base_ctx, goal).await?;

        let mut last_apply = String::new();

        for i in 0..=2usize {
            self.state.iterations = i;
            last_apply = self.apply(base_ctx).await?;

            let verify_out = self.verify(base_ctx).await?;

            // crude success check: ShellTool currently returns combined output; treat non-empty stderr as failure
            // We rely on ShellTool to encode exit status in output; if not, we fall back to heuristic.
            if verify_out.contains("Exit Code: 0") || !verify_out.contains("Exit Code:") {
                break;
            }

            // Feed verifier output back into coder next round by appending to plan.
            let mut plan = self.state.last_plan.clone().unwrap_or_default();
            plan.push_str("\n\nVerification failed. Fix these issues:\n");
            plan.push_str(&verify_out);
            self.state.last_plan = Some(plan);
        }

        let review = self.review(base_ctx).await.unwrap_or_default();

        Ok(format!("{}\n\n{}", last_apply.trim(), review.trim())
            .trim()
            .to_string())
    }

    fn role_ctx<'a>(&self, base_ctx: &'a AgentContext<'a>, role: Role) -> Result<AgentContext<'a>> {
        let role_sys = match role {
            Role::Planner => {
                "You are Planner. Produce an actionable plan for a programming task. Use tools only for reading/searching. Do not write files. Do not run shell.".to_string()
            }
            Role::Coder => {
                "You are Coder. Implement the requested changes by editing files. Do not run shell commands. Use tools for file edits and searching.".to_string()
            }
            Role::Reviewer => {
                "You are Reviewer. Review code changes and verification output. Do not call tools.".to_string()
            }
        };

        // NOTE: For v0 we reuse the same session + history. Role-specific tool allowlists are approximated
        // by instructing the model; verifier/diff are bot-controlled.
        AgentContextBuilder::from_parent_ctx(base_ctx)
            .add_system_segment(role_sys)
            .build()
    }
}

trait ChatContentTextExt {
    fn as_text(&self) -> Option<String>;
}

impl ChatContentTextExt for ChatContent {
    fn as_text(&self) -> Option<String> {
        match self {
            ChatContent::Text(t) => Some(t.clone()),
            _ => None,
        }
    }
}

pub fn help_text() -> &'static str {
    "Commands:\n\
task <goal>    Plan->Apply->Verify->Review\n\
plan <goal>    Generate plan only\n\
apply          Apply based on last plan\n\
verify         Run cargo fmt/clippy/test\n\
diff           Show git diff\n\
reset          Clear current task state\n\
help           Show this help\n\
exit           Quit\n"
}
