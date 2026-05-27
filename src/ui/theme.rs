use std::cell::RefCell;

use egui::{Color32, CornerRadius, Vec2};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// --- AppTheme: runtime-configurable color scheme ---

#[derive(Debug, Clone)]
pub struct AppTheme {
    pub name: String,
    pub bg_darkest: Color32,
    pub bg_dark: Color32,
    pub bg_card: Color32,
    pub bg_input: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
    pub snapshot_in_progress: Color32,
    pub snapshot_success: Color32,
    pub snapshot_failed: Color32,
    pub snapshot_partial: Color32,
    pub progress_start: Color32,
    pub progress_mid1: Color32,
    pub progress_mid2: Color32,
    pub progress_end: Color32,
}

impl Default for AppTheme {
    fn default() -> Self {
        Self::slate()
    }
}

// Hex serialization helpers
fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_array();
    if a == 255 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    }
}

fn dehex(s: &str) -> Color32 {
    let s = s.trim_start_matches('#');
    let bytes = hex::decode(s).unwrap_or_default();
    match bytes.len() {
        3 => Color32::from_rgb(bytes[0], bytes[1], bytes[2]),
        4 => Color32::from_rgba_premultiplied(bytes[0], bytes[1], bytes[2], bytes[3]),
        _ => Color32::BLACK,
    }
}

impl Serialize for AppTheme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppTheme", 24)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("bg_darkest", &hex(self.bg_darkest))?;
        s.serialize_field("bg_dark", &hex(self.bg_dark))?;
        s.serialize_field("bg_card", &hex(self.bg_card))?;
        s.serialize_field("bg_input", &hex(self.bg_input))?;
        s.serialize_field("text_primary", &hex(self.text_primary))?;
        s.serialize_field("text_secondary", &hex(self.text_secondary))?;
        s.serialize_field("text_muted", &hex(self.text_muted))?;
        s.serialize_field("accent", &hex(self.accent))?;
        s.serialize_field("accent_hover", &hex(self.accent_hover))?;
        s.serialize_field("success", &hex(self.success))?;
        s.serialize_field("warning", &hex(self.warning))?;
        s.serialize_field("danger", &hex(self.danger))?;
        s.serialize_field("info", &hex(self.info))?;
        s.serialize_field("snapshot_in_progress", &hex(self.snapshot_in_progress))?;
        s.serialize_field("snapshot_success", &hex(self.snapshot_success))?;
        s.serialize_field("snapshot_failed", &hex(self.snapshot_failed))?;
        s.serialize_field("snapshot_partial", &hex(self.snapshot_partial))?;
        s.serialize_field("progress_start", &hex(self.progress_start))?;
        s.serialize_field("progress_mid1", &hex(self.progress_mid1))?;
        s.serialize_field("progress_mid2", &hex(self.progress_mid2))?;
        s.serialize_field("progress_end", &hex(self.progress_end))?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for AppTheme {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            name: String,
            bg_darkest: String,
            bg_dark: String,
            bg_card: String,
            bg_input: String,
            text_primary: String,
            text_secondary: String,
            text_muted: String,
            accent: String,
            accent_hover: String,
            success: String,
            warning: String,
            danger: String,
            info: String,
            snapshot_in_progress: String,
            snapshot_success: String,
            snapshot_failed: String,
            snapshot_partial: String,
            progress_start: String,
            progress_mid1: String,
            progress_mid2: String,
            progress_end: String,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(AppTheme {
            name: raw.name,
            bg_darkest: dehex(&raw.bg_darkest),
            bg_dark: dehex(&raw.bg_dark),
            bg_card: dehex(&raw.bg_card),
            bg_input: dehex(&raw.bg_input),
            text_primary: dehex(&raw.text_primary),
            text_secondary: dehex(&raw.text_secondary),
            text_muted: dehex(&raw.text_muted),
            accent: dehex(&raw.accent),
            accent_hover: dehex(&raw.accent_hover),
            success: dehex(&raw.success),
            warning: dehex(&raw.warning),
            danger: dehex(&raw.danger),
            info: dehex(&raw.info),
            snapshot_in_progress: dehex(&raw.snapshot_in_progress),
            snapshot_success: dehex(&raw.snapshot_success),
            snapshot_failed: dehex(&raw.snapshot_failed),
            snapshot_partial: dehex(&raw.snapshot_partial),
            progress_start: dehex(&raw.progress_start),
            progress_mid1: dehex(&raw.progress_mid1),
            progress_mid2: dehex(&raw.progress_mid2),
            progress_end: dehex(&raw.progress_end),
        })
    }
}

