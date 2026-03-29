use std::collections::BTreeSet;

use egui::{Pos2, Stroke, Vec2};

use crate::git::{self, Commit, DiffOutput, InProgressOp, RebaseAction, RebaseEntry};
use crate::graph::{self, GraphRow};
use crate::ui;

/// Which mode the commit-list dialog is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogMode {
    /// Interactive rebase: all actions available, base SHA + branch stored.
    Rebase {
        base_sha: String,
        branch: String,
    },
    /// Cherry-pick: only "Pick" action, entries are commits to apply.
    CherryPick,
}

/// Main application state.
pub struct App {
    /// Path to the git repository root.
    pub repo_path: String,

    /// Whether to show all branches (--all flag).
    pub show_all: bool,

    /// Optional revision (branch/tag) to show history for.
    pub revision: Option<String>,

    /// Optional path filter for file/directory history.
    pub path_filter: Option<String>,

    /// All commits loaded from git log.
    pub commits: Vec<Commit>,

    /// Index of the currently selected commit in `commits`.
    pub selected_index: Option<usize>,

    /// Diff output for the selected commit (loaded on demand).
    pub diff: Option<DiffOutput>,

    /// Which file in the affected files list is selected (index into diff.files).
    pub selected_file_index: Option<usize>,

    /// Line index to scroll to in the diff view (set when user clicks a file in the file list).
    pub scroll_to_diff_line: Option<usize>,

    /// Computed graph layout rows, one per commit (same order as `commits`).
    pub graph_rows: Vec<GraphRow>,

    /// Current branch name.
    pub current_branch: String,

    /// Repository name (directory name).
    pub repo_name: String,

    /// Status/error message to display temporarily.
    pub status_message: Option<String>,

    /// Longest author name length in characters (for column width calculation).
    pub max_author_chars: usize,

    /// Width of the file list panel (user-adjustable by dragging the divider).
    pub file_list_width: f32,

    /// If set, the app shows only this error message (non-repo or fatal startup error).
    pub startup_error: Option<String>,

    /// If set, the commit list should scroll to this index on the next frame.
    pub scroll_to_commit_idx: Option<usize>,

    /// The visible row range from the last frame (start, end) for scroll checks.
    pub visible_commit_range: Option<(usize, usize)>,

    /// SHA for a pending "Create branch" action (needs name input).
    pub create_branch_sha: Option<String>,

    /// Text field for the new branch name in the CreateBranch dialog.
    pub new_branch_name: String,

    /// Indices of multi-selected commits (for batch operations like cherry-pick).
    /// Kept sorted via BTreeSet so iteration is always in list order.
    pub multi_selection: BTreeSet<usize>,

    /// The index of the "anchor" for shift+click range selection.
    /// Set to the last plain-clicked or ctrl+clicked index.
    pub selection_anchor: Option<usize>,

    /// When Some, the commit-list dialog is open (rebase or cherry-pick).
    pub dialog_mode: Option<DialogMode>,

    /// The list of commits in the dialog, editable by the user.
    /// Ordered oldest-first (same order as git rebase-todo).
    pub dialog_entries: Vec<RebaseEntry>,

    /// If set, a git operation is currently in progress (e.g. paused due to conflicts).
    pub in_progress_op: Option<InProgressOp>,
}

impl App {
    /// Create a new App from CLI options. Loads initial commit data.
    pub fn new(
        repo_path: String,
        show_all: bool,
        revision: Option<String>,
        path_filter: Option<String>,
    ) -> Self {
        let current_branch = git::current_branch(&repo_path).unwrap_or_else(|_| "detached".into());

        let repo_name = std::path::Path::new(&repo_path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());

        let mut app = App {
            repo_path,
            show_all,
            revision,
            path_filter,
            commits: Vec::new(),
            selected_index: None,
            diff: None,
            selected_file_index: None,
            scroll_to_diff_line: None,
            graph_rows: Vec::new(),
            current_branch,
            repo_name,
            status_message: None,
            max_author_chars: 10,
            file_list_width: 250.0,
            startup_error: None,
            scroll_to_commit_idx: None,
            visible_commit_range: None,
            create_branch_sha: None,
            new_branch_name: String::new(),
            multi_selection: BTreeSet::new(),
            selection_anchor: None,
            dialog_mode: None,
            dialog_entries: Vec::new(),
            in_progress_op: None,
        };

        app.refresh_commits();
        app.in_progress_op = git::detect_in_progress_op(&app.repo_path);
        app
    }

