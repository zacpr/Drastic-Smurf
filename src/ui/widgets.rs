use egui::{Color32, CornerRadius, Mesh, Pos2, Rect, Shape, Stroke, Ui, Vec2, Widget};

use crate::ui::animations::shimmer_overlay;
use crate::ui::theme::Theme;

pub struct GradientProgressBar {
    progress: f32,
    height: f32,
    width: f32,
    shimmer: bool,
}

impl GradientProgressBar {
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            height: 12.0,
            width: 200.0,
            shimmer: false,
        }
    }

    pub fn shimmer(mut self, on: bool) -> Self {
        self.shimmer = on;
        self
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
        let c1 = Theme::progress_start();
        let c2 = Theme::progress_mid1();
        let c3 = Theme::progress_mid2();
        let c4 = Theme::progress_end();

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
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(self.width, self.height), egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let rounding = CornerRadius::same((self.height / 2.0).round() as u8);
            let track_rect = rect;
            ui.painter()
                .rect_filled(track_rect, rounding, Theme::bg_input());

            let fill_width = track_rect.width() * self.progress;
            if fill_width > 0.0 {
                let fill_rect =
                    Rect::from_min_size(track_rect.min, Vec2::new(fill_width, track_rect.height()));
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

                if self.shimmer {
                    shimmer_overlay(ui, fill_rect, 0.5, 1.0);
                }
            }
        }

        response
    }
}

pub struct ConnectionDot {
    connected: bool,
    color_override: Option<Color32>,
    size: f32,
}

impl ConnectionDot {
    pub fn new(connected: bool) -> Self {
        Self {
            connected,
            color_override: None,
            size: 10.0,
        }
    }

    pub fn color(mut self, c: Color32) -> Self {
        self.color_override = Some(c);
        self
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
            let color = if let Some(c) = self.color_override {
                c
            } else if self.connected {
                Theme::success()
            } else {
                Theme::danger()
            };
            ui.painter()
                .circle_filled(rect.center(), rect.width() / 2.0, color);
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
        let text_color = Theme::contrast_text_color(self.color);
        let galley = ui.painter().layout_no_wrap(
            self.text.clone(),
            egui::FontId::default(),
            text_color,
        );
        let padding = Vec2::new(8.0, 4.0);
        let desired_size = galley.size() + padding * 2.0;
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), self.color);
            let text_pos = rect.min + padding;
            ui.painter().galley(text_pos, galley, text_color);
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
            color: Theme::accent(),
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

    #[allow(dead_code)]
    pub fn color(mut self, c: Color32) -> Self {
        self.color = c;
        self
    }
}

impl Widget for MiniSparkline {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(self.width, self.height), egui::Sense::hover());
        if ui.is_rect_visible(rect) && self.data.len() >= 2 {
            let min = self.data.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = self.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = if max == min { 1.0 } else { max - min };

            let points: Vec<Pos2> = self
                .data
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = rect.left() + rect.width() * (i as f32 / (self.data.len() - 1) as f32);
                    let y = rect.bottom() - rect.height() * ((v - min) / range) as f32;
                    Pos2::new(x, y)
                })
                .collect();

            // Draw line
            ui.painter()
                .add(Shape::line(points.clone(), Stroke::new(1.5, self.color)));

            // Draw area under line
            let mut path = points.clone();
            path.push(Pos2::new(rect.right(), rect.bottom()));
            path.push(Pos2::new(rect.left(), rect.bottom()));
            let mut fill_color = self.color;
            fill_color = fill_color.gamma_multiply(0.2);
            ui.painter()
                .add(Shape::convex_polygon(path, fill_color, Stroke::NONE));

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

pub fn human_nanos(nanos: u64) -> String {
    let micros = nanos / 1000;
    if micros < 1000 {
        format!("{}µs", micros)
    } else {
        let millis = micros / 1000;
        if millis < 1000 {
            format!("{}ms", millis)
        } else {
            let secs = millis / 1000;
            human_duration(secs)
        }
    }
}

pub fn human_docs(count: u64) -> String {
    const SI_PREFIXES: &[(&str, f64)] = &[
        ("", 1.0),
        ("K", 1_000.0),
        ("M", 1_000_000.0),
        ("B", 1_000_000_000.0),
        ("T", 1_000_000_000_000.0),
    ];
    if count == 0 {
        return "0".to_string();
    }
    let mut idx = 0;
    for (i, &(_, threshold)) in SI_PREFIXES.iter().enumerate() {
        if (count as f64) >= threshold {
            idx = i;
        } else {
            break;
        }
    }
    let scaled = count as f64 / SI_PREFIXES[idx].1;
    if idx == 0 {
        format!("{}", count)
    } else {
        format!("{:.1}{} ({})", scaled, SI_PREFIXES[idx].0, count)
    }
}

