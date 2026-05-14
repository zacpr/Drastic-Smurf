use egui::Ui;
use crate::core::es_client::TaskInfo;

#[derive(Debug, Clone, Default)]
pub struct TasksState {
    pub tasks: Vec<(String, TaskInfo)>,
    pub filter: String,
}

pub fn render_tasks_module(ui: &mut Ui, state: &mut TasksState) {
    ui.heading("Task Monitoring");
    ui.add_space(16.0);

    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.text_edit_singleline(&mut state.filter);
    });
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Frame::new()
            .fill(crate::ui::theme::Theme::BG_CARD)
            .corner_radius(crate::ui::theme::Theme::CARD_ROUNDING)
            .inner_margin(crate::ui::theme::Theme::CARD_PADDING)
            .show(ui, |ui| {
                if state.tasks.is_empty() {
                    ui.label("No tasks found.");
                    return;
                }

                egui::Grid::new("tasks_grid")
                    .num_columns(5)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Cluster").strong());
                        ui.label(egui::RichText::new("Action").strong());
                        ui.label(egui::RichText::new("Description").strong());
                        ui.label(egui::RichText::new("Running Time").strong());
                        ui.label(egui::RichText::new("Cancellable").strong());
                        ui.end_row();

                        for (cluster, task) in &state.tasks {
                            if !state.filter.is_empty() {
                                let f = state.filter.to_lowercase();
                                if !task.action.to_lowercase().contains(&f)
                                    && !task.description.as_ref().map(|d| d.to_lowercase()).unwrap_or_default().contains(&f)
                                {
                                    continue;
                                }
                            }

                            ui.label(cluster);
                            ui.label(&task.action);
                            ui.label(task.description.as_deref().unwrap_or("—"));
                            let millis = task.running_time_in_nanos / 1_000_000;
                            ui.label(format!("{}ms", millis));
                            ui.label(if task.cancellable { "Yes" } else { "No" });
                            ui.end_row();
                        }
                    });
            });
    });
}
