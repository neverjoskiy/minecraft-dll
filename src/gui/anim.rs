

use std::collections::HashMap;

#[derive(Default)]
pub struct MenuAnimations {
    pub open: f32,
    pub content: f32,
    pub section_key: u64,
    toggles: HashMap<u64, f32>,
    tabs: HashMap<u64, f32>,
    hover: HashMap<u64, f32>,
    press: HashMap<u64, f32>,
    pulse: HashMap<u64, f32>,
    floats: HashMap<u64, f32>,
    time: f32,
}

impl MenuAnimations {
    pub fn tick_time(&mut self, dt: f32) {
        self.time = (self.time + dt) % 1000.0;
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn tick_open(&mut self, want_open: bool, dt: f32) {
        animate(
            &mut self.open,
            want_open,
            dt,
            if want_open { 14.0 } else { 16.0 },
        );
    }

    pub fn visible(&self) -> bool {
        self.open > 0.01
    }

    pub fn open_ease(&self) -> f32 {
        ease_out_cubic(self.open)
    }

    pub fn begin_content(&mut self, section_key: u64, dt: f32) {
        if self.section_key != section_key {
            self.section_key = section_key;
            self.content = 0.0;
        }
        animate(&mut self.content, true, dt, 12.0);
    }

    pub fn content_ease(&self) -> f32 {
        ease_out_cubic(self.content)
    }

    pub fn toggle(&mut self, id: u64, on: bool, dt: f32) -> f32 {
        let slot = self.toggles.entry(id).or_insert(if on { 1.0 } else { 0.0 });
        animate(slot, on, dt, 18.0);
        ease_out_cubic(*slot)
    }

    pub fn tab(&mut self, id: u64, active: bool, dt: f32) -> f32 {
        let slot = self
            .tabs
            .entry(id)
            .or_insert(if active { 1.0 } else { 0.0 });
        animate(slot, active, dt, 16.0);
        ease_out_cubic(*slot)
    }

    pub fn hover(&mut self, id: u64, hovered: bool, dt: f32) -> f32 {
        let slot = self.hover.entry(id).or_insert(0.0);
        animate(slot, hovered, dt, 20.0);
        ease_out_cubic(*slot)
    }

    pub fn press(&mut self, id: u64, pressed: bool, dt: f32) -> f32 {
        let slot = self.press.entry(id).or_insert(0.0);
        animate(slot, pressed, dt, 28.0);
        ease_out_cubic(*slot)
    }

    
    pub fn pulse(&mut self, id: u64, active: bool, dt: f32) -> f32 {
        let intensity = self.pulse.entry(id).or_insert(0.0);
        animate(intensity, active, dt, 14.0);
        let wave = 0.5 + 0.5 * (self.time * 3.5).sin();
        ease_out_cubic(*intensity) * wave
    }

    
    pub fn chase(&mut self, id: u64, target: f32, dt: f32, speed: f32) -> f32 {
        let slot = self.floats.entry(id).or_insert(0.0);
        *slot += (target - *slot) * (dt * speed).clamp(0.0, 1.0);
        *slot
    }
}

fn animate(value: &mut f32, target_on: bool, dt: f32, speed: f32) {
    let target = if target_on { 1.0 } else { 0.0 };
    *value = (*value + (target - *value) * (dt * speed).clamp(0.0, 1.0)).clamp(0.0, 1.0);
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgba_unmultiplied(
        lerp(a.r() as f32, b.r() as f32, t).round() as u8,
        lerp(a.g() as f32, b.g() as f32, t).round() as u8,
        lerp(a.b() as f32, b.b() as f32, t).round() as u8,
        lerp(a.a() as f32, b.a() as f32, t).round() as u8,
    )
}

pub fn with_alpha(c: egui::Color32, a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn scale_alpha(c: egui::Color32, opacity: f32) -> egui::Color32 {
    with_alpha(c, ((c.a() as f32) * opacity.clamp(0.0, 1.0)).round() as u8)
}
