

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};

use super::gui_menu::MenuStyle;
use crate::gui_anim::{self, MenuAnimations};


pub(crate) fn button_fill(style: &MenuStyle) -> Color32 {
    gui_anim::lerp_color(Color32::from_rgb(0x28, 0x28, 0x2c), style.accent, 0.12)
}

pub(crate) fn button_fill_hover(style: &MenuStyle) -> Color32 {
    gui_anim::lerp_color(Color32::from_rgb(0x34, 0x34, 0x39), style.accent, 0.18)
}

pub(crate) fn button_border(style: &MenuStyle) -> Color32 {
    gui_anim::lerp_color(Color32::from_rgb(0x3a, 0x3a, 0x40), style.accent, 0.18)
}

pub(crate) fn themed_button(ui: &mut Ui, label: &str, style: &MenuStyle) -> egui::Response {
    animated_chip_button(ui, label, style, 12.0, 26.0, None, 0, 0.0, false)
}

pub(crate) fn themed_small_button(ui: &mut Ui, label: &str, style: &MenuStyle) -> egui::Response {
    animated_chip_button(ui, label, style, 11.0, 22.0, None, 0, 0.0, false)
}

pub(crate) fn themed_button_anim(
    ui: &mut Ui,
    label: &str,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
    capturing: bool,
) -> egui::Response {
    animated_chip_button(
        ui,
        label,
        style,
        12.0,
        26.0,
        Some(anim),
        anim_id,
        dt,
        capturing,
    )
}

pub(crate) fn themed_small_button_anim(
    ui: &mut Ui,
    label: &str,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
    capturing: bool,
) -> egui::Response {
    animated_chip_button(
        ui,
        label,
        style,
        11.0,
        22.0,
        Some(anim),
        anim_id,
        dt,
        capturing,
    )
}

pub(crate) fn themed_wide_button_anim(
    ui: &mut Ui,
    label: &str,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
    capturing: bool,
) -> egui::Response {
    let width = ui.available_width().max(80.0);
    animated_chip_button_sized(
        ui,
        label,
        style,
        13.0,
        vec2(width, 38.0),
        Some(anim),
        anim_id,
        dt,
        capturing,
        true,
    )
}

fn animated_chip_button(
    ui: &mut Ui,
    label: &str,
    style: &MenuStyle,
    font_size: f32,
    height: f32,
    anim: Option<&mut MenuAnimations>,
    anim_id: u64,
    dt: f32,
    capturing: bool,
) -> egui::Response {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(font_size),
            style.text,
        )
    });
    let pad_x = 10.0;
    let size = vec2((galley.size().x + pad_x * 2.0).max(36.0), height);
    animated_chip_button_sized(
        ui, label, style, font_size, size, anim, anim_id, dt, capturing, false,
    )
}

fn animated_chip_button_sized(
    ui: &mut Ui,
    label: &str,
    style: &MenuStyle,
    font_size: f32,
    size: egui::Vec2,
    anim: Option<&mut MenuAnimations>,
    anim_id: u64,
    dt: f32,
    capturing: bool,
    primary: bool,
) -> egui::Response {
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(font_size),
            style.text,
        )
    });
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let (hover_t, press_t, pulse_t) = if let Some(anim) = anim {
        (
            anim.hover(anim_id, response.hovered(), dt),
            anim.press(
                anim_id.wrapping_add(1),
                response.is_pointer_button_down_on(),
                dt,
            ),
            anim.pulse(anim_id.wrapping_add(2), capturing, dt),
        )
    } else {
        (
            if response.hovered() { 1.0 } else { 0.0 },
            if response.is_pointer_button_down_on() {
                1.0
            } else {
                0.0
            },
            if capturing { 0.6 } else { 0.0 },
        )
    };

    let base_fill = if primary {
        gui_anim::with_alpha(style.accent, 170)
    } else {
        button_fill(style)
    };
    let hover_fill = if primary {
        gui_anim::with_alpha(style.accent_hover, 190)
    } else {
        button_fill_hover(style)
    };
    let mut fill = gui_anim::lerp_color(base_fill, hover_fill, hover_t);
    fill = gui_anim::lerp_color(fill, style.accent, press_t * 0.35 + pulse_t * 0.45);
    let border = gui_anim::lerp_color(
        button_border(style),
        style.accent,
        hover_t * 0.5 + pulse_t * 0.8,
    );

    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(5), fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    if pulse_t > 0.05 {
        ui.painter().rect_stroke(
            rect.expand(1.0 + pulse_t * 2.0),
            egui::CornerRadius::same(5),
            egui::Stroke::new(
                1.0_f32,
                gui_anim::with_alpha(style.accent, (pulse_t * 120.0) as u8),
            ),
            egui::StrokeKind::Outside,
        );
    }

    let text_pos = Align2::CENTER_CENTER
        .anchor_size(rect.center(), galley.size())
        .min;
    ui.painter().galley(text_pos, galley, style.text);
    response
}


