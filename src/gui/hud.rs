use egui::{Color32, Context, Pos2, Rect, RichText, TextureId, Ui, pos2, vec2};

use crate::{
    gui_blur::BlurRegion,
    module_system::HudSnapshot,
    utils::{
        hud_data::{HudTargetData, HudWorldData},
        name_tags::{NameTagItemIconRequest, NameTagItemSlot},
        player_face::PlayerFaceIconRequest,
        potions::{PotionEntry, PotionIconRequest},
    },
};

#[derive(Clone, Default)]
pub struct HudAnimations {
    pub watermark: f32,
    pub keybind_window: f32,
    pub keybinds: Vec<KeybindAnimation>,
    pub potion_window: f32,
    pub potions: Vec<PotionEntry>,
    pub target_window: f32,
    pub staff_window: f32,
    pub armor_window: f32,
    pub fps: f32,
    pub world: HudWorldData,
    pub sync_theme: bool,
}

#[derive(Clone)]
pub struct KeybindAnimation {
    pub module_name: String,
    pub key_name: String,
    pub progress: f32,
    pub removing: bool,
}

#[derive(Clone, Default)]
pub struct HudIconRequests {
    pub items: Vec<NameTagItemIconRequest>,
    pub faces: Vec<PlayerFaceIconRequest>,
    pub potions: Vec<PotionIconRequest>,
    pub blur_regions: Vec<BlurRegion>,
}

const SPEED_WATERMARK: f32 = 10.0;
const SPEED_KEYBIND_WINDOW: f32 = 11.0;
const SPEED_KEYBIND_ROW: f32 = 13.0;
const POTION_LIST_W: f32 = 148.0;
const KEYBIND_LIST_W: f32 = 118.0;
const STAFF_LIST_W: f32 = 148.0;
const LIST_ROW_FILL_SCALE: f32 = 0.38;
const SPEED_POTION: f32 = 10.0;
const SPEED_TARGET: f32 = 12.0;
const SPEED_STAFF: f32 = 10.0;
const SPEED_ARMOR: f32 = 10.0;

impl HudAnimations {
    pub fn update(&mut self, hud: &HudSnapshot, dt: f32) -> bool {
        let mut animating = false;
        animating |= animate_value(
            &mut self.watermark,
            hud.enabled && hud.show_watermark,
            dt,
            SPEED_WATERMARK,
        );

        let instant_fps = 1.0 / dt.max(1.0 / 10000.0);
        if self.fps == 0.0 {
            self.fps = instant_fps;
        } else {
            self.fps = self.fps * 0.95 + instant_fps * 0.05;
        }

        let active_names: std::collections::HashSet<String> = hud
            .active_binds
            .iter()
            .map(|bind| bind.module_name.clone())
            .collect();

        for bind in &hud.active_binds {
            if let Some(animation) = self
                .keybinds
                .iter_mut()
                .find(|animation| animation.module_name == bind.module_name)
            {
                animation.key_name = bind.key_name.clone();
                animation.removing = false;
            } else {
                self.keybinds.push(KeybindAnimation {
                    module_name: bind.module_name.clone(),
                    key_name: bind.key_name.clone(),
                    progress: 0.0,
                    removing: false,
                });
                animating = true;
            }
        }

        for animation in &mut self.keybinds {
            if !active_names.contains(&animation.module_name) {
                animation.removing = true;
            }
            animating |= animate_value(
                &mut animation.progress,
                !animation.removing,
                dt,
                SPEED_KEYBIND_ROW,
            );
        }
        self.keybinds
            .retain(|animation| !animation.removing || animation.progress > 0.01);

        let keybind_window_visible = hud.enabled
            && hud.show_active_keybinds
            && (!self.keybinds.is_empty() || !active_names.is_empty());
        animating |= animate_value(
            &mut self.keybind_window,
            keybind_window_visible,
            dt,
            SPEED_KEYBIND_WINDOW,
        );

        let potion_window_visible = hud.enabled && hud.show_potions && !hud.potions.is_empty();
        animating |= animate_value(
            &mut self.potion_window,
            potion_window_visible,
            dt,
            SPEED_POTION,
        );
        self.potions = hud.potions.clone();
        self.world = hud.world.clone();
        self.sync_theme = hud.sync_theme;

        animating |= animate_value(
            &mut self.target_window,
            hud.enabled && hud.show_target && hud.world.target.is_some(),
            dt,
            SPEED_TARGET,
        );
        animating |= animate_value(
            &mut self.staff_window,
            hud.enabled && hud.show_staff && !hud.world.staff.is_empty(),
            dt,
            SPEED_STAFF,
        );
        animating |= animate_value(
            &mut self.armor_window,
            hud.enabled && hud.show_armor && hud.world.local_armor.iter().any(|v| *v),
            dt,
            SPEED_ARMOR,
        );

        animating
    }

