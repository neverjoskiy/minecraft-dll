

use egui::{Color32, Context, Rect, TextureId, Ui, pos2, vec2};

use crate::{
    gui_anim::{self, MenuAnimations},
    gui_icons,
    interface::{Bind, Category},
};

use super::{
    BindPopupState, SystemStats, ThemeId, ThemePalette, draw_configs_panel,
    draw_friend_actions_custom, draw_friend_list_custom, draw_module_bind_popup, draw_modules_grid,
    draw_theme_list_custom,
};

pub(crate) const WINDOW_RADIUS: u8 = 8;
pub(crate) const HEADER_H: f32 = 58.0;

pub(crate) const CONTENT_PAD: f32 = 12.0;
pub(crate) const TAB_GAP: f32 = 7.0;
pub(crate) const ICON_TAB: f32 = 14.0;
pub(crate) const ICON_LOGO: f32 = 28.0;
const MENU_SIZE: egui::Vec2 = vec2(820.0, 660.0);

#[derive(Clone, Copy)]
pub(crate) struct MenuStyle {
    pub header_bg: Color32,
    pub main_bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub tab_active_bg: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub brand: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub row_label: Color32,
    pub toggle_on_track: Color32,
    pub toggle_off_track: Color32,
    pub toggle_on_thumb: Color32,
    pub toggle_off_thumb: Color32,
    pub footer_btn: Color32,
    pub slider_track: Color32,
    pub slider_fill: Color32,
}

impl MenuStyle {
    pub(crate) fn from_palette(p: ThemePalette) -> Self {
        Self {
            header_bg: mix(p.panel, p.extreme, 0.35),
            main_bg: p.window,
            card_bg: p.panel,
            card_border: p.stroke,
            tab_active_bg: p.faint,
            accent: p.accent,
            accent_hover: p.accent_hover,
            brand: p.accent,
            text: p.text,
            text_muted: p.muted,
            row_label: mix(p.muted, p.stroke, 0.35),
            toggle_on_track: with_alpha(p.accent, 88),
            toggle_off_track: with_alpha(p.inactive, 72),
            toggle_on_thumb: p.accent_hover,
            toggle_off_thumb: mix(p.muted, p.inactive, 0.45),
            footer_btn: p.faint,
            slider_track: p.inactive,
            slider_fill: p.accent,
        }
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    gui_anim::lerp_color(a, b, t)
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuSection {
    Modules(Category),
    Skins,
    Friends,
    Themes,
    Configs,
}

impl MenuSection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MenuSection::Modules(category) => category.as_str(),
            MenuSection::Skins => "Skins",
            MenuSection::Friends => "Friends",
            MenuSection::Themes => "Themes",
            MenuSection::Configs => "Configs",
        }
    }

    pub(crate) fn anim_key(self) -> u64 {
        match self {
            MenuSection::Modules(Category::Combat) => 10,
            MenuSection::Modules(Category::Movement) => 11,
            MenuSection::Modules(Category::Render) => 12,
            MenuSection::Modules(Category::Player) => 13,
            MenuSection::Modules(Category::World) => 14,
            MenuSection::Modules(Category::Misc) => 15,
            MenuSection::Skins => 20,
            MenuSection::Friends => 21,
            MenuSection::Themes => 22,
            MenuSection::Configs => 23,
        }
    }
}

