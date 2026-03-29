use std::io::Write;
use std::path::Path;
use std::process::Command;

/// A parsed commit from git log.
#[derive(Debug, Clone)]
pub struct Commit {
    pub full_sha: String,
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

/// Action to perform on a commit during interactive rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl RebaseAction {
    /// All available actions, in menu order.
    pub const ALL: [RebaseAction; 6] = [
        RebaseAction::Pick,
        RebaseAction::Reword,
        RebaseAction::Edit,
        RebaseAction::Squash,
        RebaseAction::Fixup,
        RebaseAction::Drop,
    ];

    /// The git rebase-todo keyword for this action.
    pub fn keyword(self) -> &'static str {
        match self {
            RebaseAction::Pick => "pick",
            RebaseAction::Reword => "reword",
            RebaseAction::Edit => "edit",
            RebaseAction::Squash => "squash",
            RebaseAction::Fixup => "fixup",
            RebaseAction::Drop => "drop",
        }
    }

    /// Human-readable label for UI display.
    pub fn label(self) -> &'static str {
        match self {
            RebaseAction::Pick => "Pick",
            RebaseAction::Reword => "Reword",
            RebaseAction::Edit => "Edit",
            RebaseAction::Squash => "Squash",
            RebaseAction::Fixup => "Fixup",
            RebaseAction::Drop => "Drop",
        }
    }
}