    /// Create an App that only displays a startup error message.
    /// Used when the current directory is not a git repository or another
    /// fatal condition prevents normal startup.
    pub fn with_error(error: String) -> Self {
        App {
            repo_path: String::new(),
            show_all: false,
            revision: None,
            path_filter: None,
            commits: Vec::new(),
            selected_index: None,
            diff: None,
            selected_file_index: None,
            scroll_to_diff_line: None,
            graph_rows: Vec::new(),
            current_branch: String::new(),
            repo_name: String::new(),
            status_message: None,
            max_author_chars: 10,
            file_list_width: 250.0,
            startup_error: Some(error),
            scroll_to_commit_idx: None,
            visible_commit_range: None,
            create_branch_sha: None,
            new_branch_name: String::new(),
            multi_selection: BTreeSet::new(),
            selection_anchor: None,
            dialog_mode: None,
            dialog_entries: Vec::new(),
            in_progress_op: None,
        }
    }

    /// Reload the commit list from git.
    pub fn refresh_commits(&mut self) {
        match git::load_commits(
            &self.repo_path,
            self.show_all,
            self.revision.as_deref(),
            self.path_filter.as_deref(),
        ) {
            Ok(commits) => {
                self.graph_rows = graph::compute_graph(&commits);
                // Store the longest author name length for column sizing.
                self.max_author_chars = commits
                    .iter()
                    .map(|c| c.author_name.len())
                    .max()
                    .unwrap_or(8)
                    .clamp(8, 40);
                self.commits = commits;
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to load commits: {}", e));
            }
        }
        self.selected_index = None;
        self.diff = None;
        self.selected_file_index = None;
        self.scroll_to_diff_line = None;
        self.multi_selection.clear();
        self.selection_anchor = None;
        self.in_progress_op = git::detect_in_progress_op(&self.repo_path);
    }

    /// Select a commit by index and load its diff.
    pub fn select_commit(&mut self, index: usize) {
        if index >= self.commits.len() {
            return;
        }

        self.selected_index = Some(index);
        self.selected_file_index = None;
        self.scroll_to_diff_line = None;

        let sha = self.commits[index].full_sha.clone();
        match git::load_diff(&self.repo_path, &sha) {
            Ok(diff) => {
                self.diff = Some(diff);
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to load diff: {}", e));
                self.diff = None;
            }
        }
    }

    /// Get the selected commit, if any.
    pub fn selected_commit(&self) -> Option<&Commit> {
        self.selected_index.and_then(|i| self.commits.get(i))
    }

    /// Build the window title string.
    pub fn window_title(&self) -> String {
        let name = &self.repo_name;
        let branch = self
            .revision
            .as_deref()
            .unwrap_or(self.current_branch.as_str());

        match (&self.path_filter, self.show_all) {
            (Some(path), true) => format!("GitShrub - {} - {} (all branches)", name, path),
            (Some(path), false) => format!("GitShrub - {} - {} [{}]", name, path, branch),
            (None, true) => format!("GitShrub - {} (all branches)", name),
            (None, false) => format!("GitShrub - {} [{}]", name, branch),
        }
    }

    /// Run a git action, show any error in the status bar, and refresh.
    pub fn run_git_action<F>(&mut self, action: F)
    where
        F: FnOnce(&str) -> Result<String, String>,
    {
        match action(&self.repo_path) {
            Ok(_) => {
                self.status_message = None;
                self.current_branch =
                    git::current_branch(&self.repo_path).unwrap_or_else(|_| "detached".into());
            }
            Err(e) => {
                self.status_message = Some(e);
            }
        }
        self.refresh_commits();
        self.select_branch_commit();
    }

