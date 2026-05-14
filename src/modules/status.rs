use egui::Ui;
use crate::core::es_client::ClusterHealth;

#[derive(Debug, Clone, Default)]
pub struct StatusState {
    pub health_data: Vec<(String, Option<ClusterHealth>)>,
}

pub fn render_status_module(ui: &mut Ui, state: &StatusState) {
    ui.heading("Cluster Status");
    ui.add_space(16.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (name, health) in &state.health_data {
            let frame = egui::Frame::none()
                .fill(crate::ui::theme::Theme::BG_CARD)
                .rounding(crate::ui::theme::Theme::CARD_ROUNDING)
                .inner_margin(crate::ui::theme::Theme::CARD_PADDING);
            
            frame.show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).strong().size(16.0));
                    if let Some(h) = health {
                        let color = crate::ui::theme::Theme::health_color(&h.status);
                        ui.colored_label(color, &h.status);
                    } else {
                        ui.colored_label(crate::ui::theme::Theme::DANGER, "Unreachable");
                    }
                });
                
                if let Some(h) = health {
                    ui.horizontal(|ui| {
                        ui.label(format!("Nodes: {}", h.number_of_nodes));
                        ui.label(format!("Active Shards: {}", h.active_shards));
                        ui.label(format!("Unassigned: {}", h.unassigned_shards));
                    });
                }
            });
            ui.add_space(8.0);
        }
    });
}