pub struct WarningLight {
    pub status: String,
}

impl WarningLight {
    pub fn new(status: impl Into<String>) -> Self {
        Self {
            status: status.into(),
        }
    }
}

impl Widget for WarningLight {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let desired_size = Vec2::new(45.0, 48.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center_x = rect.center().x;
            let base_y = rect.bottom() - 4.0;
            
            // 1. Draw soft pulsing glow under the dome if active
            let glow_color = match self.status.as_str() {
                "green" => Some(Color32::from_rgba_premultiplied(46, 125, 50, 15)),
                "yellow" => Some(Color32::from_rgba_premultiplied(235, 179, 41, 15)),
                "red" => Some(Color32::from_rgba_premultiplied(229, 57, 53, 15)),
                _ => None,
            };
            
            if let Some(gc) = glow_color {
                let time = ui.input(|i| i.time);
                let pulse = (time * 3.5).sin() as f32 * 2.5 + 16.0;
                painter.circle_filled(Pos2::new(center_x, base_y - 12.0), pulse, gc);
                painter.circle_filled(Pos2::new(center_x, base_y - 12.0), pulse * 0.6, gc.linear_multiply(2.0));
            }

            // 2. Base plate
            let base_rect_stroke = Rect::from_min_max(
                Pos2::new(center_x - 16.5, base_y - 3.0),
                Pos2::new(center_x + 16.5, base_y + 3.0),
            );
            let base_rect_fill = Rect::from_min_max(
                Pos2::new(center_x - 15.5, base_y - 2.0),
                Pos2::new(center_x + 15.5, base_y + 2.0),
            );
            painter.rect_filled(base_rect_stroke, CornerRadius::same(2), Color32::from_rgb(70, 70, 72));
            painter.rect_filled(base_rect_fill, CornerRadius::same(2), Color32::from_rgb(50, 50, 52));

            // 3. Inner filament bulb (if offline)
            if self.status == "offline" {
                let bulb_center = Pos2::new(center_x, base_y - 12.0);
                painter.line_segment(
                    [Pos2::new(center_x, base_y - 3.0), Pos2::new(center_x, base_y - 9.0)],
                    Stroke::new(1.0, Color32::from_rgb(100, 100, 100))
                );
                painter.circle_stroke(bulb_center, 2.0, Stroke::new(1.0, Color32::from_rgb(140, 140, 140)));
            }

            // 4. Dome Lens
            let dome_color = match self.status.as_str() {
                "green" => Color32::from_rgba_premultiplied(46, 125, 50, 140),
                "yellow" => Color32::from_rgba_premultiplied(235, 179, 41, 140),
                "red" => Color32::from_rgba_premultiplied(229, 57, 53, 140),
                _ => Color32::from_rgba_premultiplied(80, 80, 85, 45),
            };

            let dome_rect = Rect::from_min_max(
                Pos2::new(center_x - 12.0, base_y - 25.0),
                Pos2::new(center_x + 12.0, base_y - 3.0),
            );
            let dome_rounding = CornerRadius {
                nw: 12,
                ne: 12,
                se: 0,
                sw: 0,
            };
            painter.rect_filled(dome_rect, dome_rounding, dome_color);

            // 5. Fresnel ribs
            let rib_stroke = Stroke::new(0.8, match self.status.as_str() {
                "green" => Color32::from_rgba_premultiplied(100, 220, 110, 80),
                "yellow" => Color32::from_rgba_premultiplied(255, 230, 100, 80),
                "red" => Color32::from_rgba_premultiplied(255, 120, 100, 80),
                _ => Color32::from_rgba_premultiplied(130, 130, 135, 40),
            });
            for y_offset in [5.0, 10.0, 15.0, 20.0] {
                let y = base_y - y_offset;
                let pct = (y_offset - 3.0) / 25.0;
                let half_w = 12.0 * (1.0 - pct * pct).max(0.0).sqrt();
                painter.line_segment(
                    [Pos2::new(center_x - half_w, y), Pos2::new(center_x + half_w, y)],
                    rib_stroke
                );
            }

            // 6. Protective Metal Cage
            let cage_stroke = Stroke::new(1.2, Color32::from_rgb(110, 110, 115));
            
            // Left curved bar
            let left_points = vec![
                Pos2::new(center_x - 13.0, base_y - 3.0),
                Pos2::new(center_x - 13.0, base_y - 13.0),
                Pos2::new(center_x - 12.0, base_y - 20.0),
                Pos2::new(center_x - 8.0, base_y - 25.0),
                Pos2::new(center_x, base_y - 26.5),
            ];
            painter.add(Shape::line(left_points, cage_stroke));
            
            // Right curved bar
            let right_points = vec![
                Pos2::new(center_x + 13.0, base_y - 3.0),
                Pos2::new(center_x + 13.0, base_y - 13.0),
                Pos2::new(center_x + 12.0, base_y - 20.0),
                Pos2::new(center_x + 8.0, base_y - 25.0),
                Pos2::new(center_x, base_y - 26.5),
            ];
            painter.add(Shape::line(right_points, cage_stroke));

            // Center vertical bar
            painter.line_segment(
                [Pos2::new(center_x, base_y - 3.0), Pos2::new(center_x, base_y - 26.5)],
                cage_stroke
            );

            // Horizontal reinforcing rings
            let ring_y1 = base_y - 9.0;
            let ring_y2 = base_y - 18.0;
            painter.line_segment([Pos2::new(center_x - 13.1, ring_y1), Pos2::new(center_x + 13.1, ring_y1)], cage_stroke);
            painter.line_segment([Pos2::new(center_x - 12.7, ring_y2), Pos2::new(center_x + 12.7, ring_y2)], cage_stroke);
        }

