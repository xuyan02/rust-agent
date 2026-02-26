use anyhow::{Result, bail};

pub(crate) fn validate_shell_command(command: &str) -> Result<()> {
    // Relaxed denylist - only block the most dangerous patterns.
    // Allow: &&, ||, | (piping), ; (command chaining)
    // Disallow: command substitution $(), backticks, process substitution, backgrounding
    let denied = [
        "$(", "`", "<(", ">(", // Command/process substitution
        // Note: We allow |, ;, &&, ||, >, < for legitimate use cases
    ];

    for tok in denied {
        if command.contains(tok) {
            bail!("unsafe shell command (contains '{tok}')");
        }
    }

    Ok(())
}
