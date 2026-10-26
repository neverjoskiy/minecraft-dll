use egui::{
    Color32, Context, Event, Key, Mesh, MouseWheelUnit, PointerButton, Pos2, RawInput, Rect, Shape,
    Slider as EguiSlider, Stroke, TextureHandle, TextureId, Ui, ViewportId, ViewportInfo,
    epaint::Vertex, pos2, vec2,
};
use egui_glow::{Painter, glow, glow::HasContext};
use std::{
    ffi::{CString, c_void},
    num::NonZeroU32,
    ptr::null_mut,
    sync::Arc,
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::POINT,
        Graphics::{
            Gdi::{HDC, ScreenToClient, WindowFromDC},
            OpenGL::{
                GL_UNPACK_ALIGNMENT, GL_UNPACK_LSB_FIRST, GL_UNPACK_ROW_LENGTH,
                GL_UNPACK_SKIP_PIXELS, GL_UNPACK_SKIP_ROWS, GL_UNPACK_SWAP_BYTES, HGLRC,
                glPixelStorei, wglGetCurrentContext, wglGetProcAddress,
            },
        },
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
        UI::WindowsAndMessaging::{GetClientRect, GetCursorPos},
    },
    core::{PCSTR, s},
};

use crate::{
    gui_anim::MenuAnimations,
    gui_hud::{self, HudAnimations},
    gui_icons, hooks,
    interface::{Bind, BindMode, BindTargetState, Category, Module, Setting},
    jni_bridge,
    module_system::{ClientModule, HudSnapshot, set_module_enabled, with_registry},
    utils::{
        aim_target::{self, AimAssistOverlay},
        friends,
        geometry::Vec3d,
        hud_data, jump_circle,
        minecraft::{self, MinecraftContext},
        name_tags::{
            self, NameTagEntry, NameTagItemIconRequest, NameTagPreparedIcon, NameTagsConfig,
        },
        player_face, potions, trajectories,
        world_markers::{self, WorldEntityMarker},
    },
};

pub mod anim;
pub mod blur;
pub mod hud;
pub mod icons;
pub mod sky_shader;
mod avatar_preview;
mod file_dialog;
#[path = "menu.rs"]
mod gui_menu;
#[path = "widgets.rs"]
mod gui_widgets;
mod draw;
mod input;
mod renderer;
mod system_stats;
mod theme;
pub(crate) use draw::{
    aim_fov_radius, alpha_color, collect_name_tag_item_icon_requests, color32_from_rgba,
    draw_action_wheel, draw_aim_fov, draw_dim_background, draw_name_tags, draw_neuro_progress,
    draw_target_ghosts_world, draw_world_segment, project_world,
};
pub(crate) use input::{sync_overlay_mouse_control, update_input};
pub(crate) use renderer::{EguiGlStateGuard, OverlayRenderer, drain_gl_errors};
pub(crate) use system_stats::SystemStats;
pub(crate) use theme::{ThemeId, ThemePalette, resolve_palette};
use avatar_preview::{AvatarPreviewRenderer, AvatarPreviewRequest};
use gui_menu::{MenuSection, MenuStyle};