pub(crate) fn faint_separator(ui: &mut Ui, style: &MenuStyle) {
    faint_separator_anim(ui, style, None, 0, 0.0);
}

pub(crate) fn faint_separator_anim(
    ui: &mut Ui,
    style: &MenuStyle,
    anim: Option<&mut MenuAnimations>,
    anim_id: u64,
    dt: f32,
) {
    let y = ui.cursor().top();
    let rect = ui.max_rect();
    let left = rect.left() + 16.0;
    let right = rect.right() - 16.0;
    if right <= left {
        return;
    }
    let alpha = if let Some(anim) = anim {
        let t = anim.chase(anim_id, 1.0, dt, 10.0);
        (28.0 * gui_anim::ease_out_cubic(t)) as u8
    } else {
        28
    };
    ui.painter().hline(
        left..=right,
        y,
        egui::Stroke::new(1.0_f32, gui_anim::with_alpha(style.card_border, alpha)),
    );
}


pub(crate) fn bar_slider(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    style: &MenuStyle,
) {
    bar_slider_anim(ui, name, value, min, max, style, None, 0, 0.0);
}

pub(crate) fn bar_slider_anim(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    style: &MenuStyle,
    anim: Option<&mut MenuAnimations>,
    anim_id: u64,
    dt: f32,
) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, 39.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new(name).color(style.row_label).size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(90.0, 29.0), Sense::click_and_drag());
                let strip = Rect::from_center_size(rect.center(), vec2(90.0, 5.0));

                if response.clicked() || response.dragged() {
                    if let Some(pointer_pos) = ui.input(|input| input.pointer.interact_pos()) {
                        let pct = ((pointer_pos.x - strip.left()) / strip.width()).clamp(0.0, 1.0);
                        *value = (min + pct * (max - min)).clamp(min, max);
                    } else {
                        let delta = ui.input(|input| input.pointer.delta().x);
                        let range = (max - min).max(f32::EPSILON);
                        let step = range / 140.0;
                        *value = (*value + delta * step).clamp(min, max);
                    }
                }

                if response.has_focus() {
                    let delta = ui.input(|input| {
                        let mut delta = 0.0;
                        if input.key_down(egui::Key::ArrowRight)
                            || input.key_down(egui::Key::ArrowUp)
                        {
                            delta += 1.0;
                        }
                        if input.key_down(egui::Key::ArrowLeft)
                            || input.key_down(egui::Key::ArrowDown)
                        {
                            delta -= 1.0;
                        }
                        delta
                    });
                    if delta != 0.0 {
                        let range = (max - min).max(f32::EPSILON);
                        let step = range / 140.0;
                        *value = (*value + delta * step).clamp(min, max);
                    }
                }

                let target_pct = ((*value - min) / (max - min)).clamp(0.0, 1.0);
                let hover_t = if let Some(anim) = anim {
                    let h = anim.hover(
                        anim_id.wrapping_add(7),
                        response.hovered() || response.dragged(),
                        dt,
                    );
                    h
                } else {
                    if response.hovered() { 1.0 } else { 0.0 }
                };

                let fill = Rect::from_min_max(
                    strip.min,
                    pos2(strip.left() + strip.width() * target_pct, strip.bottom()),
                );
                let rounding = egui::CornerRadius::same(2);
                ui.painter().rect_filled(
                    strip,
                    rounding,
                    gui_anim::with_alpha(style.slider_track, 115 + (hover_t * 40.0) as u8),
                );
                ui.painter().rect_filled(
                    fill,
                    rounding,
                    gui_anim::lerp_color(style.slider_fill, style.accent_hover, hover_t * 0.45),
                );
                ui.painter().circle_filled(
                    pos2(strip.left() + strip.width() * target_pct, strip.center().y),
                    3.0,
                    Color32::WHITE,
                );

                ui.add_space(7.0);
                ui.label(
                    egui::RichText::new(format!("{:.1}", *value))
                        .color(gui_anim::lerp_color(style.row_label, style.text, hover_t))
                        .size(12.0),
                );
            });
        },
    );
}


