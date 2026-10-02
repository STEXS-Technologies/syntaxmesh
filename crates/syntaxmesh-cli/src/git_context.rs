use std::path::Path;
use std::process::{Command, Output};

#[derive(Debug)]
pub enum GitContextError {
    Io(std::io::Error),
    Command(String),
}

impl std::fmt::Display for GitContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not run Git: {error}"),
            Self::Command(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for GitContextError {}

fn git(root: &Path, args: &[&str]) -> Result<Output, GitContextError> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(GitContextError::Io)
}

fn successful_text(output: &Output, operation: &str) -> Result<String, GitContextError> {
    if !output.status.success() {
        return Err(GitContextError::Command(format!(
            "Git {operation} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn describe(root: &Path) -> Result<String, GitContextError> {
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if !top.status.success() {
        return Ok(format!(
            "git_available=true repository=false reason={}",
            String::from_utf8_lossy(&top.stderr)
                .trim()
                .replace('\n', " ")
        ));
    }
    let worktree = successful_text(&top, "rev-parse")?;
    let head_output = git(root, &["rev-parse", "HEAD"])?;
    let head = successful_text(&head_output, "rev-parse HEAD")?;
    let branch_output = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let branch = if branch_output.status.success() {
        successful_text(&branch_output, "symbolic-ref")?
    } else {
        "detached".to_owned()
    };
    let status_output = git(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    let dirty = !successful_text(&status_output, "status")?.is_empty();
    Ok(format!(
        "git_available=true repository=true worktree={worktree} branch={branch} head={head} dirty={dirty}"
    ))
}
