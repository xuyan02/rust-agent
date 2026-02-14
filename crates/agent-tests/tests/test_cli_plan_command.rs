use agent_core::support::runtime::{Console, Runtime};

struct CaptureConsole {
    out: String,
}

impl Console for CaptureConsole {
    fn print_line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn print(&mut self, s: &str) {
        self.out.push_str(s);
    }
}

#[test]
fn cli_plan_command_smoke_prompt_loads() {
    let mut console = CaptureConsole { out: String::new() };

    let prompts_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("prompts");

    let mut rt = Runtime::new(&mut console, prompts_dir);
    assert!(rt.init());

    let rt2 = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let p = rt2.block_on(rt.get_prompt("intuitive")).unwrap();
    assert!(!p.is_empty());
}
