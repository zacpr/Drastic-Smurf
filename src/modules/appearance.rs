use egui::Ui;

use crate::core::config::{BackgroundEffect, VfxSettings};
use crate::ui::theme::{AppTheme, Theme};

#[derive(Debug, Clone, Default)]
pub struct AppearanceState {
    pub selected_preset: String,
    pub show_advanced: bool,
}

pub fn render_appearance_module(
    ui: &mut Ui,
    state: &mut AppearanceState,
    theme: &mut AppTheme,
    vfx: &mut VfxSettings,
    on_theme_changed: &mut bool,
    on_vfx_changed: &mut bool,
) {
    ui.heading("Appearance");
    ui.add_space(16.0);

    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.heading("Theme");
            ui.add_space(8.0);

            // Preset selector
            ui.horizontal(|ui| {
                ui.label("Preset:");
                let presets = AppTheme::all_presets();
                let preset_names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();

                let mut selected_idx = presets
                    .iter()
                    .position(|p| p.name == state.selected_preset)
                    .unwrap_or(0);
                let prev_idx = selected_idx;

                egui::ComboBox::from_id_salt("theme_preset")
                    .selected_text(&state.selected_preset)
                    .show_ui(ui, |ui| {
                        for (idx, name) in preset_names.iter().enumerate() {
                            ui.selectable_value(&mut selected_idx, idx, name);
                        }
                    });

                if selected_idx != prev_idx {
                    state.selected_preset = preset_names[selected_idx].clone();
                    *theme = presets[selected_idx].clone();
                    Theme::set(theme.clone());
                    *on_theme_changed = true;
                }
            });

            ui.add_space(8.0);

            // Advanced color picker
            ui.checkbox(&mut state.show_advanced, "Show Advanced Colors");
            if state.show_advanced {
                ui.add_space(4.0);
                color_picker_row(ui, "Darkest BG", &mut theme.bg_darkest, on_theme_changed);
                color_picker_row(ui, "Dark BG", &mut theme.bg_dark, on_theme_changed);
                color_picker_row(ui, "Card BG", &mut theme.bg_card, on_theme_changed);
                color_picker_row(ui, "Input BG", &mut theme.bg_input, on_theme_changed);
                color_picker_row(
                    ui,
                    "Primary Text",
                    &mut theme.text_primary,
                    on_theme_changed,
                );
                color_picker_row(
                    ui,
                    "Secondary Text",
                    &mut theme.text_secondary,
                    on_theme_changed,
                );
                color_picker_row(ui, "Muted Text", &mut theme.text_muted, on_theme_changed);
                color_picker_row(ui, "Accent", &mut theme.accent, on_theme_changed);
                color_picker_row(
                    ui,
                    "Accent Hover",
                    &mut theme.accent_hover,
                    on_theme_changed,
                );
                color_picker_row(ui, "Success", &mut theme.success, on_theme_changed);
                color_picker_row(ui, "Warning", &mut theme.warning, on_theme_changed);
                color_picker_row(ui, "Danger", &mut theme.danger, on_theme_changed);
                color_picker_row(ui, "Info", &mut theme.info, on_theme_changed);
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(16.0);

            // Live preview
            ui.heading("Preview");
            ui.add_space(8.0);
            render_theme_preview(ui, theme);
        });

    ui.add_space(16.0);

    // VFX Settings
    egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .show(ui, |ui| {
            ui.heading("Visual Effects");
            ui.add_space(8.0);

            if ui
                .checkbox(
                    &mut vfx.reduce_motion,
                    "Reduce Motion (disable animations for accessibility/battery)",
                )
                .changed()
            {
                *on_vfx_changed = true;
            }
            ui.add_space(4.0);

            ui.label("Background Effect:");
            ui.horizontal(|ui| {
                if ui
                    .selectable_value(&mut vfx.background_effect, BackgroundEffect::None, "None")
                    .changed()
                {
                    *on_vfx_changed = true;
                }
                if ui
                    .selectable_value(
                        &mut vfx.background_effect,
                        BackgroundEffect::Gradient,
                        "Gradient",
                    )
                    .changed()
                {
                    *on_vfx_changed = true;
                }
                if ui
                    .selectable_value(&mut vfx.background_effect, BackgroundEffect::Mesh, "Mesh")
                    .changed()
                {
                    *on_vfx_changed = true;
                }
            });
            if vfx.background_effect != BackgroundEffect::None {
                if ui
                    .add(
                        egui::Slider::new(&mut vfx.background_intensity, 0.0..=1.0)
                            .text("Intensity"),
                    )
                    .changed()
                {
                    *on_vfx_changed = true;
                }
            }

            ui.add_space(8.0);
            if ui
                .add(
                    egui::Slider::new(&mut vfx.animation_speed, 0.0..=3.0)
                        .text("Animation Speed")
                        .fixed_decimals(1),
                )
                .changed()
            {
                *on_vfx_changed = true;
            }

            if ui
                .checkbox(
                    &mut vfx.hover_effects,
                    "Hover Effects (card lift, button glow)",
                )
                .changed()
            {
                *on_vfx_changed = true;
            }
            if ui
                .checkbox(
                    &mut vfx.shimmer_effects,
                    "Shimmer Effects (progress bar, pulse)",
                )
                .changed()
            {
                *on_vfx_changed = true;
            }
            if ui.checkbox(&mut vfx.cursor_glow, "Cursor Glow").changed() {
                *on_vfx_changed = true;
            }

            if ui
                .add(
                    egui::Slider::new(&mut vfx.parallax_amount, 0.0..=1.0)
                        .text("Parallax Amount")
                        .fixed_decimals(2),
                )
                .changed()
            {
                *on_vfx_changed = true;
            }
        });
}

fn color_picker_row(ui: &mut Ui, label: &str, color: &mut egui::Color32, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut raw = [*color];
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut raw[0],
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                *color = raw[0];
                *changed = true;
                Theme::set(Theme::get());
            }
        });
    });
}

fn render_theme_preview(ui: &mut Ui, theme: &AppTheme) {
    egui::Frame::new()
        .fill(theme.bg_card)
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .stroke(egui::Stroke::new(1.0, theme.accent))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Cluster Name")
                        .strong()
                        .size(14.0)
                        .color(theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("● Connected")
                            .size(12.0)
                            .color(theme.success),
                    );
                });
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Snapshot in progress — 45%")
                    .size(12.0)
                    .color(theme.text_secondary),
            );

            ui.add_space(8.0);

            // Mini progress bar
            let bar_width = ui.available_width().min(300.0);
            let bar_height = 8.0;
            let (rect, _) = ui
                .allocate_exact_size(egui::Vec2::new(bar_width, bar_height), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                painter.rect_filled(rect, egui::CornerRadius::same(4), theme.bg_input);
                let progress = rect.width() * 0.45;
                let progress_rect =
                    egui::Rect::from_min_size(rect.min, egui::Vec2::new(progress, rect.height()));
                painter.rect_filled(progress_rect, egui::CornerRadius::same(4), theme.accent);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Data: 12.4 GB / 27.8 GB")
                        .size(11.0)
                        .color(theme.text_muted),
                );
                ui.label(
                    egui::RichText::new("ETA: 4m 32s")
                        .size(11.0)
                        .color(theme.text_muted),
                );
            });
        });
}
