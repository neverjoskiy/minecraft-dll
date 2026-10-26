use std::sync::{Mutex, OnceLock};

use egui::Color32;

#[derive(Clone, Copy, Debug)]
pub struct ThemeColors {
    pub accent: [f32; 3],
    pub accent_hover: [f32; 3],
    pub panel: [f32; 3],
    pub window: [f32; 3],
    pub stroke: [f32; 3],
    pub text: [f32; 3],
    pub extreme: [f32; 3],
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            accent: [42.0 / 255.0, 168.0 / 255.0, 212.0 / 255.0],
            accent_hover: [67.0 / 255.0, 196.0 / 255.0, 230.0 / 255.0],
            panel: [12.0 / 255.0, 17.0 / 255.0, 25.0 / 255.0],
            window: [16.0 / 255.0, 22.0 / 255.0, 31.0 / 255.0],
            stroke: [48.0 / 255.0, 64.0 / 255.0, 80.0 / 255.0],
            text: [1.0, 1.0, 1.0],
            extreme: [8.0 / 255.0, 12.0 / 255.0, 18.0 / 255.0],
        }
    }
}

impl ThemeColors {
    pub fn accent_color32(self) -> Color32 {
        rgb(self.accent)
    }

    pub fn accent_hover_color32(self) -> Color32 {
        rgb(self.accent_hover)
    }

    pub fn panel_color32(self) -> Color32 {
        rgb(self.panel)
    }

    pub fn window_color32(self) -> Color32 {
        rgb(self.window)
    }

    pub fn stroke_color32(self) -> Color32 {
        rgb(self.stroke)
    }

    pub fn text_color32(self) -> Color32 {
        rgb(self.text)
    }

    pub fn extreme_color32(self) -> Color32 {
        rgb(self.extreme)
    }
}

fn rgb(c: [f32; 3]) -> Color32 {
    Color32::from_rgb(
        (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn store() -> &'static Mutex<ThemeColors> {
    static STORE: OnceLock<Mutex<ThemeColors>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(ThemeColors::default()))
}

pub fn set_current(colors: ThemeColors) {
    if let Ok(mut slot) = store().lock() {
        *slot = colors;
    }
}

pub fn set_from_color32(
    accent: Color32,
    accent_hover: Color32,
    panel: Color32,
    window: Color32,
    stroke: Color32,
    text: Color32,
    extreme: Color32,
) {
    set_current(ThemeColors {
        accent: color32_to_rgb(accent),
        accent_hover: color32_to_rgb(accent_hover),
        panel: color32_to_rgb(panel),
        window: color32_to_rgb(window),
        stroke: color32_to_rgb(stroke),
        text: color32_to_rgb(text),
        extreme: color32_to_rgb(extreme),
    });
}

pub fn current() -> ThemeColors {
    store().lock().map(|slot| *slot).unwrap_or_default()
}

fn color32_to_rgb(color: Color32) -> [f32; 3] {
    [
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    ]
}
