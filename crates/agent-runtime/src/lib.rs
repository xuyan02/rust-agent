use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub trait Console {
    fn print_line(&mut self, s: &str);
    fn print(&mut self, s: &str);
}

pub struct Runtime<'a, C: Console> {
    console: &'a mut C,
    prompts_dir: PathBuf,
}

impl<'a, C: Console> Runtime<'a, C> {
    pub fn new(console: &'a mut C, prompts_dir: impl Into<PathBuf>) -> Self {
        Self {
            console,
            prompts_dir: prompts_dir.into(),
        }
    }

    pub fn init(&mut self) -> bool {
        // Keep parity with C++ test: init just needs prompts path to exist and be readable.
        self.prompts_dir.exists()
    }

    pub async fn get_prompt(&mut self, name: &str) -> Result<String> {
        let p = self.prompts_dir.join(format!("{name}.md"));
        tokio::fs::read_to_string(&p)
            .await
            .with_context(|| format!("failed to read {}", p.display()))
    }

    pub fn console_mut(&mut self) -> &mut C {
        self.console
    }

    pub fn prompts_dir(&self) -> &Path {
        &self.prompts_dir
    }
}