pub(crate) fn draw_menu(
    ctx: &Context,
    selected_section: &mut MenuSection,
    selected_module: &mut Option<String>,
    bind_popup: &mut Option<BindPopupState>,
    binding_capture_module: &mut Option<String>,
    friend_name_input: &mut String,
    click_friend_bind: &mut Bind,
    click_friend_bind_capture: &mut bool,
    config_name_input: &mut String,
    theme: &mut ThemeId,
    custom_palette: &mut ThemePalette,
    profile_avatar: TextureId,
    profile_banner: TextureId,
    gui_scale_percent: &mut u32,
    language_index: &mut usize,
    system_stats: &SystemStats,
    anim: &mut MenuAnimations,
    dt: f32,
    avatar_preview_texture: Option<TextureId>,
    avatar_preview_request: &mut Option<super::avatar_preview::AvatarPreviewRequest>,
    avatar_preview_yaw: &mut f32,
    avatar_preview_pitch: &mut f32,
    avatar_preview_zoom: &mut f32,
) -> Option<Rect> {
    let palette = match *theme {
        ThemeId::Custom => {
            let mut p = *custom_palette;
            p.name = "Custom";
            p.description = "Editable colors — tune accent and surfaces below";
            p
        }
        other => other.palette(),
    };
    let _ = palette;
    let style = MenuStyle::from_palette(ThemeId::Obstruction.palette());
    let open_t = anim.open_ease();
    if open_t <= 0.001 {
        return None;
    }

    anim.begin_content(selected_section.anim_key(), dt);
    let content_t = anim.content_ease();
    let content_opacity = open_t * content_t;

    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = gui_anim::scale_alpha(style.main_bg, open_t);
    visuals.window_stroke =
        egui::Stroke::new(1.0_f32, gui_anim::scale_alpha(style.card_border, open_t));
    visuals.window_corner_radius = egui::CornerRadius::same(WINDOW_RADIUS);
    visuals.override_text_color = Some(gui_anim::scale_alpha(style.text, open_t));
    visuals.selection.bg_fill = style.accent;
    visuals.widgets.inactive.bg_fill = style.tab_active_bg;
    visuals.widgets.hovered.bg_fill = style.footer_btn;
    visuals.widgets.active.bg_fill = style.accent;
    ctx.set_visuals(visuals);

    let scale = gui_anim::lerp(0.94, 1.0, gui_anim::ease_out_back(open_t.min(1.0)));
    let mut window_rect = None;

    let response = egui::Window::new("XZ GUI")
        .default_pos(pos2(40.0, 40.0))
        .fixed_size(MENU_SIZE)
        .resizable(false)
        .title_bar(false)
        .collapsible(false)
        .frame(
            egui::Frame::NONE
                .fill(gui_anim::scale_alpha(style.main_bg, open_t))
                .corner_radius(egui::CornerRadius::same(WINDOW_RADIUS))
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| {
            let _ = scale;
            ui.set_opacity(open_t);
            ui.set_min_size(MENU_SIZE);
            ui.set_max_size(MENU_SIZE);
            let panel = ui.max_rect();
            window_rect = Some(panel);

            ui.vertical(|ui| {
                ui.set_width(panel.width());
                ui.set_min_height(panel.height());
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                draw_header(
                    ui,
                    panel.width(),
                    selected_section,
                    selected_module,
                    &style,
                    anim,
                    dt,
                );

                let body_h = (panel.height() - HEADER_H).max(80.0);
                ui.allocate_ui_with_layout(
                    vec2(panel.width(), body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_height(body_h);
                        ui.painter().rect_filled(
                            ui.max_rect(),
                            egui::CornerRadius {
                                nw: 0,
                                ne: 0,
                                se: WINDOW_RADIUS,
                                sw: WINDOW_RADIUS,
                            },
                            style.main_bg,
                        );

                        let content_h = body_h.max(40.0);
                        ui.allocate_ui_with_layout(
                            vec2(ui.available_width(), content_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_height(content_h);
                                ui.set_opacity(content_opacity);
                                
                                egui::Frame::NONE
                                    .inner_margin(egui::Margin {
                                        left: CONTENT_PAD as i8,
                                        right: CONTENT_PAD as i8,
                                        top: 0,
                                        bottom: CONTENT_PAD as i8,
                                    })
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        draw_content(
                                            ui,
                                            selected_section,
                                            bind_popup,
                                            binding_capture_module,
                                            friend_name_input,
                                            click_friend_bind,
                                            click_friend_bind_capture,
                                            config_name_input,
                                            theme,
                                            custom_palette,
                                            profile_avatar,
                                            profile_banner,
                                            gui_scale_percent,
                                            language_index,
                                            system_stats,
                                            &style,
                                            anim,
                                            dt,
                                            avatar_preview_texture,
                                            avatar_preview_request,
                                            avatar_preview_yaw,
                                            avatar_preview_pitch,
                                            avatar_preview_zoom,
                                        );
                                    });
                            },
                        );
                    },
                );
            });
        });

    draw_module_bind_popup(
        ctx,
        bind_popup,
        binding_capture_module,
        &style,
        anim,
        dt,
        content_opacity,
    );

    match response {
        Some(inner) if inner.inner.is_some() => window_rect.or(Some(inner.response.rect)),
        _ => None,
    }
}