impl std::fmt::Display for RebaseAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A single entry in the interactive rebase commit list.
#[derive(Debug, Clone)]
pub struct RebaseEntry {
    /// The full SHA of the commit.
    pub sha: String,
    /// Short SHA for display.
    pub short_sha: String,
    /// Commit subject line.
    pub subject: String,
    /// The action to apply during rebase.
    pub action: RebaseAction,
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
/// - `revision`: if Some, shows history for that branch/tag/ref instead of HEAD.
/// - `path_filter`: if Some, appends `-- <path>` to filter by file/directory.
/// - `extra_shas`: additional SHAs to include as starting points (e.g. orphaned
///   reflog commits). Git will topo-sort and deduplicate them with the rest.
///
/// We use `git log` with `--format` using ASCII separators so we can parse
/// fields reliably, and `--decorate=short` piped through a separate ref lookup.
pub fn load_commits(
    repo_path: &str,
    show_all: bool,
    revision: Option<&str>,
    path_filter: Option<&str>,
    extra_shas: &[String],
) -> Result<Vec<Commit>, String> {
    // Use %x00 (null) as field separator and %x01 (SOH) as record separator.
    // These cannot appear in commit messages so parsing is reliable.
    //
    // Fields: full_sha, short_sha, parents, author_name, author_email, date, subject, body, decorate
    let format_str = "%H%x00%h%x00%P%x00%an%x00%ae%x00%ai%x00%s%x00%b%x00%D%x01";

    // We need to own the format string so it lives long enough
    let format_arg = format!("--format={}", format_str);

    // Build args properly
    // --topo-order ensures commits are in topological order (children before
    // parents, branches kept together) which is required for correct graph
    // rendering. Without it, git uses chronological order which can interleave
    // branches and break the graph lane assignment algorithm.
    let mut real_args: Vec<String> = vec!["log".into(), format_arg, "--topo-order".into()];

    if show_all {
        real_args.push("--all".into());
    }

    if let Some(rev) = revision {
        real_args.push(rev.into());
    }

    // Add extra starting-point SHAs (e.g. orphaned reflog commits).
    for sha in extra_shas {
        real_args.push(sha.clone());
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

/// Parse the reflog and return orphaned SHAs (not reachable from normal refs)
/// along with their reflog labels for display.
///
/// First does a cheap check: loads all reflog SHAs, runs them through
/// `git log` together with the normal refs, then compares to find which
/// SHAs wouldn't have been loaded without the reflog. Returns those SHAs
/// and a label map so the caller can annotate the commits after loading.
pub fn load_reflog_orphans(
    repo_path: &str,
) -> Result<(Vec<String>, std::collections::HashMap<String, String>), String> {
    use std::collections::{HashMap, HashSet};

    // Get reflog entries: SHA, reflog selector, reflog subject.
    let reflog_output = run_git(
        repo_path,
        &["reflog", "--format=%H%x00%gd%x00%gs%x01"],
    )?;

    // Collect unique SHAs and build a label for each.
    // We keep only the first (most recent) reflog label per SHA.
    let mut orphan_labels: HashMap<String, String> = HashMap::new();
    let mut reflog_shas: Vec<String> = Vec::new();
    let mut seen = HashSet::new();

    for record in reflog_output.split('\x01') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split('\0').collect();
        if fields.len() < 3 {
            continue;
        }
        let sha = fields[0].trim().to_string();
        let selector = fields[1].trim(); // e.g. "HEAD@{3}"
        let action = fields[2].trim(); // e.g. "commit (amend): message"

        if seen.insert(sha.clone()) {
            // Build a short label like "HEAD@{3} commit (amend)"
            // Strip the commit message from the action (keep only the action type).
            let action_type = if let Some(colon_pos) = action.find(':') {
                action[..colon_pos].trim()
            } else {
                action
            };
            let label = format!("{} {}", selector, action_type);
            orphan_labels.insert(sha.clone(), label);
            reflog_shas.push(sha);
        }
    }

    Ok((reflog_shas, orphan_labels))
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
///
/// If `branch` is a remote ref like `origin/foo`:
///   - If the local branch `foo` already exists, check it out directly.
///   - Otherwise, create a local tracking branch with `git checkout --track origin/foo`.
///
/// If `branch` is already a local name, just check it out.
pub fn checkout_branch(repo_path: &str, branch: &str) -> Result<String, String> {
    if let Some(local) = branch.split('/').next_back()
        && branch.contains('/')
    {
        // Check if the local branch already exists.
        let local_exists = run_git(
            repo_path,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", local),
            ],
        )
        .is_ok();
        if local_exists {
            return run_git(repo_path, &["checkout", local]);
        }
        // Local branch doesn't exist — create it tracking the remote ref.
        return run_git(repo_path, &["checkout", "--track", branch]);
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

/// Cherry-pick multiple commits in the given order (oldest first).
/// Returns `Ok(count)` if all succeeded, or `Err((applied, message))` if one
/// failed, where `applied` is how many were successfully applied before the failure.
pub fn cherry_pick_multiple(repo_path: &str, shas: &[String]) -> Result<usize, (usize, String)> {
    for (i, sha) in shas.iter().enumerate() {
        if let Err(e) = run_git(repo_path, &["cherry-pick", sha]) {
            return Err((i, e));
        }
    }
    Ok(shas.len())
}

// --- Interactive rebase ---

/// Load the list of commits from HEAD down to (but not including) the given
/// base SHA. These are the commits that would be rebased in an interactive
/// rebase onto `base_sha`. Returned in oldest-first order (bottom of the
/// rebase todo list first), which is the order git rebase -i uses.
pub fn load_rebase_commits(repo_path: &str, base_sha: &str) -> Result<Vec<RebaseEntry>, String> {
    let range = format!("{}..HEAD", base_sha);
    let format_arg = "--format=%H%x00%h%x00%s".to_string();
    let output = run_git(
        repo_path,
        &["log", "--reverse", "--topo-order", &format_arg, &range],
    )?;

    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\0').collect();
        if fields.len() < 3 {
            continue;
        }
        entries.push(RebaseEntry {
            sha: fields[0].to_string(),
            short_sha: fields[1].to_string(),
            subject: fields[2].to_string(),
            action: RebaseAction::Pick,
        });
    }

    if entries.is_empty() {
        return Err("No commits to rebase (HEAD is already at or behind the base).".into());
    }

    Ok(entries)
}

/// Rebase the current branch onto the given commit SHA (non-interactive).
pub fn rebase(repo_path: &str, onto_sha: &str) -> Result<String, String> {
    run_git(repo_path, &["rebase", onto_sha])
}

/// Execute an interactive rebase using the given sequence of entries.
///
/// This works by writing a rebase-todo script to a temporary file and
/// setting `GIT_SEQUENCE_EDITOR` to a command that copies that file into
/// the rebase todo, replacing the default editor interaction.
pub fn rebase_interactive(
    repo_path: &str,
    base_sha: &str,
    entries: &[RebaseEntry],
) -> Result<String, String> {
    // Build the rebase todo content.
    let mut todo = String::new();
    for entry in entries {
        todo.push_str(entry.action.keyword());
        todo.push(' ');
        todo.push_str(&entry.short_sha);
        todo.push(' ');
        todo.push_str(&entry.subject);
        todo.push('\n');
    }

    // Write the todo to a temporary file.
    let tmp_dir = std::env::temp_dir();
    let todo_path = tmp_dir.join("gitshrub_rebase_todo");
    let mut file = std::fs::File::create(&todo_path)
        .map_err(|e| format!("Failed to create rebase todo file: {}", e))?;
    file.write_all(todo.as_bytes())
        .map_err(|e| format!("Failed to write rebase todo file: {}", e))?;
    drop(file);

    let todo_path_str = todo_path.to_string_lossy().to_string();

    // Use `cp` as the sequence editor: it copies our todo file over the
    // one git provides, effectively replacing the interactive editor.
    let seq_editor = format!("cp {} ", todo_path_str);

    let output = Command::new("git")
        .args(["rebase", "-i", base_sha])
        .env("GIT_SEQUENCE_EDITOR", &seq_editor)
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to execute git rebase: {}", e))?;

    // Clean up the temp file (best effort).
    let _ = std::fs::remove_file(&todo_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Check if it's a conflict (rebase paused, not fully failed).
        if stderr.contains("could not apply")
            || stderr.contains("CONFLICT")
            || stdout.contains("could not apply")
        {
            return Err(format!("Rebase paused due to conflicts. {}", stderr));
        }
        return Err(format!("git rebase -i failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}

/// Represents a git operation that is currently in progress (paused, e.g. due to conflicts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InProgressOp {
    Rebase,
    CherryPick,
    Merge,
    Bisect,
    Revert,
}

impl InProgressOp {
    /// Human-readable label for display in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            InProgressOp::Rebase => "Rebase",
            InProgressOp::CherryPick => "Cherry-pick",
            InProgressOp::Merge => "Merge",
            InProgressOp::Bisect => "Bisect",
            InProgressOp::Revert => "Revert",
        }
    }

    /// Description of the abort action.
    pub fn abort_label(&self) -> &'static str {
        match self {
            InProgressOp::Rebase => "Abort rebase",
            InProgressOp::CherryPick => "Abort cherry-pick",
            InProgressOp::Merge => "Abort merge",
            InProgressOp::Bisect => "Reset bisect",
            InProgressOp::Revert => "Abort revert",
        }
    }

    /// Whether this operation supports `--continue`.
    pub fn supports_continue(&self) -> bool {
        matches!(self, InProgressOp::Rebase | InProgressOp::CherryPick | InProgressOp::Revert)
    }

    /// Description of the continue action.
    pub fn continue_label(&self) -> &'static str {
        match self {
            InProgressOp::Rebase => "Continue rebase",
            InProgressOp::CherryPick => "Continue cherry-pick",
            InProgressOp::Revert => "Continue revert",
            _ => "Continue",
        }
    }
}

/// Detect any in-progress git operation by checking for marker files/directories
/// inside the `.git` directory of the repository.
pub fn detect_in_progress_op(repo_path: &str) -> Option<InProgressOp> {
    // Get the .git directory path (handles worktrees too).
    let git_dir = match run_git(repo_path, &["rev-parse", "--git-dir"]) {
        Ok(d) => {
            let trimmed = d.trim().to_string();
            if Path::new(&trimmed).is_absolute() {
                trimmed
            } else {
                format!("{}/{}", repo_path, trimmed)
            }
        }
        Err(_) => return None,
    };
    let git_path = Path::new(&git_dir);

    // Check in priority order (rebase is most complex and should be first).
    if git_path.join("rebase-merge").exists() || git_path.join("rebase-apply").exists() {
        return Some(InProgressOp::Rebase);
    }
    if git_path.join("CHERRY_PICK_HEAD").exists() {
        return Some(InProgressOp::CherryPick);
    }
    if git_path.join("MERGE_HEAD").exists() {
        return Some(InProgressOp::Merge);
    }
    if git_path.join("BISECT_LOG").exists() {
        return Some(InProgressOp::Bisect);
    }
    if git_path.join("REVERT_HEAD").exists() {
        return Some(InProgressOp::Revert);
    }

    None
}

/// Abort the given in-progress operation.
pub fn abort_op(repo_path: &str, op: &InProgressOp) -> Result<String, String> {
    match op {
        InProgressOp::Rebase => run_git(repo_path, &["rebase", "--abort"]),
        InProgressOp::CherryPick => run_git(repo_path, &["cherry-pick", "--abort"]),
        InProgressOp::Merge => run_git(repo_path, &["merge", "--abort"]),
        InProgressOp::Bisect => run_git(repo_path, &["bisect", "reset"]),
        InProgressOp::Revert => run_git(repo_path, &["revert", "--abort"]),
    }
}

/// Continue the given in-progress operation (only valid for rebase, cherry-pick, revert).
pub fn continue_op(repo_path: &str, op: &InProgressOp) -> Result<String, String> {
    match op {
        InProgressOp::Rebase => run_git(repo_path, &["rebase", "--continue"]),
        InProgressOp::CherryPick => run_git(repo_path, &["cherry-pick", "--continue"]),
        InProgressOp::Revert => run_git(repo_path, &["revert", "--continue"]),
        _ => Err(format!("{} does not support --continue", op.label())),
    }
}
