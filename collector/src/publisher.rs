use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use tracing::info;

use crate::config::Config;

pub struct GitPublisher {
    worktree: PathBuf,
    remote: String,
    branch: String,
    author_name: String,
    author_email: String,
}

impl GitPublisher {
    pub fn from_config(config: &Config) -> Self {
        Self {
            worktree: config.git_worktree.clone(),
            remote: config.git_remote.clone(),
            branch: config.git_branch.clone(),
            author_name: config.git_author_name.clone(),
            author_email: config.git_author_email.clone(),
        }
    }

    pub fn publish_archive(&self, archive_path: &Path, last_day: NaiveDate) -> Result<()> {
        let relative_archive = archive_path
            .strip_prefix(&self.worktree)
            .with_context(|| {
                format!(
                    "archive {} must be below Git worktree {}",
                    archive_path.display(),
                    self.worktree.display()
                )
            })?
            .to_path_buf();

        let relative_archive_string = relative_archive.to_string_lossy().into_owned();
        run_git(&self.worktree, ["add", "--", &relative_archive_string])?;

        let diff = Command::new("git")
            .arg("-C")
            .arg(&self.worktree)
            .args(["diff", "--cached", "--quiet", "--"])
            .arg(&relative_archive)
            .status()
            .context("check staged archive changes")?;

        match diff.code() {
            Some(0) => {}
            Some(1) => {
                let message = format!("data: daily metrics through {last_day}");
                let status = Command::new("git")
                    .arg("-C")
                    .arg(&self.worktree)
                    .arg("-c")
                    .arg(format!("user.name={}", self.author_name))
                    .arg("-c")
                    .arg(format!("user.email={}", self.author_email))
                    .args(["commit", "--only", "-m"])
                    .arg(message)
                    .arg("--")
                    .arg(&relative_archive)
                    .status()
                    .context("commit daily archive")?;
                if !status.success() {
                    bail!("git commit failed with status {status}");
                }
            }
            _ => bail!("git diff --cached failed with status {diff}"),
        }

        run_git(
            &self.worktree,
            ["push", self.remote.as_str(), self.branch.as_str()],
        )?;
        info!(
            archive = %relative_archive.display(),
            remote = %self.remote,
            branch = %self.branch,
            "pushed daily archive"
        );
        Ok(())
    }
}

fn run_git<const N: usize>(worktree: &Path, arguments: [&str; N]) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(arguments)
        .status()
        .with_context(|| format!("run git in {}", worktree.display()))?;
    if !status.success() {
        bail!("git command failed with status {status}");
    }
    Ok(())
}