pub struct Overlay {
    egui: Context,
    renderer: OverlayRenderer,
    gl_context: HGLRC,
    menu_open: bool,
    start_time: Instant,
    last_frame: Instant,
    framebuffer_size: [u32; 2],
    logical_size: [f32; 2],
    pixels_per_point: f32,
    mouse_pos: Pos2,
    mouse_left: bool,
    mouse_right: bool,
    mouse_middle: bool,
    selected_section: MenuSection,
    selected_module: Option<String>,
    bind_editor_module: Option<String>,
    bind_popup: Option<BindPopupState>,
    binding_capture_module: Option<String>,
    friend_name_input: String,
    click_friend_bind: Bind,
    click_friend_bind_capture: bool,
    config_name_input: String,
    theme: ThemeId,
    custom_palette: ThemePalette,
    icon_hud_keybinds: TextureHandle,
    profile_avatar: TextureHandle,
    profile_banner: TextureHandle,
    gui_scale_percent: u32,
    language_index: usize,
    system_stats: SystemStats,
    mouse_control_active: bool,
    mouse_regrab_on_close: bool,
    frame_dt: f32,
    hud_animations: HudAnimations,
    menu_animations: MenuAnimations,
    last_module_tick_game_time: Option<i64>,
    menu_blur_rect: Option<Rect>,
    avatar_preview_request: Option<AvatarPreviewRequest>,
    avatar_preview_yaw: f32,
    avatar_preview_pitch: f32,
    avatar_preview_zoom: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct BindPopupState {
    module_name: String,
    pos: Pos2,
}

impl Drop for Overlay {
    fn drop(&mut self) {
        if self.mouse_control_active && self.mouse_regrab_on_close {
            jni_bridge::grab_mouse_after_overlay();
        }
        hooks::set_menu_open(false);
    }
}


thread_local! {
    static OVERLAY: std::cell::RefCell<Option<Overlay>> = const { std::cell::RefCell::new(None) };
}

pub fn render(hdc: HDC) {
    OVERLAY.with(|slot| {
        let mut slot = slot.borrow_mut();
        let gl_context = unsafe { wglGetCurrentContext() };
        if gl_context.0.is_null() {
            return;
        }

        let context_changed = slot
            .as_ref()
            .map(|overlay| overlay.gl_context != gl_context)
            .unwrap_or(true);

        if context_changed {
            *slot = create_overlay(gl_context);
        }

        let Some(overlay) = slot.as_mut() else {
            return;
        };

        let input = update_input(overlay, hdc);
        sync_overlay_mouse_control(overlay);

        jni_bridge::keep_afk_alive();

        let world_active = jni_bridge::is_world_active();
        let game_active = jni_bridge::is_game_active();
        let module_tick_game_time = if game_active {
            jni_bridge::with_env(|env| {
                let Some(mut ctx) = MinecraftContext::new(env)? else {
                    return Ok(None);
                };
                minecraft::level_game_time(&mut ctx).map(Some)
            })
            .flatten()
        } else {
            overlay.last_module_tick_game_time = None;
            None
        };
        let should_tick_modules = module_tick_game_time
            .is_some_and(|game_time| overlay.last_module_tick_game_time != Some(game_time));
        if world_active && crate::modules::refresh_sky_shader_snapshot() {
            overlay.egui.request_repaint();
        }

        
        
        if overlay.menu_open
            && matches!(overlay.selected_section, MenuSection::Skins)
            && let Some(request) = overlay.avatar_preview_request.clone()
        {
            let gl = Arc::clone(overlay.renderer.painter.gl());
            let _gl_state = unsafe { EguiGlStateGuard::reset_for_egui(gl) };
            overlay
                .renderer
                .prepare_avatar_preview(&request, overlay.pixels_per_point);
            overlay.egui.request_repaint();
        }
        let bindings_enabled = game_active && !overlay.menu_open;
        let pressed_keys = if bindings_enabled {
            hooks::take_pressed_keys()
        } else {
            hooks::clear_bind_input();
            Vec::new()
        };
        let (mut hud, name_tags_config) = with_registry(|registry| {
            if bindings_enabled {
                registry.process_binds(&pressed_keys);
                registry.process_action_binds(&pressed_keys);
            }
            if should_tick_modules {
                registry.tick_modules();
            }
            let hud = if world_active {
                registry.hud_snapshot()
            } else {
                HudSnapshot::default()
            };
            let name_tags_config = if world_active {
                registry.name_tags_config()
            } else {
                None
            };
            (hud, name_tags_config)
        })
        .unwrap_or_default();
        if bindings_enabled && pressed_keys.contains(&overlay.click_friend_bind.key_code) {
            add_crosshair_friend();
        }
        
        if world_active && !overlay.menu_open && pressed_keys.contains(&0x2D) {
            crate::avatar::action_wheel::ensure_default_page();
            crate::avatar::action_wheel::toggle();
        }
        if world_active
            && !overlay.menu_open
            && crate::avatar::action_wheel::state_snapshot().open
            && pressed_keys.contains(&0x1B)
        {
            crate::avatar::action_wheel::close();
        }
        if world_active && !overlay.menu_open {
            with_registry(|registry| registry.frame_modules());
        }
        if should_tick_modules {
            overlay.last_module_tick_game_time = module_tick_game_time;
        }

        let vanilla_screen_clear = if world_active {
            jni_bridge::with_env(|env| {
                let mut ctx =
                    MinecraftContext::new(env)?.ok_or(jni::errors::Error::NullPtr("minecraft"))?;
                aim_target::screen_is_none(&mut ctx)
            })
            .unwrap_or(false)
        } else {
            false
        };
        let name_tags_active = world_active
            && !overlay.menu_open
            && vanilla_screen_clear
            && name_tags_config.is_some();
        let _ = jni_bridge::with_env(|env| {
            let result = name_tags::restore_vanilla_player_names(env);
            if result.is_err() {
                jni_bridge::clear_pending_exception(env);
            }
            result
        });

        let name_tags = if name_tags_active {
            name_tags_config
                .as_ref()
                .and_then(|config| {
                    jni_bridge::with_env(|env| {
                        let result = name_tags::collect(env, config, overlay.logical_size);
                        if result.is_err() {
                            jni_bridge::clear_pending_exception(env);
                        }
                        result
                    })
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if name_tags_active {
            let _ = jni_bridge::with_env(|env| {
                let result = name_tags::hide_vanilla_player_names_for_next_frame(env);
                if result.is_err() {
                    jni_bridge::clear_pending_exception(env);
                }
                result
            });
        }

        if hud.enabled {
            hud.potions = potions::collect_active_effects();
            hud_data::refresh();
            hud.world = hud_data::cached();
        } else {
            hud_data::clear();
        }

        let hud_animating = if !world_active {
            overlay.hud_animations = HudAnimations::default();
            hud_data::clear();
            false
        } else if overlay.hud_animations.update(&hud, overlay.frame_dt) {
            true
        } else {
            overlay.hud_animations.is_animating()
        };
        if hud_animating {
            overlay.egui.request_repaint();
        }

        let should_draw_hud = world_active && overlay.hud_animations.should_draw();
        
        let should_draw_name_tags = world_active && !overlay.menu_open && !name_tags.is_empty();
        let should_draw_player_esp = false;
        let aim_overlay = aim_target::overlay_state();
        let should_draw_aim_overlay = world_active && !overlay.menu_open && aim_overlay.visible;
        let target_esp_frame = if world_active && !overlay.menu_open {
            world_markers::target_esp_config()
        } else {
            None
        };
        let china_hat_frame = if world_active && !overlay.menu_open {
            world_markers::china_hat_config()
        } else {
            None
        };
        let should_draw_target_esp = target_esp_frame.is_some();
        let should_draw_china_hat = china_hat_frame.is_some();
        let neuro_progress = crate::utils::neuro::progress_overlay();
        let should_draw_neuro_progress = world_active && neuro_progress.visible;
        if should_draw_name_tags
            || should_draw_player_esp
            || should_draw_target_esp
            || should_draw_china_hat
            || should_draw_neuro_progress
            || crate::avatar::action_wheel::state_snapshot().open
        {
            overlay.egui.request_repaint();
        }

        let mut name_tag_icon_requests = if should_draw_name_tags {
            collect_name_tag_item_icon_requests(&name_tags)
        } else {
            Vec::new()
        };
        let mut player_face_requests = Vec::new();
        let mut potion_icon_requests = Vec::new();

        let mut selected_section = overlay.selected_section;
        let mut selected_module = overlay.selected_module.clone();
        let mut bind_editor_module = overlay.bind_editor_module.clone();
        let mut bind_popup = overlay.bind_popup.clone();
        let mut binding_capture_module = overlay.binding_capture_module.clone();
        let mut friend_name_input = overlay.friend_name_input.clone();
        let mut click_friend_bind = overlay.click_friend_bind.clone();
        let mut click_friend_bind_capture = overlay.click_friend_bind_capture;
        let mut config_name_input = overlay.config_name_input.clone();
        let mut theme = overlay.theme;
        let mut custom_palette = overlay.custom_palette;
        let mut gui_scale_percent = overlay.gui_scale_percent;
        let mut language_index = overlay.language_index;
        overlay.system_stats.refresh_if_needed();
        let system_stats = overlay.system_stats.clone();
        let active_palette = resolve_palette(theme, custom_palette);
        publish_theme_colors(active_palette);
        let mut menu_blur_rect = overlay.menu_blur_rect;
        let mut hud_blur_rects = Vec::new();

        
        
        
        if world_active {
            if let Err(error) = crate::world_render::ensure_armed() {
                
                static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
                if let Ok(mut last) = LAST.lock()
                    && *last != error
                {
                    crate::diag::log(format!("world: ensure_armed -> {error}"));
                    *last = error;
                }
            }
            with_registry(|registry| {
                for module in registry.modules_mut() {
                    let menu = module.menu_module().clone();
                    match menu.name.as_str() {
                        "Trajectories" => trajectories::tick(&menu),
                        "JumpCircle" => jump_circle::tick(&menu),
                        _ => {}
                    }
                }
            });
        } else {
            jump_circle::clear();
        }

        let output = overlay.egui.run(input, |ctx| {
            if let Some(config) = name_tags_config.as_ref() {
                if should_draw_name_tags {
                    draw_name_tags(ctx, &name_tags, config, active_palette);
                }
            }
            let mut hud_icons = gui_hud::HudIconRequests::default();
            if should_draw_hud {
                hud_icons = gui_hud::draw_hud(
                    ctx,
                    &overlay.hud_animations,
                    overlay.menu_open,
                    gui_hud::HudHeaderIcons {
                        keybinds: overlay.icon_hud_keybinds.id(),
                    },
                );
            }
            name_tag_icon_requests.extend(hud_icons.items);
            player_face_requests.extend(hud_icons.faces);
            potion_icon_requests = hud_icons.potions;
            hud_blur_rects = hud_icons.blur_regions;
            if should_draw_aim_overlay {
                draw_aim_fov(ctx, aim_overlay, active_palette);
            }
            if should_draw_neuro_progress {
                draw_neuro_progress(ctx, neuro_progress.clone(), overlay.menu_open);
            }
            let action_wheel_snap = crate::avatar::action_wheel::overlay_snapshot();
            if action_wheel_snap.visible && world_active && !overlay.menu_open {
                draw_action_wheel(ctx, &action_wheel_snap, active_palette);
            }
            overlay
                .menu_animations
                .tick_open(overlay.menu_open, overlay.frame_dt);
            overlay.menu_animations.tick_time(overlay.frame_dt);
            if overlay.menu_animations.visible() {
                if let Some(rect) = gui_menu::draw_menu(
                    ctx,
                    &mut selected_section,
                    &mut selected_module,
                    &mut bind_popup,
                    &mut binding_capture_module,
                    &mut friend_name_input,
                    &mut click_friend_bind,
                    &mut click_friend_bind_capture,
                    &mut config_name_input,
                    &mut theme,
                    &mut custom_palette,
                    overlay.profile_avatar.id(),
                    overlay.profile_banner.id(),
                    &mut gui_scale_percent,
                    &mut language_index,
                    &system_stats,
                    &mut overlay.menu_animations,
                    overlay.frame_dt,
                    overlay.renderer.avatar_preview_texture(),
                    &mut overlay.avatar_preview_request,
                    &mut overlay.avatar_preview_yaw,
                    &mut overlay.avatar_preview_pitch,
                    &mut overlay.avatar_preview_zoom,
                ) {
                    let palette = resolve_palette(theme, custom_palette);
                    draw_dim_background(ctx, palette, overlay.menu_animations.open_ease());
                    menu_blur_rect = Some(rect);
                } else {
                    menu_blur_rect = None;
                }
                bind_editor_module = None;
            } else {
                menu_blur_rect = None;
                bind_popup = None;
                overlay.avatar_preview_request = None;
            }
        });

        if hooks::take_unload_requested() {
            unload_client();
            return;
        }

        overlay.selected_section = selected_section;
        if !matches!(selected_section, MenuSection::Skins) {
            overlay.avatar_preview_request = None;
        }
        overlay.selected_module = selected_module;
        overlay.bind_editor_module = bind_editor_module;
        overlay.bind_popup = bind_popup;
        overlay.binding_capture_module = binding_capture_module;
        overlay.friend_name_input = friend_name_input;
        overlay.click_friend_bind = click_friend_bind;
        overlay.click_friend_bind_capture = click_friend_bind_capture;
        overlay.config_name_input = config_name_input;
        overlay.theme = theme;
        overlay.custom_palette = custom_palette;
        overlay.gui_scale_percent = gui_scale_percent;
        overlay.language_index = language_index;
        overlay.menu_blur_rect = menu_blur_rect;

        let mut blur_rects = hud_blur_rects;
        if let Some(rect) = menu_blur_rect {
            blur_rects.push(crate::gui_blur::BlurRegion::rounded(rect, 8.0));
        }

        let clipped_primitives = overlay
            .egui
            .tessellate(output.shapes, output.pixels_per_point);

        overlay.renderer.render(
            overlay.framebuffer_size,
            output.pixels_per_point,
            &clipped_primitives,
            &output.textures_delta,
            &blur_rects,
        );

        if !name_tag_icon_requests.is_empty()
            || !player_face_requests.is_empty()
            || !potion_icon_requests.is_empty()
        {
            let prepared_icons = jni_bridge::with_env(|env| {
                let mut icons = Vec::new();
                if !name_tag_icon_requests.is_empty() {
                    match name_tags::prepare_item_icons(env, &name_tag_icon_requests) {
                        Ok(prepared) => icons.extend(prepared),
                        Err(_) => jni_bridge::clear_pending_exception(env),
                    }
                }
                if !player_face_requests.is_empty() {
                    match player_face::prepare_player_faces(env, &player_face_requests) {
                        Ok(prepared) => icons.extend(prepared),
                        Err(_) => jni_bridge::clear_pending_exception(env),
                    }
                }
                if !potion_icon_requests.is_empty() {
                    match potions::prepare_effect_icons(env, &potion_icon_requests) {
                        Ok(prepared) => icons.extend(prepared),
                        Err(_) => jni_bridge::clear_pending_exception(env),
                    }
                }
                Ok(icons)
            })
            .unwrap_or_default();
            overlay
                .renderer
                .render_item_icons(overlay.framebuffer_size, &prepared_icons);
        }
    });
}

fn load_icon(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let (w, h, mut rgba) = match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            (
                rgba.width() as usize,
                rgba.height() as usize,
                rgba.into_raw(),
            )
        }
        Err(_) => (32, 32, vec![255_u8, 255, 255, 255].repeat(32 * 32)),
    };

    
    for px in rgba.chunks_exact_mut(4) {
        let r = px[0];
        let g = px[1];
        let b = px[2];
        if px[3] > 0 && r < 12 && g < 12 && b < 12 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        }
    }

    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

fn load_texture(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let (w, h, rgba) = match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            (
                rgba.width() as usize,
                rgba.height() as usize,
                rgba.into_raw(),
            )
        }
        Err(_) => (1, 1, vec![0_u8, 0, 0, 0]),
    };

    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}

fn create_solid_texture(
    ctx: &Context,
    name: &str,
    size: [usize; 2],
    color: Color32,
) -> TextureHandle {
    let mut rgba = Vec::with_capacity(size[0] * size[1] * 4);
    for _ in 0..(size[0] * size[1]) {
        rgba.extend_from_slice(&[color.r(), color.g(), color.b(), color.a()]);
    }
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
    ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR)
}


