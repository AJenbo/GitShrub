use egui::{self, Pos2, Rect, Stroke, Vec2};

use crate::app::App;
use crate::git;
use crate::graph;

/// Horizontal spacing between lane centers in the graph column.
const LANE_WIDTH: f32 = 16.0;

/// Radius of the commit node circle.
const NODE_RADIUS: f32 = 4.0;

/// Minimum width reserved for the graph column even when there are few lanes.
const MIN_GRAPH_WIDTH: f32 = 32.0;

/// Number of characters in the date string ("2018-12-02 08:55:32").
const DATE_CHARS: usize = 19;

/// Padding added to each right-side column.
const COL_PADDING: f32 = 24.0;

/// Renders the commit list in the top pane.
/// Each row shows: graph | refs | message | author | date
/// Clicking a row selects it. Right-clicking opens a context menu.
/// Up/Down/Home/End navigate the list via keyboard.
pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let item_spacing_y = ui.spacing().item_spacing.y;
    let interact_size_y = ui.spacing().interact_size.y;
    // show_rows() expects row height WITHOUT spacing — it adds item_spacing.y
    // internally. Each commit row is a ui.horizontal() whose rect height is
    // max(text_height, interact_size.y) + inner margins (4.0).
    let show_rows_height = text_height.max(interact_size_y) + 4.0;
    // The actual step between row tops (for scroll offset math) is what
    // show_rows computes internally: show_rows_height + item_spacing.y.
    let row_step = show_rows_height + item_spacing_y;

    // Measure the actual monospace character width from the font.
    let mono_char_width = ui
        .painter()
        .layout_no_wrap(
            "M".to_string(),
            egui::FontId::monospace(text_height),
            egui::Color32::WHITE,
        )
        .size()
        .x;

    let date_col_width = DATE_CHARS as f32 * mono_char_width + COL_PADDING;
    let author_col_width = app.max_author_chars as f32 * mono_char_width + COL_PADDING;

    let num_commits = app.commits.len();
    if num_commits == 0 {
        ui.centered_and_justified(|ui| {
            if app.path_filter.is_some() {
                ui.label("No commits found for this path.");
            } else {
                ui.label("No commits in this repository yet.");
            }
        });
        return;
    }

    // --- Keyboard navigation (Up / Down / Home / End) ---
    // We handle keys before building the scroll area so that a
    // scroll-to request generated here is picked up in the same frame.
    let mut keyboard_select: Option<usize> = None;

    if app.create_branch_sha.is_none() {
        let events = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
                i.key_pressed(egui::Key::PageUp),
                i.key_pressed(egui::Key::PageDown),
            )
        });

        let (up, down, home, end, page_up, page_down) = events;

        if up || down || home || end || page_up || page_down {
            let current = app.selected_index.unwrap_or(0);
            let visible_height = ui.available_height();
            let page_rows = ((visible_height / row_step).floor() as usize).max(1);

            let new_idx = if home {
                0
            } else if end {
                num_commits - 1
            } else if up {
                current.saturating_sub(1)
            } else if down {
                (current + 1).min(num_commits - 1)
            } else if page_up {
                current.saturating_sub(page_rows)
            } else {
                // page_down
                (current + page_rows).min(num_commits - 1)
            };

            if Some(new_idx) != app.selected_index {
                keyboard_select = Some(new_idx);
                app.scroll_to_commit_idx = Some(new_idx);
                // Keyboard navigation clears multi-selection.
                app.multi_selection.clear();
            }
        }
    }

    // Determine graph column width from the maximum number of active lanes.
    let max_lanes = app
        .graph_rows
        .iter()
        .map(|r| r.num_lanes)
        .max()
        .unwrap_or(1)
        .max(1);
    let graph_col_width = (max_lanes as f32 * LANE_WIDTH + LANE_WIDTH).max(MIN_GRAPH_WIDTH);

    // Track which commit the user clicked this frame (if any),
    // together with the modifier keys that were held.
    let mut clicked_index: Option<usize> = None;
    let mut click_modifiers = egui::Modifiers::NONE;

    // If a scroll-to request is pending, compute the offset to apply.
    // We set it on the ScrollArea before rendering so egui jumps there.
    let mut scroll_area = egui::ScrollArea::vertical()
        .id_salt("commit_list_scroll")
        .auto_shrink([false, false]);

    if let Some(target_idx) = app.scroll_to_commit_idx.take() {
        let visible_height = ui.available_height();
        let visible_rows = (visible_height / row_step).floor() as usize;

        // Scroll only if the target is outside the currently visible range,
        // then place it just inside the visible edge. Use row_step (the
        // measured per-row distance) for offset math so it matches reality.
        if let Some((prev_start, prev_end)) = app.visible_commit_range {
            if target_idx < prev_start {
                // Scrolling up: place target near the top with a 1-row margin.
                let target_top = target_idx.saturating_sub(1);
                let offset = target_top as f32 * row_step;
                scroll_area = scroll_area.vertical_scroll_offset(offset);
            } else if target_idx >= prev_end {
                // Scrolling down: place target near the bottom with a 1-row margin.
                let target_bottom = (target_idx + 2).min(num_commits);
                let offset = (target_bottom as f32 * row_step - visible_height).max(0.0);
                scroll_area = scroll_area.vertical_scroll_offset(offset);
            }
            // else: already visible, no scroll needed.
        } else {
            // No previous range known; center the target.
            let half_page = visible_rows / 2;
            let target_top = target_idx.saturating_sub(half_page);
            let offset = target_top as f32 * row_step;
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
    }

    // Total width available for the row content.
    let total_width = ui.available_width();

    scroll_area.show_rows(ui, show_rows_height, num_commits, |ui, row_range| {
        // Store the visible range so we can check it next frame.
        app.visible_commit_range = Some((row_range.start, row_range.end));

        for idx in row_range.clone() {
            let is_active = app.selected_index == Some(idx);
            let is_multi = app.multi_selection.contains(&idx);

            // Read commit fields before any mutable borrow.
            let commit = &app.commits[idx];
            let refs = commit.refs.clone();
            let full_sha = commit.full_sha.clone();
            let subject = commit.subject.clone();
            let author_name = commit.author_name.clone();
            let date = commit.date.clone();

            let response = ui.horizontal(|ui| {
                ui.style_mut().interaction.selectable_labels = false;
                ui.set_min_width(total_width);

                // Reserve space for the graph column.
                let (graph_rect, _) = ui.allocate_exact_size(
                    Vec2::new(graph_col_width, show_rows_height),
                    egui::Sense::hover(),
                );

                // Draw graph elements into the reserved rect.
                if let Some(graph_row) = app.graph_rows.get(idx) {
                    paint_graph_row(ui, graph_rect, graph_row, show_rows_height);
                }

                // Compute right-side column positions from the row rect.
                // Layout: ... | message ... | author | date |
                let row_right = ui.max_rect().right();
                let date_left = row_right - date_col_width;
                let author_left = date_left - author_col_width;
                let message_right = author_left - 8.0;

                // Ref labels (branches and tags).
                for ref_name in &refs {
                    let (label_text, bg_color, text_color) = if ref_name.starts_with("tag: ") {
                        let tag = ref_name.trim_start_matches("tag: ");
                        (
                            format!("<{}>", tag),
                            egui::Color32::from_rgb(80, 60, 20),
                            egui::Color32::from_rgb(240, 200, 80),
                        )
                    } else if ref_name.contains('/') {
                        (
                            format!("[{}]", ref_name),
                            egui::Color32::from_rgb(30, 60, 30),
                            egui::Color32::from_rgb(130, 220, 130),
                        )
                    } else {
                        (
                            format!("[{}]", ref_name),
                            egui::Color32::from_rgb(20, 50, 80),
                            egui::Color32::from_rgb(100, 180, 255),
                        )
                    };

                    let rich = egui::RichText::new(&label_text)
                        .monospace()
                        .color(text_color)
                        .background_color(bg_color);
                    ui.label(rich);
                }

                // Commit message: render it but clip to the available message area.
                let msg_color = if is_active {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(220, 220, 220)
                };
                let cursor_x = ui.cursor().left();
                let msg_avail = (message_right - cursor_x).max(20.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(msg_avail, show_rows_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        let message_text =
                            egui::RichText::new(&subject).monospace().color(msg_color);
                        ui.label(message_text);
                    },
                );

                // Paint a solid background behind the author+date columns
                // to cover any graph lines that bleed into this area.
                let painter = ui.painter();
                let row_top = ui.min_rect().top();
                let text_y = row_top + (show_rows_height - text_height) * 0.5;
                let bg_color = ui.visuals().window_fill;

                let columns_rect = Rect::from_min_max(
                    Pos2::new(author_left - 5.0, row_top),
                    Pos2::new(row_right, row_top + show_rows_height),
                );
                painter.rect_filled(columns_rect, 0.0, bg_color);

                let sep_color = egui::Color32::from_rgb(50, 50, 55);
                let sep_stroke = Stroke::new(1.0, sep_color);

                // Separator line before author column.
                painter.line_segment(
                    [
                        Pos2::new(author_left - 4.0, row_top),
                        Pos2::new(author_left - 4.0, row_top + show_rows_height),
                    ],
                    sep_stroke,
                );

                // Author name (truncated to fit).
                let author_rect = Rect::from_min_size(
                    Pos2::new(author_left, text_y),
                    Vec2::new(author_col_width - COL_PADDING * 0.5, text_height),
                );
                let author_galley = painter.layout_no_wrap(
                    author_name.clone(),
                    egui::FontId::monospace(text_height),
                    egui::Color32::from_rgb(180, 160, 200),
                );
                painter.with_clip_rect(author_rect.expand(1.0)).galley(
                    author_rect.min,
                    author_galley,
                    egui::Color32::TRANSPARENT,
                );

                // Separator line before date column.
                painter.line_segment(
                    [
                        Pos2::new(date_left - 4.0, row_top),
                        Pos2::new(date_left - 4.0, row_top + show_rows_height),
                    ],
                    sep_stroke,
                );

                // Date.
                let date_rect = Rect::from_min_size(
                    Pos2::new(date_left, text_y),
                    Vec2::new(date_col_width - COL_PADDING * 0.5, text_height),
                );
                let date_galley = painter.layout_no_wrap(
                    date.clone(),
                    egui::FontId::monospace(text_height),
                    egui::Color32::from_rgb(140, 140, 140),
                );
                painter.with_clip_rect(date_rect.expand(1.0)).galley(
                    date_rect.min,
                    date_galley,
                    egui::Color32::TRANSPARENT,
                );
            });

            // Place an invisible click-sensing rect over the entire row.
            let row_rect = response.response.rect;
            let row_id = ui.id().with(("commit_row", idx));
            let row_response = ui.interact(row_rect, row_id, egui::Sense::click());
            if row_response.clicked() {
                clicked_index = Some(idx);
                click_modifiers = ui.input(|i| i.modifiers);
            }

            // Highlight: active row (blue), multi-selected rows (teal).
            if is_active {
                ui.painter().rect_filled(
                    row_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(60, 80, 120, 80),
                );
            } else if is_multi {
                ui.painter().rect_filled(
                    row_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(40, 90, 90, 70),
                );
            }

            // Context menu: show multi-select menu when there is a
            // multi-selection, otherwise show the normal single-commit menu.
            let multi_count = app.multi_selection.len();
            row_response.context_menu(|ui| {
                if multi_count > 1 {
                    // --- Multi-select context menu ---
                    ui.label(
                        egui::RichText::new(format!("{} commits selected", multi_count))
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 200)),
                    );
                    ui.separator();

                    if ui
                        .button(format!("Cherry-pick {} commits", multi_count))
                        .clicked()
                    {
                        app.cherry_pick_multi_selection();
                        ui.close();
                    }
                } else {
                    // --- Single-commit context menu ---
                    let short = &full_sha[..full_sha.len().min(12)];
                    ui.label(
                        egui::RichText::new(format!("{}  {}", short, &subject))
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 200)),
                    );
                    ui.separator();

                    // Branch/tag operations (shown only when refs are present).
                    let local_branches: Vec<&String> = refs
                        .iter()
                        .filter(|r| !r.starts_with("tag: ") && !r.contains('/'))
                        .collect();
                    let remote_branches: Vec<&String> =
                        refs.iter().filter(|r| r.contains('/')).collect();

                    if !local_branches.is_empty() || !remote_branches.is_empty() {
                        for branch in &local_branches {
                            ui.menu_button(
                                egui::RichText::new(format!("[{}]", branch))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(100, 180, 255)),
                                |ui| {
                                    if ui.button("Checkout").clicked() {
                                        let b = (*branch).clone();
                                        app.run_git_action(|repo| git::checkout_branch(repo, &b));
                                        ui.close();
                                    }
                                    if ui.button("Delete branch").clicked() {
                                        let b = (*branch).clone();
                                        app.run_git_action(|repo| git::delete_branch(repo, &b));
                                        ui.close();
                                    }
                                },
                            );
                        }

                        for branch in &remote_branches {
                            ui.menu_button(
                                egui::RichText::new(format!("[{}]", branch))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(130, 220, 130)),
                                |ui| {
                                    if ui.button("Checkout").clicked() {
                                        let b = (*branch).clone();
                                        app.run_git_action(|repo| git::checkout_branch(repo, &b));
                                        ui.close();
                                    }
                                },
                            );
                        }

                        ui.separator();
                    }

                    // Generic commit operations (always shown).
                    if ui.button("Create branch here...").clicked() {
                        app.create_branch_sha = Some(full_sha.clone());
                        ui.close();
                    }

                    ui.separator();

                    if ui.button("Cherry-pick").clicked() {
                        let sha = full_sha.clone();
                        app.run_git_action(|repo| git::cherry_pick(repo, &sha));
                        ui.close();
                    }
                    if ui.button("Revert").clicked() {
                        let sha = full_sha.clone();
                        app.run_git_action(|repo| git::revert_commit(repo, &sha));
                        ui.close();
                    }

                    ui.separator();

                    if ui.button("Reset --mixed to here").clicked() {
                        let sha = full_sha.clone();
                        app.run_git_action(|repo| git::reset_mixed(repo, &sha));
                        ui.close();
                    }
                    if ui
                        .button(
                            egui::RichText::new("Reset --hard to here")
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        )
                        .clicked()
                    {
                        let sha = full_sha.clone();
                        app.run_git_action(|repo| git::reset_hard(repo, &sha));
                        ui.close();
                    }
                }
            });
        }
    });

    // Apply the click outside the borrow of commits.
    // Keyboard selection takes precedence if both happen in the same frame.
    if let Some(idx) = keyboard_select {
        app.select_commit(idx);
    } else if let Some(idx) = clicked_index {
        let ctrl = click_modifiers.command || click_modifiers.ctrl;
        let shift = click_modifiers.shift;

        if ctrl {
            // Ctrl+click: toggle this commit in/out of multi-selection.
            if app.multi_selection.contains(&idx) {
                app.multi_selection.remove(&idx);
                // If we just deselected the active commit, clear focus and diff.
                if app.selected_index == Some(idx) {
                    app.selected_index = None;
                    app.diff = None;
                    app.selected_file_index = None;
                    app.scroll_to_diff_line = None;
                }
            } else {
                app.multi_selection.insert(idx);
                // Make the newly selected commit the active one (show its diff).
                app.select_commit(idx);
            }
            app.selection_anchor = Some(idx);
        } else if shift {
            // Shift+click: select range from anchor to clicked index.
            let anchor = app
                .selection_anchor
                .unwrap_or(app.selected_index.unwrap_or(0));
            let lo = anchor.min(idx);
            let hi = anchor.max(idx);
            app.multi_selection.clear();
            for i in lo..=hi {
                app.multi_selection.insert(i);
            }
            // Make the clicked commit the active one (show its diff),
            // but keep the anchor where it was for further shift+clicks.
            app.select_commit(idx);
        } else {
            // Plain click: single select, clear multi-selection.
            app.multi_selection.clear();
            app.selection_anchor = Some(idx);
            app.select_commit(idx);
        }
    }
}