    pub fn should_draw(&self) -> bool {
        self.watermark > 0.01
            || self.keybind_window > 0.01
            || !self.keybinds.is_empty()
            || self.potion_window > 0.01
            || self.target_window > 0.01
            || self.staff_window > 0.01
            || self.armor_window > 0.01
    }

    pub fn is_animating(&self) -> bool {
        is_between(self.watermark)
            || is_between(self.keybind_window)
            || is_between(self.potion_window)
            || is_between(self.target_window)
            || is_between(self.staff_window)
            || is_between(self.armor_window)
            || self
                .keybinds
                .iter()
                .any(|animation| is_between(animation.progress))
    }
}

#[derive(Clone, Copy)]
pub struct HudHeaderIcons {
    pub keybinds: TextureId,
}

#[derive(Clone, Copy)]
enum HeaderLead {
    None,
    WhiteRect,
    KeybindsIcon(TextureId),
    Potion,
}

pub fn draw_hud(
    ctx: &Context,
    hud: &HudAnimations,
    draggable: bool,
    header_icons: HudHeaderIcons,
) -> HudIconRequests {
    let mut icons = HudIconRequests::default();
    let screen = ctx.screen_rect();
    let style = HudStyle::resolve(hud.sync_theme);
    let panel = with_alpha(style.panel, 0.72);

    if hud.watermark > 0.01 {
        let opacity = ease_out(hud.watermark);
        egui::Area::new(egui::Id::new("xz_hud_info"))
            .default_pos(pos2(12.0, 12.0))
            .interactable(true)
            .movable(draggable)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                
                watermark_pill(ui, opacity, style, |ui| {
                    hud_label(
                        ui,
                        RichText::new("XZ")
                            .strong()
                            .size(13.0)
                            .color(style.accent_hover),
                    );
                    ui.add_space(6.0);
                    hud_label(
                        ui,
                        RichText::new(format!("Fps:{:.0}", hud.fps))
                            .size(13.0)
                            .color(style.text),
                    );
                });
                
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(5.0, 0.0);
                    watermark_pill(ui, opacity, style, |ui| {
                        hud_label(
                            ui,
                            RichText::new(format!("Ping:{}", hud.world.info.ping_ms))
                                .size(12.0)
                                .color(style.text),
                        );
                    });
                    watermark_pill(ui, opacity, style, |ui| {
                        hud_label(
                            ui,
                            RichText::new(format!("User:{}", hud.world.info.user))
                                .size(12.0)
                                .color(style.text),
                        );
                    });
                });
            });
    }

    if hud.potion_window > 0.01 {
        let opacity = ease_out(hud.potion_window);
        egui::Area::new(egui::Id::new("xz_hud_potions"))
            .default_pos(pos2(12.0, 78.0))
            .interactable(true)
            .movable(draggable)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                ui.set_width(POTION_LIST_W);
                ui.spacing_mut().item_spacing = vec2(0.0, 3.0);
                draw_pill_header(
                    ui,
                    opacity,
                    style,
                    HeaderLead::Potion,
                    "Potions",
                    POTION_LIST_W,
                    Some(&mut icons.blur_regions),
                );
                for potion in &hud.potions {
                    let level = potion.amplifier + 1;
                    let time = format_duration(potion.duration_ticks);
                    draw_pill_row(
                        ui,
                        POTION_LIST_W,
                        opacity,
                        1.0,
                        LIST_ROW_FILL_SCALE,
                        style,
                        PillLead::EffectIcon(&potion.sprite_id),
                        &format!("{} {}", potion.name, level),
                        &time,
                        Some(&mut icons.blur_regions),
                        Some(&mut icons.potions),
                    );
                }
            });
    }

    if hud.keybind_window > 0.01 {
        let opacity = ease_out(hud.keybind_window);
        let default_x = (screen.right() - KEYBIND_LIST_W - 16.0).max(12.0);
        egui::Area::new(egui::Id::new("xz_hud_keybinds"))
            .default_pos(pos2(default_x, screen.center().y - 40.0))
            .interactable(true)
            .movable(draggable)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                ui.set_width(KEYBIND_LIST_W);
                ui.spacing_mut().item_spacing = vec2(0.0, 3.0);
                draw_pill_header(
                    ui,
                    opacity,
                    style,
                    HeaderLead::KeybindsIcon(header_icons.keybinds),
                    "Keybinds",
                    KEYBIND_LIST_W,
                    Some(&mut icons.blur_regions),
                );
                for bind in &hud.keybinds {
                    let row_opacity = ease_out(bind.progress);
                    if row_opacity <= 0.01 {
                        continue;
                    }
                    draw_pill_row(
                        ui,
                        KEYBIND_LIST_W,
                        opacity,
                        row_opacity,
                        LIST_ROW_FILL_SCALE,
                        style,
                        PillLead::None,
                        &bind.module_name,
                        &bind.key_name,
                        Some(&mut icons.blur_regions),
                        None,
                    );
                }
            });
    }

    if hud.staff_window > 0.01 {
        let opacity = ease_out(hud.staff_window);
        let default_x = (screen.right() - STAFF_LIST_W - 16.0).max(12.0);
        egui::Area::new(egui::Id::new("xz_hud_staff"))
            .default_pos(pos2(default_x, screen.bottom() - 180.0))
            .interactable(draggable)
            .movable(draggable)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.multiply_opacity(opacity);
                ui.set_width(STAFF_LIST_W);
                ui.spacing_mut().item_spacing = vec2(0.0, 3.0);
                draw_pill_header(
                    ui,
                    opacity,
                    style,
                    HeaderLead::WhiteRect,
                    "StaffList",
                    STAFF_LIST_W,
                    None,
                );
                for entry in &hud.world.staff {
                    let lead_color = if entry.online && !entry.spectating {
                        style.text
                    } else {
                        Color32::from_rgb(220, 70, 70)
                    };
                    let status = if entry.spectating {
                        "Spec"
                    } else {
                        entry.role.as_str()
                    };
                    draw_pill_row(
                        ui,
                        STAFF_LIST_W,
                        opacity,
                        1.0,
                        LIST_ROW_FILL_SCALE,
                        style,
                        PillLead::Color(lead_color),
                        &entry.name,
                        status,
                        None,
                        None,
                    );
                }
            });
    }

    if hud.target_window > 0.01 {
        if let Some(target) = hud.world.target.as_ref() {
            let opacity = ease_out(hud.target_window);
            egui::Area::new(egui::Id::new("xz_hud_target"))
                .default_pos(pos2(
                    (screen.width() - 220.0) * 0.5,
                    (screen.bottom() - 140.0).max(12.0),
                ))
                .interactable(true)
                .movable(draggable)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.multiply_opacity(opacity);
                    draw_target_card(ui, target, panel, opacity, style, &mut icons);
                });
        }
    }

    if hud.armor_window > 0.01 {
        if let Some(local_id) = hud.world.local_entity_id {
            let opacity = ease_out(hud.armor_window);
            egui::Area::new(egui::Id::new("xz_hud_armor"))
                .anchor(egui::Align2::CENTER_BOTTOM, vec2(-80.0, -58.0))
                .interactable(draggable)
                .movable(draggable)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.multiply_opacity(opacity);
                    draw_armor_row(ui, local_id, hud.world.local_armor, opacity, &mut icons);
                });
        }
    }

    icons
}

