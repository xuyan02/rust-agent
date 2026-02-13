use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

fn write_text(p: &Path, s: &str) -> Result<()> {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(p, s).with_context(|| format!("failed to write {}", p.display()))
}

fn expand_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(rest).to_string_lossy().to_string();
    }

    s.to_string()
}

#[test]
fn config_path_tilde_expansion_smoke() -> Result<()> {
    let tmp = std::env::temp_dir().join("agent-test-config-path");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;

    let cfg_path = tmp.join("agent.yaml");
    write_text(
        &cfg_path,
        "model: test-model\nopenai:\n  base_url: http://example\n  api_key: test\n",
    )?;

    // load by absolute path works
    let cfg = agent_config::load_agent_config_yaml(&cfg_path)?;
    assert_eq!(cfg.model, "test-model");

    // tilde expansion behavior (we mimic cpp test intent via helper)
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        let home_tmp = PathBuf::from(&home).join("agent-test-config-path");
        let _ = fs::remove_dir_all(&home_tmp);
        fs::create_dir_all(&home_tmp)?;
        let home_cfg = home_tmp.join("agent.yaml");
        write_text(
            &home_cfg,
            "model: test-model\nopenai:\n  base_url: http://example\n  api_key: test\n",
        )?;

        let tilde_path = "~/agent-test-config-path/agent.yaml".to_string();
        let expanded = expand_tilde(&tilde_path);
        let cfg2 = agent_config::load_agent_config_yaml(&expanded)?;
        assert_eq!(cfg2.model, "test-model");

        let _ = fs::remove_dir_all(&home_tmp);
    }

    let _ = fs::remove_dir_all(&tmp);
    Ok(())
}