impl AppTheme {
    // ---------- Presets ----------

    pub fn slate() -> Self {
        Self {
            name: "Slate".into(),
            bg_darkest: Color32::from_rgb(15, 23, 42),
            bg_dark: Color32::from_rgb(30, 41, 59),
            bg_card: Color32::from_rgb(30, 41, 59),
            bg_input: Color32::from_rgb(51, 65, 85),
            text_primary: Color32::from_rgb(248, 250, 252),
            text_secondary: Color32::from_rgb(148, 163, 184),
            text_muted: Color32::from_rgb(100, 116, 139),
            accent: Color32::from_rgb(59, 130, 246),
            accent_hover: Color32::from_rgb(37, 99, 235),
            success: Color32::from_rgb(34, 197, 94),
            warning: Color32::from_rgb(234, 179, 8),
            danger: Color32::from_rgb(239, 68, 68),
            info: Color32::from_rgb(56, 189, 248),
            snapshot_in_progress: Color32::from_rgb(59, 130, 246),
            snapshot_success: Color32::from_rgb(34, 197, 94),
            snapshot_failed: Color32::from_rgb(239, 68, 68),
            snapshot_partial: Color32::from_rgb(234, 179, 8),
            progress_start: Color32::from_rgb(239, 68, 68),
            progress_mid1: Color32::from_rgb(234, 179, 8),
            progress_mid2: Color32::from_rgb(34, 197, 94),
            progress_end: Color32::from_rgb(59, 130, 246),
        }
    }