#[derive(Clone, Copy)]
struct HudStyle {
    accent: Color32,
    accent_hover: Color32,
    panel: Color32,
    extreme: Color32,
    text: Color32,
}

impl HudStyle {
    fn resolve(sync_theme: bool) -> Self {
        if sync_theme {
            let theme = crate::theme_colors::current();
            Self {
                accent: theme.accent_color32(),
                accent_hover: theme.accent_hover_color32(),
                panel: theme.panel_color32(),
                extreme: theme.extreme_color32(),
                text: theme.text_color32(),
            }
        } else {
            Self {
                accent: Color32::from_rgb(120, 210, 255),
                accent_hover: Color32::from_rgb(120, 210, 255),
                panel: Color32::from_rgb(18, 18, 22),
                extreme: Color32::from_rgb(8, 8, 12),
                text: Color32::WHITE,
            }
        }
    }
}

fn pill_fill(style: HudStyle, opacity: f32, fill_scale: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        style.extreme.r(),
        style.extreme.g(),
        style.extreme.b(),
        (235.0 * opacity.clamp(0.0, 1.0) * fill_scale.clamp(0.0, 1.0)) as u8,
    )
}

fn draw_fixed_pill(
    ui: &mut Ui,
    width: f32,
    height: f32,
    radius: f32,
    opacity: f32,
    fill_scale: f32,
    style: HudStyle,
    blur_regions: Option<&mut Vec<BlurRegion>>,
    add_contents: impl FnOnce(&mut Ui),
) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    if let Some(regions) = blur_regions {
        regions.push(BlurRegion::rounded(rect, radius));
    }
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(radius as u8),
        pill_fill(style, opacity, fill_scale),
    );
    let inner = Rect::from_min_max(
        pos2(rect.left() + 12.0, rect.top()),
        pos2(rect.right() - 12.0, rect.bottom()),
    );
    ui.allocate_new_ui(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            add_contents(ui);
        },
    );
}

