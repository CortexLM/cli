//! Pull Request checkout command.
//!
//! Provides commands for working with GitHub pull requests:
//! - `cortex pr <number>` - Checkout a PR branch locally
//!
//! SECURITY: All git command arguments are validated and passed as separate
//! arguments to prevent shell injection attacks.

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Validates a branch name to ensure it doesn't contain shell metacharacters.
/// Branch names should only contain alphanumeric characters, hyphens, underscores,
/// forward slashes, and dots.
fn validate_branch_name(name: &str) -> Result<()> {
    // Git branch names have restrictions - we enforce a strict subset
    // to prevent any potential injection through crafted branch names
    let is_valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
        && !name.is_empty()
        && !name.starts_with('-')
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.ends_with('/')
        && !name.ends_with(".lock");

    if !is_valid {
        bail!(
            "Invalid branch name: '{}'. Branch names must contain only alphanumeric characters, hyphens, underscores, forward slashes, and dots.",
            name
        );
    }
    Ok(())
}

/// Validates a git refspec for fetch operations.
fn validate_refspec(refspec: &str) -> Result<()> {
    // Refspecs should only contain alphanumeric characters, hyphens, underscores,
    // forward slashes, colons, and dots
    let is_valid = refspec.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == ':' || c == '.'
    }) && !refspec.is_empty();

    if !is_valid {
        bail!(
            "Invalid refspec: '{}'. Contains invalid characters.",
            refspec
        );
    }
    Ok(())
}

/// Pull Request CLI.
#[derive(Debug, Parser)]
pub struct PrCli {
    /// PR number to checkout.
    pub number: u64,

    /// Path to the repository root (defaults to current directory).
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Custom local branch name for the PR checkout.
    /// If not specified, defaults to "pr-{number}".
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Force checkout even if there are uncommitted changes.
    /// WARNING: This may result in data loss! Uncommitted changes in your working
    /// directory may be overwritten. Consider using 'git stash' first to save your work.
    #[arg(short = 'F', long)]
    pub force: bool,

    /// Show PR details without checking out.
    #[arg(long)]
    pub info: bool,

    /// Show PR diff without checking out.
    #[arg(long)]
    pub diff: bool,

    /// Show PR comments.
    #[arg(long)]
    pub comments: bool,

    /// Apply AI-suggested changes to working tree.
    #[arg(long)]
    pub apply: bool,

    /// GitHub token for API access (for private repos).
    #[arg(long)]
    pub token: Option<String>,
}

impl PrCli {
    /// Run the PR command.
    pub async fn run(self) -> Result<()> {
        run_pr_checkout(self).await
    }
}

