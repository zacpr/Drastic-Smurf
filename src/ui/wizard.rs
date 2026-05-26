use egui::{Color32, Stroke};

use crate::app::Tab;
use crate::core::cluster_manager::ClusterManager;
use crate::core::config::ClusterConfig;
use crate::ui::theme::Theme;
use crate::ui::toasts::Toasts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    Welcome,
    AddCluster,
    SnapshotMon,
    ClusterStatus,
    TaskMon,
    ElasticConsole,
    DiscoverTab,
    Finish,
}

impl WizardStep {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Welcome => Some(Self::AddCluster),
            Self::AddCluster => Some(Self::SnapshotMon),
            Self::SnapshotMon => Some(Self::ClusterStatus),
            Self::ClusterStatus => Some(Self::TaskMon),
            Self::TaskMon => Some(Self::ElasticConsole),
            Self::ElasticConsole => Some(Self::DiscoverTab),
            Self::DiscoverTab => Some(Self::Finish),
            Self::Finish => None,
        }
    }

    pub fn prev(self) -> Option<Self> {
        match self {
            Self::Welcome => None,
            Self::AddCluster => Some(Self::Welcome),
            Self::SnapshotMon => Some(Self::AddCluster),
            Self::ClusterStatus => Some(Self::SnapshotMon),
            Self::TaskMon => Some(Self::ClusterStatus),
            Self::ElasticConsole => Some(Self::TaskMon),
            Self::DiscoverTab => Some(Self::ElasticConsole),
            Self::Finish => Some(Self::DiscoverTab),
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::AddCluster => 1,
            Self::SnapshotMon => 2,
            Self::ClusterStatus => 3,
            Self::TaskMon => 4,
            Self::ElasticConsole => 5,
            Self::DiscoverTab => 6,
            Self::Finish => 7,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to DRASTIC SMURF",
            Self::AddCluster => "1. Connect a Cluster",
            Self::SnapshotMon => "2. Snapshot Monitoring",
            Self::ClusterStatus => "3. Cluster Health Status",
            Self::TaskMon => "4. Task Management",
            Self::ElasticConsole => "5. Elastic Console",
            Self::DiscoverTab => "6. Kibana Discover",
            Self::Finish => "Ready to Roll!",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: WizardStep,
    pub name: String,
    pub host: String,
    pub username: String,
    pub password: String,
    pub error_msg: Option<String>,
    pub add_success: bool,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            step: WizardStep::Welcome,
            name: "Local Dev".to_string(),
            host: "http://127.0.0.1:9200".to_string(),
            username: "elastic".to_string(),
            password: "".to_string(),
            error_msg: None,
            add_success: false,
        }
    }
}

