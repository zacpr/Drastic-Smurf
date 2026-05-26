use crate::core::es_client::TaskInfo;
use crate::ui::widgets::human_nanos;
use crate::ui::theme::Theme;
use egui::{Ui, Vec2, Frame, Margin, CornerRadius};

#[derive(Debug, Clone, Default)]
pub struct TasksState {
    pub tasks: Vec<(String, TaskInfo)>,
    pub filter: String,
    pub errors: std::collections::HashMap<String, String>,
    pub expanded_tasks: std::collections::HashSet<String>, // format: "{cluster}:{node}:{id}"
}

pub fn render_tasks_module(ui: &mut Ui, state: &mut TasksState) {
    ui.heading("Task Monitoring");
    ui.add_space(16.0);

    // Search and Filter Bar
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter Tasks:").strong().color(Theme::text_secondary()));
        let filter_edit = egui::TextEdit::singleline(&mut state.filter)
            .hint_text("Search action, description, or cluster...");
        ui.add_sized([300.0, ui.available_height()], filter_edit);
        
        if !state.filter.is_empty() {
            if ui.button("Clear").clicked() {
                state.filter.clear();
            }
        }
    });
    ui.add_space(12.0);

    egui::ScrollArea::vertical()
        .id_salt("tasks_scroll")
        .show(ui, |ui| {
            // Show cluster connection/fetch errors first
            if !state.errors.is_empty() {
                for (cluster, err) in &state.errors {
                    if !state.filter.is_empty()
                        && !cluster.to_lowercase().contains(&state.filter.to_lowercase())
                    {
                        continue;
                    }
                    Frame::new()
                        .fill(Theme::bg_card())
                        .corner_radius(Theme::CARD_ROUNDING)
                        .inner_margin(Theme::CARD_PADDING)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    Theme::danger(),
                                    format!("⚠ {}: {}", cluster, err),
                                );
                            });
                        });
                    ui.add_space(8.0);
                }
            }

            if state.tasks.is_empty() {
                Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("No active tasks found on any connected clusters.");
                    });
                return;
            }

            // Filtered tasks count
            let filtered_tasks: Vec<&(String, TaskInfo)> = state.tasks.iter().filter(|(cluster, task)| {
                if state.filter.is_empty() {
                    return true;
                }
                let f = state.filter.to_lowercase();
                cluster.to_lowercase().contains(&f)
                    || task.action.to_lowercase().contains(&f)
                    || task.node.to_lowercase().contains(&f)
                    || task.description.as_ref().map(|d| d.to_lowercase()).unwrap_or_default().contains(&f)
            }).collect();

            if filtered_tasks.is_empty() {
                Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label("No active tasks match your search filter.");
                    });
                return;
            }

            // Render active tasks as modern collapsible cards
            for (cluster, task) in filtered_tasks {
                let full_task_id = format!("{}:{}", task.node, task.id);
                let task_key = format!("{}:{}", cluster, full_task_id);
                let is_expanded = state.expanded_tasks.contains(&task_key);

                Frame::new()
                    .fill(Theme::bg_card())
                    .corner_radius(Theme::CARD_ROUNDING)
                    .inner_margin(Theme::CARD_PADDING)
                    .stroke(egui::Stroke::new(1.0, if is_expanded { Theme::accent() } else { Theme::border() }))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        // Header row
                        ui.horizontal(|ui| {
                            // Cluster Pill
                            Frame::new()
                                .fill(Theme::bg_input())
                                .corner_radius(CornerRadius::same(4))
                                .inner_margin(Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(cluster).strong().size(11.0).color(Theme::text_primary()));
                                });

                            ui.add_space(4.0);

                            // Action Title
                            ui.label(egui::RichText::new(&task.action).strong().size(13.0).color(Theme::text_primary()));

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Cancellable pill
                                if task.cancellable {
                                    Frame::new()
                                        .fill(Theme::danger().linear_multiply(0.15))
                                        .corner_radius(CornerRadius::same(4))
                                        .inner_margin(Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.label(egui::RichText::new("Cancellable").size(10.0).color(Theme::danger()));
                                        });
                                }

                                ui.add_space(8.0);

                                // Running time pill
                                Frame::new()
                                    .fill(Theme::bg_input())
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(6, 2))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(human_nanos(task.running_time_in_nanos)).size(11.0).color(Theme::text_primary()));
                                    });
                            });
                        });

                        ui.add_space(8.0);

                        // Body row (Task ID & Description)
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Task ID:").color(Theme::text_secondary()).size(12.0));
                            ui.label(egui::RichText::new(&full_task_id).code().color(Theme::text_primary()).size(12.0));

                            // Copy ID Button
                            if ui.button("📋").on_hover_text("Copy Task ID to clipboard").clicked() {
                                ui.ctx().copy_text(full_task_id.clone());
                            }

                            if let Some(ref desc) = task.description {
                                ui.add_space(16.0);
                                ui.label(egui::RichText::new("Description:").color(Theme::text_secondary()).size(12.0));
                                ui.label(egui::RichText::new(desc).strong().color(Theme::text_primary()).size(12.0));
                            }
                        });

                        // Live Progress / ETA estimation
                        let running_mins = task.running_time_in_nanos as f64 / 60_000_000_000.0;
                        let has_progress = get_task_progress_and_eta(task);

                        if running_mins >= 1.0 || has_progress.is_some() {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if let Some((progress, eta_str)) = has_progress {
                                    // Custom visual progress bar
                                    let progress_pct = progress * 100.0;
                                    ui.label(egui::RichText::new(format!("Progress: {:.1}%", progress_pct)).size(12.0).color(Theme::text_primary()));
                                    
                                    let bar_width = 180.0;
                                    let (rect, _response) = ui.allocate_at_least(Vec2::new(bar_width, 10.0), egui::Sense::hover());
                                    
                                    // Draw background
                                    ui.painter().rect_filled(rect, CornerRadius::same(5), Theme::bg_input());
                                    // Draw progress
                                    let mut progress_rect = rect;
                                    progress_rect.set_width(bar_width * progress);
                                    ui.painter().rect_filled(progress_rect, CornerRadius::same(5), Theme::accent());

                                    ui.add_space(8.0);
                                    ui.colored_label(Theme::success(), egui::RichText::new(format!("⏳ ETA: {}", eta_str)).strong().size(12.0));
                                } else {
                                    ui.colored_label(Theme::text_secondary(), egui::RichText::new("⏳ Long Running Task (> 1 minute, no progress indicators)").italics().size(11.0));
                                }
                            });
                        }

                        ui.add_space(6.0);

                        // Expand Details block
                        ui.horizontal(|ui| {
                            let btn_txt = if is_expanded { "Hide Details 🔼" } else { "Show Details 🔽" };
                            if ui.button(btn_txt).clicked() {
                                if is_expanded {
                                    state.expanded_tasks.remove(&task_key);
                                } else {
                                    state.expanded_tasks.insert(task_key.clone());
                                }
                            }
                        });

                        if is_expanded {
                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Metadata Grid
                            egui::Grid::new(format!("meta_grid_{}", task_key))
                                .num_columns(2)
                                .spacing([16.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Task Type:").color(Theme::text_secondary()));
                                    ui.label(&task.task_type);
                                    ui.end_row();

                                    ui.label(egui::RichText::new("Node ID:").color(Theme::text_secondary()));
                                    ui.label(&task.node);
                                    ui.end_row();

                                    ui.label(egui::RichText::new("Start Time:").color(Theme::text_secondary()));
                                    let dt = chrono::DateTime::from_timestamp(task.start_time_in_millis / 1000, 0)
                                        .map(|d| d.to_rfc2822())
                                        .unwrap_or_else(|| "Unknown".to_string());
                                    ui.label(dt);
                                    ui.end_row();

                                    if let Some(ref parent) = task.parent_task_id {
                                        ui.label(egui::RichText::new("Parent Task ID:").color(Theme::text_secondary()));
                                        ui.label(parent);
                                        ui.end_row();
                                    }
                                });

                            ui.add_space(12.0);

                            // Full task status/headers JSON details
                            let details_json = serde_json::json!({
                                "status": task.status,
                                "headers": task.headers,
                            });
                            let pretty_json = serde_json::to_string_pretty(&details_json).unwrap_or_default();

                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Task Raw Details:").strong().color(Theme::text_primary()));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("📋 Copy Details JSON").clicked() {
                                        ui.ctx().copy_text(pretty_json.clone());
                                    }
                                });
                            });
                            ui.add_space(4.0);

                            egui::ScrollArea::vertical()
                                .id_salt(format!("details_scroll_{}", task_key))
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    let mut contents = pretty_json.clone();
                                    ui.add(
                                        egui::TextEdit::multiline(&mut contents)
                                            .font(egui::TextStyle::Monospace)
                                            .code_editor()
                                            .desired_width(ui.available_width())
                                            .interactive(false),
                                    );
                                });
                        }
                    });
                ui.add_space(8.0);
            }
        });
}