/// Checkout a pull request branch.
async fn run_pr_checkout(args: PrCli) -> Result<()> {
    use cortex_engine::github::GitHubClient;

    if args.apply {
        bail!("Automatic PR suggestion application is not supported. No suggestions were applied.");
    }
    let repo_path = args
        .path
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .context("Could not resolve repository path")?;
    let pr_number = args.number;

    // Validate PR number is positive
    if pr_number == 0 {
        bail!("Error: PR number must be a positive integer");
    }

    // Check if we're in a git repo
    if !repo_path.join(".git").exists() {
        bail!("Not a git repository. Run this command from a git repository root.");
    }

    // Get the remote URL to determine owner/repo
    let remote_url = get_git_remote_url(&repo_path)?;
    let (owner, repo) = parse_github_url(&remote_url)?;

    let repository = format!("{}/{}", owner, repo);

    println!("[PR] Pull Request #{}", pr_number);
    println!("{}", "=".repeat(40));
    println!("Repository: {}", repository);
    println!();

    // Fetch PR metadata from GitHub API
    let client = if let Some(ref token) = args.token {
        GitHubClient::new(token, &repository)?
    } else {
        GitHubClient::anonymous(&repository)?
    };

    let pr_info = client.get_pull_request(pr_number).await?;

    // Issue #2325: Display PR info with timestamp to indicate when metadata was fetched
    // If the PR status changes during long operations (like diff analysis), the user
    // knows when the displayed status was retrieved.
    let metadata_timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    println!("Title: {}", pr_info.title);
    println!("Author: @{}", pr_info.author);
    // Display state with draft indicator if applicable
    let state_display = if pr_info.draft {
        format!("{} (draft)", pr_info.state)
    } else {
        pr_info.state.clone()
    };
    println!("State: {} (as of {})", state_display, metadata_timestamp);
    println!(
        "Base: {} ← Head: {}",
        pr_info.base_branch, pr_info.head_branch
    );
    if let Some(ref body) = pr_info.body
        && !body.is_empty()
    {
        println!();
        println!("Description:");
        // Truncate long descriptions
        let preview: String = body.chars().take(500).collect();
        if body.len() > 500 {
            println!("  {}...", preview);
        } else {
            println!("  {}", preview);
        }
    }
    println!();

    // If --info flag, just show info and exit
    if args.info {
        println!("URL: https://github.com/{}/pull/{}", repository, pr_number);
        return Ok(());
    }

    // If --diff flag, show diff without checkout
    if args.diff {
        println!("Fetching PR diff...");
        println!();

        // Fetch the PR branch to show diff
        let branch_name = format!("pr-{}", pr_number);
        let refspec = format!("pull/{}/head:{}", pr_number, branch_name);

        let fetch_output = Command::new("git")
            .current_dir(&repo_path)
            .args(["fetch", "origin", &refspec])
            .output()
            .context("Failed to fetch PR")?;

        if !fetch_output.status.success() {
            let stderr = String::from_utf8_lossy(&fetch_output.stderr);
            bail!("Failed to fetch PR: {}", stderr);
        }

        let diff_output = Command::new("git")
            .current_dir(&repo_path)
            .args([
                "diff",
                &format!("{}...{}", pr_info.base_branch, branch_name),
            ])
            .output()
            .context("Failed to run git diff")?;

        if diff_output.status.success() {
            let diff_content = String::from_utf8_lossy(&diff_output.stdout);
            if diff_content.is_empty() {
                println!("No changes in this PR.");
            } else {
                println!("{}", diff_content);
            }
        } else {
            bail!(
                "Failed to get diff: {}",
                String::from_utf8_lossy(&diff_output.stderr)
            );
        }
        return Ok(());
    }

    // If --comments flag, show PR comments
    if args.comments {
        println!("Fetching PR comments...");
        println!();

        // Use gh CLI to get comments if available, otherwise show message
        let gh_output = Command::new("gh")
            .current_dir(&repo_path)
            .args([
                "pr",
                "view",
                &pr_number.to_string(),
                "--repo",
                &repository,
                "--comments",
            ])
            .output();

        match gh_output {
            Ok(output) if output.status.success() => {
                let comments = String::from_utf8_lossy(&output.stdout);
                if comments.trim().is_empty() {
                    println!("No comments on this PR.");
                } else {
                    println!("{}", comments);
                }
            }
            _ => {
                println!("Note: Install GitHub CLI (gh) for full comment viewing.");
                println!();
                println!("Alternative: View comments at:");
                println!("  https://github.com/{}/pull/{}", repository, pr_number);
            }
        }
        return Ok(());
    }

    // Check for uncommitted changes
    if !args.force {
        let status_output = Command::new("git")
            .current_dir(&repo_path)
            .args(["status", "--porcelain"])
            .output()
            .context("Failed to run git status")?;

        if !status_output.status.success() {
            bail!("Could not determine repository worktree status");
        }
        if !status_output.stdout.is_empty() {
            bail!(
                "Uncommitted changes detected. Commit or stash changes first, or use --force to override."
            );
        }
    }

    // Fetch the PR
    // Use custom branch name if provided, otherwise default to "pr-{number}"
    let branch_name = args
        .branch
        .clone()
        .unwrap_or_else(|| format!("pr-{}", pr_number));

    // SECURITY: Validate the branch name to prevent injection
    validate_branch_name(&branch_name)?;

    let refspec = format!("pull/{}/head:{}", pr_number, branch_name);

    // Validate refspec to ensure no injection is possible
    validate_refspec(&refspec)?;

    println!("Fetching PR #{}...", pr_number);
    print!("  [WAIT] Downloading PR data from origin...");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    // SECURITY: Arguments are passed as separate array elements, not interpolated into a string
    let fetch_output = Command::new("git")
        .current_dir(&repo_path)
        .args(["fetch", "origin", &refspec])
        .output()
        .context("Failed to fetch PR")?;

    println!(" done");

    if !fetch_output.status.success() {
        let stderr = String::from_utf8_lossy(&fetch_output.stderr);
        bail!("Failed to fetch PR: {}", stderr);
    }

    // Checkout the branch
    print!("  [WAIT] Checking out branch '{}'...", branch_name);
    std::io::Write::flush(&mut std::io::stdout()).ok();

    let checkout_args = if args.force {
        vec!["checkout", "-f", &branch_name]
    } else {
        vec!["checkout", &branch_name]
    };

    let checkout_output = Command::new("git")
        .current_dir(&repo_path)
        .args(&checkout_args)
        .output()
        .context("Failed to checkout PR branch")?;

    if checkout_output.status.success() {
        println!(" done");
    }

    if !checkout_output.status.success() {
        let stderr = String::from_utf8_lossy(&checkout_output.stderr);

        bail!("Could not checkout PR branch: {}", stderr);
    }

    println!();
    println!(
        "Checked out PR #{} to branch '{}' from '{}:{}'",
        pr_number, branch_name, pr_info.author, pr_info.head_branch
    );
    println!();
    println!("Commands:");
    // SECURITY: Validate base_branch before displaying in commands
    // This prevents any potential display-based injection
    if validate_branch_name(&pr_info.base_branch).is_ok() {
        println!(
            "  • View diff:     git diff {}...{}",
            pr_info.base_branch, branch_name
        );
        println!("  • Return:        git checkout {}", pr_info.base_branch);
    } else {
        println!("  • View diff:     git diff <base>...{}", branch_name);
        println!("  • Return:        git checkout <base>");
    }
    println!(
        "  • View on web:   https://github.com/{}/pull/{}",
        repository, pr_number
    );

    Ok(())
}