fn create_keybinds_icon_texture(ctx: &Context) -> TextureHandle {
    const OUT_W: u32 = 96;
    const OUT_H: u32 = 72;

    let (w, h, mut rgba) =
        match image::load_from_memory(include_bytes!("../../resources/Keybinds.ttf")) {
            Ok(img) => {
                let img = img.resize(OUT_W, OUT_H, image::imageops::FilterType::CatmullRom);
                let rgba = img.to_rgba8();
                (
                    rgba.width() as usize,
                    rgba.height() as usize,
                    rgba.into_raw(),
                )
            }
            Err(_) => {
                return ctx.load_texture(
                    "hud_keybinds_icon",
                    egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]),
                    egui::TextureOptions::LINEAR,
                );
            }
        };

    
    let is_bg = |px: &[u8]| -> bool {
        if px[3] < 16 {
            return true;
        }
        let lum = (px[0] as u16 + px[1] as u16 + px[2] as u16) / 3;
        lum < 28
    };

    let mut visited = vec![false; w * h];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for &(x, y) in &[(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        stack.push((x, y));
    }
    while let Some((x, y)) = stack.pop() {
        let idx = y * w + x;
        if visited[idx] {
            continue;
        }
        visited[idx] = true;
        let i = idx * 4;
        if !is_bg(&rgba[i..i + 4]) {
            continue;
        }
        rgba[i] = 0;
        rgba[i + 1] = 0;
        rgba[i + 2] = 0;
        rgba[i + 3] = 0;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }

    
    for px in rgba.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let lum = (px[0] as u16 + px[1] as u16 + px[2] as u16) / 3;
        if lum > 200 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        } else if lum < 50 {
            px[0] = 18;
            px[1] = 18;
            px[2] = 22;
        }
    }

    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    ctx.load_texture(
        "hud_keybinds_icon",
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

fn create_overlay(gl_context: HGLRC) -> Option<Overlay> {
    reset_pixel_unpack_state();
    let egui = Context::default();
    gui_icons::install(&egui);
    let theme = ThemeId::Obstruction;
    apply_theme(&egui, theme.palette());
    let custom_palette = ThemeId::Custom.palette();

    let icon_hud_keybinds = create_keybinds_icon_texture(&egui);
    let profile_avatar = load_texture(
        &egui,
        "settings_profile_avatar",
        include_bytes!("../../resources/profile_avatar.png"),
    );
    let profile_banner = load_texture(
        &egui,
        "settings_profile_banner",
        include_bytes!("../../resources/profile_banner.jpg"),
    );

    let gl =
        unsafe { glow::Context::from_loader_function(|name| load_gl_proc(name) as *const c_void) };
    let renderer = OverlayRenderer::new(gl)?;
    let now = Instant::now();

    Some(Overlay {
        egui,
        renderer,
        gl_context,
        menu_open: true,
        start_time: now,
        last_frame: now,
        framebuffer_size: [1, 1],
        logical_size: [1.0, 1.0],
        pixels_per_point: 1.0,
        mouse_pos: Pos2::ZERO,
        mouse_left: false,
        mouse_right: false,
        mouse_middle: false,
        selected_section: MenuSection::Modules(Category::Combat),
        selected_module: None,
        bind_editor_module: None,
        bind_popup: None,
        binding_capture_module: None,
        friend_name_input: String::new(),
        click_friend_bind: Bind::new("None", 0),
        click_friend_bind_capture: false,
        config_name_input: crate::config::active_name(),
        theme,
        custom_palette,
        icon_hud_keybinds,
        profile_avatar,
        profile_banner,
        gui_scale_percent: 100,
        language_index: 0,
        system_stats: SystemStats::new(),
        mouse_control_active: false,
        mouse_regrab_on_close: false,
        frame_dt: 1.0 / 60.0,
        hud_animations: HudAnimations::default(),
        menu_animations: {
            let mut anim = MenuAnimations::default();
            anim.open = 1.0;
            anim.content = 1.0;
            anim
        },
        last_module_tick_game_time: None,
        menu_blur_rect: None,
        avatar_preview_request: None,
        avatar_preview_yaw: 0.35,
        avatar_preview_pitch: -0.08,
        avatar_preview_zoom: 1.0,
    })
}

fn publish_theme_colors(palette: ThemePalette) {
    crate::theme_colors::set_from_color32(
        palette.accent,
        palette.accent_hover,
        palette.panel,
        palette.window,
        palette.stroke,
        palette.text,
        palette.extreme,
    );
}

fn apply_theme(ctx: &Context, palette: ThemePalette) {
    publish_theme_colors(palette);
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(palette.text);
    visuals.window_fill = palette.window;
    visuals.panel_fill = palette.panel;
    visuals.faint_bg_color = palette.faint;
    visuals.extreme_bg_color = palette.extreme;
    visuals.code_bg_color = palette.code;
    visuals.hyperlink_color = palette.accent_hover;
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke = egui::Stroke::new(1.0, palette.text);
    visuals.window_stroke = egui::Stroke::new(1.0, palette.stroke);
    visuals.window_corner_radius = egui::CornerRadius::same(6);
    visuals.menu_corner_radius = egui::CornerRadius::same(6);
    visuals.window_shadow = Default::default();
    visuals.popup_shadow = Default::default();
    visuals.widgets.noninteractive.bg_fill = palette.window;
    visuals.widgets.noninteractive.weak_bg_fill = palette.panel;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, palette.stroke);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, palette.text);
    visuals.widgets.inactive.bg_fill = palette.inactive;
    visuals.widgets.inactive.weak_bg_fill = palette.inactive;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, palette.stroke);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, palette.text);
    visuals.widgets.hovered.bg_fill = palette.hovered;
    visuals.widgets.hovered.weak_bg_fill = palette.hovered;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, palette.accent_hover);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, palette.text);
    visuals.widgets.active.bg_fill = palette.accent;
    visuals.widgets.active.weak_bg_fill = palette.accent;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, palette.accent_hover);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, palette.text);
    visuals.widgets.open.bg_fill = palette.hovered;
    visuals.widgets.open.weak_bg_fill = palette.hovered;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, palette.accent);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, palette.text);
    ctx.set_visuals(visuals);
}


fn custom_toggle_switch(
    ui: &mut Ui,
    value: &mut bool,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 16.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
    }

    let t = anim.toggle(anim_id, *value, dt);
    let track = crate::gui_anim::lerp_color(style.toggle_off_track, style.toggle_on_track, t);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(16), track);

    let thumb_radius = 5.0;
    let thumb_x = crate::gui_anim::lerp(rect.min.x + 8.0, rect.max.x - 8.0, t);
    let thumb = crate::gui_anim::lerp_color(style.toggle_off_thumb, style.toggle_on_thumb, t);
    ui.painter()
        .circle_filled(pos2(thumb_x, rect.center().y), thumb_radius, thumb);

    if t > 0.2 {
        ui.painter().circle_filled(
            pos2(thumb_x, rect.center().y),
            thumb_radius + 2.5,
            crate::gui_anim::with_alpha(style.accent, (t * 24.0) as u8),
        );
        ui.painter()
            .circle_filled(pos2(thumb_x, rect.center().y), thumb_radius, thumb);
    }

    response.clicked()
}


fn custom_slider(
    ui: &mut Ui,
    name: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) {
    gui_widgets::bar_slider_anim(ui, name, value, min, max, style, Some(anim), anim_id, dt);
}

fn custom_checkbox(
    ui: &mut Ui,
    value: &mut bool,
    name: &str,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) {
    fixed_height_row(ui, 39.0, |ui| {
        let t = anim.toggle(anim_id, *value, 0.0);
        let label_color = crate::gui_anim::lerp_color(style.row_label, style.text, t);
        ui.label(egui::RichText::new(name).color(label_color).size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            custom_toggle_switch(ui, value, style, anim, anim_id, dt);
        });
    });
}

fn draw_custom_setting(
    ui: &mut Ui,
    module_name: &str,
    setting: &mut Setting,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    match setting {
        Setting::CheckBox(s) => {
            let id = egui::Id::new("set_chk")
                .with(module_name)
                .with(s.name.as_str())
                .value();
            custom_checkbox(ui, &mut s.value, &s.name, style, anim, id, dt);
        }
        Setting::Slider(s) => {
            let id = egui::Id::new("set_sld")
                .with(module_name)
                .with(s.name.as_str())
                .value();
            custom_slider(ui, &s.name, &mut s.value, s.min, s.max, style, anim, id, dt);
        }
        Setting::ColorPicker(s) => {
            gui_widgets::color_picker_row_anim(
                ui,
                &s.name,
                &mut s.value,
                style,
                (module_name, s.name.as_str()),
                Some(anim),
                dt,
            );
        }
        Setting::Choice(s) => {
            let id = egui::Id::new("set_choice")
                .with(module_name)
                .with(s.name.as_str())
                .value();
            let label = s.name.clone();
            draw_mode_list(ui, &label, s, style, anim, id, dt);
        }
        Setting::ActionBind(s) => {
            let id = egui::Id::new("set_abind")
                .with(module_name)
                .with(s.name.as_str())
                .value();
            fixed_height_row(ui, 39.0, |ui| {
                ui.label(
                    egui::RichText::new(&s.name)
                        .color(style.row_label)
                        .size(13.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let capture_id = format!("action:{}:{}", module_name, s.name);
                    draw_bind_control(
                        ui,
                        &mut s.bind,
                        capture_id,
                        binding_capture_module,
                        style,
                        anim,
                        id,
                        dt,
                    );
                });
            });
        }
    }
}

fn draw_modules_grid(
    ui: &mut Ui,
    category: Category,
    bind_popup: &mut Option<BindPopupState>,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    const GAP: f32 = 10.0;
    let total_w = ui.available_width().max(160.0);
    let col_w = ((total_w - GAP) * 0.5).max(140.0);

    let _ = with_registry(|registry| {
        let modules = registry.modules_mut();
        let mut indices: Vec<usize> = modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.menu_module().category == category)
            .map(|(index, _)| index)
            .collect();

        if category == Category::Render {
            indices.sort_by_key(|&index| match modules[index].menu_module().name.as_str() {
                "SkyShader" => 0,
                "Trajectories" => 1,
                "Chams" => 2,
                "TargetESP" => 3,
                "ChinaHat" => 4,
                "NameTags" => 5,
                "NoOverlay" => 6,
                "FullBright" => 7,
                "ChangeTime" => 8,
                _ => 100,
            });
        }

        if category == Category::Combat {
            draw_combat_grid(
                ui,
                modules,
                &indices,
                col_w,
                total_w,
                binding_capture_module,
                bind_popup,
                style,
                anim,
                dt,
            );
            return;
        }

        ui.horizontal_top(|ui| {
            ui.set_min_width(total_w);
            ui.vertical(|ui| {
                ui.set_width(col_w);
                ui.set_min_width(col_w);
                ui.set_max_width(col_w);
                for (n, &index) in indices.iter().enumerate() {
                    if n % 2 != 0 {
                        continue;
                    }
                    draw_module_card(
                        ui,
                        modules[index].as_mut(),
                        bind_popup,
                        binding_capture_module,
                        style,
                        anim,
                        dt,
                    );
                    ui.add_space(GAP);
                }
            });
            ui.add_space(GAP);
            ui.vertical(|ui| {
                ui.set_width(col_w);
                ui.set_min_width(col_w);
                ui.set_max_width(col_w);
                for (n, &index) in indices.iter().enumerate() {
                    if n % 2 != 1 {
                        continue;
                    }
                    draw_module_card(
                        ui,
                        modules[index].as_mut(),
                        bind_popup,
                        binding_capture_module,
                        style,
                        anim,
                        dt,
                    );
                    ui.add_space(GAP);
                }
            });
        });
    });
}

fn draw_combat_grid(
    ui: &mut Ui,
    modules: &mut [Box<dyn ClientModule>],
    indices: &[usize],
    col_w: f32,
    total_w: f32,
    binding_capture_module: &mut Option<String>,
    bind_popup: &mut Option<BindPopupState>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    const GAP: f32 = 10.0;
    let aim_index = indices
        .iter()
        .copied()
        .find(|&index| modules[index].menu_module().name == "AimAssist");
    let others: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&index| modules[index].menu_module().name != "AimAssist")
        .collect();

    ui.horizontal_top(|ui| {
        ui.set_min_width(total_w);
        ui.vertical(|ui| {
            ui.set_width(col_w);
            ui.set_min_width(col_w);
            ui.set_max_width(col_w);
            if let Some(index) = aim_index {
                draw_aim_assist_sections(
                    ui,
                    modules[index].as_mut(),
                    0..2,
                    binding_capture_module,
                    bind_popup,
                    style,
                    anim,
                    dt,
                );
                ui.add_space(GAP);
            }
        });
        ui.add_space(GAP);
        ui.vertical(|ui| {
            ui.set_width(col_w);
            ui.set_min_width(col_w);
            ui.set_max_width(col_w);
            for &index in &others {
                draw_module_card(
                    ui,
                    modules[index].as_mut(),
                    bind_popup,
                    binding_capture_module,
                    style,
                    anim,
                    dt,
                );
                ui.add_space(GAP);
            }
            if let Some(index) = aim_index {
                draw_aim_assist_sections(
                    ui,
                    modules[index].as_mut(),
                    2..4,
                    binding_capture_module,
                    bind_popup,
                    style,
                    anim,
                    dt,
                );
            }
        });
    });
}