fn draw_header(
    ui: &mut Ui,
    width: f32,
    selected_section: &mut MenuSection,
    selected_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    let (header_rect, _) = ui.allocate_exact_size(vec2(width, HEADER_H), egui::Sense::hover());
    ui.painter().rect_filled(
        header_rect,
        egui::CornerRadius {
            nw: WINDOW_RADIUS,
            ne: WINDOW_RADIUS,
            se: 0,
            sw: 0,
        },
        style.header_bg,
    );

    ui.allocate_new_ui(
        egui::UiBuilder::new()
            .max_rect(header_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(header_rect);
            ui.add_space(14.0);

            
            let logo_center = pos2(ui.cursor().left() + ICON_LOGO * 0.5, header_rect.center().y);
            ui.allocate_exact_size(vec2(ICON_LOGO, ICON_LOGO), egui::Sense::hover());
            gui_icons::paint(ui, gui_icons::LOGO, logo_center, ICON_LOGO, Color32::WHITE);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Syntax")
                    .color(style.brand)
                    .size(17.0)
                    .strong(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(14.0);
                ui.spacing_mut().item_spacing = vec2(TAB_GAP, 0.0);

                let tabs = [
                    ("Settings", MenuSection::Configs, gui_icons::FOLDER, 5u64),
                    ("Skins", MenuSection::Skins, gui_icons::SKINS, 4),
                    (
                        "Misc",
                        MenuSection::Modules(Category::Misc),
                        gui_icons::MISC,
                        3,
                    ),
                    (
                        "Visuals",
                        MenuSection::Modules(Category::Render),
                        gui_icons::RENDER,
                        2,
                    ),
                    (
                        "Aimbot",
                        MenuSection::Modules(Category::Combat),
                        gui_icons::COMBAT,
                        1,
                    ),
                ];

                for (label, section, glyph, id) in tabs {
                    let active = match section {
                        MenuSection::Modules(Category::Misc) => matches!(
                            *selected_section,
                            MenuSection::Modules(Category::Movement)
                                | MenuSection::Modules(Category::Player)
                                | MenuSection::Modules(Category::Misc)
                                | MenuSection::Themes
                                | MenuSection::Friends
                        ),
                        MenuSection::Skins => *selected_section == MenuSection::Skins,
                        MenuSection::Configs => *selected_section == MenuSection::Configs,
                        other => *selected_section == other,
                    };
                    let active_t = anim.tab(id, active, dt);
                    if draw_header_tab(ui, label, glyph, active_t, style, anim, id + 100, dt)
                        && !active
                    {
                        *selected_section = section;
                        *selected_module = None;
                    }
                }
            });
        },
    );
}

fn draw_header_tab(
    ui: &mut Ui,
    label: &str,
    glyph: char,
    active_t: f32,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    hover_id: u64,
    dt: f32,
) -> bool {
    let text_color = gui_anim::lerp_color(style.text_muted, Color32::WHITE, active_t);
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_string(),
            egui::FontId::proportional(13.0),
            text_color,
        )
    });
    
    let tab_w = (34.0 + 8.0 + galley.size().x + 9.0).max(34.0);
    let size = vec2(tab_w, 34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hover_t = anim.hover(hover_id, response.hovered(), dt);

    let bg_a = (active_t * 255.0 + hover_t * 20.0 * (1.0 - active_t)).clamp(0.0, 255.0) as u8;
    if bg_a > 0 {
        let bg = if active_t > 0.01 {
            gui_anim::scale_alpha(style.tab_active_bg, active_t.max(hover_t * 0.35))
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, (hover_t * 12.0) as u8)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), bg);
    }

    let icon_color = gui_anim::lerp_color(style.text_muted, style.accent, active_t);
    let icon_center = pos2(rect.left() + 17.0, rect.center().y);
    gui_icons::paint(ui, glyph, icon_center, ICON_TAB, icon_color);

    ui.painter().galley(
        pos2(rect.left() + 34.0, rect.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );

    response.clicked()
}

