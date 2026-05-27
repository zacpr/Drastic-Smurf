use egui::{Color32, Pos2, Rect, Response, Ui, Vec2};

use crate::ui::theme::Theme;

/// Time helper: returns animated time in seconds, scaled by speed.
pub fn anim_time(ui: &Ui, speed: f32) -> f32 {
    ui.ctx().input(|i| i.time as f32) * speed
}

/// A shimmer effect for progress bars and other filled regions.
/// Draws a diagonal light band that sweeps across the rect.
pub fn shimmer_overlay(ui: &Ui, rect: Rect, intensity: f32, speed: f32) {
    if intensity <= 0.0 {
        return;
    }
    let time = anim_time(ui, speed);
    let period = 2.5f32;
    let t = (time % period) / period;

    let band_width = rect.width() * 0.4;
    let x = rect.min.x - band_width + (rect.width() + band_width * 2.0) * t;

    let gradient_rect = Rect::from_min_max(
        Pos2::new(x, rect.min.y),
        Pos2::new(x + band_width, rect.max.y),
    );

    let painter = ui.painter();
    let base = Theme::accent();
    let shine = Color32::from_rgba_premultiplied(
        base.r().saturating_add(80),
        base.g().saturating_add(80),
        base.b().saturating_add(80),
        (80.0 * intensity) as u8,
    );

    painter.rect_filled(
        gradient_rect.intersect(rect),
        egui::CornerRadius::same(0),
        shine,
    );
}

/// A pulsing glow around a rect, useful for "in progress" indicators.
#[allow(dead_code)]
pub fn pulse_glow(ui: &Ui, rect: Rect, color: Color32, speed: f32) {
    let time = anim_time(ui, speed);
    let pulse = (time.sin() + 1.0) / 2.0; // 0..1
    let alpha = (pulse * 60.0) as u8;
    let glow_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha);

    let expand = 2.0 + pulse * 3.0;
    let glow_rect = rect.expand(expand);
    ui.painter().rect_stroke(
        glow_rect,
        egui::CornerRadius::same(6),
        egui::Stroke::new(1.5, glow_color),
        egui::StrokeKind::Middle,
    );
}

/// Hover lift effect: when the response is hovered, offset the content slightly
/// and draw a shadow/glow. Returns the offset to apply to child widgets.
#[allow(dead_code)]
pub fn hover_lift(ui: &Ui, response: &Response, amount: f32) -> Vec2 {
    let hovered = response.hovered();
    let ctx = ui.ctx();
    let id = response.id;

    let target = if hovered { amount } else { 0.0 };
    let current = ctx.animate_value_with_time(id, target, 0.15);

    Vec2::new(0.0, -current)
}

/// Draw a card with optional hover lift and subtle glow.
/// Returns the inner response so callers can place content inside.
#[allow(dead_code)]
pub fn animated_card<R>(
    ui: &mut Ui,
    id_salt: &str,
    hover_effects: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::new()
        .fill(Theme::bg_card())
        .corner_radius(Theme::CARD_ROUNDING)
        .inner_margin(Theme::CARD_PADDING)
        .stroke(egui::Stroke::new(1.0, Theme::bg_input()));

    if !hover_effects {
        return frame.show(ui, add_contents).inner;
    }

    // Allocate the card area first so we can detect hover
    let id = ui.id().with(id_salt);
    let desired = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());

    let lift = hover_lift(ui, &response, 2.0);
    let glow_alpha = ui.ctx().animate_value_with_time(
        id.with("glow"),
        if response.hovered() { 0.08 } else { 0.0 },
        0.15,
    );

    if glow_alpha > 0.0 {
        let glow_rect = rect.expand(1.0);
        let accent = Theme::accent();
        let glow_color = Color32::from_rgba_premultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            (glow_alpha * 255.0) as u8,
        );
        ui.painter().rect_stroke(
            glow_rect,
            Theme::CARD_ROUNDING,
            egui::Stroke::new(1.0, glow_color),
            egui::StrokeKind::Middle,
        );
    }

    // Place the actual frame inside the rect with lift offset
    let mut child_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.translate(lift))
            .layout(*ui.layout()),
    );
    frame.show(&mut child_ui, add_contents).inner
}

/// An animated progress bar with optional shimmer.
#[allow(dead_code)]
pub fn animated_progress_bar(
    ui: &mut Ui,
    fraction: f32,
    shimmer: bool,
    shimmer_speed: f32,
    bar_height: f32,
) -> Response {
    let available = ui.available_size_before_wrap();
    let desired = Vec2::new(available.x.max(100.0), bar_height);
    let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::hover());

    if !ui.is_rect_visible(rect) {
        return response;
    }

    let painter = ui.painter();
    let rounding = egui::CornerRadius::same((bar_height / 2.0) as u8);

    // Background track
    painter.rect_filled(rect, rounding, Theme::bg_input());

    // Progress fill with gradient
    let fill_width = rect.width() * fraction.clamp(0.0, 1.0);
    if fill_width > 0.0 {
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height()));

        // Simple gradient: start -> mid1 -> end based on fraction
        let colors = [
            Theme::progress_start(),
            Theme::progress_mid1(),
            Theme::progress_mid2(),
            Theme::progress_end(),
        ];
        let color_idx = ((fraction * (colors.len() - 1) as f32) as usize).min(colors.len() - 2);
        let t = (fraction * (colors.len() - 1) as f32) - color_idx as f32;
        let start_c = colors[color_idx];
        let end_c = colors[color_idx + 1];
        let fill_color = Color32::from_rgb(
            (start_c.r() as f32 * (1.0 - t) + end_c.r() as f32 * t) as u8,
            (start_c.g() as f32 * (1.0 - t) + end_c.g() as f32 * t) as u8,
            (start_c.b() as f32 * (1.0 - t) + end_c.b() as f32 * t) as u8,
        );

        painter.rect_filled(fill_rect, rounding, fill_color);

        // Shimmer overlay
        if shimmer {
            shimmer_overlay(ui, fill_rect, 0.5, shimmer_speed);
        }
    }

    response
}

/// A dot that pulses in size/opacity, useful for live indicators.
#[allow(dead_code)]
pub fn pulsing_dot(ui: &mut Ui, color: Color32, speed: f32) -> Response {
    let base_radius = 5.0;
    let time = anim_time(ui, speed);
    let pulse = (time.sin() + 1.0) / 2.0;
    let radius = base_radius + pulse * 2.0;
    let alpha = 180 + (pulse * 75.0) as u8;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(base_radius * 2.0 + 4.0), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let dot_color = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha);
        ui.painter().circle_filled(center, radius, dot_color);
    }

    response
}
