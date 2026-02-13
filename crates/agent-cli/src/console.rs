use agent_core::RunnerConsole;

pub struct StdoutRunnerConsole;

impl RunnerConsole for StdoutRunnerConsole {
    fn print_line(&mut self, s: &str) {
        println!("{s}");
    }
}