fn draw_content(
    ui: &mut Ui,
    selected_section: &mut MenuSection,
    bind_popup: &mut Option<BindPopupState>,
    binding_capture_module: &mut Option<String>,
    friend_name_input: &mut String,
    click_friend_bind: &mut Bind,
    click_friend_bind_capture: &mut bool,
    config_name_input: &mut String,
    theme: &mut ThemeId,
    custom_palette: &mut ThemePalette,
    profile_avatar: TextureId,
    profile_banner: TextureId,
    gui_scale_percent: &mut u32,
    language_index: &mut usize,
    system_stats: &SystemStats,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
    avatar_preview_texture: Option<TextureId>,
    avatar_preview_request: &mut Option<super::avatar_preview::AvatarPreviewRequest>,
    avatar_preview_yaw: &mut f32,
    avatar_preview_pitch: &mut f32,
    avatar_preview_zoom: &mut f32,
) {
    let show_misc_subtabs = matches!(
        *selected_section,
        MenuSection::Modules(Category::Movement)
            | MenuSection::Modules(Category::Player)
            | MenuSection::Modules(Category::Misc)
            | MenuSection::Themes
            | MenuSection::Friends
    );

    if show_misc_subtabs {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 0.0);
            let sub_tabs = [
                ("Movement", MenuSection::Modules(Category::Movement), 30u64),
                ("Player", MenuSection::Modules(Category::Player), 31),
                ("Misc", MenuSection::Modules(Category::Misc), 33),
                ("Themes", MenuSection::Themes, 34),
                ("Friends", MenuSection::Friends, 35),
            ];
            for (name, section, id) in sub_tabs {
                let active = *selected_section == section;
                let t = anim.tab(id, active, dt);
                if draw_misc_subtab(ui, name, t, style, anim, id + 300, dt) {
                    *selected_section = section;
                }
            }
        });
        ui.add_space(10.0);
    }

    let scroll_h = ui.available_height().max(60.0);
    egui::ScrollArea::vertical()
        .id_salt("figma_content_scroll")
        .auto_shrink([false; 2])
        .max_height(scroll_h)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .animated(true)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_width(ui.available_width());
            match *selected_section {
                MenuSection::Modules(category) => {
                    draw_modules_grid(
                        ui,
                        category,
                        bind_popup,
                        binding_capture_module,
                        style,
                        anim,
                        dt,
                    );
                }
                MenuSection::Skins => {
                    *bind_popup = None;
                    draw_skins_panel(
                        ui,
                        style,
                        avatar_preview_texture,
                        avatar_preview_request,
                        avatar_preview_yaw,
                        avatar_preview_pitch,
                        avatar_preview_zoom,
                    );
                }
                MenuSection::Friends => {
                    *bind_popup = None;
                    ui.columns(2, |cols| {
                        draw_friend_list_custom(&mut cols[0], friend_name_input, style, anim, dt);
                        draw_friend_actions_custom(
                            &mut cols[1],
                            friend_name_input,
                            click_friend_bind,
                            click_friend_bind_capture,
                            style,
                            anim,
                            dt,
                        );
                    });
                }
                MenuSection::Themes => {
                    *bind_popup = None;
                    draw_theme_list_custom(ui, theme, custom_palette, style, anim, dt)
                }
                MenuSection::Configs => {
                    *bind_popup = None;
                    draw_configs_panel(
                        ui,
                        config_name_input,
                        profile_avatar,
                        profile_banner,
                        gui_scale_percent,
                        language_index,
                        system_stats,
                        style,
                        anim,
                        dt,
                    );
                }
            }
        });
}

fn draw_misc_subtab(
    ui: &mut Ui,
    label: &str,
    active_t: f32,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    hover_id: u64,
    dt: f32,
) -> bool {
    let text_color = gui_anim::lerp_color(style.text_muted, style.text, active_t);
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            egui::FontId::proportional(12.0),
            text_color,
        )
    });
    let width = (galley.size().x + 16.0).max(54.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 24.0), egui::Sense::click());
    let hover_t = anim.hover(hover_id, response.hovered(), dt);
    let fill = gui_anim::lerp_color(
        Color32::TRANSPARENT,
        style.tab_active_bg,
        (active_t * 0.72 + hover_t * 0.16).clamp(0.0, 1.0),
    );
    if active_t > 0.01 || hover_t > 0.01 {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), fill);
    }

    ui.painter().galley(
        pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        gui_anim::lerp_color(style.text_muted, style.text, active_t.max(hover_t * 0.45)),
    );

    response.clicked()
}