/// Paint the graph column for a single row: edges (lines) and the commit node (circle).
fn paint_graph_row(ui: &egui::Ui, rect: Rect, row: &graph::GraphRow, row_height: f32) {
    let painter = ui.painter_at(rect);
    let line_width = 1.8;

    let center_y = rect.top() + row_height * 0.5;

    // Helper: x position for a given lane column within the graph rect.
    let lane_x = |col: usize| -> f32 { rect.left() + LANE_WIDTH * 0.5 + col as f32 * LANE_WIDTH };

    // Draw edges first (behind the node).
    for edge in &row.edges {
        let color = graph::lane_color(edge.color_index);
        let stroke = Stroke::new(line_width, color);

        let from_x = lane_x(edge.from_col);
        let to_x = lane_x(edge.to_col);

        if edge.from_col == edge.to_col {
            // Straight vertical line through the full row height.
            painter.line_segment(
                [
                    Pos2::new(from_x, rect.top()),
                    Pos2::new(to_x, rect.bottom()),
                ],
                stroke,
            );
        } else {
            // Diagonal connector: go from (from_x, center) to (to_x, bottom).
            // Draw in two segments for a smoother look:
            // 1. Vertical from top to center (at from_x).
            // 2. Diagonal from center to bottom (from from_x to to_x).
            painter.line_segment(
                [Pos2::new(from_x, rect.top()), Pos2::new(from_x, center_y)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(from_x, center_y), Pos2::new(to_x, rect.bottom())],
                stroke,
            );
        }
    }

    // Draw the commit node circle on top of the edges.
    let node_x = lane_x(row.node_col);
    let node_center = Pos2::new(node_x, center_y);
    let node_color = graph::lane_color(row.node_color_index);

    painter.circle_filled(node_center, NODE_RADIUS, node_color);
    // Dark outline to make the node pop against the lines.
    painter.circle_stroke(
        node_center,
        NODE_RADIUS,
        Stroke::new(1.0, egui::Color32::from_rgb(30, 30, 30)),
    );
}
