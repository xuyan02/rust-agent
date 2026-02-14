use anyhow::{Result, bail};

pub(crate) fn validate_shell_command(command: &str) -> Result<()> {
    // Strict denylist to match Mi Code safety constraints.
    // Disallow substitution, redirection, piping, backgrounding, and command chaining.
    let denied = [
        "$(", "`", "<(", ">(", "|", ";", "&&", "||", "&", ">", "<", "2>", "1>", "&>",
    ];

    for tok in denied {
        if command.contains(tok) {
            bail!("unsafe shell command (contains '{tok}')");
        }
    }

    Ok(())
}
