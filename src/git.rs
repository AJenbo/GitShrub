use std::path::Path;
use std::process::Command;

/// A parsed commit from git log.
#[derive(Debug, Clone)]
pub struct Commit {
    pub full_sha: String,
    #[expect(dead_code, reason = "reserved for future use in commit list display")]
    pub short_sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub date: String,
    pub subject: String,
    pub body: String,
    /// Ref names like "HEAD -> master", "origin/master", "tag: v1.0"
    pub refs: Vec<String>,
}

/// The parsed diff output for a single commit.
#[derive(Debug, Clone)]
pub struct DiffOutput {
    /// Pre-split diff lines for efficient row-based rendering.
    pub lines: Vec<String>,
    /// Index of `diff --git` header lines: maps file path → line index.
    /// Used by the diff view to jump to a file's section without scanning.
    pub file_header_lines: Vec<(String, usize)>,
    /// List of files affected in this commit.
    pub files: Vec<String>,
}

/// Run a git command and return stdout as a String.
/// Returns Err with stderr contents if git exits non-zero.
fn run_git(repo_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {}", args.join(" "), stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Get the current branch name. Returns Err if detached HEAD or not a repo.
pub fn current_branch(repo_path: &str) -> Result<String, String> {
    let output = run_git(repo_path, &["symbolic-ref", "--short", "HEAD"])?;
    let branch = output.trim().to_string();
    if branch.is_empty() {
        Err("Detached HEAD".into())
    } else {
        Ok(branch)
    }
}

/// Load commits from git log.
///
/// - `show_all`: if true, passes `--all` to show all branches.
/// - `path_filter`: if Some, appends `-- <path>` to filter by file/directory.
///
/// We use `git log` with `--format` using ASCII separators so we can parse
/// fields reliably, and `--decorate=short` piped through a separate ref lookup.
pub fn load_commits(
    repo_path: &str,
    show_all: bool,
    path_filter: Option<&str>,
) -> Result<Vec<Commit>, String> {
    // Use %x00 (null) as field separator and %x01 (SOH) as record separator.
    // These cannot appear in commit messages so parsing is reliable.
    //
    // Fields: full_sha, short_sha, parents, author_name, author_email, date, subject, body, decorate
    let format_str = "%H%x00%h%x00%P%x00%an%x00%ae%x00%ai%x00%s%x00%b%x00%D%x01";

    // We need to own the format string so it lives long enough
    let format_arg = format!("--format={}", format_str);

    // Build args properly
    let mut real_args: Vec<String> = vec!["log".into(), format_arg];

    if show_all {
        real_args.push("--all".into());
    }

    if let Some(path) = path_filter {
        real_args.push("--".into());
        real_args.push(path.into());
    }

    let args_refs: Vec<&str> = real_args.iter().map(|s| s.as_str()).collect();
    let output = match run_git(repo_path, &args_refs) {
        Ok(o) => o,
        Err(e) => {
            // Empty repos have no commits yet; treat as an empty list, not an error.
            if e.contains("does not have any commits yet") || e.contains("bad default revision") {
                return Ok(Vec::new());
            }
            return Err(e);
        }
    };

    let mut commits = Vec::new();

    for record in output.split('\x01') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }

        let fields: Vec<&str> = record.split('\0').collect();
        if fields.len() < 8 {
            continue;
        }

        let full_sha = fields[0].trim().to_string();
        let short_sha = fields[1].trim().to_string();
        let parents: Vec<String> = fields[2]
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let author_name = fields[3].trim().to_string();
        let author_email = fields[4].trim().to_string();
        let date = format_date(fields[5].trim());
        let subject = fields[6].trim().to_string();
        let body = fields[7].trim().to_string();

        // Parse decoration (refs)
        let refs = if fields.len() > 8 && !fields[8].trim().is_empty() {
            parse_refs(fields[8].trim())
        } else {
            Vec::new()
        };

        commits.push(Commit {
            full_sha,
            short_sha,
            parents,
            author_name,
            author_email,
            date,
            subject,
            body,
            refs,
        });
    }

    Ok(commits)
}

/// Parse the %D decoration string into a list of ref names.
/// Input looks like: "HEAD -> master, origin/master, tag: v1.0"
fn parse_refs(decoration: &str) -> Vec<String> {
    decoration
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        // Strip "HEAD -> " prefix, keep the branch name
        .map(|s| {
            if let Some(rest) = s.strip_prefix("HEAD -> ") {
                rest.to_string()
            } else {
                s
            }
        })
        // Filter out remote HEAD pointers like "origin/HEAD" (not useful).
        .filter(|s| !s.ends_with("/HEAD"))
        .collect()
}

