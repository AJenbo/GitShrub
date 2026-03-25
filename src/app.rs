use crate::git::{self, Commit, DiffOutput};
use crate::graph::{self, GraphRow};
use crate::ui;

/// Main application state.
pub struct App {
    /// Path to the git repository root.
    pub repo_path: String,

    /// Whether to show all branches (--all flag).
    pub show_all: bool,

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

    /// File to scroll to in the diff view when user clicks a file in the file list.
    pub scroll_to_file: Option<String>,

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

    /// If set, the commit list should scroll to this index on the next frame.
    pub scroll_to_commit_idx: Option<usize>,

    /// The visible row range from the last frame (start, end) for scroll checks.
    pub visible_commit_range: Option<(usize, usize)>,

    /// SHA for a pending "Create branch" action (needs name input).
    pub create_branch_sha: Option<String>,

    /// Text field for the new branch name in the CreateBranch dialog.
    pub new_branch_name: String,
}

impl App {
    /// Create a new App from CLI options. Loads initial commit data.
    pub fn new(repo_path: String, show_all: bool, path_filter: Option<String>) -> Self {
        let current_branch = git::current_branch(&repo_path).unwrap_or_else(|_| "detached".into());

        let repo_name = std::path::Path::new(&repo_path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".into());

        let mut app = App {
            repo_path,
            show_all,
            path_filter,
            commits: Vec::new(),
            selected_index: None,
            diff: None,
            selected_file_index: None,
            scroll_to_file: None,
            graph_rows: Vec::new(),
            current_branch,
            repo_name,
            status_message: None,
            max_author_chars: 10,
            file_list_width: 250.0,
            scroll_to_commit_idx: None,
            visible_commit_range: None,
            create_branch_sha: None,
            new_branch_name: String::new(),
        };

        app.refresh_commits();
        app
    }

    /// Reload the commit list from git.
    pub fn refresh_commits(&mut self) {
        match git::load_commits(&self.repo_path, self.show_all, self.path_filter.as_deref()) {
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
        self.scroll_to_file = None;
    }

    /// Select a commit by index and load its diff.
    pub fn select_commit(&mut self, index: usize) {
        if index >= self.commits.len() {
            return;
        }

        self.selected_index = Some(index);
        self.selected_file_index = None;
        self.scroll_to_file = None;

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
        let branch = &self.current_branch;

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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Ctrl+Q to quit.
        if ctx.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.command) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Update window title
        let title = self.window_title();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // Create branch name input dialog.
        self.show_create_branch_dialog(ctx);

        // Error/status banner at the top
        if let Some(ref msg) = self.status_message {
            egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(255, 180, 100), format!("⚠ {}", msg));
                    if ui.small_button("✕").clicked() {
                        // Can't clear here due to borrow, handled below
                    }
                });
            });
            // TODO: add a timer or dismiss button that works with borrow checker
        }

        // Bottom panel: commit info + diff + file list.
        // Use a filled frame so the panel background covers any commit list
        // text that bleeds past the panel boundary above.
        let panel_frame = egui::Frame::new()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(4.0);
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .min_height(150.0)
            .default_height(350.0)
            .frame(panel_frame)
            .show(ctx, |ui| {
                self.render_bottom_pane(ui);
            });

        // Central panel: commit list (takes remaining space)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::commit_list::show(self, ui);
        });
    }
}

impl App {
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
    fn render_bottom_pane(&mut self, ui: &mut egui::Ui) {
        // Commit info bar
        if let Some(commit) = self.selected_commit().cloned() {
            ui::commit_info::show(ui, &commit);
            ui.separator();
        }

        // Diff + file list side by side
        match self.diff.clone() {
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
                        ui::diff_view::show(ui, &diff.raw, &mut self.scroll_to_file);
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
                                self.scroll_to_file = Some(file_path.clone());
                            }
                        }
                    });
                });
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