fn draw_skins_panel(
    ui: &mut Ui,
    style: &MenuStyle,
    preview_texture: Option<TextureId>,
    preview_request: &mut Option<super::avatar_preview::AvatarPreviewRequest>,
    preview_yaw: &mut f32,
    preview_pitch: &mut f32,
    preview_zoom: &mut f32,
) {
    use std::sync::{Mutex, OnceLock};

    use crate::avatar::runtime;

    
    const CARD_W: f32 = 191.0;
    const CARD_H: f32 = 150.0;
    const CARD_GAP: f32 = 10.0;
    const MAIN_PAD: f32 = 11.0;
    
    const BAR_H: f32 = 60.0;
    
    const BAND_UP: f32 = 25.0;
    const BAND_DOWN: f32 = 35.0;

    struct SkinsDraft {
        message: String,
        
        selected: Option<String>,
        
        
        
        live: Option<String>,
        
        detail: Option<String>,
        add_open: bool,
    }

    fn draft() -> &'static Mutex<SkinsDraft> {
        static STATE: OnceLock<Mutex<SkinsDraft>> = OnceLock::new();
        STATE.get_or_init(|| {
            Mutex::new(SkinsDraft {
                message: String::new(),
                selected: None,
                live: None,
                detail: None,
                add_open: false,
            })
        })
    }

    enum Entry<'a> {
        Add,
        Vanilla,
        Avatar(&'a runtime::LoadedAvatar),
    }

    let mut draft_guard = match draft().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let avatars = runtime::list_avatars();
    let preferred = runtime::preferred_local_avatar_id();
    let vanilla = runtime::vanilla_skin_active();
    let known = |id: &str| avatars.iter().any(|avatar| avatar.id == id);
    if draft_guard.detail.as_deref().is_some_and(|id| !known(id)) {
        draft_guard.detail = None;
    }
    if draft_guard.selected.as_deref().is_some_and(|id| !known(id)) {
        draft_guard.selected = None;
    }

    egui::Frame::NONE
        .fill(style.main_bg)
        .inner_margin(egui::Margin::same(MAIN_PAD as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let full_h = ui.available_height().max(320.0);
            ui.set_min_height(full_h);

            if let Some(id) = draft_guard.detail.clone() {
                let Some(avatar) = avatars.iter().find(|avatar| avatar.id == id) else {
                    return;
                };
                
                
                let preview_h = (full_h - BAR_H - 34.0 - CARD_GAP * 2.0).max(180.0);
                let (preview, response) = ui
                    .allocate_exact_size(vec2(ui.available_width(), preview_h), egui::Sense::drag());
                ui.painter().rect_filled(
                    preview,
                    egui::CornerRadius::same(8),
                    mix(style.card_bg, style.main_bg, 0.30),
                );
                ui.painter().rect_stroke(
                    preview,
                    egui::CornerRadius::same(8),
                    egui::Stroke::new(1.0, style.card_border),
                    egui::StrokeKind::Inside,
                );
                if draft_guard.live.as_deref() == Some(id.as_str())
                    && let Some(texture) = preview_texture
                {
                    ui.painter().image(
                        texture,
                        preview.shrink(6.0),
                        Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 0.0)),
                        Color32::WHITE,
                    );
                }
                if response.dragged() {
                    let delta = response.drag_delta();
                    *preview_yaw += delta.x * 0.012;
                    *preview_pitch = (*preview_pitch - delta.y * 0.01).clamp(-1.35, 1.35);
                    ui.ctx().request_repaint();
                }
                if response.hovered() {
                    let scroll = ui.input(|input| input.raw_scroll_delta.y);
                    if scroll.abs() > 0.1 {
                        *preview_zoom = (*preview_zoom * (1.0 + scroll * 0.0015)).clamp(0.45, 2.5);
                        ui.ctx().request_repaint();
                    }
                }
                if response.double_clicked() {
                    *preview_yaw = 0.35;
                    *preview_pitch = -0.08;
                    *preview_zoom = 1.0;
                }
                ui.painter().text(
                    preview.left_top() + vec2(14.0, 16.0),
                    egui::Align2::LEFT_CENTER,
                    &avatar.display_name,
                    egui::FontId::proportional(13.0),
                    style.text,
                );
                ui.painter().text(
                    preview.left_top() + vec2(14.0, 31.0),
                    egui::Align2::LEFT_CENTER,
                    "Drag to orbit / scroll to zoom / double-click to reset",
                    egui::FontId::proportional(9.5),
                    with_alpha(style.text, 130),
                );
                *preview_request = Some(super::avatar_preview::AvatarPreviewRequest {
                    avatar_id: id.clone(),
                    size: [preview.width() as u32, preview.height() as u32],
                    yaw: *preview_yaw,
                    pitch: *preview_pitch,
                    zoom: *preview_zoom,
                });
                draft_guard.live = Some(id.clone());

                ui.add_space(CARD_GAP);
                let models = avatar.package.models.len().to_string();
                let source = if avatar.local_path.is_some() {
                    "Local package"
                } else {
                    "Built-in"
                };
                let status = if !vanilla && avatar.id == preferred {
                    "Applied"
                } else {
                    "Ready"
                };
                ui.horizontal(|ui| {
                    detail_cell(ui, "SOURCE", source, style);
                    detail_cell(ui, "MODELS", &models, style);
                    detail_cell(ui, "STATUS", status, style);
                });

                let bar = Rect::from_min_size(
                    pos2(ui.max_rect().left(), ui.max_rect().bottom() - BAR_H),
                    vec2(ui.max_rect().width(), BAR_H),
                );
                let inner = bar.shrink(10.0);
                let button_w = (inner.width() - CARD_GAP) * 0.5;
                let back = Rect::from_min_size(inner.min, vec2(button_w, inner.height()));
                let apply = Rect::from_min_size(
                    pos2(inner.min.x + button_w + CARD_GAP, inner.min.y),
                    vec2(button_w, inner.height()),
                );
                if bar_button(ui, back, "Back to Choice", false, style).clicked() {
                    draft_guard.detail = None;
                }
                if bar_button(ui, apply, "Apply", true, style).clicked() {
                    draft_guard.message = match runtime::activate_local(&id) {
                        Ok(player) => format!("applied {id} for {player}"),
                        Err(err) => err,
                    };
                }
            } else {
                let avail = ui.available_width();
                let cols = (((avail + CARD_GAP) / (CARD_W + CARD_GAP)).floor() as usize).max(1);
                let card_w =
                    ((avail - CARD_GAP * (cols as f32 - 1.0)) / cols as f32).min(CARD_W * 1.4);
                let mut entries = vec![Entry::Add, Entry::Vanilla];
                entries.extend(avatars.iter().map(Entry::Avatar));
                let textured = draft_guard.live.clone();
                let mut hover_target = None;

                egui::ScrollArea::vertical()
                    .max_height(full_h)
                    .show(ui, |ui| {
                        for row in entries.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = CARD_GAP;
                                for entry in row {
                                    let size = vec2(card_w, CARD_H);
                                    match entry {
                                        Entry::Add if draft_guard.add_open => {
                                            let (rect, response) =
                                                ui.allocate_exact_size(size, egui::Sense::click());
                                            paint_plot_bg(ui, rect, style, false, true);
                                            ui.painter().text(
                                                pos2(rect.center().x, rect.min.y + 22.0),
                                                egui::Align2::CENTER_CENTER,
                                                "Add a new Model",
                                                egui::FontId::proportional(10.0),
                                                style.text_muted,
                                            );
                                            let width = (rect.width() - 34.0).max(60.0);
                                            let folder = Rect::from_center_size(
                                                pos2(rect.center().x, rect.center().y + 2.0),
                                                vec2(width, 26.0),
                                            );
                                            let archive = Rect::from_center_size(
                                                pos2(rect.center().x, rect.center().y + 34.0),
                                                vec2(width, 26.0),
                                            );
                                            let picked =
                                                if bar_button(ui, folder, "Folder", false, style)
                                                    .clicked()
                                                {
                                                    Some(super::file_dialog::pick_avatar_folder())
                                                } else if bar_button(
                                                    ui,
                                                    archive,
                                                    "ZIP / file",
                                                    true,
                                                    style,
                                                )
                                                .clicked()
                                                {
                                                    Some(super::file_dialog::pick_avatar_file())
                                                } else {
                                                    None
                                                };
                                            if let Some(path) = picked {
                                                draft_guard.add_open = false;
                                                draft_guard.message = match path {
                                                    Some(path) => {
                                                        match runtime::load_local_path(&path) {
                                                            Ok(id) => {
                                                                
                                                                
                                                                
                                                                draft_guard.detail =
                                                                    Some(id.clone());
                                                                format!("loaded {id}")
                                                            }
                                                            Err(err) => err.to_string(),
                                                        }
                                                    }
                                                    None => "cancelled".to_string(),
                                                };
                                            } else if response.clicked() {
                                                draft_guard.add_open = false;
                                            }
                                        }
                                        Entry::Add => {
                                            if plot_card(
                                                ui,
                                                size,
                                                gui_icons::FOLDER,
                                                "Add a new Model",
                                                style,
                                                false,
                                            )
                                            .clicked()
                                            {
                                                draft_guard.add_open = true;
                                            }
                                        }
                                        Entry::Vanilla => {
                                            let label = if vanilla {
                                                "None / vanilla skin active"
                                            } else {
                                                "None / vanilla skin"
                                            };
                                            if plot_card(
                                                ui,
                                                size,
                                                gui_icons::SKINS,
                                                label,
                                                style,
                                                vanilla,
                                            )
                                            .clicked()
                                            {
                                                draft_guard.message =
                                                    match runtime::use_vanilla_skin() {
                                                        Ok(()) => {
                                                            "vanilla skin restored".to_string()
                                                        }
                                                        Err(err) => err,
                                                    };
                                            }
                                        }
                                        Entry::Avatar(avatar) => {
                                            let applied = !vanilla && avatar.id == preferred;
                                            let (rect, response) =
                                                ui.allocate_exact_size(size, egui::Sense::click());
                                            let stripe = if applied {
                                                style.accent
                                            } else {
                                                mix(style.accent, style.card_border, 0.55)
                                            };
                                            paint_model_card(
                                                ui,
                                                rect,
                                                style,
                                                stripe,
                                                response.hovered(),
                                                applied,
                                            );
                                            let inner = Rect::from_min_max(
                                                pos2(rect.min.x + 3.0, rect.min.y),
                                                rect.max,
                                            )
                                            .shrink(2.0);
                                            let center = Rect::from_min_max(
                                                pos2(inner.min.x, inner.min.y + BAND_UP),
                                                pos2(inner.max.x, inner.max.y - BAND_DOWN),
                                            );
                                            if let Some(art) =
                                                avatar_art(ui.ctx(), &avatar.id)
                                            {
                                                paint_art_fitted(ui, center, &art);
                                            } else if textured.as_deref()
                                                == Some(avatar.id.as_str())
                                                && let Some(texture) = preview_texture
                                            {
                                                ui.painter().image(
                                                    texture,
                                                    center,
                                                    Rect::from_min_max(
                                                        pos2(0.0, 1.0),
                                                        pos2(1.0, 0.0),
                                                    ),
                                                    Color32::WHITE,
                                                );
                                            } else {
                                                gui_icons::paint(
                                                    ui,
                                                    gui_icons::SKINS,
                                                    center.center(),
                                                    26.0,
                                                    with_alpha(style.text_muted, 110),
                                                );
                                            }
                                            let models = avatar.package.models.len();
                                            ui.painter().text(
                                                pos2(inner.min.x + 5.0, inner.max.y - 22.0),
                                                egui::Align2::LEFT_CENTER,
                                                &avatar.display_name,
                                                egui::FontId::proportional(12.0),
                                                style.text,
                                            );
                                            ui.painter().text(
                                                pos2(inner.min.x + 5.0, inner.max.y - 8.0),
                                                egui::Align2::LEFT_CENTER,
                                                &avatar.id,
                                                egui::FontId::proportional(10.0),
                                                with_alpha(style.text, 128),
                                            );
                                            ui.painter().text(
                                                pos2(inner.max.x - 5.0, inner.max.y - 8.0),
                                                egui::Align2::RIGHT_CENTER,
                                                format!(
                                                    "{models} model{}",
                                                    if models == 1 { "" } else { "s" }
                                                ),
                                                egui::FontId::proportional(10.0),
                                                if applied {
                                                    style.accent
                                                } else {
                                                    style.text_muted
                                                },
                                            );
                                            if response.hovered() {
                                                hover_target = Some(avatar.id.clone());
                                            }
                                            if response.clicked() {
                                                draft_guard.detail = Some(avatar.id.clone());
                                            }
                                        }
                                    }
                                }
                            });
                            ui.add_space(CARD_GAP);
                        }
                    });

                
                
                let target = hover_target
                    .or_else(|| draft_guard.selected.clone())
                    .or_else(|| {
                        avatars
                            .iter()
                            .find(|avatar| avatar.id == preferred)
                            .or_else(|| avatars.first())
                            .map(|avatar| avatar.id.clone())
                    });
                if let Some(id) = target {
                    if draft_guard.selected.as_deref() != Some(id.as_str()) {
                        *preview_yaw = 0.35;
                        *preview_pitch = -0.08;
                        *preview_zoom = 1.0;
                    }
                    draft_guard.selected = Some(id.clone());
                    *preview_request = Some(super::avatar_preview::AvatarPreviewRequest {
                        avatar_id: id.clone(),
                        size: [
                            (card_w - 10.0).max(64.0) as u32,
                            (CARD_H - BAND_UP - BAND_DOWN - 4.0) as u32,
                        ],
                        yaw: *preview_yaw,
                        pitch: *preview_pitch,
                        zoom: *preview_zoom,
                    });
                    draft_guard.live = Some(id);
                }
            }

            if !draft_guard.message.is_empty() {
                let rect = Rect::from_min_size(
                    pos2(ui.max_rect().left(), ui.max_rect().bottom() - 20.0),
                    vec2((ui.max_rect().width() * 0.6).max(220.0), 20.0),
                );
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(4),
                    with_alpha(style.accent, 32),
                );
                ui.painter().text(
                    rect.left_center() + vec2(9.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &draft_guard.message,
                    egui::FontId::proportional(10.0),
                    style.accent,
                );
            }
        });
}