    /// Open the cherry-pick dialog for the current multi-selection.
    /// Converts selected commits into dialog entries (oldest-first).
    pub fn open_cherry_pick_dialog(&mut self) {
        if self.multi_selection.is_empty() {
            return;
        }

        // Collect entries in reverse index order (oldest commit first = highest
        // index first, since the commit list is newest-first).
        let entries: Vec<RebaseEntry> = self
            .multi_selection
            .iter()
            .rev()
            .filter_map(|&idx| {
                self.commits.get(idx).map(|c| RebaseEntry {
                    sha: c.full_sha.clone(),
                    short_sha: c.short_sha.clone(),
                    subject: c.subject.clone(),
                    action: RebaseAction::Pick,
                })
            })
            .collect();

        self.dialog_mode = Some(DialogMode::CherryPick);
        self.dialog_entries = entries;
    }

    /// Execute the cherry-pick from dialog entries.
    fn execute_cherry_pick(&mut self) {
        let shas: Vec<String> = self
            .dialog_entries
            .iter()
            .filter(|e| e.action == RebaseAction::Pick)
            .map(|e| e.sha.clone())
            .collect();

        let count = shas.len();
        if count == 0 {
            self.status_message = Some("No commits selected for cherry-pick".to_string());
            return;
        }

        match git::cherry_pick_multiple(&self.repo_path, &shas) {
            Ok(_applied) => {
                self.status_message = None;
                self.current_branch =
                    git::current_branch(&self.repo_path).unwrap_or_else(|_| "detached".into());
            }
            Err((applied, err)) => {
                self.status_message = Some(format!(
                    "Cherry-pick failed after {}/{} commits: {}",
                    applied, count, err
                ));
            }
        }
        self.multi_selection.clear();
        self.selection_anchor = None;
        self.refresh_commits();
        self.select_branch_commit();
    }

    /// Find the commit that the current branch points to, select it,
    /// and request the commit list to scroll there.
    fn select_branch_commit(&mut self) {
        let branch = &self.current_branch;
        if let Some(idx) = self
            .commits
            .iter()
            .position(|c| c.refs.iter().any(|r| r == branch))
        {
            self.select_commit(idx);
            self.scroll_to_commit_idx = Some(idx);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Ctrl+Q to quit.
        if ctx.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.command) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Update window title
        if self.startup_error.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("GitShrub".to_string()));
        } else {
            let title = self.window_title();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // If the app was created with a startup error, show only that.
        if let Some(ref error) = self.startup_error {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.3);
                    ui.heading(
                        egui::RichText::new("Not a Git repository")
                            .color(egui::Color32::from_rgb(255, 140, 100))
                            .size(20.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(error)
                            .color(egui::Color32::from_rgb(180, 180, 180))
                            .size(14.0),
                    );
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new("Run GitShrub from inside a git repository.")
                            .color(egui::Color32::from_rgb(140, 140, 140)),
                    );
                });
            });
            return;
        }

        // Create branch name input dialog.
        self.show_create_branch_dialog(&ctx);

        // Commit list dialog (interactive rebase or cherry-pick).
        self.show_commit_list_dialog(&ctx);

        // Error/status banner at the top
        // In-progress operation banner (shown above status bar).
        let mut abort_clicked = false;
        let mut continue_clicked = false;
        if let Some(ref op) = self.in_progress_op {
            let op_label = op.label().to_string();
            let abort_label = op.abort_label().to_string();
            let can_continue = op.supports_continue();
            let continue_label = op.continue_label().to_string();

            egui::Panel::top("op_in_progress_bar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("⚠ {} in progress", op_label))
                            .strong()
                            .color(egui::Color32::from_rgb(255, 220, 100))
                            .size(14.0),
                    );
                    ui.add_space(12.0);
                    if ui
                        .button(
                            egui::RichText::new(&abort_label)
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        )
                        .clicked()
                    {
                        abort_clicked = true;
                    }
                    if can_continue
                        && ui
                            .button(
                                egui::RichText::new(&continue_label)
                                    .color(egui::Color32::from_rgb(100, 220, 100)),
                            )
                            .clicked()
                    {
                        continue_clicked = true;
                    }
                });
            });
        }
        if abort_clicked {
            if let Some(ref op) = self.in_progress_op.clone() {
                match git::abort_op(&self.repo_path, op) {
                    Ok(_) => {
                        self.status_message = None;
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Abort failed: {}", e));
                    }
                }
                self.current_branch =
                    git::current_branch(&self.repo_path).unwrap_or_else(|_| "detached".into());
                self.refresh_commits();
                self.select_branch_commit();
            }
        }
        if continue_clicked {
            if let Some(ref op) = self.in_progress_op.clone() {
                match git::continue_op(&self.repo_path, op) {
                    Ok(_) => {
                        self.status_message = None;
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Continue failed: {}", e));
                    }
                }
                self.current_branch =
                    git::current_branch(&self.repo_path).unwrap_or_else(|_| "detached".into());
                self.refresh_commits();
                self.select_branch_commit();
            }
        }

        // Error/status banner
        let mut clear_status = false;
        if let Some(ref msg) = self.status_message {
            egui::Panel::top("status_bar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 180, 100), msg.as_str());
                    if ui.button("Dismiss").clicked() {
                        clear_status = true;
                    }
                });
            });
        }
        if clear_status {
            self.status_message = None;
        }

        // Bottom panel: commit info + diff + file list.
        // Use a filled frame so the panel background covers any commit list
        // text that bleeds past the panel boundary above.
        let panel_frame = egui::Frame::new()
            .fill(ctx.global_style().visuals.panel_fill)
            .inner_margin(4.0);
        egui::Panel::bottom("bottom_panel")
            .resizable(true)
            .min_size(150.0)
            .default_size(350.0)
            .frame(panel_frame)
            .show_inside(ui, |ui| {
                self.render_bottom_pane(ui);
            });

        // Central panel: commit list (takes remaining space)
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui::commit_list::show(self, ui);
        });
    }
}

