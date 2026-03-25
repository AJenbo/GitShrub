use crate::git::{self, Commit, DiffOutput};
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

    /// Current branch name.
    pub current_branch: String,

    /// Repository name (directory name).
    pub repo_name: String,

    /// Status/error message to display temporarily.
    pub status_message: Option<String>,
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
            current_branch,
            repo_name,
            status_message: None,
        };

        app.refresh_commits();
        app
    }

    /// Reload the commit list from git.
    pub fn refresh_commits(&mut self) {
        match git::load_commits(&self.repo_path, self.show_all, self.path_filter.as_deref()) {
            Ok(commits) => {
                self.commits = commits;
                self.status_message = None;
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Update window title
        let title = self.window_title();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

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
            // Clear status on dismiss — check if button was clicked
            // We handle this after the panel by just letting it persist for now.
            // TODO: add a timer or dismiss button that works with borrow checker
        }

        // Bottom panel: commit info + diff + file list
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .min_height(150.0)
            .default_height(350.0)
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
                let file_list_width = 250.0_f32.min(available.x * 0.3);

                // Use a right-to-left approach: file list on the right, diff fills the rest
                let layout = egui::Layout::left_to_right(egui::Align::Min);
                ui.with_layout(layout, |ui| {
                    // Left: diff view — give it all remaining width minus file list
                    let diff_width = (available.x - file_list_width - 8.0).max(100.0);
                    let diff_height = available.y;

                    ui.vertical(|ui| {
                        ui.set_width(diff_width);
                        ui.set_height(diff_height);
                        ui::diff_view::show(ui, &diff.raw, &mut self.scroll_to_file);
                    });

                    ui.separator();

                    // Right: file list
                    ui.vertical(|ui| {
                        ui.set_width(file_list_width);
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