fn paint_plot_bg(ui: &Ui, rect: Rect, style: &MenuStyle, hovered: bool, highlight: bool) {
    let fill = if hovered {
        mix(style.tab_active_bg, style.accent, 0.10)
    } else {
        style.tab_active_bg
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(8),
        egui::Stroke::new(
            1.0,
            if highlight {
                with_alpha(style.accent, 170)
            } else {
                style.card_border
            },
        ),
        egui::StrokeKind::Inside,
    );
}



fn plot_card(
    ui: &mut Ui,
    size: egui::Vec2,
    glyph: char,
    label: &str,
    style: &MenuStyle,
    highlight: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    paint_plot_bg(ui, rect, style, response.hovered(), highlight);
    gui_icons::paint(
        ui,
        glyph,
        pos2(rect.center().x, rect.center().y - 9.0),
        24.0,
        if highlight {
            style.accent_hover
        } else {
            style.accent
        },
    );
    ui.painter().text(
        pos2(rect.center().x, rect.center().y + 17.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        style.text_muted,
    );
    response
}




fn avatar_art(ctx: &Context, id: &str) -> Option<egui::TextureHandle> {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };

    use crate::avatar::runtime;

    static CACHE: OnceLock<Mutex<(u64, HashMap<String, Option<egui::TextureHandle>>)>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new((0, HashMap::new())));
    let generation = runtime::avatar_content_generation();
    let mut guard = cache.lock().ok()?;
    if guard.0 != generation {
        guard.0 = generation;
        guard.1.clear();
    }
    if let Some(entry) = guard.1.get(id) {
        return entry.clone();
    }
    let handle = runtime::avatar_png(id)
        .and_then(|bytes| image::load_from_memory(&bytes).ok())
        .map(|image| {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            ctx.load_texture(
                format!("xz_avatar_art_{id}"),
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                egui::TextureOptions::LINEAR,
            )
        });
    guard.1.insert(id.to_string(), handle.clone());
    handle
}