fn get_task_progress_and_eta(task: &TaskInfo) -> Option<(f32, String)> {
    let status_val = task.status.as_ref()?;
    let total = status_val.get("total")?.as_f64()?;
    if total <= 0.0 {
        return None;
    }
    let created = status_val.get("created").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let updated = status_val.get("updated").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let deleted = status_val.get("deleted").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let completed = created + updated + deleted;
    if completed <= 0.0 {
        return None;
    }
    
    let progress = (completed / total) as f32;
    let running_time_secs = task.running_time_in_nanos as f64 / 1_000_000_000.0;
    
    if progress >= 1.0 {
        return Some((1.0, "Finishing...".to_string()));
    }
    
    if running_time_secs < 5.0 {
        return Some((progress, "Calculating ETA...".to_string()));
    }
    
    let total_est_secs = running_time_secs / progress as f64;
    let remaining_secs = total_est_secs - running_time_secs;
    if remaining_secs < 0.0 {
        return None;
    }
    
    let eta_str = if remaining_secs < 60.0 {
        format!("{:.0}s remaining", remaining_secs)
    } else if remaining_secs < 3600.0 {
        format!("{:.0}m {:.0}s remaining", (remaining_secs / 60.0).floor(), remaining_secs % 60.0)
    } else {
        format!("{:.1}h remaining", remaining_secs / 3600.0)
    };
    
    Some((progress, eta_str))
}