fn fixed_height_row(ui: &mut Ui, height: f32, add_contents: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        add_contents,
    );
}

fn draw_module_card(
    ui: &mut Ui,
    module: &mut dyn ClientModule,
    bind_popup: &mut Option<BindPopupState>,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    if module.menu_module().name == "AimAssist" {
        draw_aim_assist_sections(
            ui,
            module,
            0..4,
            binding_capture_module,
            bind_popup,
            style,
            anim,
            dt,
        );
        return;
    }

    let all_settings: Vec<usize> = (0..module.menu_module().settings.len()).collect();
    let title = display_module_name(module.menu_module().name.as_str()).to_owned();
    draw_module_section_card(
        ui,
        module,
        &title,
        true,
        true,
        &all_settings,
        binding_capture_module,
        bind_popup,
        style,
        anim,
        dt,
    );
}

fn draw_aim_assist_sections(
    ui: &mut Ui,
    module: &mut dyn ClientModule,
    section_range: std::ops::Range<usize>,
    binding_capture_module: &mut Option<String>,
    bind_popup: &mut Option<BindPopupState>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    let groups: [(&str, bool, &[&str]); 4] = [
        (
            "Aimbot",
            true,
            &[
                "Draw FOV radius",
                "Only with weapon",
                "Only players",
                "No aim in inventory",
                "Disable on world change",
            ],
        ),
        (
            "Target",
            false,
            &[
                "Distance",
                "FOV",
                "Priority",
                "Hit invisible",
                "Only armored",
                "Ignore naked",
                "Only X",
            ],
        ),
        (
            "Accuracy",
            false,
            &["Speed", "Multipoint", "Wall check", "Micro movements"],
        ),
        (
            "Neuro",
            false,
            &["Rotation Mode", "Neuro max samples", "Reset Neuro profile"],
        ),
    ];

    let first_section = section_range.start;
    for section_index in section_range {
        let (title, include_bind, names) = groups[section_index];
        let setting_indices: Vec<usize> = module
            .menu_module()
            .settings
            .iter()
            .enumerate()
            .filter(|(_, setting)| names.contains(&setting_name(setting)))
            .map(|(index, _)| index)
            .collect();

        if !include_bind && setting_indices.is_empty() {
            continue;
        }

        if section_index > first_section {
            ui.add_space(10.0);
        }
        draw_module_section_card(
            ui,
            module,
            title,
            section_index == 0,
            include_bind,
            &setting_indices,
            binding_capture_module,
            bind_popup,
            style,
            anim,
            dt,
        );
    }
}

fn draw_module_section_card(
    ui: &mut Ui,
    module: &mut dyn ClientModule,
    title: &str,
    include_toggle: bool,
    include_bind: bool,
    setting_indices: &[usize],
    binding_capture_module: &mut Option<String>,
    bind_popup: &mut Option<BindPopupState>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    let category = module.menu_module().category;
    let icon = match category {
        Category::Combat => gui_icons::COMBAT,
        Category::Movement => gui_icons::MOVEMENT,
        Category::Render => gui_icons::RENDER,
        Category::Player | Category::World | Category::Misc => gui_icons::MISC,
    };
    let card_frame = egui::Frame::NONE
        .fill(style.card_bg)
        .stroke(egui::Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(
                style.card_border.r(),
                style.card_border.g(),
                style.card_border.b(),
                26,
            ),
        ))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(0));

    card_frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_max_width(ui.available_width());

        let mut toggle_clicked = false;
        let mut new_enabled = false;
        let toggle_id = egui::Id::new("mod_toggle")
            .with(module.menu_module().name.as_str())
            .with(title)
            .value();

        egui::Frame::NONE
            .fill(crate::gui_anim::with_alpha(style.tab_active_bg, 34))
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 10,
                bottom: 10,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let menu = module.menu_module_mut();
                let header_width = ui.available_width();
                let (header_rect, header_response) =
                    ui.allocate_exact_size(vec2(header_width, 24.0), egui::Sense::click());
                if include_bind && header_response.secondary_clicked() {
                    *bind_popup = Some(BindPopupState {
                        module_name: menu.name.clone(),
                        pos: pos2(header_rect.right() + 8.0, header_rect.top()),
                    });
                }

                ui.allocate_new_ui(
                    egui::UiBuilder::new()
                        .max_rect(header_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| {
                        let icon_center = pos2(ui.cursor().left() + 8.0, ui.cursor().top() + 12.0);
                        ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                        gui_icons::paint(ui, icon, icon_center, 15.0, style.brand);
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(title)
                                .color(style.text)
                                .size(13.0)
                                .strong(),
                        );
                        if include_toggle {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut enabled = menu.enabled;
                                    if custom_toggle_switch(
                                        ui,
                                        &mut enabled,
                                        style,
                                        anim,
                                        toggle_id,
                                        dt,
                                    ) {
                                        toggle_clicked = true;
                                        new_enabled = enabled;
                                    }
                                },
                            );
                        }
                    },
                );
            });

        if toggle_clicked {
            set_module_enabled(module, new_enabled);
        }

        let has_settings = !setting_indices.is_empty();
        if has_settings {
            egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: 14,
                    right: 14,
                    top: 8,
                    bottom: 12,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let menu = module.menu_module_mut();

                    let module_name = menu.name.clone();
                    for (row, &si) in setting_indices.iter().enumerate() {
                        if let Some(setting) = menu.settings.get_mut(si) {
                            draw_custom_setting(
                                ui,
                                &module_name,
                                setting,
                                binding_capture_module,
                                style,
                                anim,
                                dt,
                            );
                            if row + 1 < setting_indices.len() {
                                gui_widgets::faint_separator_anim(
                                    ui,
                                    style,
                                    Some(anim),
                                    egui::Id::new("sep")
                                        .with(module_name.as_str())
                                        .with(title)
                                        .with(si)
                                        .value(),
                                    dt,
                                );
                            }
                        }
                    }
                });
        }
    });
}

fn display_module_name(name: &str) -> &str {
    match name {
        "AimAssist" => "Aimbot",
        "NoFriendDamage" => "No Friend Damage",
        "TriggerBot" => "Trigger Bot",
        "NoJumpDelay" => "No Jump Delay",
        "PlayerESP" => "Player ESP",
        other => other,
    }
}

fn setting_name(setting: &Setting) -> &str {
    match setting {
        Setting::CheckBox(s) => s.name.as_str(),
        Setting::Slider(s) => s.name.as_str(),
        Setting::ColorPicker(s) => s.name.as_str(),
        Setting::ActionBind(s) => s.name.as_str(),
        Setting::Choice(s) => s.name.as_str(),
    }
}

fn draw_settings_card(
    ui: &mut Ui,
    title: &str,
    style: &MenuStyle,
    add_contents: impl FnOnce(&mut Ui),
) {
    egui::Frame::NONE
        .fill(style.card_bg)
        .stroke(egui::Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(
                style.card_border.r(),
                style.card_border.g(),
                style.card_border.b(),
                36,
            ),
        ))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Frame::NONE
                .fill(crate::gui_anim::with_alpha(style.tab_active_bg, 34))
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 16,
                    top: 13,
                    bottom: 13,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("в°").color(style.brand).size(22.0));
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(title)
                                .color(style.text)
                                .size(15.0)
                                .strong(),
                        );
                    });
                });
            egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: 18,
                    right: 18,
                    top: 14,
                    bottom: 14,
                })
                .show(ui, add_contents);
        });
}

fn draw_labeled_text_edit(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    style: &MenuStyle,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(style.row_label).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width((ui.available_width() * 0.58).max(170.0))
                    .hint_text(hint),
            )
        })
        .inner
    })
    .inner
}

fn draw_info_line(ui: &mut Ui, label: &str, value: &str, style: &MenuStyle) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{label}:"))
                .color(style.text)
                .size(15.0)
                .strong(),
        );
        ui.label(
            egui::RichText::new(value)
                .color(style.brand)
                .size(15.0)
                .strong(),
        );
    });
}

fn draw_bind_control(
    ui: &mut Ui,
    bind: &mut Bind,
    capture_id: String,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) {
    let width = 158.0_f32.min(ui.available_width().max(112.0));
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), egui::Sense::hover());
    let key_rect = Rect::from_min_max(rect.min, pos2(rect.right() - 54.0, rect.bottom()));
    let mode_rect = Rect::from_min_max(pos2(rect.right() - 54.0, rect.top()), rect.max);
    let key_response = ui.interact(key_rect, response.id.with("key"), egui::Sense::click());
    let mode_response = ui.interact(mode_rect, response.id.with("mode"), egui::Sense::click());
    let is_capturing = binding_capture_module.as_deref() == Some(capture_id.as_str());
    let key_hover = anim.hover(anim_id, key_response.hovered() || is_capturing, dt);
    let mode_hover = anim.hover(anim_id.wrapping_add(1), mode_response.hovered(), dt);
    let fill = crate::gui_anim::lerp_color(
        crate::gui_anim::with_alpha(style.tab_active_bg, 128),
        crate::gui_anim::with_alpha(style.accent, 70),
        key_hover * 0.55,
    );

    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(5), fill);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(
            1.0,
            crate::gui_anim::lerp_color(
                crate::gui_anim::with_alpha(style.card_border, 170),
                style.accent,
                key_hover.max(mode_hover),
            ),
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().vline(
        mode_rect.left(),
        (rect.top() + 5.0)..=(rect.bottom() - 5.0),
        egui::Stroke::new(1.0, crate::gui_anim::with_alpha(style.card_border, 120)),
    );

    let key_text = if is_capturing {
        "Press key"
    } else {
        bind.key_name.as_str()
    };
    ui.painter().text(
        pos2(key_rect.left() + 9.0, key_rect.center().y),
        egui::Align2::LEFT_CENTER,
        "KEY",
        egui::FontId::proportional(9.0),
        style.text_muted,
    );
    ui.painter().text(
        pos2(key_rect.left() + 34.0, key_rect.center().y),
        egui::Align2::LEFT_CENTER,
        key_text,
        egui::FontId::proportional(12.0),
        crate::gui_anim::lerp_color(style.row_label, style.text, key_hover),
    );
    ui.painter().text(
        mode_rect.center(),
        egui::Align2::CENTER_CENTER,
        bind.mode.as_str(),
        egui::FontId::proportional(11.0),
        crate::gui_anim::lerp_color(style.row_label, style.text, mode_hover),
    );

    if key_response.clicked() || (key_response.hovered() && ui.input(|i| i.pointer.middle_down())) {
        hooks::clear_last_key_pressed();
        *binding_capture_module = Some(capture_id);
    }
    if mode_response.clicked() {
        bind.mode = bind.mode.toggle();
    }
    if is_capturing {
        if let Some(key_code) = hooks::take_last_key_pressed() {
            if apply_captured_bind_key(key_code, &mut bind.key_code, &mut bind.key_name) {
                *binding_capture_module = None;
            }
        }
    }
}