fn draw_pill_header(
    ui: &mut Ui,
    opacity: f32,
    style: HudStyle,
    lead: HeaderLead,
    title: &str,
    width: f32,
    blur_regions: Option<&mut Vec<BlurRegion>>,
) {
    draw_fixed_pill(
        ui,
        width,
        28.0,
        10.0,
        opacity,
        1.0,
        style,
        blur_regions,
        |ui| {
            match lead {
                HeaderLead::None => {}
                HeaderLead::WhiteRect => {
                    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 2.0, with_alpha(style.accent, opacity * 0.92));
                }
                HeaderLead::KeybindsIcon(texture) => {
                    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 15.0), egui::Sense::hover());
                    ui.painter().image(
                        texture,
                        rect,
                        egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        with_alpha(style.accent_hover, opacity),
                    );
                }
                HeaderLead::Potion => {
                    let (rect, _) = ui.allocate_exact_size(vec2(11.0, 13.0), egui::Sense::hover());
                    draw_potion_icon(
                        ui.painter(),
                        rect,
                        with_alpha(style.accent_hover, opacity * 0.95),
                    );
                }
            }
            ui.add_space(6.0);
            hud_label(ui, RichText::new(title).size(13.0).color(style.text));
        },
    );
}

enum PillLead<'a> {
    None,
    Color(Color32),
    EffectIcon(&'a str),
}