/// Get the git remote URL for 'origin'.
fn get_git_remote_url(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .context("Failed to get git remote URL")?;

    if !output.status.success() {
        // Provide clearer error message (Issue #1970)
        bail!(
            "No 'origin' remote found.\n\n\
             This is a git repository, but it doesn't have an 'origin' remote configured.\n\
             Add one with:\n  \
             git remote add origin <repository-url>\n\n\
             Example:\n  \
             git remote add origin https://github.com/username/repo.git"
        );
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(url)
}

/// Parse a GitHub URL to extract owner and repo.
fn parse_github_url(url: &str) -> Result<(String, String)> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .context("The origin remote must use a supported github.com SSH or HTTPS URL")?;
    cortex_engine::github::client::parse_repository(path.strip_suffix(".git").unwrap_or(path))
}

/// Pull request information.
#[derive(Debug)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub body: Option<String>,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    pub mergeable: Option<bool>,
    pub draft: bool,
    pub labels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url_ssh() {
        let (owner, repo) = parse_github_url("git@github.com:cortex-ai/cortex.git").unwrap();
        assert_eq!(owner, "cortex-ai");
        assert_eq!(repo, "cortex");
    }

    #[test]
    fn test_parse_github_url_https() {
        let (owner, repo) = parse_github_url("https://github.com/cortex-ai/cortex.git").unwrap();
        assert_eq!(owner, "cortex-ai");
        assert_eq!(repo, "cortex");
    }

    #[test]
    fn test_parse_github_url_https_no_git() {
        let (owner, repo) = parse_github_url("https://github.com/cortex-ai/cortex").unwrap();
        assert_eq!(owner, "cortex-ai");
        assert_eq!(repo, "cortex");
    }
}