    pub fn dracula() -> Self {
        Self {
            name: "Dracula".into(),
            bg_darkest: Color32::from_rgb(40, 42, 54),
            bg_dark: Color32::from_rgb(68, 71, 90),
            bg_card: Color32::from_rgb(68, 71, 90),
            bg_input: Color32::from_rgb(98, 100, 120),
            text_primary: Color32::from_rgb(248, 248, 242),
            text_secondary: Color32::from_rgb(189, 147, 249),
            text_muted: Color32::from_rgb(139, 137, 160),
            accent: Color32::from_rgb(189, 147, 249),
            accent_hover: Color32::from_rgb(180, 130, 240),
            success: Color32::from_rgb(80, 250, 123),
            warning: Color32::from_rgb(241, 250, 140),
            danger: Color32::from_rgb(255, 85, 85),
            info: Color32::from_rgb(139, 233, 253),
            snapshot_in_progress: Color32::from_rgb(189, 147, 249),
            snapshot_success: Color32::from_rgb(80, 250, 123),
            snapshot_failed: Color32::from_rgb(255, 85, 85),
            snapshot_partial: Color32::from_rgb(241, 250, 140),
            progress_start: Color32::from_rgb(255, 85, 85),
            progress_mid1: Color32::from_rgb(241, 250, 140),
            progress_mid2: Color32::from_rgb(80, 250, 123),
            progress_end: Color32::from_rgb(189, 147, 249),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "Nord".into(),
            bg_darkest: Color32::from_rgb(46, 52, 64),
            bg_dark: Color32::from_rgb(59, 66, 82),
            bg_card: Color32::from_rgb(59, 66, 82),
            bg_input: Color32::from_rgb(76, 86, 106),
            text_primary: Color32::from_rgb(216, 222, 233),
            text_secondary: Color32::from_rgb(136, 192, 208),
            text_muted: Color32::from_rgb(96, 110, 130),
            accent: Color32::from_rgb(136, 192, 208),
            accent_hover: Color32::from_rgb(129, 161, 193),
            success: Color32::from_rgb(163, 190, 140),
            warning: Color32::from_rgb(235, 203, 139),
            danger: Color32::from_rgb(191, 97, 106),
            info: Color32::from_rgb(129, 161, 193),
            snapshot_in_progress: Color32::from_rgb(136, 192, 208),
            snapshot_success: Color32::from_rgb(163, 190, 140),
            snapshot_failed: Color32::from_rgb(191, 97, 106),
            snapshot_partial: Color32::from_rgb(235, 203, 139),
            progress_start: Color32::from_rgb(191, 97, 106),
            progress_mid1: Color32::from_rgb(235, 203, 139),
            progress_mid2: Color32::from_rgb(163, 190, 140),
            progress_end: Color32::from_rgb(136, 192, 208),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            name: "Solarized Dark".into(),
            bg_darkest: Color32::from_rgb(0, 43, 54),
            bg_dark: Color32::from_rgb(7, 54, 66),
            bg_card: Color32::from_rgb(7, 54, 66),
            bg_input: Color32::from_rgb(20, 70, 80),
            text_primary: Color32::from_rgb(253, 246, 227),
            text_secondary: Color32::from_rgb(131, 148, 150),
            text_muted: Color32::from_rgb(88, 110, 117),
            accent: Color32::from_rgb(38, 139, 210),
            accent_hover: Color32::from_rgb(30, 110, 170),
            success: Color32::from_rgb(133, 153, 0),
            warning: Color32::from_rgb(181, 137, 0),
            danger: Color32::from_rgb(220, 50, 47),
            info: Color32::from_rgb(42, 161, 152),
            snapshot_in_progress: Color32::from_rgb(38, 139, 210),
            snapshot_success: Color32::from_rgb(133, 153, 0),
            snapshot_failed: Color32::from_rgb(220, 50, 47),
            snapshot_partial: Color32::from_rgb(181, 137, 0),
            progress_start: Color32::from_rgb(220, 50, 47),
            progress_mid1: Color32::from_rgb(181, 137, 0),
            progress_mid2: Color32::from_rgb(133, 153, 0),
            progress_end: Color32::from_rgb(38, 139, 210),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night".into(),
            bg_darkest: Color32::from_rgb(26, 27, 38),
            bg_dark: Color32::from_rgb(36, 40, 59),
            bg_card: Color32::from_rgb(36, 40, 59),
            bg_input: Color32::from_rgb(54, 58, 79),
            text_primary: Color32::from_rgb(192, 202, 245),
            text_secondary: Color32::from_rgb(169, 177, 214),
            text_muted: Color32::from_rgb(86, 95, 137),
            accent: Color32::from_rgb(122, 162, 247),
            accent_hover: Color32::from_rgb(100, 140, 230),
            success: Color32::from_rgb(158, 206, 106),
            warning: Color32::from_rgb(224, 175, 104),
            danger: Color32::from_rgb(247, 118, 142),
            info: Color32::from_rgb(125, 207, 255),
            snapshot_in_progress: Color32::from_rgb(122, 162, 247),
            snapshot_success: Color32::from_rgb(158, 206, 106),
            snapshot_failed: Color32::from_rgb(247, 118, 142),
            snapshot_partial: Color32::from_rgb(224, 175, 104),
            progress_start: Color32::from_rgb(247, 118, 142),
            progress_mid1: Color32::from_rgb(224, 175, 104),
            progress_mid2: Color32::from_rgb(158, 206, 106),
            progress_end: Color32::from_rgb(122, 162, 247),
        }
    }