fn paint_art_fitted(ui: &Ui, rect: Rect, art: &egui::TextureHandle) {
    let size = art.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let fitted = Rect::from_center_size(rect.center(), size * scale);
    ui.painter().image(
        art.id(),
        fitted,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}


fn paint_model_card(
    ui: &Ui,
    rect: Rect,
    style: &MenuStyle,
    stripe: Color32,
    hovered: bool,
    applied: bool,
) {
    let radius = egui::CornerRadius::same(8);
    ui.painter().rect_filled(
        rect,
        radius,
        mix(style.card_bg, stripe, if hovered { 0.22 } else { 0.12 }),
    );
    
    
    let sheen = rect.shrink(2.0);
    let mut mesh = egui::Mesh::default();
    for (pos, alpha) in [
        (sheen.left_top(), 30),
        (sheen.right_top(), 14),
        (sheen.right_bottom(), 0),
        (sheen.left_bottom(), 14),
    ] {
        mesh.colored_vertex(pos, with_alpha(stripe, alpha));
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            1.0,
            if applied {
                with_alpha(style.accent, 150)
            } else {
                style.card_border
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.min.x + 3.0, rect.max.y)),
        egui::CornerRadius {
            nw: 8,
            sw: 8,
            ne: 0,
            se: 0,
        },
        stripe,
    );
    
    if applied {
        ui.painter()
            .circle_filled(pos2(rect.max.x - 13.0, rect.min.y + 13.0), 4.0, stripe);
    }
}



fn bar_button(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    primary: bool,
    style: &MenuStyle,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.id().with(("skins_bar", label)),
        egui::Sense::click(),
    );
    let radius = egui::CornerRadius::same(4);
    let tint = mix(style.accent, style.text_muted, 0.18);
    if primary {
        ui.painter().rect_filled(
            rect,
            radius,
            if response.hovered() {
                style.accent_hover
            } else {
                tint
            },
        );
    } else {
        ui.painter().rect_filled(
            rect,
            radius,
            with_alpha(tint, if response.hovered() { 96 } else { 64 }),
        );
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, tint),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        style.text,
    );
    response
}

fn detail_cell(ui: &mut Ui, label: &str, value: &str, style: &MenuStyle) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(style.text_muted)
                .size(8.0)
                .strong(),
        );
        ui.label(egui::RichText::new(value).color(style.text).size(9.5));
    });
    ui.add_space(18.0);
}