pub(crate) fn color_picker_row(
    ui: &mut Ui,
    name: &str,
    rgba: &mut [f32; 4],
    style: &MenuStyle,
    id_salt: impl std::hash::Hash,
) {
    color_picker_row_anim(ui, name, rgba, style, id_salt, None, 0.0);
}

pub(crate) fn color_picker_row_anim(
    ui: &mut Ui,
    name: &str,
    rgba: &mut [f32; 4],
    style: &MenuStyle,
    id_salt: impl std::hash::Hash,
    mut anim: Option<&mut MenuAnimations>,
    dt: f32,
) {
    let popup_id = ui.id().with("figma_color").with(id_salt);
    let anim_id = egui::Id::new("color_swatch").with(popup_id).value();

    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, 40.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new(name).color(style.row_label).size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let swatch = vec2(28.0, 16.0);
                let (rect, response) = ui.allocate_exact_size(swatch, Sense::click());
                let open = ui.memory(|m| m.is_popup_open(popup_id));
                let hover_t = if let Some(anim) = anim.as_deref_mut() {
                    anim.hover(anim_id, response.hovered() || open, dt)
                } else if response.hovered() || open {
                    1.0
                } else {
                    0.0
                };
                paint_color_swatch(ui, rect.expand(hover_t * 1.0), *rgba, hover_t, style);

                if response.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }

                egui::popup_below_widget(
                    ui,
                    popup_id,
                    &response,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(220.0);
                        egui::Frame::NONE
                            .fill(style.card_bg)
                            .stroke(egui::Stroke::new(1.0_f32, style.card_border))
                            .corner_radius(egui::CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                draw_color_picker_popup(ui, rgba, style);
                            });
                    },
                );
            });
        },
    );
}

fn paint_color_swatch(ui: &mut Ui, rect: Rect, rgba: [f32; 4], hover_t: f32, style: &MenuStyle) {
    let rounding = egui::CornerRadius::same(2);
    let cell = 4.0;
    let mut y = rect.top();
    let mut row = 0u32;
    while y < rect.bottom() {
        let mut x = rect.left();
        let mut col = 0u32;
        while x < rect.right() {
            let light = (row + col) % 2 == 0;
            let c = if light {
                Color32::from_rgb(0x55, 0x55, 0x55)
            } else {
                Color32::from_rgb(0x2a, 0x2a, 0x2a)
            };
            let cell_rect = Rect::from_min_max(
                pos2(x, y),
                pos2((x + cell).min(rect.right()), (y + cell).min(rect.bottom())),
            );
            ui.painter().rect_filled(cell_rect, 0.0, c);
            x += cell;
            col += 1;
        }
        y += cell;
        row += 1;
    }
    let color = Color32::from_rgba_unmultiplied(
        (rgba[0].clamp(0.0, 1.0) * 255.0) as u8,
        (rgba[1].clamp(0.0, 1.0) * 255.0) as u8,
        (rgba[2].clamp(0.0, 1.0) * 255.0) as u8,
        (rgba[3].clamp(0.0, 1.0) * 255.0) as u8,
    );
    ui.painter().rect_filled(rect, rounding, color);
    let stroke_c = gui_anim::lerp_color(Color32::from_white_alpha(25), style.accent, hover_t);
    ui.painter().rect_stroke(
        rect,
        rounding,
        egui::Stroke::new(1.0_f32, stroke_c),
        egui::StrokeKind::Outside,
    );
}