fn draw_pill_row(
    ui: &mut Ui,
    width: f32,
    opacity: f32,
    row_opacity: f32,
    fill_scale: f32,
    style: HudStyle,
    lead: PillLead<'_>,
    left: &str,
    right: &str,
    blur_regions: Option<&mut Vec<BlurRegion>>,
    potion_icons: Option<&mut Vec<PotionIconRequest>>,
) {
    let alpha = (opacity * row_opacity).clamp(0.0, 1.0);
    draw_fixed_pill(
        ui,
        width,
        26.0,
        13.0,
        alpha,
        fill_scale,
        style,
        blur_regions,
        |ui| {
            let text_alpha = (255.0 * alpha) as u8;
            match lead {
                PillLead::None => {}
                PillLead::Color(color) => {
                    let (rect, _) = ui.allocate_exact_size(vec2(7.0, 7.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 2.0, with_alpha(color, alpha));
                    ui.add_space(6.0);
                }
                PillLead::EffectIcon(sprite_id) => {
                    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
                    if let Some(out) = potion_icons {
                        if !sprite_id.is_empty() {
                            out.push(PotionIconRequest {
                                sprite_id: sprite_id.to_owned(),
                                x: rect.left(),
                                y: rect.top(),
                                size: rect.width(),
                            });
                        }
                    }
                    ui.add_space(6.0);
                }
            }

            let right_galley = ui.fonts(|f| {
                f.layout_no_wrap(
                    right.to_owned(),
                    egui::FontId::proportional(12.0),
                    Color32::from_rgba_unmultiplied(
                        style.text.r(),
                        style.text.g(),
                        style.text.b(),
                        text_alpha,
                    ),
                )
            });
            let right_w = right_galley.size().x + 2.0;
            let name_w = (ui.available_width() - right_w).max(20.0);

            ui.allocate_ui_with_layout(
                vec2(name_w, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_max_width(name_w);
                    ui.add(
                        egui::Label::new(RichText::new(left).size(12.0).color(
                            Color32::from_rgba_unmultiplied(
                                style.text.r(),
                                style.text.g(),
                                style.text.b(),
                                text_alpha,
                            ),
                        ))
                        .truncate()
                        .selectable(false),
                    );
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(right_galley).selectable(false));
            });
        },
    );
}

fn draw_potion_icon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let stroke = egui::Stroke::new(1.25, color);
    let neck_w = rect.width() * 0.38;
    let neck_h = rect.height() * 0.28;
    let neck = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + neck_h * 0.55),
        vec2(neck_w, neck_h),
    );
    painter.rect_stroke(neck, 1.0, stroke, egui::StrokeKind::Inside);
    let body = Rect::from_min_max(
        pos2(rect.left() + 1.0, neck.bottom() - 0.5),
        pos2(rect.right() - 1.0, rect.bottom() - 0.5),
    );
    painter.rect_stroke(body, 3.0, stroke, egui::StrokeKind::Inside);
    let fill = Rect::from_min_max(
        pos2(body.left() + 2.2, body.center().y),
        pos2(body.right() - 2.2, body.bottom() - 2.0),
    );
    painter.rect_filled(fill, 2.0, color);
}

fn draw_target_card(
    ui: &mut Ui,
    target: &HudTargetData,
    panel: Color32,
    opacity: f32,
    style: HudStyle,
    icons: &mut HudIconRequests,
) {
    panel_frame(panel, opacity).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 0.0);
        ui.horizontal(|ui| {
            let (head, _) = ui.allocate_exact_size(vec2(32.0, 32.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(head, 6.0, with_alpha(style.extreme, opacity * 0.75));
            icons.faces.push(PlayerFaceIconRequest {
                entity_id: target.entity_id,
                x: head.left(),
                y: head.top(),
                size: head.width(),
            });

            ui.vertical(|ui| {
                hud_label(
                    ui,
                    RichText::new(&target.name)
                        .strong()
                        .size(13.0)
                        .color(style.text),
                );
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let slots = [
                        NameTagItemSlot::Head,
                        NameTagItemSlot::Chest,
                        NameTagItemSlot::Legs,
                        NameTagItemSlot::Feet,
                    ];
                    for (i, slot) in slots.into_iter().enumerate() {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            with_alpha(style.panel, opacity * 0.85),
                        );
                        if target.armor[i] {
                            icons.items.push(NameTagItemIconRequest {
                                entity_id: target.entity_id,
                                slot,
                                inventory_slot: None,
                                x: rect.left(),
                                y: rect.top(),
                                size: rect.width(),
                            });
                        }
                        ui.add_space(3.0);
                    }
                });
            });

            ui.add_space(3.0);
            let ratio = (target.health / target.max_health).clamp(0.0, 1.0);
            let (ring, _) = ui.allocate_exact_size(vec2(38.0, 38.0), egui::Sense::hover());
            draw_health_ring(ui.painter(), ring.center(), 16.0, ratio, opacity, style);
            ui.painter().text(
                ring.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}", target.health),
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
        });
    });
}

