use std::env;
use std::process;

mod app;
mod git;
mod graph;
mod ui;

fn main() -> eframe::Result<()> {
    let (show_all, path_filter, repo_result) = parse_args();

    let (title, make_app): (String, Box<dyn FnOnce() -> app::App + Send>) = match repo_result {
        Ok(repo_path) => {
            let repo_name = std::path::Path::new(&repo_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "GitShrub".to_string());

            let t = match (&path_filter, show_all) {
                (Some(path), true) => {
                    format!("GitShrub - {} - {} (all branches)", repo_name, path)
                }
                (Some(path), false) => format!("GitShrub - {} - {}", repo_name, path),
                (None, true) => format!("GitShrub - {} (all branches)", repo_name),
                (None, false) => format!("GitShrub - {}", repo_name),
            };

            (
                t,
                Box::new(move || app::App::new(repo_path, show_all, path_filter)),
            )
        }
        Err(error) => (
            "GitShrub".to_string(),
            Box::new(move || app::App::with_error(error)),
        ),
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        &title,
        native_options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(make_app()))
        }),
    )
}

/// Parse CLI arguments. Returns (show_all, path_filter, repo_path).
fn parse_args() -> (bool, Option<String>, Result<String, String>) {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut show_all = false;
    let mut path_filter = None;

    for arg in &args {
        match arg.as_str() {
            "--all" => show_all = true,
            "--help" | "-h" => {
                eprintln!("Usage: gitshrub [--all] [<path>]");
                eprintln!();
                eprintln!("  --all       Show all branches (default: current branch only)");
                eprintln!("  <path>      Show history for a specific file or directory");
                process::exit(0);
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    process::exit(1);
                }
                path_filter = Some(other.to_string());
            }
        }
    }

    // Verify we're inside a git repo
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());

    let repo_result = git::verify_repo(&cwd);

    (show_all, path_filter, repo_result)
}