    pub fn monokai() -> Self {
        Self {
            name: "Monokai".into(),
            bg_darkest: Color32::from_rgb(39, 40, 34),
            bg_dark: Color32::from_rgb(56, 57, 49),
            bg_card: Color32::from_rgb(56, 57, 49),
            bg_input: Color32::from_rgb(80, 81, 70),
            text_primary: Color32::from_rgb(248, 248, 242),
            text_secondary: Color32::from_rgb(174, 129, 255),
            text_muted: Color32::from_rgb(117, 113, 94),
            accent: Color32::from_rgb(166, 226, 46),
            accent_hover: Color32::from_rgb(150, 210, 40),
            success: Color32::from_rgb(166, 226, 46),
            warning: Color32::from_rgb(253, 151, 31),
            danger: Color32::from_rgb(249, 38, 114),
            info: Color32::from_rgb(102, 217, 239),
            snapshot_in_progress: Color32::from_rgb(174, 129, 255),
            snapshot_success: Color32::from_rgb(166, 226, 46),
            snapshot_failed: Color32::from_rgb(249, 38, 114),
            snapshot_partial: Color32::from_rgb(253, 151, 31),
            progress_start: Color32::from_rgb(249, 38, 114),
            progress_mid1: Color32::from_rgb(253, 151, 31),
            progress_mid2: Color32::from_rgb(166, 226, 46),
            progress_end: Color32::from_rgb(174, 129, 255),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha".into(),
            bg_darkest: Color32::from_rgb(30, 30, 46),
            bg_dark: Color32::from_rgb(36, 36, 52),
            bg_card: Color32::from_rgb(36, 36, 52),
            bg_input: Color32::from_rgb(49, 50, 68),
            text_primary: Color32::from_rgb(205, 214, 244),
            text_secondary: Color32::from_rgb(180, 190, 254),
            text_muted: Color32::from_rgb(108, 112, 134),
            accent: Color32::from_rgb(137, 180, 250),
            accent_hover: Color32::from_rgb(120, 160, 240),
            success: Color32::from_rgb(166, 227, 161),
            warning: Color32::from_rgb(249, 226, 175),
            danger: Color32::from_rgb(243, 139, 168),
            info: Color32::from_rgb(137, 220, 235),
            snapshot_in_progress: Color32::from_rgb(137, 180, 250),
            snapshot_success: Color32::from_rgb(166, 227, 161),
            snapshot_failed: Color32::from_rgb(243, 139, 168),
            snapshot_partial: Color32::from_rgb(249, 226, 175),
            progress_start: Color32::from_rgb(243, 139, 168),
            progress_mid1: Color32::from_rgb(249, 226, 175),
            progress_mid2: Color32::from_rgb(166, 227, 161),
            progress_end: Color32::from_rgb(137, 180, 250),
        }
    }

    pub fn one_dark() -> Self {
        Self {
            name: "One Dark".into(),
            bg_darkest: Color32::from_rgb(40, 44, 52),
            bg_dark: Color32::from_rgb(50, 54, 62),
            bg_card: Color32::from_rgb(50, 54, 62),
            bg_input: Color32::from_rgb(60, 64, 72),
            text_primary: Color32::from_rgb(171, 178, 191),
            text_secondary: Color32::from_rgb(97, 175, 239),
            text_muted: Color32::from_rgb(92, 99, 112),
            accent: Color32::from_rgb(97, 175, 239),
            accent_hover: Color32::from_rgb(80, 160, 220),
            success: Color32::from_rgb(152, 195, 121),
            warning: Color32::from_rgb(209, 154, 102),
            danger: Color32::from_rgb(224, 108, 117),
            info: Color32::from_rgb(86, 182, 194),
            snapshot_in_progress: Color32::from_rgb(97, 175, 239),
            snapshot_success: Color32::from_rgb(152, 195, 121),
            snapshot_failed: Color32::from_rgb(224, 108, 117),
            snapshot_partial: Color32::from_rgb(209, 154, 102),
            progress_start: Color32::from_rgb(224, 108, 117),
            progress_mid1: Color32::from_rgb(209, 154, 102),
            progress_mid2: Color32::from_rgb(152, 195, 121),
            progress_end: Color32::from_rgb(97, 175, 239),
        }
    }

    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::slate(),
            Self::dracula(),
            Self::nord(),
            Self::solarized_dark(),
            Self::tokyo_night(),
            Self::monokai(),
            Self::catppuccin_mocha(),
            Self::one_dark(),
        ]
    }

    // Convert to egui Visuals for native widgets
    pub fn to_egui_visuals(&self) -> egui::Visuals {
        let mut v = egui::Visuals::dark();
        v.panel_fill = self.bg_dark;
        v.window_fill = self.bg_card;
        v.window_stroke = egui::Stroke::new(1.0, self.bg_input);
        v.widgets.inactive.bg_fill = self.bg_input;
        v.widgets.inactive.weak_bg_fill = self.bg_input;
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.bg_input);
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        v.widgets.hovered.bg_fill = self.accent.linear_multiply(0.15);
        v.widgets.hovered.weak_bg_fill = self.accent.linear_multiply(0.15);
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.accent);
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        v.widgets.active.bg_fill = self.accent.linear_multiply(0.25);
        v.widgets.active.weak_bg_fill = self.accent.linear_multiply(0.25);
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.accent_hover);
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        v.widgets.open.bg_fill = self.bg_card;
        v.selection.bg_fill = self.accent.linear_multiply(0.3);
        v.selection.stroke = egui::Stroke::new(1.0, self.accent);
        v.hyperlink_color = self.accent;
        v.faint_bg_color = self.bg_card.linear_multiply(0.5);
        v.extreme_bg_color = self.bg_darkest;
        v.code_bg_color = self.bg_input;
        v.warn_fg_color = self.warning;
        v.error_fg_color = self.danger;
        v.window_shadow = egui::epaint::Shadow {
            offset: [0, 4],
            color: self.bg_darkest.linear_multiply(0.5),
            spread: 12,
            blur: 20,
        };
        v.popup_shadow = egui::epaint::Shadow {
            offset: [0, 2],
            color: self.bg_darkest.linear_multiply(0.5),
            spread: 6,
            blur: 10,
        };
        v
    }

    pub fn health_color(&self, status: &str) -> Color32 {
        match status.to_lowercase().as_str() {
            "green" => self.success,
            "yellow" => self.warning,
            "red" => self.danger,
            _ => self.text_muted,
        }
    }

    #[allow(dead_code)]
    pub fn snapshot_state_color(&self, state: &str) -> Color32 {
        match state.to_lowercase().as_str() {
            "success" | "completed" => self.snapshot_success,
            "in_progress" | "started" | "init" => self.snapshot_in_progress,
            "failed" | "aborted" | "incompatible" => self.snapshot_failed,
            "partial" => self.snapshot_partial,
            _ => self.text_muted,
        }
    }
}