fn draw_armor_row(
    ui: &mut Ui,
    local_id: i32,
    armor: [bool; 4],
    opacity: f32,
    icons: &mut HudIconRequests,
) {
    ui.horizontal(|ui| {
        let slots = [
            NameTagItemSlot::Head,
            NameTagItemSlot::Chest,
            NameTagItemSlot::Legs,
            NameTagItemSlot::Feet,
        ];
        for (i, slot) in slots.into_iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                4.0,
                Color32::from_rgba_unmultiplied(20, 20, 24, (140.0 * opacity) as u8),
            );
            if armor[i] {
                icons.items.push(NameTagItemIconRequest {
                    entity_id: local_id,
                    slot,
                    inventory_slot: None,
                    x: rect.left() + 2.0,
                    y: rect.top() + 2.0,
                    size: 18.0,
                });
            }
            ui.add_space(4.0);
        }
    });
}

fn draw_health_ring(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    ratio: f32,
    opacity: f32,
    style: HudStyle,
) {
    let bg = with_alpha(style.panel, opacity * 0.85);
    let fg = with_alpha(style.accent_hover, opacity * 0.95);
    painter.circle_stroke(center, radius, egui::Stroke::new(4.0, bg));
    let segments = 48;
    let start = -std::f32::consts::FRAC_PI_2;
    let sweep = std::f32::consts::TAU * ratio;
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let a = start + sweep * t;
        points.push(pos2(
            center.x + a.cos() * radius,
            center.y + a.sin() * radius,
        ));
    }
    if points.len() >= 2 {
        painter.add(egui::Shape::line(points, egui::Stroke::new(4.0, fg)));
    }
}


fn hud_label(ui: &mut Ui, text: RichText) {
    ui.add(egui::Label::new(text).selectable(false));
}

fn panel_frame(fill: Color32, opacity: f32) -> egui::Frame {
    egui::Frame::NONE
        .fill(with_alpha(fill, opacity))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(8, 6))
}

fn watermark_pill(ui: &mut Ui, opacity: f32, style: HudStyle, add_contents: impl FnOnce(&mut Ui)) {
    let fill = Color32::from_rgba_unmultiplied(
        style.panel.r(),
        style.panel.g(),
        style.panel.b(),
        (185.0 * opacity) as u8,
    );
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                add_contents(ui);
            });
        });
}

fn with_alpha(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        ((color.a() as f32) * opacity.clamp(0.0, 1.0)) as u8,
    )
}

fn format_duration(ticks: i32) -> String {
    if ticks < 0 {
        return "∞".to_string();
    }
    let total_seconds = ticks / 20;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn animate_value(value: &mut f32, visible: bool, dt: f32, speed: f32) -> bool {
    let target = if visible { 1.0 } else { 0.0 };
    let previous = *value;
    *value = (*value + (target - *value) * (dt * speed).clamp(0.0, 1.0)).clamp(0.0, 1.0);
    (previous - *value).abs() > 0.0001
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

fn is_between(value: f32) -> bool {
    value > 0.01 && value < 0.99
}

#[allow(dead_code)]
fn _rect_marker(r: Rect) -> Rect {
    r
}