#[cfg(test)]
mod integration_contract_tests {
    use super::*;
    #[test]
    fn remote_urls_require_exact_host_and_repository_components() {
        for url in [
            "https://github.com.evil.test/owner/repo",
            "https://evil.test/github.com/owner/repo",
            "https://github.com/owner/repo/more",
            "https://github.com//repo",
            "git@github.com:../repo",
        ] {
            assert!(parse_github_url(url).is_err());
        }
    }
    /// Local repository with a controlled `origin` and no reachable network.
    fn fixture_repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap();
            assert!(status.status.success(), "git {args:?}");
        };
        git(&["init", "--quiet"]);
        git(&[
            "remote",
            "add",
            "origin",
            "https://github.com/fixture/project.git",
        ]);
        dir
    }

    #[tokio::test]
    async fn checkout_stops_before_the_network_on_unusable_repositories() {
        // A path that cannot be resolved is refused before any git invocation.
        let missing = PrCli::try_parse_from(["pr", "1", "--path", "/nonexistent/fixture"]).unwrap();
        assert!(
            missing
                .run()
                .await
                .unwrap_err()
                .to_string()
                .contains("Could not resolve")
        );

        // An existing directory that is not a repository is refused next.
        let plain = tempfile::tempdir().unwrap();
        let cli =
            PrCli::try_parse_from(["pr", "1", "--path", plain.path().to_str().unwrap()]).unwrap();
        assert!(
            cli.run()
                .await
                .unwrap_err()
                .to_string()
                .contains("Not a git repository")
        );

        // PR 0 does not identify a pull request.
        let repository = fixture_repository();
        let zero =
            PrCli::try_parse_from(["pr", "0", "--path", repository.path().to_str().unwrap()])
                .unwrap();
        assert!(
            zero.run()
                .await
                .unwrap_err()
                .to_string()
                .contains("positive integer")
        );
    }

    #[tokio::test]
    async fn checkout_requires_a_supported_origin_remote() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .current_dir(dir.path())
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap()
        };
        assert!(git(&["init", "--quiet"]).status.success());
        let path = dir.path().to_str().unwrap().to_string();

        // No origin at all produces the actionable setup message.
        let error = PrCli::try_parse_from(["pr", "1", "--path", &path])
            .unwrap()
            .run()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("No 'origin' remote found"), "{error}");
        assert!(get_git_remote_url(dir.path()).is_err());

        // A non-GitHub origin is rejected rather than guessed at.
        assert!(
            git(&["remote", "add", "origin", "https://example.test/owner/repo"])
                .status
                .success()
        );
        let error = PrCli::try_parse_from(["pr", "1", "--path", &path])
            .unwrap()
            .run()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("github.com"), "{error}");
        assert_eq!(
            get_git_remote_url(dir.path()).unwrap(),
            "https://example.test/owner/repo"
        );
    }

    #[test]
    fn branch_names_and_refspecs_reject_injection_and_git_reserved_forms() {
        for name in ["pr-1", "feature/nested-name", "release.2024", "PR_1"] {
            validate_branch_name(name).unwrap();
        }
        for name in [
            "",
            "-flag",
            ".hidden",
            "a..b",
            "trailing/",
            "branch.lock",
            "with space",
            "semi;colon",
            "back`tick`",
            "dollar$sign",
            "pipe|name",
        ] {
            assert!(validate_branch_name(name).is_err(), "{name}");
        }
        validate_refspec("pull/12/head:pr-12").unwrap();
        for refspec in ["", "pull/1/head:pr 1", "pull/1/head:pr;1", "a|b"] {
            assert!(validate_refspec(refspec).is_err(), "{refspec}");
        }
    }

    #[tokio::test]
    async fn apply_is_rejected_before_repository_or_account_access() {
        let cli = PrCli::try_parse_from(["pr", "1", "--apply", "--path", "/nonexistent/fixture"])
            .unwrap();
        let error = cli.run().await.unwrap_err().to_string();
        assert!(error.contains("not supported"));
        assert!(error.contains("No suggestions were applied"));
    }
}