impl App {
    /// Open the interactive rebase dialog for a given branch base.
    pub fn open_rebase_dialog(&mut self, base_sha: &str, branch: &str) {
        match git::load_rebase_commits(&self.repo_path, base_sha) {
            Ok(entries) => {
                self.dialog_mode = Some(DialogMode::Rebase {
                    base_sha: base_sha.to_string(),
                    branch: branch.to_string(),
                });
                self.dialog_entries = entries;
            }
            Err(e) => {
                self.status_message = Some(e);
            }
        }
    }

    /// Show the unified commit-list dialog (interactive rebase or cherry-pick).
    /// Supports drag-and-drop reordering and per-row action dropdowns.
    fn show_commit_list_dialog(&mut self, ctx: &egui::Context) {
        let mode = match self.dialog_mode.clone() {
            Some(m) => m,
            None => return,
        };

        let mut confirmed = false;
        let mut cancelled = false;
        let mut open = true;

        // Handle Escape outside the window closure to avoid borrow conflict with .open().
        let escape_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape_pressed {
            self.dialog_mode = None;
            self.dialog_entries.clear();
            return;
        }

        let (title, description) = match &mode {
            DialogMode::Rebase { base_sha, branch } => (
                "Interactive Rebase".to_string(),
                format!(
                    "Rebase onto {} (branch: {})",
                    &base_sha[..base_sha.len().min(12)],
                    branch
                ),
            ),
            DialogMode::CherryPick => (
                "Cherry-pick Commits".to_string(),
                "Cherry-pick the selected commits onto the current branch.".to_string(),
            ),
        };

        let is_cherry_pick = mode == DialogMode::CherryPick;

        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(600.0)
            .default_height(400.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&description);
                ui.add_space(4.0);
                ui.label("Drag the grip handle to reorder. Choose an action for each commit.");

                ui.add_space(8.0);

                // Column headers.
                ui.horizontal(|ui| {
                    ui.add_space(20.0); // Space for drag handle
                    ui.label(egui::RichText::new("Action").strong().monospace());
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("SHA").strong().monospace());
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new("Message").strong().monospace());
                });
                ui.separator();

                // Scrollable commit list with drag-and-drop.
                // We implement DnD manually so that only the grip handle
                // initiates a drag and the row follows the grab offset
                // instead of jumping its center to the cursor.
                let mut drop_target: Option<(usize, usize)> = None;

                // Stable ID for storing the grab-offset between frames.
                // Must not depend on ui.id() so it's the same in both
                // the outer read site and the inner paint_dialog_row write site.
                let grab_offset_id = egui::Id::new("dnd_grab_offset_stable");

                egui::ScrollArea::vertical()
                    .id_salt("dialog_entries_scroll")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let entry_count = self.dialog_entries.len();

                        for i in 0..entry_count {
                            // Read fields before mutable borrows.
                            let current_action = self.dialog_entries[i].action;
                            let short_sha = self.dialog_entries[i].short_sha.clone();
                            let subject = self.dialog_entries[i].subject.clone();

                            let mut new_action: Option<git::RebaseAction> = None;

                            let handle_id = egui::Id::new(("dialog_entry_handle", i));
                            let is_being_dragged = ui.ctx().is_being_dragged(handle_id);

                            // When this row is being dragged, paint it on a
                            // floating tooltip layer so it renders above everything.
                            // Otherwise paint it normally in-place.
                            let row_response = if is_being_dragged {
                                // Set the DnD payload so drop targets can read it.
                                egui::DragAndDrop::set_payload(ui.ctx(), i);

                                // Lay out the row on a tooltip layer.
                                let layer_id = egui::LayerId::new(egui::Order::Tooltip, handle_id);
                                let resp = ui.scope_builder(
                                    egui::UiBuilder::new().layer_id(layer_id),
                                    |ui| {
                                        Self::paint_dialog_row(
                                            ui,
                                            i,
                                            handle_id,
                                            grab_offset_id,
                                            current_action,
                                            &short_sha,
                                            &subject,
                                            is_cherry_pick,
                                            &mut new_action,
                                        );
                                    },
                                );

                                // Move the layer so the row follows the cursor,
                                // preserving the initial grab offset (no jump).
                                if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                                    let grab_offset: f32 = ui.ctx().data(|d| {
                                        d.get_temp(grab_offset_id).unwrap_or(0.0)
                                    });
                                    let delta_y = pointer_pos.y - grab_offset
                                        - resp.response.rect.top();
                                    let delta = Vec2::new(0.0, delta_y);
                                    ui.ctx().transform_layer_shapes(
                                        layer_id,
                                        egui::emath::TSTransform::from_translation(delta),
                                    );
                                }

                                resp.response
                            } else {
                                let resp = ui.scope(|ui| {
                                    Self::paint_dialog_row(
                                        ui,
                                        i,
                                        handle_id,
                                        grab_offset_id,
                                        current_action,
                                        &short_sha,
                                        &subject,
                                        is_cherry_pick,
                                        &mut new_action,
                                    );
                                });
                                resp.response
                            };

                            // Apply action change after the row closure.
                            if let Some(action) = new_action {
                                self.dialog_entries[i].action = action;
                            }

                            // Check if something is being dragged over this row.
                            if let Some(dragged_idx) =
                                row_response.dnd_hover_payload::<usize>()
                            {
                                let src = *dragged_idx;
                                if src != i {
                                    // Draw a line to indicate drop position.
                                    let rect = row_response.rect;
                                    let y = if src < i {
                                        rect.bottom()
                                    } else {
                                        rect.top()
                                    };
                                    let stroke = Stroke::new(
                                        2.0,
                                        egui::Color32::from_rgb(100, 180, 255),
                                    );
                                    ui.painter().hline(rect.x_range(), y, stroke);
                                }
                            }

                            // Check if something was dropped on this row.
                            if let Some(dragged_idx) =
                                row_response.dnd_release_payload::<usize>()
                            {
                                let src = *dragged_idx;
                                if src != i {
                                    drop_target = Some((src, i));
                                }
                            }
                        }
                    });

                // Apply the drag-and-drop reorder.
                if let Some((src, dst)) = drop_target {
                    let entry = self.dialog_entries.remove(src);
                    self.dialog_entries.insert(dst, entry);
                }

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    let ok_label = if is_cherry_pick {
                        "Cherry-pick"
                    } else {
                        "Rebase"
                    };
                    if ui.button(ok_label).clicked() {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if confirmed {
            let entries = self.dialog_entries.clone();
            self.dialog_mode = None;
            self.dialog_entries.clear();

            match &mode {
                DialogMode::Rebase { base_sha, .. } => {
                    match git::rebase_interactive(&self.repo_path, base_sha, &entries) {
                        Ok(_) => {
                            self.status_message = None;
                            self.current_branch = git::current_branch(&self.repo_path)
                                .unwrap_or_else(|_| "detached".into());
                        }
                        Err(e) => {
                            self.status_message = Some(e);
                        }
                    }
                    self.refresh_commits();
                    self.select_branch_commit();
                }
                DialogMode::CherryPick => {
                    // Re-populate dialog_entries temporarily for execute_cherry_pick.
                    self.dialog_entries = entries;
                    self.execute_cherry_pick();
                    self.dialog_entries.clear();
                }
            }
        } else if !open || cancelled {
            // The X or Cancel button was clicked.
            self.dialog_mode = None;
            self.dialog_entries.clear();
        }
    }

    /// Paint one row of the commit-list dialog (handle + action + SHA + subject).
    /// Extracted so it can be called from both the normal and dragged-layer paths.
    #[expect(clippy::too_many_arguments)]
    fn paint_dialog_row(
        ui: &mut egui::Ui,
        index: usize,
        handle_id: egui::Id,
        grab_offset_id: egui::Id,
        current_action: RebaseAction,
        short_sha: &str,
        subject: &str,
        is_cherry_pick: bool,
        new_action: &mut Option<RebaseAction>,
    ) {

        ui.horizontal(|ui| {
            // --- Drag handle: a painted grip icon (three horizontal lines). ---
            // Allocate space, then interact with our explicit handle_id so that
            // `ctx.is_being_dragged(handle_id)` works in the outer loop.
            let handle_size = Vec2::new(16.0, 16.0);
            let (handle_rect, _) = ui.allocate_exact_size(handle_size, egui::Sense::hover());
            let handle_response = ui.interact(handle_rect, handle_id, egui::Sense::drag());

            // Store the grab offset on drag start so the row doesn't jump.
            if handle_response.drag_started() {
                if let Some(pos) = ui.ctx().pointer_interact_pos() {
                    let offset = pos.y - handle_rect.top();
                    ui.ctx().data_mut(|d| d.insert_temp(grab_offset_id, offset));
                }
            }

            // Paint three horizontal grip lines.
            let painter = ui.painter();
            let grip_color = if handle_response.hovered() || handle_response.dragged() {
                egui::Color32::from_rgb(180, 180, 180)
            } else {
                egui::Color32::from_rgb(100, 100, 100)
            };
            let grip_stroke = Stroke::new(1.5, grip_color);
            let cx = handle_rect.center().x;
            let cy = handle_rect.center().y;
            let half_w = 5.0;
            for dy in [-3.0_f32, 0.0, 3.0] {
                painter.line_segment(
                    [
                        Pos2::new(cx - half_w, cy + dy),
                        Pos2::new(cx + half_w, cy + dy),
                    ],
                    grip_stroke,
                );
            }

            // Show grab cursor when hovering the handle.
            if handle_response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if handle_response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // --- Action dropdown ---
            if is_cherry_pick {
                let label = if current_action == RebaseAction::Drop {
                    "Skip"
                } else {
                    "Pick"
                };
                egui::ComboBox::from_id_salt(ui.id().with(("dialog_action", index)))
                    .selected_text(label)
                    .width(60.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(current_action == RebaseAction::Pick, "Pick")
                            .clicked()
                        {
                            *new_action = Some(RebaseAction::Pick);
                        }
                        if ui
                            .selectable_label(current_action == RebaseAction::Drop, "Skip")
                            .clicked()
                        {
                            *new_action = Some(RebaseAction::Drop);
                        }
                    });
            } else {
                egui::ComboBox::from_id_salt(ui.id().with(("dialog_action", index)))
                    .selected_text(current_action.label())
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for action in git::RebaseAction::ALL {
                            if ui
                                .selectable_label(current_action == action, action.label())
                                .clicked()
                            {
                                *new_action = Some(action);
                            }
                        }
                    });
            }

            // --- SHA ---
            ui.label(
                egui::RichText::new(short_sha)
                    .monospace()
                    .color(egui::Color32::from_rgb(180, 180, 100)),
            );

            // --- Subject (dimmed if dropped/skipped) ---
            let subject_color = if current_action == RebaseAction::Drop {
                egui::Color32::from_rgb(120, 120, 120)
            } else {
                egui::Color32::from_rgb(220, 220, 220)
            };
            ui.label(
                egui::RichText::new(subject)
                    .monospace()
                    .color(subject_color),
            );
        });
    }

    /// Show the branch name input dialog when a CreateBranch action is pending.
    fn show_create_branch_dialog(&mut self, ctx: &egui::Context) {
        let sha = match self.create_branch_sha.clone() {
            Some(s) => s,
            None => return,
        };

        let mut confirmed = false;
        let mut cancelled = false;

        egui::Window::new("Create Branch")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Create a new branch at {}:",
                    &sha[..sha.len().min(12)]
                ));

                ui.add_space(8.0);
                let text_edit = ui.text_edit_singleline(&mut self.new_branch_name);
                text_edit.request_focus();

                if text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirmed = true;
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancelled = true;
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        confirmed = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                });
            });

        if confirmed {
            let name = self.new_branch_name.trim().to_string();
            self.create_branch_sha = None;
            self.new_branch_name.clear();
            if !name.is_empty() {
                let sha_clone = sha;
                self.run_git_action(|repo| git::create_branch(repo, &name, &sha_clone));
            }
        } else if cancelled {
            self.create_branch_sha = None;
            self.new_branch_name.clear();
        }
    }

    /// Render the bottom pane: commit info bar, then diff view + file list side by side.
    ///
    /// Uses `take()` + put-back to avoid cloning `DiffOutput` every frame.
    fn render_bottom_pane(&mut self, ui: &mut egui::Ui) {
        // Commit info bar
        if let Some(commit) = self.selected_commit().cloned() {
            ui::commit_info::show(ui, &commit);
            ui.separator();
        }

        // Temporarily take the diff out of self so we can borrow it
        // immutably while still mutating other fields on self.
        let diff = self.diff.take();

        match diff {
            Some(diff) => {
                let available = ui.available_size();
                // Clamp file list width to reasonable bounds.
                let min_file_width = 100.0_f32;
                let max_file_width = (available.x - 200.0).max(min_file_width);
                self.file_list_width = self.file_list_width.clamp(min_file_width, max_file_width);

                let file_list_width = self.file_list_width;
                let diff_width = (available.x - file_list_width - 12.0).max(100.0);

                let layout = egui::Layout::left_to_right(egui::Align::Min);
                ui.with_layout(layout, |ui| {
                    let diff_height = available.y;

                    ui.vertical(|ui| {
                        ui.set_width(diff_width);
                        ui.set_height(diff_height);
                        ui::diff_view::show(ui, &diff.lines, &mut self.scroll_to_diff_line);
                    });

                    // Draggable divider.
                    let separator_response = ui.separator();
                    let sep_rect = separator_response.rect.expand2(egui::vec2(4.0, 0.0));
                    let sep_id = ui.id().with("diff_file_divider");
                    let sep_interact = ui.interact(sep_rect, sep_id, egui::Sense::click_and_drag());

                    if sep_interact.hovered() || sep_interact.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                    }
                    if sep_interact.dragged() {
                        self.file_list_width -= sep_interact.drag_delta().x;
                        self.file_list_width =
                            self.file_list_width.clamp(min_file_width, max_file_width);
                    }

                    ui.vertical(|ui| {
                        ui.set_width(self.file_list_width);
                        ui.set_height(available.y);
                        let response =
                            ui::file_list::show(ui, &diff.files, self.selected_file_index);
                        if let Some(clicked_idx) = response.clicked_file_index {
                            self.selected_file_index = Some(clicked_idx);
                            if let Some(file_path) = diff.files.get(clicked_idx) {
                                // Look up the line index for this file in the prebuilt header index.
                                if let Some((_, line_idx)) = diff
                                    .file_header_lines
                                    .iter()
                                    .find(|(path, _)| path == file_path)
                                {
                                    self.scroll_to_diff_line = Some(*line_idx);
                                }
                            }
                        }
                    });
                });

                // Put the diff back.
                self.diff = Some(diff);
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(140, 140, 140),
                        "Select a commit to view the diff",
                    );
                });
            }
        }
    }
}