pub(crate) fn draw_module_bind_popup(
    ctx: &Context,
    bind_popup: &mut Option<BindPopupState>,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
    opacity: f32,
) {
    let Some(state) = bind_popup.clone() else {
        return;
    };

    let mut should_close = false;
    let mut drag_delta = egui::Vec2::ZERO;
    egui::Area::new(egui::Id::new("module_bind_popup"))
        .order(egui::Order::Foreground)
        .fixed_pos(state.pos)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);
            let mut module_found = false;
            egui::Frame::NONE
                .fill(crate::gui_anim::scale_alpha(style.main_bg, opacity))
                .stroke(egui::Stroke::new(
                    1.0,
                    crate::gui_anim::scale_alpha(style.card_border, opacity),
                ))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.set_min_width(286.0);
                    ui.set_max_width(286.0);
                    let _ = with_registry(|registry| {
                        let Some(module) = registry
                            .modules_mut()
                            .iter_mut()
                            .find(|module| module.menu_module().name == state.module_name)
                        else {
                            return;
                        };
                        module_found = true;
                        let menu = module.menu_module_mut();
                        let title = display_module_name(menu.name.as_str()).to_owned();
                        let (header_rect, header_response) = ui
                            .allocate_exact_size(vec2(286.0, 38.0), egui::Sense::click_and_drag());
                        if header_response.dragged() {
                            drag_delta = ui.input(|input| input.pointer.delta());
                        }
                        ui.painter().rect_filled(
                            header_rect,
                            egui::CornerRadius {
                                nw: 6,
                                ne: 6,
                                sw: 0,
                                se: 0,
                            },
                            style.header_bg,
                        );
                        ui.painter().text(
                            pos2(header_rect.left() + 12.0, header_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("Bind: {title}"),
                            egui::FontId::proportional(13.0),
                            style.text,
                        );
                        let close_rect = Rect::from_center_size(
                            pos2(header_rect.right() - 18.0, header_rect.center().y),
                            vec2(22.0, 22.0),
                        );
                        let close_response = ui.interact(
                            close_rect,
                            ui.id().with("bind_popup_close"),
                            egui::Sense::click(),
                        );
                        let close_hover = anim.hover(
                            egui::Id::new("bind_popup_close_hover")
                                .with(menu.name.as_str())
                                .value(),
                            close_response.hovered(),
                            dt,
                        );
                        if close_hover > 0.01 {
                            ui.painter().rect_filled(
                                close_rect,
                                egui::CornerRadius::same(4),
                                crate::gui_anim::with_alpha(
                                    style.accent,
                                    (close_hover * 70.0) as u8,
                                ),
                            );
                        }
                        ui.painter().text(
                            close_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "x",
                            egui::FontId::proportional(13.0),
                            crate::gui_anim::lerp_color(style.text_muted, style.text, close_hover),
                        );
                        if close_response.clicked() {
                            should_close = true;
                        }

                        if menu.bind.is_none() {
                            menu.bind = Some(Bind::new("None", 0));
                        }
                        let bind = menu.bind.as_mut().expect("bind was just created");
                        egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: 12,
                                right: 12,
                                top: 10,
                                bottom: 12,
                            })
                            .show(ui, |ui| {
                                draw_bind_popup_row_mode(ui, bind, style, anim, dt);
                                ui.add_space(8.0);
                                if draw_bind_popup_row_key(
                                    ui,
                                    bind,
                                    format!("bind:{}", menu.name),
                                    binding_capture_module,
                                    style,
                                    anim,
                                    egui::Id::new("bind_popup_key")
                                        .with(menu.name.as_str())
                                        .value(),
                                    dt,
                                ) {
                                    should_close = true;
                                }
                            });
                    });
                });
            if !module_found {
                should_close = true;
            }
        });

    if drag_delta != egui::Vec2::ZERO {
        if let Some(state) = bind_popup.as_mut() {
            state.pos += drag_delta;
        }
    }
    if should_close {
        *binding_capture_module = None;
        *bind_popup = None;
    }
}

fn draw_bind_popup_row_mode(
    ui: &mut Ui,
    bind: &mut Bind,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    fixed_height_row(ui, 30.0, |ui| {
        ui.label(
            egui::RichText::new("Mode")
                .color(style.row_label)
                .size(13.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let width = 150.0_f32.min(ui.available_width().max(120.0));
            let (rect, _) = ui.allocate_exact_size(vec2(width, 24.0), egui::Sense::hover());
            let cell_w = rect.width() * 0.5;
            let modes = [(BindMode::Toggle, "Toggle"), (BindMode::Hold, "Hold")];
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(4),
                crate::gui_anim::with_alpha(style.tab_active_bg, 112),
            );
            for (index, (mode, label)) in modes.into_iter().enumerate() {
                let cell = Rect::from_min_max(
                    pos2(rect.left() + cell_w * index as f32, rect.top()),
                    pos2(rect.left() + cell_w * (index + 1) as f32, rect.bottom()),
                );
                let response = ui.interact(
                    cell,
                    ui.id().with("bind_mode").with(label),
                    egui::Sense::click(),
                );
                let selected = bind.mode == mode;
                let hover_t = anim.hover(
                    egui::Id::new("bind_popup_mode_hover").with(label).value(),
                    response.hovered(),
                    dt,
                );
                if selected || hover_t > 0.01 {
                    ui.painter().rect_filled(
                        cell.shrink(2.0),
                        egui::CornerRadius::same(3),
                        if selected {
                            crate::gui_anim::with_alpha(style.accent, 112)
                        } else {
                            crate::gui_anim::with_alpha(style.accent, (hover_t * 42.0) as u8)
                        },
                    );
                }
                ui.painter().text(
                    cell.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(11.0),
                    if selected {
                        style.text
                    } else {
                        style.row_label
                    },
                );
                if response.clicked() {
                    bind.mode = mode;
                }
            }
        });
    });
}

fn draw_bind_popup_row_key(
    ui: &mut Ui,
    bind: &mut Bind,
    capture_id: String,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> bool {
    let mut captured = false;
    fixed_height_row(ui, 30.0, |ui| {
        ui.label(egui::RichText::new("Key").color(style.row_label).size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            captured = draw_bind_key_field(
                ui,
                bind,
                capture_id,
                binding_capture_module,
                style,
                anim,
                anim_id,
                dt,
            );
        });
    });
    captured
}

fn draw_bind_key_field(
    ui: &mut Ui,
    bind: &mut Bind,
    capture_id: String,
    binding_capture_module: &mut Option<String>,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> bool {
    let width = 150.0_f32.min(ui.available_width().max(100.0));
    let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), egui::Sense::click());
    let is_capturing = binding_capture_module.as_deref() == Some(capture_id.as_str());
    let hover_t = anim.hover(anim_id, response.hovered() || is_capturing, dt);
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(5),
        crate::gui_anim::lerp_color(
            crate::gui_anim::with_alpha(style.tab_active_bg, 135),
            crate::gui_anim::with_alpha(style.accent, 78),
            hover_t,
        ),
    );
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(
            1.0,
            crate::gui_anim::lerp_color(
                crate::gui_anim::with_alpha(style.card_border, 150),
                style.accent,
                hover_t,
            ),
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if is_capturing {
            "Press key"
        } else {
            bind.key_name.as_str()
        },
        egui::FontId::proportional(12.0),
        crate::gui_anim::lerp_color(style.row_label, style.text, hover_t),
    );

    if response.clicked() {
        hooks::clear_last_key_pressed();
        *binding_capture_module = Some(capture_id);
    }
    if is_capturing {
        if let Some(key_code) = hooks::take_last_key_pressed() {
            if apply_captured_bind_key(key_code, &mut bind.key_code, &mut bind.key_name) {
                *binding_capture_module = None;
                return true;
            }
        }
    }
    false
}

fn draw_mode_list(
    ui: &mut Ui,
    label: &str,
    choice: &mut crate::interface::Choice,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 39.0), egui::Sense::hover());
    let option_count = choice.options.len().max(1) as f32;
    let control_w = (option_count * 54.0)
        .clamp(126.0, 218.0)
        .min((rect.width() * 0.58).max(126.0));
    let control_rect = Rect::from_min_max(
        pos2(rect.right() - control_w, rect.center().y - 12.0),
        pos2(rect.right(), rect.center().y + 12.0),
    );
    let hover_t = anim.hover(anim_id, response.hovered(), dt);

    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        style.row_label,
    );
    ui.painter().rect_filled(
        control_rect,
        egui::CornerRadius::same(4),
        crate::gui_anim::with_alpha(style.tab_active_bg, 112),
    );
    ui.painter().rect_stroke(
        control_rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(
            1.0,
            crate::gui_anim::lerp_color(
                crate::gui_anim::with_alpha(style.card_border, 118),
                crate::gui_anim::with_alpha(style.accent, 150),
                hover_t,
            ),
        ),
        egui::StrokeKind::Inside,
    );

    let cell_w = control_rect.width() / option_count;
    for index in 0..choice.options.len() {
        let option = choice.options[index].clone();
        let cell = Rect::from_min_max(
            pos2(
                control_rect.left() + cell_w * index as f32,
                control_rect.top(),
            ),
            pos2(
                control_rect.left() + cell_w * (index + 1) as f32,
                control_rect.bottom(),
            ),
        );
        let option_response = ui.interact(
            cell,
            ui.id().with("mode_option").with(anim_id).with(index),
            egui::Sense::click(),
        );
        let selected = index == choice.selected_index;
        let option_hover_t = anim.hover(
            anim_id.wrapping_add(20 + index as u64),
            option_response.hovered(),
            dt,
        );
        if selected || option_hover_t > 0.01 {
            ui.painter().rect_filled(
                cell.shrink(2.0),
                egui::CornerRadius::same(3),
                if selected {
                    crate::gui_anim::with_alpha(style.accent, 112)
                } else {
                    crate::gui_anim::with_alpha(style.accent, (option_hover_t * 42.0) as u8)
                },
            );
        }
        if index > 0 {
            ui.painter().vline(
                cell.left(),
                (cell.top() + 6.0)..=(cell.bottom() - 6.0),
                egui::Stroke::new(1.0, crate::gui_anim::with_alpha(style.card_border, 80)),
            );
        }
        ui.painter().text(
            cell.center(),
            egui::Align2::CENTER_CENTER,
            option,
            egui::FontId::proportional(10.5),
            crate::gui_anim::lerp_color(
                style.row_label,
                style.text,
                if selected { 1.0 } else { option_hover_t * 0.55 },
            ),
        );
        if option_response.clicked() {
            choice.selected_index = index;
        }
    }

    response
}