fn draw_color_picker_popup(ui: &mut Ui, rgba: &mut [f32; 4], style: &MenuStyle) {
    ui.set_min_width(220.0);
    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);

    let mut hsva = RgbaHsva::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]);

    let sv_size = vec2(200.0, 120.0);
    let (sv_rect, sv_resp) = ui.allocate_exact_size(sv_size, Sense::click_and_drag());
    paint_sv_square(ui, sv_rect, hsva.h);
    if sv_resp.clicked() || sv_resp.dragged() {
        if let Some(pos) = sv_resp.interact_pointer_pos() {
            hsva.s = ((pos.x - sv_rect.left()) / sv_rect.width()).clamp(0.0, 1.0);
            hsva.v = 1.0 - ((pos.y - sv_rect.top()) / sv_rect.height()).clamp(0.0, 1.0);
        }
    }
    let cross = pos2(
        sv_rect.left() + hsva.s * sv_rect.width(),
        sv_rect.top() + (1.0 - hsva.v) * sv_rect.height(),
    );
    ui.painter()
        .circle_stroke(cross, 5.0, egui::Stroke::new(1.5_f32, Color32::WHITE));
    ui.painter()
        .circle_stroke(cross, 6.0, egui::Stroke::new(1.0_f32, Color32::BLACK));

    let (hue_rect, hue_resp) = ui.allocate_exact_size(vec2(200.0, 12.0), Sense::click_and_drag());
    paint_hue_bar(ui, hue_rect);
    if hue_resp.clicked() || hue_resp.dragged() {
        if let Some(pos) = hue_resp.interact_pointer_pos() {
            hsva.h = ((pos.x - hue_rect.left()) / hue_rect.width()).clamp(0.0, 1.0);
        }
    }
    let hx = hue_rect.left() + hsva.h * hue_rect.width();
    ui.painter().rect_stroke(
        Rect::from_center_size(pos2(hx, hue_rect.center().y), vec2(4.0, 14.0)),
        egui::CornerRadius::same(1),
        egui::Stroke::new(1.0_f32, Color32::WHITE),
        egui::StrokeKind::Outside,
    );

    let (alpha_rect, alpha_resp) =
        ui.allocate_exact_size(vec2(200.0, 12.0), Sense::click_and_drag());
    paint_alpha_bar(ui, alpha_rect, hsva.to_rgb());
    if alpha_resp.clicked() || alpha_resp.dragged() {
        if let Some(pos) = alpha_resp.interact_pointer_pos() {
            hsva.a = ((pos.x - alpha_rect.left()) / alpha_rect.width()).clamp(0.0, 1.0);
        }
    }
    let ax = alpha_rect.left() + hsva.a * alpha_rect.width();
    ui.painter().rect_stroke(
        Rect::from_center_size(pos2(ax, alpha_rect.center().y), vec2(4.0, 14.0)),
        egui::CornerRadius::same(1),
        egui::Stroke::new(1.0_f32, Color32::WHITE),
        egui::StrokeKind::Outside,
    );

    let (r, g, b, a) = hsva.to_rgba();
    rgba[0] = r;
    rgba[1] = g;
    rgba[2] = b;
    rgba[3] = a;

    ui.label(
        egui::RichText::new(format!(
            "RGBA {:.0},{:.0},{:.0},{:.0}",
            r * 255.0,
            g * 255.0,
            b * 255.0,
            a * 255.0
        ))
        .color(style.row_label)
        .size(11.0),
    );
}