        response
    }
}

pub fn json_layouter(ui: &Ui, text: &str, wrap_width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = json_highlight(ui, text);
    job.wrap.max_width = wrap_width;
    ui.fonts(|f| f.layout_job(job))
}

pub fn json_highlight(_ui: &Ui, text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let trimmed = text.trim_start();
    
    // Check if the output is JSON
    let is_json = trimmed.starts_with('{') || trimmed.starts_with('[');
    
    if is_json {
        // --- JSON MODE ---
        let key_color = Color32::from_rgb(125, 211, 252);     // Cyan/Blue (Sky 300)
        let string_color = Color32::from_rgb(251, 146, 60);  // Amber/Orange (Amber 400)
        let number_color = Color32::from_rgb(74, 222, 128);  // Green (Green 400)
        let bool_color = Color32::from_rgb(251, 113, 133);   // Rose/Pink (Rose 400)
        let null_color = Color32::from_rgb(192, 132, 252);   // Purple/Lavender (Purple 400)
        let punc_color = Color32::from_rgb(156, 163, 175);   // Border Gray (Gray 400)
        let comment_color = Color32::from_rgb(107, 114, 128); // Muted Dark Gray (Gray 500)
        let text_color = Theme::text_primary();              // Default text
        
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let font_id = egui::FontId::monospace(11.0);
        
        while i < chars.len() {
            // Handle line comments (//)
            if chars[i] == '/' && i + 1 < chars.len() && chars[i+1] == '/' {
                let start = i;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                let slice: String = chars[start..i].iter().collect();
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: comment_color, ..Default::default() });
                continue;
            }
            
            // Handle shell comments (#)
            if chars[i] == '#' {
                let start = i;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                let slice: String = chars[start..i].iter().collect();
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: comment_color, ..Default::default() });
                continue;
            }

            // Handle string literals
            if chars[i] == '"' {
                let start = i;
                i += 1; // skip opening quote
                let mut escaped = false;
                while i < chars.len() {
                    if escaped {
                        escaped = false;
                    } else if chars[i] == '\\' {
                        escaped = true;
                    } else if chars[i] == '"' {
                        i += 1; // include closing quote
                        break;
                    }
                    i += 1;
                }
                let slice: String = chars[start..i].iter().collect();
                
                // Check if this string is a JSON key (followed by some spaces and a colon ':')
                let mut is_key = false;
                let mut peek = i;
                while peek < chars.len() && chars[peek].is_whitespace() {
                    peek += 1;
                }
                if peek < chars.len() && chars[peek] == ':' {
                    is_key = true;
                }
                
                let color = if is_key {
                    key_color
                } else {
                    let inner = slice.trim_matches('"').to_lowercase();
                    match inner.as_str() {
                        "green" | "active" | "success" | "ok" => Theme::success(),
                        "yellow" | "warning" | "pending" => Theme::warning(),
                        "red" | "danger" | "failed" | "error" => Theme::danger(),
                        _ => string_color,
                    }
                };
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color, ..Default::default() });
                continue;
            }
            
            // Handle numbers
            if chars[i].is_ascii_digit() || (chars[i] == '-' && i + 1 < chars.len() && chars[i+1].is_ascii_digit()) {
                let start = i;
                if chars[i] == '-' {
                    i += 1;
                }
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let slice: String = chars[start..i].iter().collect();
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: number_color, ..Default::default() });
                continue;
            }
            
            // Handle keywords (true, false, null)
            if chars[i].is_alphabetic() {
                let start = i;
                while i < chars.len() && chars[i].is_alphanumeric() {
                    i += 1;
                }
                let slice: String = chars[start..i].iter().collect();
                let color = match slice.as_str() {
                    "true" | "false" => bool_color,
                    "null" => null_color,
                    _ => text_color,
                };
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color, ..Default::default() });
                continue;
            }
            
            // Handle punctuation/brackets
            if "{}[],:".contains(chars[i]) {
                let slice = chars[i].to_string();
                i += 1;
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: punc_color, ..Default::default() });
                continue;
            }
            
            // Default white-space or unstyled character
            let slice = chars[i].to_string();
            i += 1;
            job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: text_color, ..Default::default() });
        }
    } else {
        // --- TABULAR / CAT API MODE ---
        let header_color = Color32::from_rgb(147, 197, 253);  // Sky Blue for headers
        let success_color = Theme::success();
        let warning_color = Theme::warning();
        let danger_color = Theme::danger();
        let number_color = Color32::from_rgb(74, 222, 128);   // Vibrant Green
        let text_color = Theme::text_primary();
        
        let font_id = egui::FontId::monospace(11.0);
        
        let lines: Vec<&str> = text.split('\n').collect();
        for (line_idx, line) in lines.iter().enumerate() {
            let is_header = line_idx == 0 && lines.len() > 1 && !line.trim().is_empty();
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            
            while i < chars.len() {
                // Headers get bold accent color
                if is_header && !chars[i].is_whitespace() {
                    let start = i;
                    while i < chars.len() && !chars[i].is_whitespace() {
                        i += 1;
                    }
                    let slice: String = chars[start..i].iter().collect();
                    job.append(&slice, 0.0, egui::TextFormat {
                        font_id: font_id.clone(),
                        color: header_color,
                        ..Default::default()
                    });
                    continue;
                }
                
                // Numbers & quantities (sizes, percentages, unit values)
                if chars[i].is_ascii_digit() || (chars[i] == '-' && i + 1 < chars.len() && chars[i+1].is_ascii_digit()) {
                    let start = i;
                    if chars[i] == '-' {
                        i += 1;
                    }
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '%' || chars[i].is_alphabetic()) {
                        i += 1;
                    }
                    let slice: String = chars[start..i].iter().collect();
                    job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: number_color, ..Default::default() });
                    continue;
                }
                
                // Status indicator keywords
                if chars[i].is_alphabetic() {
                    let start = i;
                    while i < chars.len() && chars[i].is_alphanumeric() {
                        i += 1;
                    }
                    let slice: String = chars[start..i].iter().collect();
                    let color = match slice.to_lowercase().as_str() {
                        "green" | "active" | "success" | "ok" => success_color,
                        "yellow" | "warning" | "pending" => warning_color,
                        "red" | "danger" | "failed" | "error" => danger_color,
                        _ => text_color,
                    };
                    job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color, ..Default::default() });
                    continue;
                }
                
                let slice = chars[i].to_string();
                i += 1;
                job.append(&slice, 0.0, egui::TextFormat { font_id: font_id.clone(), color: text_color, ..Default::default() });
            }
            
            if line_idx < lines.len() - 1 {
                job.append("\n", 0.0, egui::TextFormat { font_id: font_id.clone(), color: text_color, ..Default::default() });
            }
        }
    }
    
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_duration() {
        assert_eq!(human_duration(0), "0s");
        assert_eq!(human_duration(45), "45s");
        assert_eq!(human_duration(125), "2m 5s");
        assert_eq!(human_duration(3665), "1h 1m");
        assert_eq!(human_duration(90065), "1d 1h");
    }

    #[test]
    fn test_human_nanos() {
        assert_eq!(human_nanos(500), "0µs");
        assert_eq!(human_nanos(5000), "5µs");
        assert_eq!(human_nanos(5_000_000), "5ms");
        assert_eq!(human_nanos(5_000_000_000), "5s");
    }

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1024), "1.00 KB");
        assert_eq!(human_bytes(1_500_000), "1.43 MB");
    }

    #[test]
    fn test_human_docs() {
        assert_eq!(human_docs(0), "0");
        assert_eq!(human_docs(500), "500");
        assert_eq!(human_docs(1500), "1.5K (1500)");
    }
}