fn draw_choice_list(
    ui: &mut Ui,
    label: &str,
    choice: &mut crate::interface::Choice,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> egui::Response {
    let width = ui.available_width();
    let row_id = ui.id().with("choice_open").with(anim_id);
    let open = ui.memory(|memory| memory.is_popup_open(row_id));
    let (rect, response) = ui.allocate_exact_size(vec2(width, 39.0), egui::Sense::click());
    let hover_t = anim.hover(anim_id, response.hovered() || open, dt);

    let control_w = 154.0_f32.min((rect.width() * 0.48).max(112.0));
    let control_rect = Rect::from_min_max(
        pos2(rect.right() - control_w, rect.center().y - 13.0),
        pos2(rect.right(), rect.center().y + 13.0),
    );

    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        style.row_label,
    );
    ui.painter().rect_filled(
        control_rect,
        egui::CornerRadius::same(5),
        crate::gui_anim::lerp_color(
            crate::gui_anim::with_alpha(style.tab_active_bg, 142),
            crate::gui_anim::with_alpha(style.accent, 74),
            hover_t,
        ),
    );
    ui.painter().rect_stroke(
        control_rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(
            1.0,
            crate::gui_anim::lerp_color(
                crate::gui_anim::with_alpha(style.card_border, 160),
                style.accent,
                hover_t,
            ),
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        pos2(control_rect.left() + 10.0, control_rect.center().y),
        egui::Align2::LEFT_CENTER,
        choice.selected(),
        egui::FontId::proportional(12.0),
        style.text,
    );
    ui.painter().text(
        pos2(control_rect.right() - 12.0, control_rect.center().y - 0.5),
        egui::Align2::CENTER_CENTER,
        if open { "в–І" } else { "в–ј" },
        egui::FontId::proportional(10.0),
        style.text_muted,
    );

    if response.clicked() {
        ui.memory_mut(|memory| memory.toggle_popup(row_id));
    }

    if open {
        ui.add_space(2.0);
        egui::Frame::NONE
            .fill(crate::gui_anim::with_alpha(style.tab_active_bg, 112))
            .stroke(egui::Stroke::new(
                1.0,
                crate::gui_anim::with_alpha(style.card_border, 145),
            ))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(5))
            .show(ui, |ui| {
                ui.set_width(width);
                for (index, option) in choice.options.iter().enumerate() {
                    let (row_rect, row_response) =
                        ui.allocate_exact_size(vec2(width - 10.0, 25.0), egui::Sense::click());
                    let selected = index == choice.selected_index;
                    let row_hover = anim.hover(
                        anim_id.wrapping_add(20 + index as u64),
                        row_response.hovered(),
                        dt,
                    );
                    if selected || row_hover > 0.01 {
                        ui.painter().rect_filled(
                            row_rect,
                            egui::CornerRadius::same(4),
                            crate::gui_anim::with_alpha(
                                style.accent,
                                if selected {
                                    122
                                } else {
                                    (row_hover * 60.0) as u8
                                },
                            ),
                        );
                    }
                    ui.painter().text(
                        pos2(row_rect.left() + 9.0, row_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        option,
                        egui::FontId::proportional(12.0),
                        crate::gui_anim::lerp_color(
                            style.row_label,
                            style.text,
                            if selected { 1.0 } else { row_hover * 0.55 },
                        ),
                    );
                    if selected {
                        ui.painter().circle_filled(
                            pos2(row_rect.right() - 10.0, row_rect.center().y),
                            3.0,
                            style.accent_hover,
                        );
                    }
                    if row_response.clicked() {
                        choice.selected_index = index;
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                }
            });
    }

    response
}

#[allow(dead_code)]
fn draw_static_select(ui: &mut Ui, label: &str, style: &MenuStyle) {
    let width = 190.0_f32.min(ui.available_width().max(110.0));
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(5),
        crate::gui_anim::with_alpha(style.tab_active_bg, 135),
    );
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        egui::Stroke::new(1.0, crate::gui_anim::with_alpha(style.card_border, 160)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        style.row_label,
    );
    ui.painter().text(
        pos2(rect.right() - 11.0, rect.center().y - 0.5),
        egui::Align2::CENTER_CENTER,
        "в–ј",
        egui::FontId::proportional(10.0),
        style.text_muted,
    );
}

fn draw_selectable_row(
    ui: &mut Ui,
    label: &str,
    active: bool,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    anim_id: u64,
    dt: f32,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 43.0), egui::Sense::click());
    let hover_t = anim.hover(anim_id, response.hovered(), dt);
    let fill_t = if active {
        0.14 + hover_t * 0.05
    } else {
        hover_t * 0.04
    };
    if fill_t > 0.0 {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same(5),
            crate::gui_anim::with_alpha(style.accent, (fill_t * 255.0) as u8),
        );
    }
    ui.painter().text(
        pos2(rect.left() + 1.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(15.0),
        crate::gui_anim::lerp_color(
            style.row_label,
            style.text,
            if active { 1.0 } else { hover_t * 0.4 },
        ),
    );
    let dot_center = pos2(rect.right() - 11.0, rect.center().y);
    ui.painter().circle_filled(
        dot_center,
        11.0,
        if active {
            crate::gui_anim::with_alpha(style.accent, 170)
        } else {
            crate::gui_anim::with_alpha(style.toggle_off_track, 90)
        },
    );
    if active {
        ui.painter()
            .circle_filled(dot_center, 6.0, style.accent_hover);
    }
    response.clicked()
}

fn selected_or_active(input: &str, active: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        active.to_string()
    } else {
        trimmed.to_string()
    }
}

fn config_file_age(name: &str) -> (&'static str, String) {
    let Ok(root) = crate::config::ensure_root_dir() else {
        return ("Created", "Unknown".to_string());
    };
    let path = root.join(format!("{name}.json"));
    let Ok(metadata) = std::fs::metadata(path) else {
        return ("Created", "Not saved yet".to_string());
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.elapsed().ok())
        .map(|elapsed| {
            if elapsed.as_secs() < 60 {
                "Now".to_string()
            } else if elapsed.as_secs() < 3600 {
                format!("{}m ago", elapsed.as_secs() / 60)
            } else if elapsed.as_secs() < 86_400 {
                format!("{}h ago", elapsed.as_secs() / 3600)
            } else {
                format!("{}d ago", elapsed.as_secs() / 86_400)
            }
        })
        .unwrap_or_else(|| "Unknown".to_string());
    ("Modified", modified)
}

fn draw_two_button_row(
    ui: &mut Ui,
    left: &str,
    right: &str,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    left_id: u64,
    right_id: u64,
    dt: f32,
) -> (bool, bool) {
    let mut left_clicked = false;
    let mut right_clicked = false;
    ui.columns(2, |cols| {
        left_clicked = gui_widgets::themed_wide_button_anim(
            &mut cols[0],
            left,
            style,
            anim,
            left_id,
            dt,
            false,
        )
        .clicked();
        right_clicked = gui_widgets::themed_wide_button_anim(
            &mut cols[1],
            right,
            style,
            anim,
            right_id,
            dt,
            false,
        )
        .clicked();
    });
    (left_clicked, right_clicked)
}

fn draw_profile_card(
    ui: &mut Ui,
    profile_avatar: TextureId,
    profile_banner: TextureId,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    let width = ui.available_width();
    let banner_h = 210.0;
    let site_y_offset = 74.0;
    let height = banner_h + site_y_offset + 20.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let hover_t = anim.hover(
        egui::Id::new("settings_profile_card").value(),
        response.hovered(),
        dt,
    );

    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(6),
        crate::gui_anim::lerp_color(
            style.card_bg,
            crate::gui_anim::with_alpha(style.tab_active_bg, 150),
            hover_t * 0.12,
        ),
    );
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        egui::Stroke::new(
            1.0,
            crate::gui_anim::lerp_color(
                crate::gui_anim::with_alpha(style.card_border, 70),
                crate::gui_anim::with_alpha(style.accent, 120),
                hover_t,
            ),
        ),
        egui::StrokeKind::Inside,
    );

    let banner = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + banner_h));
    ui.painter().rect_filled(
        banner,
        egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        crate::gui_anim::lerp_color(style.header_bg, style.accent, 0.18),
    );
    ui.put(
        banner,
        egui::Image::new((profile_banner, banner.size()))
            .corner_radius(egui::CornerRadius::same(6)),
    );
    let avatar_center = pos2(rect.left() + 82.0, banner.bottom() + 4.0);
    let avatar_size = vec2(106.0, 106.0);
    let avatar_rect = Rect::from_center_size(avatar_center, avatar_size);
    ui.painter()
        .circle_filled(avatar_center, 56.0, style.card_bg);
    ui.painter().circle_stroke(
        avatar_center,
        56.0,
        egui::Stroke::new(3.0, crate::gui_anim::with_alpha(style.accent, 145)),
    );
    ui.put(
        avatar_rect,
        egui::Image::new((profile_avatar, avatar_size)).corner_radius(egui::CornerRadius::same(53)),
    );

    let text_x = avatar_center.x + 72.0;
    let name_y = banner.bottom() + 22.0;
    ui.painter().text(
        pos2(text_x, name_y),
        egui::Align2::LEFT_CENTER,
        "Syntax",
        egui::FontId::proportional(22.0),
        style.text,
    );
    ui.painter().text(
        pos2(text_x, name_y + 28.0),
        egui::Align2::LEFT_CENTER,
        "Registration: Unknown",
        egui::FontId::proportional(13.0),
        style.row_label,
    );

    let info_x = rect.right() - 26.0;
    ui.painter().text(
        pos2(info_x, banner.bottom() + 26.0),
        egui::Align2::RIGHT_CENTER,
        "Subscription: 30.08.2026",
        egui::FontId::proportional(13.0),
        style.text,
    );
    ui.painter().text(
        pos2(info_x, banner.bottom() + 50.0),
        egui::Align2::RIGHT_CENTER,
        "Client: v0.1.0",
        egui::FontId::proportional(13.0),
        style.row_label,
    );
    ui.painter().text(
        pos2(info_x, banner.bottom() + site_y_offset),
        egui::Align2::RIGHT_CENTER,
        "Syntax",
        egui::FontId::proportional(13.0),
        style.accent_hover,
    );
}

fn draw_configs_panel(
    ui: &mut Ui,
    config_name_input: &mut String,
    profile_avatar: TextureId,
    profile_banner: TextureId,
    gui_scale_percent: &mut u32,
    language_index: &mut usize,
    system_stats: &SystemStats,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    let _ = config_name_input;
    draw_profile_card(ui, profile_avatar, profile_banner, style, anim, dt);
    ui.add_space(51.0);
    ui.spacing_mut().item_spacing.x = 10.0;
    ui.columns(2, |cols| {
        draw_settings_card(&mut cols[0], "Interface", style, |ui| {
            draw_settings_combo_row(
                ui,
                "GUI Size",
                gui_scale_percent,
                &[
                    (75, "75%"),
                    (100, "100%"),
                    (125, "125%"),
                    (150, "150%"),
                    (175, "175%"),
                ],
                style,
            );
            ui.add_space(14.0);
            draw_language_combo_row(ui, language_index, style);
        });

        draw_settings_card(&mut cols[1], "Information", style, |ui| {
            draw_info_line(
                ui,
                "CPU load",
                &format_percent(system_stats.cpu_percent),
                style,
            );
            ui.add_space(10.0);
            draw_info_line(ui, "RAM", &format_ram(system_stats), style);
            ui.add_space(10.0);
            draw_info_line(ui, "GPU load", "N/A", style);
            ui.add_space(10.0);
            draw_info_line(ui, "GPU temp", "N/A", style);
            ui.add_space(10.0);
            draw_info_line(ui, "CPU temp", "N/A", style);
        });
    });
}

