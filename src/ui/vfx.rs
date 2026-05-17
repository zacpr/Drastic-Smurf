use egui::{Color32, Context, Pos2, Rect, Vec2};

use crate::core::config::{BackgroundEffect, VfxSettings};
use crate::ui::theme::Theme;

/// Paint the background visual effect behind the main UI.
/// Call this before rendering the central panel.
pub fn paint_background(ctx: &Context, settings: &VfxSettings, rect: Rect) {
    match settings.background_effect {
        BackgroundEffect::None => {}
        BackgroundEffect::Gradient => paint_gradient(ctx, rect, settings),
        BackgroundEffect::Mesh => paint_mesh(ctx, rect, settings),
    }
}

fn paint_gradient(ctx: &Context, rect: Rect, settings: &VfxSettings) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let intensity = settings.background_intensity;
    if intensity <= 0.0 {
        return;
    }

    let time = if settings.reduce_motion {
        0.0
    } else {
        ctx.input(|i| i.time as f32) * settings.animation_speed * 0.1
    };

    let accent = Theme::accent();

    // Create a soft radial-ish gradient by painting a large quad with color variation
    let center = rect.center();
    let t1 = (time.sin() + 1.0) / 2.0;
    let t2 = ((time + 2.0).sin() + 1.0) / 2.0;

    let offset1 = Vec2::new(
        (t1 * 2.0 - 1.0) * rect.width() * 0.3,
        (t2 * 2.0 - 1.0) * rect.height() * 0.3,
    );

    let glow_center = Pos2::new(center.x + offset1.x, center.y + offset1.y);

    let glow_radius = rect.width().min(rect.height()) * 0.6;

    // Paint a soft circle gradient by drawing concentric circles with decreasing alpha
    let steps = 20;
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let radius = glow_radius * (1.0 - f * 0.8);
        let alpha = ((1.0 - f) * intensity * 30.0) as u8;
        let color = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);
        painter.circle_filled(glow_center, radius, color);
    }

    // Second, smaller accent spot
    let offset2 = Vec2::new(
        ((time + std::f32::consts::PI).sin() + 1.0) / 2.0 * 2.0 - 1.0,
        ((time + 1.57).cos() + 1.0) / 2.0 * 2.0 - 1.0,
    ) * rect.width()
        * 0.25;

    let glow_center2 = Pos2::new(center.x + offset2.x, center.y + offset2.y);
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let radius = glow_radius * 0.5 * (1.0 - f * 0.8);
        let alpha = ((1.0 - f) * intensity * 20.0) as u8;
        let color = Color32::from_rgba_premultiplied(
            accent.r().saturating_add(40),
            accent.g().saturating_add(20),
            accent.b(),
            alpha,
        );
        painter.circle_filled(glow_center2, radius, color);
    }
}

fn paint_mesh(ctx: &Context, rect: Rect, settings: &VfxSettings) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let intensity = settings.background_intensity;
    if intensity <= 0.0 {
        return;
    }

    let time = if settings.reduce_motion {
        0.0
    } else {
        ctx.input(|i| i.time as f32) * settings.animation_speed * 0.05
    };
    let accent = Theme::accent();

    // Draw a subtle grid/mesh of lines
    let spacing = 60.0;
    let alpha = (intensity * 25.0) as u8;
    let line_color = Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha);

    // Vertical lines with wave offset
    let cols = (rect.width() / spacing).ceil() as i32 + 2;
    let wave_amp = 15.0 * intensity;

    for i in -1..cols {
        let base_x = rect.min.x + i as f32 * spacing;
        let mut points = Vec::new();
        let rows = (rect.height() / 10.0).ceil() as i32;
        for j in 0..=rows {
            let y = rect.min.y + j as f32 * 10.0;
            let wave = (y * 0.02 + time + i as f32 * 0.5).sin() * wave_amp;
            points.push(Pos2::new(base_x + wave, y));
        }
        if points.len() >= 2 {
            for k in 0..points.len() - 1 {
                painter.line_segment(
                    [points[k], points[k + 1]],
                    egui::Stroke::new(0.5, line_color),
                );
            }
        }
    }

    // Horizontal lines with wave offset
    let rows = (rect.height() / spacing).ceil() as i32 + 2;
    for j in -1..rows {
        let base_y = rect.min.y + j as f32 * spacing;
        let mut points = Vec::new();
        let cols = (rect.width() / 10.0).ceil() as i32;
        for i in 0..=cols {
            let x = rect.min.x + i as f32 * 10.0;
            let wave = (x * 0.02 + time + j as f32 * 0.5).cos() * wave_amp;
            points.push(Pos2::new(x, base_y + wave));
        }
        if points.len() >= 2 {
            for k in 0..points.len() - 1 {
                painter.line_segment(
                    [points[k], points[k + 1]],
                    egui::Stroke::new(0.5, line_color),
                );
            }
        }
    }
}