fn paint_sv_square(ui: &mut Ui, rect: Rect, hue: f32) {
    let steps = 24;
    let dw = rect.width() / steps as f32;
    let dh = rect.height() / steps as f32;
    for yi in 0..steps {
        for xi in 0..steps {
            let s = (xi as f32 + 0.5) / steps as f32;
            let v = 1.0 - (yi as f32 + 0.5) / steps as f32;
            let (r, g, b) = hsv_to_rgb(hue, s, v);
            let cell = Rect::from_min_size(
                pos2(rect.left() + xi as f32 * dw, rect.top() + yi as f32 * dh),
                vec2(dw + 0.5, dh + 0.5),
            );
            ui.painter().rect_filled(
                cell,
                0.0,
                Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
            );
        }
    }
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(2),
        egui::Stroke::new(1.0_f32, Color32::from_white_alpha(30)),
        egui::StrokeKind::Outside,
    );
}

fn paint_hue_bar(ui: &mut Ui, rect: Rect) {
    let steps = 48;
    let dw = rect.width() / steps as f32;
    for i in 0..steps {
        let h = (i as f32 + 0.5) / steps as f32;
        let (r, g, b) = hsv_to_rgb(h, 1.0, 1.0);
        let cell = Rect::from_min_size(
            pos2(rect.left() + i as f32 * dw, rect.top()),
            vec2(dw + 0.5, rect.height()),
        );
        ui.painter().rect_filled(
            cell,
            0.0,
            Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8),
        );
    }
}

fn paint_alpha_bar(ui: &mut Ui, rect: Rect, rgb: (f32, f32, f32)) {
    let cell = 4.0;
    let mut x = rect.left();
    let mut col = 0u32;
    while x < rect.right() {
        let light = col % 2 == 0;
        let c = if light {
            Color32::from_rgb(0x55, 0x55, 0x55)
        } else {
            Color32::from_rgb(0x2a, 0x2a, 0x2a)
        };
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(x, rect.top()),
                pos2((x + cell).min(rect.right()), rect.bottom()),
            ),
            0.0,
            c,
        );
        x += cell;
        col += 1;
    }
    let steps = 32;
    let dw = rect.width() / steps as f32;
    for i in 0..steps {
        let a = (i as f32 + 0.5) / steps as f32;
        let cell_r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * dw, rect.top()),
            vec2(dw + 0.5, rect.height()),
        );
        ui.painter().rect_filled(
            cell_r,
            0.0,
            Color32::from_rgba_unmultiplied(
                (rgb.0 * 255.0) as u8,
                (rgb.1 * 255.0) as u8,
                (rgb.2 * 255.0) as u8,
                (a * 255.0) as u8,
            ),
        );
    }
}

struct RgbaHsva {
    h: f32,
    s: f32,
    v: f32,
    a: f32,
}

impl RgbaHsva {
    fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d < 1e-6 {
            0.0
        } else if max == r {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
        } else if max == g {
            ((b - r) / d + 2.0) / 6.0
        } else {
            ((r - g) / d + 4.0) / 6.0
        };
        let s = if max < 1e-6 { 0.0 } else { d / max };
        Self { h, s, v: max, a }
    }

    fn to_rgb(&self) -> (f32, f32, f32) {
        hsv_to_rgb(self.h, self.s, self.v)
    }

    fn to_rgba(&self) -> (f32, f32, f32, f32) {
        let (r, g, b) = self.to_rgb();
        (r, g, b, self.a)
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = h.fract();
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}