fn draw_settings_combo_row(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    options: &[(u32, &str)],
    style: &MenuStyle,
) {
    fixed_height_row(ui, 34.0, |ui| {
        ui.label(egui::RichText::new(label).color(style.row_label).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let selected = options
                .iter()
                .find(|(candidate, _)| candidate == value)
                .map(|(_, text)| *text)
                .unwrap_or("100%");
            egui::ComboBox::from_id_salt(format!("settings_{label}_combo"))
                .width(118.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (candidate, text) in options {
                        ui.selectable_value(value, *candidate, *text);
                    }
                });
        });
    });
}

fn draw_language_combo_row(ui: &mut Ui, language_index: &mut usize, style: &MenuStyle) {
    const LANGUAGES: [&str; 4] = ["ru", "eu", "pl", "ch"];
    *language_index = (*language_index).min(LANGUAGES.len().saturating_sub(1));
    fixed_height_row(ui, 34.0, |ui| {
        ui.label(
            egui::RichText::new("Language")
                .color(style.row_label)
                .size(14.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("settings_language_combo")
                .width(118.0)
                .selected_text(LANGUAGES[*language_index])
                .show_ui(ui, |ui| {
                    for (index, language) in LANGUAGES.iter().enumerate() {
                        ui.selectable_value(language_index, index, *language);
                    }
                });
        });
    });
}

fn format_percent(value: Option<f32>) -> String {
    value.map_or_else(|| "N/A".to_string(), |value| format!("{value:.0}%"))
}

fn format_ram(stats: &SystemStats) -> String {
    match (stats.ram_used_gb, stats.ram_total_gb, stats.ram_percent) {
        (Some(used), Some(total), Some(percent)) => {
            format!("{used:.1}/{total:.1} GB ({percent:.0}%)")
        }
        _ => "N/A".to_string(),
    }
}

fn draw_friend_list_custom(
    ui: &mut Ui,
    friend_name_input: &mut String,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    draw_settings_card(ui, "Create", style, |ui| {
        let response =
            draw_labeled_text_edit(ui, "Nickname", friend_name_input, "Player name", style);
        ui.add_space(16.0);
        let enter_pressed =
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if gui_widgets::themed_wide_button_anim(ui, "Add Friend", style, anim, 7101, dt, false)
            .clicked()
            || enter_pressed
        {
            if friends::add(friend_name_input) {
                friend_name_input.clear();
            }
        }
    });

    ui.add_space(14.0);
    draw_settings_card(ui, "Friends", style, |ui| {
        let list = friends::list();
        if list.is_empty() {
            ui.label(
                egui::RichText::new("No friends added yet.")
                    .color(style.row_label)
                    .size(14.0),
            );
            return;
        }
        for name in list {
            let active = friend_name_input.trim() == name;
            if draw_selectable_row(
                ui,
                &name,
                active,
                style,
                anim,
                egui::Id::new("friend_row").with(name.as_str()).value(),
                dt,
            ) {
                *friend_name_input = name.clone();
            }
            gui_widgets::faint_separator_anim(
                ui,
                style,
                Some(anim),
                egui::Id::new("friend_sep").with(name.as_str()).value(),
                dt,
            );
        }
    });
}

fn draw_friend_actions_custom(
    ui: &mut Ui,
    friend_name_input: &mut String,
    click_friend_bind: &mut Bind,
    click_friend_bind_capture: &mut bool,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    draw_settings_card(ui, "Information", style, |ui| {
        let friends = friends::list();
        let selected = if friend_name_input.trim().is_empty() {
            "None"
        } else {
            friend_name_input.trim()
        };
        draw_info_line(ui, "Selected", selected, style);
        ui.add_space(18.0);
        draw_info_line(ui, "Friends", &friends.len().to_string(), style);
        ui.add_space(18.0);
        draw_info_line(ui, "Bind", click_friend_bind.key_name.as_str(), style);
    });

    ui.add_space(14.0);
    draw_settings_card(ui, "Integration", style, |ui| {
        let (add, delete) = draw_two_button_row(ui, "Add", "Delete", style, anim, 7111, 7112, dt);
        if add && friends::add(friend_name_input) {
            friend_name_input.clear();
        }
        if delete && friends::remove(friend_name_input) {
            friend_name_input.clear();
        }
        ui.add_space(8.0);
        let (clear, from_crosshair) =
            draw_two_button_row(ui, "Clear", "Crosshair", style, anim, 7113, 7114, dt);
        if clear {
            friend_name_input.clear();
        }
        if from_crosshair {
            add_crosshair_friend();
        }
    });

    ui.add_space(14.0);
    draw_settings_card(ui, "Click Friend", style, |ui| {
        fixed_height_row(ui, 39.0, |ui| {
            ui.label(
                egui::RichText::new("Bind")
                    .color(style.row_label)
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let capture_id = "click_friend".to_string();
                let mut capture = if *click_friend_bind_capture {
                    Some(capture_id.clone())
                } else {
                    None
                };
                draw_bind_control(
                    ui,
                    click_friend_bind,
                    capture_id,
                    &mut capture,
                    style,
                    anim,
                    7121,
                    dt,
                );
                *click_friend_bind_capture = capture.is_some();
            });
        });
        ui.add_space(8.0);
        let (_, unbind) = draw_two_button_row(ui, "Capture", "Unbind", style, anim, 7122, 7123, dt);
        if unbind {
            click_friend_bind.key_code = 0;
            click_friend_bind.key_name = "None".to_string();
            *click_friend_bind_capture = false;
        }
        if *click_friend_bind_capture {
            if let Some(key_code) = hooks::take_last_key_pressed() {
                if apply_captured_bind_key(
                    key_code,
                    &mut click_friend_bind.key_code,
                    &mut click_friend_bind.key_name,
                ) {
                    *click_friend_bind_capture = false;
                }
            }
        }
    });
}

fn draw_theme_list_custom(
    ui: &mut Ui,
    theme: &mut ThemeId,
    custom_palette: &mut ThemePalette,
    style: &MenuStyle,
    anim: &mut MenuAnimations,
    dt: f32,
) {
    ui.columns(2, |cols| {
        for (index, theme_id) in ThemeId::ALL.iter().enumerate() {
            let col_ui = &mut cols[index % 2];
            let palette = match theme_id {
                ThemeId::Custom => *custom_palette,
                other => other.palette(),
            };
            let active = *theme == *theme_id;
            let select_t = anim.tab(egui::Id::new("theme_card").with(index).value(), active, dt);
            egui::Frame::NONE
                .fill(crate::gui_anim::lerp_color(
                    style.card_bg,
                    style.tab_active_bg,
                    select_t * 0.55,
                ))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    crate::gui_anim::lerp_color(style.card_border, style.accent, select_t),
                ))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(12))
                .show(col_ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(palette.name)
                                .color(style.text)
                                .size(14.0)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn_text = if active { "Selected" } else { "Apply" };
                            if gui_widgets::themed_small_button_anim(
                                ui,
                                btn_text,
                                style,
                                anim,
                                egui::Id::new("theme_btn").with(index).value(),
                                dt,
                                active,
                            )
                            .clicked()
                            {
                                *theme = *theme_id;
                                apply_theme(ui.ctx(), resolve_palette(*theme, *custom_palette));
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(palette.description)
                            .color(style.row_label)
                            .size(11.0),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        draw_color_swatch(ui, palette.window);
                        draw_color_swatch(ui, palette.panel);
                        draw_color_swatch(ui, palette.accent);
                        draw_color_swatch(ui, palette.text);
                    });
                });
            col_ui.add_space(10.0);
        }
    });

    ui.add_space(8.0);
    egui::Frame::NONE
        .fill(style.card_bg)
        .stroke(egui::Stroke::new(1.0_f32, style.card_border))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new("Custom editor")
                    .color(style.text)
                    .size(14.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("РњРµРЅСЏРµС‚ С‚РµРјСѓ Custom. Apply Custom СЃРІРµСЂС…Сѓ, С‡С‚РѕР±С‹ РїСЂРёРјРµРЅРёС‚СЊ.")
                    .color(style.row_label)
                    .size(11.0),
            );
            ui.add_space(8.0);

            let mut accent = color32_to_rgba(custom_palette.accent);
            let mut window = color32_to_rgba(custom_palette.window);
            let mut panel = color32_to_rgba(custom_palette.panel);
            let mut text = color32_to_rgba(custom_palette.text);
            let mut muted = color32_to_rgba(custom_palette.muted);

            gui_widgets::color_picker_row(ui, "Accent", &mut accent, style, "custom_accent");
            gui_widgets::faint_separator(ui, style);
            gui_widgets::color_picker_row(ui, "Window", &mut window, style, "custom_window");
            gui_widgets::faint_separator(ui, style);
            gui_widgets::color_picker_row(ui, "Panel", &mut panel, style, "custom_panel");
            gui_widgets::faint_separator(ui, style);
            gui_widgets::color_picker_row(ui, "Text", &mut text, style, "custom_text");
            gui_widgets::faint_separator(ui, style);
            gui_widgets::color_picker_row(ui, "Muted", &mut muted, style, "custom_muted");

            custom_palette.accent = rgba_to_color32(accent);
            custom_palette.accent_hover = brighten(custom_palette.accent, 1.12);
            custom_palette.window = rgba_to_color32(window);
            custom_palette.panel = rgba_to_color32(panel);
            custom_palette.faint = brighten(custom_palette.panel, 1.08);
            custom_palette.extreme = darken_color(custom_palette.window, 0.7);
            custom_palette.text = rgba_to_color32(text);
            custom_palette.muted = rgba_to_color32(muted);
            custom_palette.stroke = mix_colors(custom_palette.muted, custom_palette.accent, 0.2);
            custom_palette.inactive = mix_colors(custom_palette.panel, custom_palette.muted, 0.35);
            custom_palette.hovered =
                mix_colors(custom_palette.inactive, custom_palette.accent, 0.25);
            custom_palette.code = mix_colors(custom_palette.panel, custom_palette.accent, 0.12);

            if *theme == ThemeId::Custom {
                apply_theme(ui.ctx(), resolve_palette(ThemeId::Custom, *custom_palette));
            }
        });
}

fn color32_to_rgba(c: Color32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        c.a() as f32 / 255.0,
    ]
}