pub fn render_wizard_overlay(
    ctx: &egui::Context,
    state: &mut WizardState,
    cluster_manager: &mut ClusterManager,
    current_tab: &mut Tab,
    toasts: &mut Toasts,
    on_dismiss: &mut bool, // set to true if completed or skipped
) {
    // Large centered modal dialog
    egui::Window::new("🚀 Quick Start Onboarding Wizard")
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .fill(Theme::bg_card())
                .stroke(Stroke::new(1.5, Theme::accent()))
                .corner_radius(Theme::CARD_ROUNDING)
                .inner_margin(egui::Margin::same(24))
        )
        .show(ctx, |ui| {
            ui.set_width(520.0);
            ui.set_height(400.0);

            // Title block
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(state.step.title())
                        .size(24.0)
                        .strong()
                        .color(Theme::accent()),
                );
                ui.add_space(4.0);
                
                // Progress indicators (circles)
                ui.horizontal(|ui| {
                    let total_steps = 8;
                    let current_idx = state.step.index();
                    let spacing = 6.0;
                    let size = 8.0;

                    ui.allocate_ui(egui::Vec2::new((size + spacing) * total_steps as f32, size), |ui| {
                        let (_, rect) = ui.allocate_space(egui::Vec2::new((size + spacing) * total_steps as f32, size));
                        for i in 0..total_steps {
                            let center = rect.min + egui::Vec2::new(
                                size / 2.0 + i as f32 * (size + spacing),
                                size / 2.0,
                            );
                            let color = if i <= current_idx {
                                Theme::accent()
                            } else {
                                Theme::border()
                            };
                            ui.painter().circle_filled(center, size / 2.0, color);
                        }
                    });
                });
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(16.0);

            // Step Content Area (inside a scrollable or fixed area)
            ui.allocate_ui(egui::Vec2::new(ui.available_width(), 200.0), |ui| {
                match state.step {
                    WizardStep::Welcome => {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("👾").size(48.0));
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new("Your sleek, high-performance immediate-mode desktop client for mastering Elasticsearch and Kibana clusters is ready.")
                                    .size(13.0)
                                    .color(Theme::text_primary())
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("This quick 2-minute tour will show you around the core modules and help connect your very first cluster. Let's make monitoring feel premium!")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::AddCluster => {
                        *current_tab = Tab::Clusters;
                        let existing_clusters = cluster_manager.clusters().len();

                        if existing_clusters > 0 {
                            ui.vertical_centered(|ui| {
                                ui.label(egui::RichText::new("✅").size(36.0));
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(format!("Perfect! You already have {} cluster(s) configured.", existing_clusters))
                                        .strong()
                                        .color(Theme::success())
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("You can continue the tour or add another cluster right here in the background.")
                                        .size(11.0)
                                        .color(Theme::text_muted())
                                );
                            });
                        } else {
                            ui.label(
                                egui::RichText::new("First, let's connect your first Elasticsearch cluster:")
                                    .strong()
                                    .color(Theme::text_secondary())
                            );
                            ui.add_space(8.0);

                            egui::Grid::new("wizard_add_cluster_grid")
                                .num_columns(2)
                                .spacing([8.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Cluster Name:");
                                    ui.text_edit_singleline(&mut state.name);
                                    ui.end_row();

                                    ui.label("ES Host URL:");
                                    ui.text_edit_singleline(&mut state.host);
                                    ui.end_row();

                                    ui.label("Username:");
                                    ui.text_edit_singleline(&mut state.username);
                                    ui.end_row();

                                    ui.label("Password:");
                                    ui.add(egui::TextEdit::singleline(&mut state.password).password(true));
                                    ui.end_row();
                                });

                            ui.add_space(8.0);

                            if state.add_success {
                                ui.label(
                                    egui::RichText::new("🎉 Cluster added successfully! Hit Next to continue.")
                                        .color(Theme::success())
                                        .strong()
                                );
                            } else {
                                if let Some(err) = &state.error_msg {
                                    ui.label(
                                        egui::RichText::new(format!("⚠️ Add Error: {}", err))
                                            .color(Theme::danger())
                                            .strong()
                                    );
                                }
                                
                                let add_btn = egui::Button::new(egui::RichText::new("➕ Add & Save Cluster").color(Color32::WHITE)).fill(Theme::accent());
                                if ui.add(add_btn).clicked() {
                                    let cfg = ClusterConfig {
                                        name: state.name.trim().to_string(),
                                        host: state.host.trim().to_string(),
                                        username: state.username.trim().to_string(),
                                        ..Default::default()
                                    };

                                    if cfg.name.is_empty() || cfg.host.is_empty() {
                                        state.error_msg = Some("Name and Host cannot be empty!".to_string());
                                    } else {
                                        match cluster_manager.add_cluster(cfg, Some(state.password.as_str())) {
                                            Ok(_) => {
                                                state.add_success = true;
                                                state.error_msg = None;
                                                toasts.info("First cluster successfully configured!");
                                            }
                                            Err(e) => {
                                                state.error_msg = Some(e.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    WizardStep::SnapshotMon => {
                        *current_tab = Tab::Snapshot;
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📸").size(36.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Track and monitor snapshots instantly.")
                                    .size(13.0)
                                    .strong()
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("The Snapshot tab gives you premium card-based status feeds of ongoing backups, real-time throughput metrics, accurate ETA bars, and direct SLM policy triggers.")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::ClusterStatus => {
                        *current_tab = Tab::Status;
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📊").size(36.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("A Cross-cluster health dashboard.")
                                    .size(13.0)
                                    .strong()
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Inspect real-time health states (Green/Yellow/Red), shard allocation charts, physical store metrics, document volumes, and detailed, tolerant JVM memory stats.")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::TaskMon => {
                        *current_tab = Tab::Tasks;
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("📋").size(36.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Keep long-running queries and tasks in check.")
                                    .size(13.0)
                                    .strong()
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("The Task Monitoring module renders a clear grid of running background operations. From here, you can instantly search tasks or terminate runaway queries via the Cancel button.")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::ElasticConsole => {
                        *current_tab = Tab::Console;
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("💻").size(36.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("A powerful request workspace with over 40+ presets.")
                                    .size(13.0)
                                    .strong()
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Execute raw HTTP commands on your cluster or Kibana host. Features automatic target host toggling, documentation shortcuts, interpolation variables, command history cycling, and custom query saves.")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::DiscoverTab => {
                        *current_tab = Tab::Discover;
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("🔍").size(36.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Kibana-like Discover right inside your desktop GUI.")
                                    .size(13.0)
                                    .strong()
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Query index patterns (e.g. logstash-*), build queries with KQL/Lucene terms, select custom columns recursively extracted from mappings, and view pretty-printed detail drawers.")
                                    .size(11.0)
                                    .color(Theme::text_muted())
                            );
                        });
                    }

                    WizardStep::Finish => {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("🌟").size(48.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("You are fully onboarded and ready to dominate!")
                                    .size(14.0)
                                    .strong()
                                    .color(Theme::success())
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("Pro Tip: Head over to the Appearance tab and turn on the animated Mesh Background or custom glow shaders to elevate the app's visuals!")
                                    .size(11.0)
                                    .color(Theme::text_secondary())
                            );
                        });
                    }
                }
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(12.0);

            // Controls Footer
            ui.horizontal(|ui| {
                // Back Button
                if let Some(prev) = state.step.prev() {
                    if ui.button("◀ Back").clicked() {
                        state.step = prev;
                        state.add_success = false;
                        state.error_msg = None;
                    }
                } else {
                    // Grayed placeholder
                    ui.add_enabled(false, egui::Button::new("◀ Back"));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(next) = state.step.next() {
                        // Next / Skip
                        let next_btn_text = if state.step == WizardStep::AddCluster && cluster_manager.clusters().is_empty() {
                            "Skip & Add Later ➔"
                        } else {
                            "Next ➔"
                        };

                        let next_btn = egui::Button::new(egui::RichText::new(next_btn_text).color(Color32::WHITE)).fill(Theme::accent());
                        if ui.add(next_btn).clicked() {
                            state.step = next;
                            state.add_success = false;
                            state.error_msg = None;
                        }
                    } else {
                        // Finish Button
                        let finish_btn = egui::Button::new(egui::RichText::new("Launch App 🚀").color(Color32::WHITE)).fill(Theme::success());
                        if ui.add(finish_btn).clicked() {
                            *on_dismiss = true;
                        }
                    }

                    // Skip Tour button (always visible except on last step)
                    if state.step != WizardStep::Finish {
                        if ui.button("Skip Tour").clicked() {
                            *on_dismiss = true;
                        }
                    }
                });
            });
        });
}
