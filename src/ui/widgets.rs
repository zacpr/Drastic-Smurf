use egui::{Color32, CornerRadius, Mesh, Pos2, Rect, Shape, Stroke, Ui, Vec2, Widget};

use crate::ui::theme::Theme;

pub struct GradientProgressBar {
    progress: f32,
    height: f32,
    width: f32,
}

impl GradientProgressBar {
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            height: 12.0,
            width: 200.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    fn gradient_color(t: f32) -> Color32 {
        let c1 = Theme::PROGRESS_START;
        let c2 = Theme::PROGRESS_MID1;
        let c3 = Theme::PROGRESS_MID2;
        let c4 = Theme::PROGRESS_END;

        let (a, b, local_t) = if t < 0.33 {
            (c1, c2, t / 0.33)
        } else if t < 0.66 {
            (c2, c3, (t - 0.33) / 0.33)
        } else {
            (c3, c4, (t - 0.66) / 0.34)
        };

        Color32::from_rgba_premultiplied(
            (a.r() as f32 * (1.0 - local_t) + b.r() as f32 * local_t) as u8,
            (a.g() as f32 * (1.0 - local_t) + b.g() as f32 * local_t) as u8,
            (a.b() as f32 * (1.0 - local_t) + b.b() as f32 * local_t) as u8,
            (a.a() as f32 * (1.0 - local_t) + b.a() as f32 * local_t) as u8,
        )
    }
}

impl Widget for GradientProgressBar {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(self.width, self.height), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let rounding = CornerRadius::same((self.height / 2.0).round() as u8);
            let track_rect = rect;
            ui.painter().rect_filled(track_rect, rounding, Theme::BG_INPUT);

            let fill_width = track_rect.width() * self.progress;
            if fill_width > 0.0 {
                let fill_rect = Rect::from_min_size(track_rect.min, Vec2::new(fill_width, track_rect.height()));
                let mut mesh = Mesh::default();
                let left = fill_rect.left();
                let right = fill_rect.right();
                let top = fill_rect.top();
                let bottom = fill_rect.bottom();

                let segments = 40;
                for i in 0..segments {
                    let x0 = left + (right - left) * (i as f32 / segments as f32);
                    let x1 = left + (right - left) * ((i + 1) as f32 / segments as f32);
                    let t0 = i as f32 / segments as f32;
                    let t1 = (i + 1) as f32 / segments as f32;
                    let c0 = Self::gradient_color(t0);
                    let c1 = Self::gradient_color(t1);

                    let vi = mesh.vertices.len() as u32;
                    mesh.colored_vertex(Pos2::new(x0, top), c0);
                    mesh.colored_vertex(Pos2::new(x1, top), c1);
                    mesh.colored_vertex(Pos2::new(x1, bottom), c1);
                    mesh.colored_vertex(Pos2::new(x0, bottom), c0);
                    mesh.add_triangle(vi, vi + 1, vi + 2);
                    mesh.add_triangle(vi, vi + 2, vi + 3);
                }
                ui.painter().add(Shape::mesh(mesh));
            }
        }

        response
    }
}

pub struct ConnectionDot {
    connected: bool,
    size: f32,
}

impl ConnectionDot {
    pub fn new(connected: bool) -> Self {
        Self {
            connected,
            size: 10.0,
        }
    }

    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
}

impl Widget for ConnectionDot {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(self.size), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            let color = if self.connected {
                Theme::SUCCESS
            } else {
                Theme::DANGER
            };
            ui.painter().circle_filled(rect.center(), rect.width() / 2.0, color);
        }
        response
    }
}

pub struct StatePill {
    text: String,
    color: Color32,
}

impl StatePill {
    pub fn new(text: impl Into<String>, color: Color32) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

impl Widget for StatePill {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let galley = ui.painter().layout_no_wrap(self.text.clone(), egui::FontId::default(), Theme::TEXT_PRIMARY);
        let padding = Vec2::new(8.0, 4.0);
        let desired_size = galley.size() + padding * 2.0;
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter().rect_filled(rect, CornerRadius::same(4), self.color);
            let text_pos = rect.min + padding;
            ui.painter().galley(text_pos, galley, Theme::TEXT_PRIMARY);
        }
        response
    }
}

pub struct MiniSparkline {
    data: Vec<f64>,
    width: f32,
    height: f32,
    color: Color32,
}

impl MiniSparkline {
    pub fn new(data: Vec<f64>) -> Self {
        Self {
            data,
            width: 120.0,
            height: 30.0,
            color: Theme::ACCENT,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn color(mut self, c: Color32) -> Self {
        self.color = c;
        self
    }
}

impl Widget for MiniSparkline {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(self.width, self.height), egui::Sense::hover());
        if ui.is_rect_visible(rect) && self.data.len() >= 2 {
            let min = self.data.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = if max == min { 1.0 } else { max - min };

            let points: Vec<Pos2> = self.data
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = rect.left() + rect.width() * (i as f32 / (self.data.len() - 1) as f32);
                    let y = rect.bottom() - rect.height() * ((v - min) / range) as f32;
                    Pos2::new(x, y)
                })
                .collect();

            // Draw line
            ui.painter().add(Shape::line(points.clone(), Stroke::new(1.5, self.color)));

            // Draw area under line
            let mut path = points.clone();
            path.push(Pos2::new(rect.right(), rect.bottom()));
            path.push(Pos2::new(rect.left(), rect.bottom()));
            let mut fill_color = self.color;
            fill_color = fill_color.gamma_multiply(0.2);
            ui.painter().add(Shape::convex_polygon(path, fill_color, Stroke::NONE));

            // Dot at latest point
            if let Some(last) = points.last() {
                ui.painter().circle_filled(*last, 3.0, self.color);
            }
        }
        response
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let exp = (bytes as f64).log(1024.0).min(UNITS.len() as f64 - 1.0) as usize;
    let val = bytes as f64 / 1024f64.powi(exp as i32);
    if exp == 0 {
        format!("{} {}", bytes, UNITS[exp])
    } else {
        format!("{:.2} {}", val, UNITS[exp])
    }
}

pub fn human_speed(bps: f64) -> String {
    format!("{}/s", human_bytes(bps as u64))
}

pub fn human_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else if seconds < 86400 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}d {}h", seconds / 86400, (seconds % 86400) / 3600)
    }
}
