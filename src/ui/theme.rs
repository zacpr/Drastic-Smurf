use egui::{Color32, Rounding, Stroke, Vec2};

pub struct Theme;

impl Theme {
    // Backgrounds
    pub const BG_DARKEST: Color32 = Color32::from_rgb(15, 23, 42);    // slate-900
    pub const BG_DARK: Color32 = Color32::from_rgb(30, 41, 59);       // slate-800
    pub const BG_CARD: Color32 = Color32::from_rgb(30, 41, 59);       // slate-800
    pub const BG_INPUT: Color32 = Color32::from_rgb(51, 65, 85);      // slate-700

    // Text
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(248, 250, 252);   // slate-50
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 163, 184); // slate-400
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);     // slate-500

    // Accents
    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);      // blue-500
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(37, 99, 235); // blue-600

    // Health / Status colors
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);      // green-500
    pub const WARNING: Color32 = Color32::from_rgb(234, 179, 8);      // yellow-500
    pub const DANGER: Color32 = Color32::from_rgb(239, 68, 68);       // red-500
    pub const INFO: Color32 = Color32::from_rgb(56, 189, 248);        // sky-400

    // Snapshot states
    pub const SNAPSHOT_IN_PROGRESS: Color32 = Color32::from_rgb(59, 130, 246); // blue
    pub const SNAPSHOT_SUCCESS: Color32 = Color32::from_rgb(34, 197, 94);      // green
    pub const SNAPSHOT_FAILED: Color32 = Color32::from_rgb(239, 68, 68);       // red
    pub const SNAPSHOT_PARTIAL: Color32 = Color32::from_rgb(234, 179, 8);      // yellow

    // Progress bar gradient stops
    pub const PROGRESS_START: Color32 = Color32::from_rgb(239, 68, 68);   // red
    pub const PROGRESS_MID1: Color32 = Color32::from_rgb(234, 179, 8);    // yellow
    pub const PROGRESS_MID2: Color32 = Color32::from_rgb(34, 197, 94);    // green
    pub const PROGRESS_END: Color32 = Color32::from_rgb(59, 130, 246);    // blue

    // Spacing
    pub const CARD_ROUNDING: Rounding = Rounding::same(12);
    pub const BUTTON_ROUNDING: Rounding = Rounding::same(8);
    pub const INPUT_ROUNDING: Rounding = Rounding::same(6);
    pub const CARD_PADDING: Vec2 = Vec2::new(16.0, 16.0);
    pub const SECTION_SPACING: f32 = 12.0;
    pub const ITEM_SPACING: f32 = 8.0;

    pub fn health_color(status: &str) -> Color32 {
        match status.to_lowercase().as_str() {
            "green" => Self::SUCCESS,
            "yellow" => Self::WARNING,
            "red" => Self::DANGER,
            _ => Self::TEXT_MUTED,
        }
    }

    pub fn snapshot_state_color(state: &str) -> Color32 {
        match state.to_lowercase().as_str() {
            "success" | "completed" => Self::SNAPSHOT_SUCCESS,
            "in_progress" | "started" | "init" => Self::SNAPSHOT_IN_PROGRESS,
            "failed" | "aborted" | "incompatible" => Self::SNAPSHOT_FAILED,
            "partial" => Self::SNAPSHOT_PARTIAL,
            _ => Self::TEXT_MUTED,
        }
    }
}
