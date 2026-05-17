use std::time::{Duration, Instant};

use egui::{Align2, Color32, Id, Vec2};

const TOAST_DURATION: Duration = Duration::from_secs(5);
const FADE_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warn,
    Error,
}

impl ToastLevel {
    pub fn color(&self) -> Color32 {
        match self {
            ToastLevel::Info => Color32::from_rgb(56, 189, 248),
            ToastLevel::Warn => Color32::from_rgb(234, 179, 8),
            ToastLevel::Error => Color32::from_rgb(239, 68, 68),
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            ToastLevel::Info => "ℹ",
            ToastLevel::Warn => "⚠",
            ToastLevel::Error => "✖",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created: Instant::now(),
        }
    }

    pub fn alpha(&self) -> f32 {
        let elapsed = self.created.elapsed();
        if elapsed > TOAST_DURATION {
            let fade = (elapsed - TOAST_DURATION).as_secs_f32() / FADE_DURATION.as_secs_f32();
            (1.0 - fade).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() > TOAST_DURATION + FADE_DURATION
    }
}

#[derive(Debug, Clone, Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    pub fn info(&mut self, message: impl Into<String>) {
        self.items.push(Toast::new(message, ToastLevel::Info));
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.items.push(Toast::new(message, ToastLevel::Warn));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.items.push(Toast::new(message, ToastLevel::Error));
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        self.items.retain(|t| !t.is_expired());
        if self.items.is_empty() {
            return;
        }

        let margin = Vec2::new(20.0, 20.0);
        let toast_spacing = 8.0;
        let toast_width = 320.0;

        // Stack from bottom-right
        for (i, toast) in self.items.iter().enumerate() {
            let alpha = toast.alpha();
            if alpha <= 0.0 {
                continue;
            }

            let y_offset = -(margin.y + i as f32 * (56.0 + toast_spacing));

            egui::Area::new(Id::new("toast").with(i))
                .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-margin.x, y_offset))
                .interactable(false)
                .show(ctx, |ui| {
                    let base_color = Color32::from_rgb(30, 41, 59);
                    let bg = Color32::from_rgba_premultiplied(
                        base_color.r(),
                        base_color.g(),
                        base_color.b(),
                        (alpha * 240.0) as u8,
                    );
                    let stroke_color = Color32::from_rgba_premultiplied(
                        toast.level.color().r(),
                        toast.level.color().g(),
                        toast.level.color().b(),
                        (alpha * 255.0) as u8,
                    );
                    let text_color =
                        Color32::from_rgba_premultiplied(248, 250, 252, (alpha * 255.0) as u8);

                    let frame = egui::Frame::new()
                        .fill(bg)
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(Vec2::new(12.0, 10.0))
                        .stroke(egui::Stroke::new(1.0, stroke_color));

                    frame.show(ui, |ui| {
                        ui.set_max_width(toast_width);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(toast.level.icon())
                                    .color(stroke_color)
                                    .size(16.0),
                            );
                            ui.label(
                                egui::RichText::new(&toast.message)
                                    .color(text_color)
                                    .size(13.0),
                            );
                        });
                    });
                });
        }
    }
}