fn rgba_to_color32(c: [f32; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn brighten(c: Color32, factor: f32) -> Color32 {
    Color32::from_rgb(
        ((c.r() as f32 * factor).min(255.0)) as u8,
        ((c.g() as f32 * factor).min(255.0)) as u8,
        ((c.b() as f32 * factor).min(255.0)) as u8,
    )
}

fn darken_color(c: Color32, factor: f32) -> Color32 {
    Color32::from_rgb(
        (c.r() as f32 * factor) as u8,
        (c.g() as f32 * factor) as u8,
        (c.b() as f32 * factor) as u8,
    )
}

fn mix_colors(a: Color32, b: Color32, t: f32) -> Color32 {
    crate::gui_anim::lerp_color(a, b, t)
}

fn draw_friend_list(ui: &mut Ui, friend_name_input: &mut String) {
    ui.heading("Friends");
    ui.separator();

    let friends = friends::list();
    if friends.is_empty() {
        ui.label("No friends added");
        return;
    }

    for name in friends {
        ui.push_id(&name, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(false, &name).clicked() {
                    *friend_name_input = name.clone();
                }
                if ui.button("Remove").clicked() {
                    friends::remove(&name);
                }
            });
        });
    }
}

fn draw_friend_actions(
    ui: &mut Ui,
    friend_name_input: &mut String,
    click_friend_bind: &mut Bind,
    click_friend_bind_capture: &mut bool,
) {
    ui.heading("Edit friend");
    ui.separator();

    ui.label("Nickname");
    let response = ui.add(
        egui::TextEdit::singleline(friend_name_input)
            .hint_text("Player name")
            .desired_width(f32::INFINITY),
    );

    let enter_pressed =
        response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if ui.button("Add").clicked() || enter_pressed {
        if friends::add(friend_name_input) {
            friend_name_input.clear();
        }
    }

    if ui.button("Delete").clicked() && friends::remove(friend_name_input) {
        friend_name_input.clear();
    }

    ui.separator();
    ui.heading("ClickFriend");
    ui.label("Bind");
    let key_label = if *click_friend_bind_capture {
        "Key / Del...".to_string()
    } else {
        format!(
            "Key: {} ({:#X})",
            click_friend_bind.key_name, click_friend_bind.key_code
        )
    };

    if ui.button(key_label).clicked() {
        hooks::clear_last_key_pressed();
        *click_friend_bind_capture = true;
    }

    if *click_friend_bind_capture {
        if let Some(key_code) = hooks::take_last_key_pressed() {
            if apply_captured_bind_key(
                key_code,
                &mut click_friend_bind.key_code,
                &mut click_friend_bind.key_name,
            ) {
                *click_friend_bind_capture = false;
            }
        }
    }

    if ui.button("Unbind ClickFriend").clicked() {
        click_friend_bind.key_code = 0;
        click_friend_bind.key_name = "None".to_string();
        *click_friend_bind_capture = false;
    }
}

fn add_crosshair_friend() {
    let _ = jni_bridge::with_env(|env| {
        let result = (|| {
            let Some(mut ctx) = MinecraftContext::new(env)? else {
                return Ok(());
            };
            if !minecraft::has_player_and_world(&ctx) {
                return Ok(());
            }

            if let Some(name) = minecraft::crosshair_player_name_within_distance(&mut ctx, 3.0)? {
                friends::add(&name);
            }

            Ok(())
        })();
        if result.is_err() {
            jni_bridge::clear_pending_exception(env);
        }
        result
    });
}

fn draw_color_swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, Color32::from_white_alpha(36)),
        egui::StrokeKind::Inside,
    );
}

fn draw_bind_editor_window(
    ctx: &Context,
    bind_editor_module: &mut Option<String>,
    binding_capture_module: &mut Option<String>,
) {
    let Some(module_name) = bind_editor_module.clone() else {
        return;
    };

    let mut open = true;
    egui::Window::new(format!("Bind Editor##{module_name}"))
        .default_pos(pos2(760.0, 80.0))
        .default_size(vec2(280.0, 180.0))
        .open(&mut open)
        .show(ctx, |ui| {
            let _ = with_registry(|registry| {
                let Some(module) = registry
                    .modules_mut()
                    .iter_mut()
                    .find(|module| module.menu_module().name == module_name)
                else {
                    ui.label("Module disappeared");
                    return;
                };

                if draw_bind_editor_contents(ui, module.menu_module_mut(), binding_capture_module) {
                    *bind_editor_module = None;
                    *binding_capture_module = None;
                }
            });
        });

    if !open {
        *bind_editor_module = None;
        *binding_capture_module = None;
    }
}

fn draw_bind_editor_contents(
    ui: &mut Ui,
    menu: &mut Module,
    binding_capture_module: &mut Option<String>,
) -> bool {
    ui.heading(format!("Bind: {}", menu.name));
    ui.separator();

    if menu.bind.is_none() {
        menu.bind = Some(Bind::new("None", 0));
    }

    let bind = menu.bind.as_mut().expect("bind was just created");
    let is_capturing = binding_capture_module.as_deref() == Some(menu.name.as_str());
    let key_label = if is_capturing {
        "Key / Del...".to_string()
    } else {
        format!("Key: {} ({:#X})", bind.key_name, bind.key_code)
    };

    if ui.button(key_label).clicked() {
        hooks::clear_last_key_pressed();
        *binding_capture_module = Some(menu.name.clone());
    }

    if is_capturing {
        if let Some(key_code) = hooks::take_last_key_pressed() {
            if apply_captured_bind_key(key_code, &mut bind.key_code, &mut bind.key_name) {
                *binding_capture_module = None;
            }
        }
    }

    ui.horizontal(|ui| {
        ui.label("Type");
        ui.radio_value(&mut bind.mode, BindMode::Toggle, "Toggle");
        ui.radio_value(&mut bind.mode, BindMode::Hold, "Hold");
    });

    ui.horizontal(|ui| {
        ui.label("Set value");
        ui.radio_value(&mut bind.target_state, BindTargetState::On, "On");
        ui.radio_value(&mut bind.target_state, BindTargetState::Off, "Off");
    });

    ui.separator();
    if ui.button("Unbind").clicked() {
        menu.bind = None;
        *binding_capture_module = None;
    }

    if ui.button("Close").clicked() {
        *binding_capture_module = None;
        return true;
    }

    false
}

fn draw_setting(
    ui: &mut Ui,
    module_name: &str,
    setting: &mut Setting,
    binding_capture_module: &mut Option<String>,
) {
    match setting {
        Setting::CheckBox(setting) => {
            ui.checkbox(&mut setting.value, &setting.name);
        }
        Setting::Slider(setting) => {
            ui.add(
                EguiSlider::new(&mut setting.value, setting.min..=setting.max).text(&setting.name),
            );
        }
        Setting::ColorPicker(setting) => {
            ui.horizontal(|ui| {
                ui.label(&setting.name);
                ui.color_edit_button_rgba_unmultiplied(&mut setting.value);
            });
        }
        Setting::ActionBind(setting) => {
            let capture_id = format!("action:{}:{}", module_name, setting.name);
            let is_capturing = binding_capture_module.as_deref() == Some(capture_id.as_str());
            let key_label = if is_capturing {
                format!("{}: Key / Del...", setting.name)
            } else {
                format!(
                    "{}: {} ({:#X})",
                    setting.name, setting.bind.key_name, setting.bind.key_code
                )
            };

            ui.horizontal(|ui| {
                if ui.button(key_label).clicked() {
                    hooks::clear_last_key_pressed();
                    *binding_capture_module = Some(capture_id.clone());
                }
                if ui.button("Unbind").clicked() {
                    setting.bind.key_code = 0;
                    setting.bind.key_name = "None".to_string();
                    *binding_capture_module = None;
                }
            });

            if is_capturing {
                if let Some(key_code) = hooks::take_last_key_pressed() {
                    if apply_captured_bind_key(
                        key_code,
                        &mut setting.bind.key_code,
                        &mut setting.bind.key_name,
                    ) {
                        *binding_capture_module = None;
                    }
                }
            }
        }
        Setting::Choice(setting) => {
            ui.horizontal(|ui| {
                ui.label(&setting.name);
                egui::ComboBox::from_id_salt(format!("{}_choice", setting.name))
                    .selected_text(setting.selected())
                    .show_ui(ui, |ui| {
                        for (index, option) in setting.options.iter().enumerate() {
                            ui.selectable_value(&mut setting.selected_index, index, option);
                        }
                    });
            });
        }
    }
}

fn unload_client() {
    let _ = with_registry(|registry| {
        registry.disable_all();
        registry.save_config();
    });
    unsafe {
        hooks::shutdown();
    }
}



fn apply_captured_bind_key(
    key_code: u32,
    bind_key_code: &mut u32,
    bind_key_name: &mut String,
) -> bool {
    const VK_LBUTTON: u32 = 0x01;
    const VK_RBUTTON: u32 = 0x02;
    const VK_DELETE: u32 = 0x2E;

    if key_code == VK_LBUTTON || key_code == VK_RBUTTON {
        return false;
    }
    if key_code == VK_DELETE {
        *bind_key_code = 0;
        *bind_key_name = "None".to_string();
        return true;
    }
    *bind_key_code = key_code;
    *bind_key_name = key_name(key_code);
    true
}

fn key_name(key_code: u32) -> String {
    match key_code {
        0 => "None".to_string(),
        0x01 => "M1".to_string(),
        0x02 => "M2".to_string(),
        0x04 => "M3".to_string(),
        0x05 => "M4".to_string(),
        0x06 => "M5".to_string(),
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x10 => "Shift".to_string(),
        0x11 => "Ctrl".to_string(),
        0x12 => "Alt".to_string(),
        0x14 => "CapsLock".to_string(),
        0x1B => "Escape".to_string(),
        0x20 => "Space".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2E => "Delete".to_string(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(key_code)
            .map(|ch| ch.to_string())
            .unwrap_or_else(|| format!("VK_{key_code:02X}")),
        0x70..=0x87 => format!("F{}", key_code - 0x6F),
        _ => format!("VK_{key_code:02X}"),
    }
}

unsafe fn load_gl_proc(name: &str) -> *const c_void {
    let Ok(name) = CString::new(name) else {
        return null_mut();
    };

    let address = unsafe { wglGetProcAddress(PCSTR(name.as_ptr().cast())) };
    if let Some(proc) = address {
        let ptr = proc as *const c_void;
        if is_valid_wgl_proc_address(ptr) {
            return ptr;
        }
    }

    let opengl32 = match unsafe { GetModuleHandleA(s!("opengl32.dll")) } {
        Ok(module) => module,
        Err(_) => return null_mut(),
    };

    unsafe { GetProcAddress(opengl32, PCSTR(name.as_ptr().cast())) }
        .map(|proc| proc as *const c_void)
        .unwrap_or(null_mut())
}

fn is_valid_wgl_proc_address(ptr: *const c_void) -> bool {
    !matches!(ptr as usize, 0 | 1 | 2 | 3 | usize::MAX)
}

fn reset_pixel_unpack_state() {
    unsafe {
        glPixelStorei(GL_UNPACK_ALIGNMENT, 1);
        glPixelStorei(GL_UNPACK_ROW_LENGTH, 0);
        glPixelStorei(GL_UNPACK_SKIP_ROWS, 0);
        glPixelStorei(GL_UNPACK_SKIP_PIXELS, 0);
        glPixelStorei(GL_UNPACK_LSB_FIRST, 0);
        glPixelStorei(GL_UNPACK_SWAP_BYTES, 0);
    }
}