/// Format a git date string (e.g. "2024-01-15 12:34:56 +0100") into
/// a shorter form (e.g. "2024-01-15 12:34:56").
fn format_date(date: &str) -> String {
    // The %ai format gives "2024-01-15 12:34:56 +0100"
    // We strip the timezone for a cleaner display
    if let Some(pos) = date.rfind(' ') {
        date[..pos].to_string()
    } else {
        date.to_string()
    }
}

/// Load the diff for a given commit SHA.
/// Returns the full diff text and a list of affected file paths.
pub fn load_diff(repo_path: &str, sha: &str) -> Result<DiffOutput, String> {
    let root = is_root_commit(repo_path, sha);

    // Get the list of changed files (root commits need --root)
    let files_output = if root {
        run_git(
            repo_path,
            &[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "-r",
                "--name-only",
                sha,
            ],
        )?
    } else {
        run_git(
            repo_path,
            &["diff-tree", "--no-commit-id", "-r", "--name-only", sha],
        )?
    };

    let files: Vec<String> = files_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    // Get the full diff (root commits need --root)
    let raw_output = if root {
        run_git(repo_path, &["diff-tree", "--root", "-p", "--stat", sha])?
    } else {
        run_git(repo_path, &["diff-tree", "-p", "--stat", sha])?
    };

    // diff-tree prints the commit SHA as the first line. Strip it.
    let raw = if let Some(stripped) = raw_output.strip_prefix(sha) {
        stripped.trim_start_matches('\n').to_string()
    } else {
        // Also handle short SHA prefix on the first line
        let first_newline = raw_output.find('\n').unwrap_or(0);
        let first_line = &raw_output[..first_newline];
        if first_line.chars().all(|c| c.is_ascii_hexdigit()) {
            raw_output[first_newline..]
                .trim_start_matches('\n')
                .to_string()
        } else {
            raw_output
        }
    };

    // Pre-split diff into lines and build file header index.
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let mut file_header_lines = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("diff --git") {
            // Extract the b/ path from "diff --git a/foo b/foo"
            if let Some(b_path) = line.rsplit(" b/").next() {
                file_header_lines.push((b_path.to_string(), i));
            }
        }
    }

    Ok(DiffOutput {
        lines,
        file_header_lines,
        files,
    })
}

/// Check if a commit is the root commit (has no parents).
fn is_root_commit(repo_path: &str, sha: &str) -> bool {
    let format_arg = "--format=%P".to_string();
    match run_git(repo_path, &["log", "-1", &format_arg, sha]) {
        Ok(output) => output.trim().is_empty(),
        Err(_) => false,
    }
}

/// Verify that the given path is inside a git repository.
/// Returns the repo root path if valid.
pub fn verify_repo(path: &str) -> Result<String, String> {
    let check_path = if Path::new(path).exists() {
        path.to_string()
    } else {
        return Err(format!("Path does not exist: {}", path));
    };

    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(&check_path)
        .output()
        .map_err(|e| format!("Failed to execute git: {}", e))?;

    if !output.status.success() {
        return Err("Not a git repository (or any parent up to mount point /)".to_string());
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        Err("Could not determine repository root".into())
    } else {
        Ok(root)
    }
}

// --- Branch operations ---

/// Checkout a branch by name.
/// If the branch is a remote like `origin/foo`, checks out the local name `foo`.
/// Git will auto-create a tracking branch if it doesn't exist yet (DWIM mode).
pub fn checkout_branch(repo_path: &str, branch: &str) -> Result<String, String> {
    if let Some(local) = branch.split('/').next_back()
        && branch.contains('/')
    {
        return run_git(repo_path, &["checkout", local]);
    }
    run_git(repo_path, &["checkout", branch])
}

/// Delete a local branch. Uses `-D` (force delete).
pub fn delete_branch(repo_path: &str, branch: &str) -> Result<String, String> {
    run_git(repo_path, &["branch", "-D", branch])
}

/// Create a new branch at the given commit SHA.
pub fn create_branch(repo_path: &str, name: &str, sha: &str) -> Result<String, String> {
    run_git(repo_path, &["branch", name, sha])
}

// --- Commit operations ---

/// Reset the current branch to the given SHA with `--mixed` (keeps working tree).
pub fn reset_mixed(repo_path: &str, sha: &str) -> Result<String, String> {
    run_git(repo_path, &["reset", "--mixed", sha])
}

/// Reset the current branch to the given SHA with `--hard` (discards everything).
pub fn reset_hard(repo_path: &str, sha: &str) -> Result<String, String> {
    run_git(repo_path, &["reset", "--hard", sha])
}

/// Revert the given commit (creates a new commit that undoes it).
pub fn revert_commit(repo_path: &str, sha: &str) -> Result<String, String> {
    run_git(repo_path, &["revert", "--no-edit", sha])
}

/// Cherry-pick the given commit onto the current branch.
pub fn cherry_pick(repo_path: &str, sha: &str) -> Result<String, String> {
    run_git(repo_path, &["cherry-pick", sha])
}
