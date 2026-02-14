use anyhow::Result;

pub trait Console: Send {
    fn print(&mut self, s: &str) -> Result<()>;
    fn eprint(&mut self, s: &str) -> Result<()>;
}

pub struct CliConsole;

impl CliConsole {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CliConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl Console for CliConsole {
    fn print(&mut self, s: &str) -> Result<()> {
        print!("{s}");
        Ok(())
    }

    fn eprint(&mut self, s: &str) -> Result<()> {
        eprint!("{s}");
        Ok(())
    }
}