// --- Thread-local active theme ---

thread_local! {
    static ACTIVE_THEME: RefCell<AppTheme> = RefCell::new(AppTheme::slate());
}

/// Zero-sized accessor for the active theme.
/// All color methods read from the thread-local storage.
pub struct Theme;

impl Theme {
    pub fn set(theme: AppTheme) {
        ACTIVE_THEME.with(|t| *t.borrow_mut() = theme);
    }

    pub fn get() -> AppTheme {
        ACTIVE_THEME.with(|t| t.borrow().clone())
    }

    // Layout constants (unchanged, not color-dependent)
    pub const CARD_ROUNDING: CornerRadius = CornerRadius::same(12);
    #[allow(dead_code)]
    pub const BUTTON_ROUNDING: CornerRadius = CornerRadius::same(8);
    #[allow(dead_code)]
    pub const INPUT_ROUNDING: CornerRadius = CornerRadius::same(6);
    pub const CARD_PADDING: Vec2 = Vec2::new(16.0, 16.0);
    #[allow(dead_code)]
    pub const SECTION_SPACING: f32 = 12.0;
    #[allow(dead_code)]
    pub const ITEM_SPACING: f32 = 8.0;

    // Dynamic color accessors
    pub fn bg_darkest() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().bg_darkest)
    }
    pub fn bg_dark() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().bg_dark)
    }
    pub fn bg_card() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().bg_card)
    }
    pub fn bg_input() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().bg_input)
    }
    pub fn text_primary() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().text_primary)
    }
    pub fn text_secondary() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().text_secondary)
    }
    pub fn text_muted() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().text_muted)
    }
    pub fn accent() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().accent)
    }
    pub fn accent_hover() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().accent_hover)
    }
    pub fn success() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().success)
    }
    pub fn warning() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().warning)
    }
    pub fn danger() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().danger)
    }
    #[allow(dead_code)]
    pub fn info() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().info)
    }
    pub fn snapshot_in_progress() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().snapshot_in_progress)
    }
    pub fn snapshot_success() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().snapshot_success)
    }
    pub fn snapshot_failed() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().snapshot_failed)
    }
    pub fn snapshot_partial() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().snapshot_partial)
    }
    pub fn progress_start() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().progress_start)
    }
    pub fn progress_mid1() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().progress_mid1)
    }
    pub fn progress_mid2() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().progress_mid2)
    }
    pub fn progress_end() -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().progress_end)
    }

    pub fn health_color(status: &str) -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().health_color(status))
    }

    #[allow(dead_code)]
    pub fn snapshot_state_color(state: &str) -> Color32 {
        ACTIVE_THEME.with(|t| t.borrow().snapshot_state_color(state))
    }

    pub fn border() -> Color32 {
        ACTIVE_THEME.with(|t| {
            let bg = t.borrow().bg_darkest;
            let [r, g, b, a] = bg.to_array();
            Color32::from_rgba_premultiplied(
                (r as u16 + 30).min(255) as u8,
                (g as u16 + 30).min(255) as u8,
                (b as u16 + 30).min(255) as u8,
                a,
            )
        })
    }
}
