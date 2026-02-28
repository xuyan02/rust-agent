use crate::AgentContext;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use super::{tool, tool_fn};

/// File operations tool for workspace
pub struct FileTool;

impl FileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[tool(id = "file", description = "Workspace file operations")]
impl FileTool {
    #[tool_fn(
        name = "file-read",
        description = "Read a UTF-8 text file under workspace",
        args(offset_lines(default = 0), limit_lines(default = 200))
    )]
    pub async fn read(
        &self,
        ctx: &AgentContext<'_>,
        path: String,
        offset_lines: i64,
        limit_lines: i64,
    ) -> Result<String> {
        let abs = resolve_workspace_relative(ctx.session().workspace_path(), &path)?;

        let meta = tokio::fs::metadata(&abs)
            .await
            .with_context(|| format!("failed to stat {}", abs.display()))?;
        if meta.is_dir() {
            bail!(
                "path is a directory (file-read expects a file): {}",
                abs.display()
            );
        }

        let s = tokio::fs::read_to_string(&abs)
            .await
            .with_context(|| format!("failed to read {}", abs.display()))?;

        let offset = usize::try_from(offset_lines.max(0)).unwrap_or(0);
        let limit = usize::try_from(limit_lines.max(0)).unwrap_or(0);

        if offset == 0 && limit == 0 {
            return Ok(String::new());
        }
        if offset == 0 && limit >= s.lines().count() {
            return Ok(s);
        }

        let out = s
            .lines()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(out)
    }

    #[tool_fn(
        name = "file-glob",
        description = "Find files under workspace by glob pattern. Pattern is relative to workspace. Note: recursive wildcard '**' must be a full path component (use '**/*.rs', not '**.rs'). Examples: '**/*.rs', '.github/workflows/*.yml', 'crates/*/src/**/*.rs'.",
        args(limit(default = 200))
    )]
    pub async fn glob(
        &self,
        ctx: &AgentContext<'_>,
        pattern: String,
        limit: i64,
    ) -> Result<String> {
        let workspace = ctx.session().workspace_path();
        let pattern = pattern.trim();
        if pattern.is_empty() {
            bail!("pattern must not be empty");
        }
        if std::path::Path::new(pattern).is_absolute() {
            bail!("pattern must be relative");
        }

        let full_pattern = workspace.join(pattern).to_string_lossy().to_string();
        let limit = usize::try_from(limit.max(0)).unwrap_or(0).min(10_000);

        let mut out = Vec::new();
        for entry in glob::glob(&full_pattern).with_context(|| "invalid glob pattern")? {
            let path = entry?;
            if path.is_dir() {
                continue;
            }
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            out.push(rel);
            if limit != 0 && out.len() >= limit {
                break;
            }
        }

        Ok(out.join("\n"))
    }

    #[tool_fn(
        name = "file-write",
        description = "Write a UTF-8 text file under workspace",
        args(overwrite(default = false))
    )]
    pub async fn write(
        &self,
        ctx: &AgentContext<'_>,
        path: String,
        content: String,
        overwrite: bool,
    ) -> Result<String> {
        let abs = resolve_workspace_relative(ctx.session().workspace_path(), &path)?;
        if tokio::fs::try_exists(&abs).await.unwrap_or(false) && !overwrite {
            bail!("file exists (set overwrite=true)");
        }
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create dir {}", parent.display()))?;
        }
        tokio::fs::write(&abs, content)
            .await
            .with_context(|| format!("failed to write {}", abs.display()))?;

        Ok(String::new())
    }

    #[tool_fn(
        name = "file-edit",
        description = "Edit a file by finding and replacing text. Performs exact string matching (not regex). Use this instead of file-read + file-write for surgical edits.",
        args(replace_all(default = false))
    )]
    pub async fn edit(
        &self,
        ctx: &AgentContext<'_>,
        path: String,
        old_text: String,
        new_text: String,
        replace_all: bool,
    ) -> Result<String> {
        let abs = resolve_workspace_relative(ctx.session().workspace_path(), &path)?;

        // Check if file exists
        if !tokio::fs::try_exists(&abs).await.unwrap_or(false) {
            bail!("file does not exist: {}", abs.display());
        }

        // Read the file
        let content = tokio::fs::read_to_string(&abs)
            .await
            .with_context(|| format!("failed to read {}", abs.display()))?;

        // Check if old_text exists in content
        if !content.contains(&old_text) {
            bail!("old_text not found in file");
        }

        // Perform replacement
        let new_content = if replace_all {
            content.replace(&old_text, &new_text)
        } else {
            // Replace only the first occurrence
            content.replacen(&old_text, &new_text, 1)
        };

        // Count how many replacements were made
        let old_count = content.matches(&old_text).count();
        let new_count = new_content.matches(&old_text).count();
        let replaced_count = old_count - new_count;

        // Write back
        tokio::fs::write(&abs, new_content)
            .await
            .with_context(|| format!("failed to write {}", abs.display()))?;

        Ok(format!("Replaced {} occurrence(s) in {}", replaced_count, path))
    }
}

/// Resolve a workspace-relative path, ensuring it doesn't escape the workspace
fn resolve_workspace_relative(workspace: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        bail!("path must be relative");
    }

    for c in rel_path.components() {
        if matches!(c, std::path::Component::ParentDir) {
            bail!("path must not contain '..'");
        }
    }

    let joined = workspace.join(rel_path);
    let canon_workspace = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    let canon_joined = joined.canonicalize().unwrap_or_else(|_| joined.clone());

    if !canon_joined.starts_with(&canon_workspace) {
        bail!("path escapes workspace");
    }

    Ok(joined)
}