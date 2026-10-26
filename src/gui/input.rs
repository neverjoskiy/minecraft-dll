use egui::{Event, Key, PointerButton, RawInput, Rect, ViewportId, ViewportInfo, pos2, vec2};
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{HDC, ScreenToClient, WindowFromDC},
    UI::WindowsAndMessaging::{GetClientRect, GetCursorPos},
};

use crate::{hooks, jni_bridge};

use super::Overlay;

pub(crate) fn update_input(overlay: &mut Overlay, hdc: HDC) -> RawInput {
    let now = std::time::Instant::now();
    let delta = now.saturating_duration_since(overlay.last_frame);
    overlay.last_frame = now;
    overlay.frame_dt = delta.as_secs_f32().clamp(1.0 / 10000.0, 1.0 / 15.0);

    if hooks::take_menu_toggle_requested() {
        overlay.menu_open = !overlay.menu_open;
    }

    let mut events = Vec::new();
    let menu_visible = overlay.menu_open || overlay.menu_animations.visible();
    if menu_visible {
        for ch in hooks::take_text_input() {
            match ch {
                '\r' | '\n' => events.push(Event::Key {
                    key: Key::Enter,
                    physical_key: Some(Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }),
                '\t' => events.push(Event::Key {
                    key: Key::Tab,
                    physical_key: Some(Key::Tab),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }),
                _ => events.push(Event::Text(ch.to_string())),
            }
        }

        for key_code in hooks::take_pressed_keys() {
            if let Some(key) = egui_key(key_code) {
                events.push(Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
        }
    }

    unsafe {
        let hwnd = WindowFromDC(hdc);
        if !hwnd.0.is_null() {
            hooks::install_wndproc_hook(hwnd);

            let mut rect = Default::default();
            if GetClientRect(hwnd, &mut rect).is_ok() {
                let client_width = (rect.right - rect.left).max(1);
                let client_height = (rect.bottom - rect.top).max(1);
                overlay.framebuffer_size = [client_width as u32, client_height as u32];
                if client_width > 100 && client_height > 100 {
                    overlay.logical_size = [client_width as f32, client_height as f32];
                }
                overlay.pixels_per_point =
                    (overlay.gui_scale_percent as f32 / 100.0).clamp(0.75, 1.75);
            }

            let mut cursor = POINT::default();
            if GetCursorPos(&mut cursor).is_ok() && ScreenToClient(hwnd, &mut cursor).as_bool() {
                let scale = overlay.pixels_per_point.max(0.01);
                let pos = pos2(cursor.x as f32 / scale, cursor.y as f32 / scale);
                overlay.mouse_pos = pos;
                events.push(Event::PointerMoved(pos));
            }
        }
    }

    let mouse = hooks::mouse_state();
    push_pointer_button_event(
        &mut events,
        overlay.mouse_pos,
        PointerButton::Primary,
        &mut overlay.mouse_left,
        mouse.left,
    );
    push_pointer_button_event(
        &mut events,
        overlay.mouse_pos,
        PointerButton::Secondary,
        &mut overlay.mouse_right,
        mouse.right,
    );
    push_pointer_button_event(
        &mut events,
        overlay.mouse_pos,
        PointerButton::Middle,
        &mut overlay.mouse_middle,
        mouse.middle,
    );

    if menu_visible {
        let (scroll_x, scroll_y) = hooks::take_scroll_delta();
        if scroll_x != 0.0 || scroll_y != 0.0 {
            events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: vec2(scroll_x, scroll_y),
                modifiers: Default::default(),
            });
        }
    }

    RawInput {
        viewport_id: ViewportId::ROOT,
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(
                overlay.logical_size[0] / overlay.pixels_per_point.max(0.01),
                overlay.logical_size[1] / overlay.pixels_per_point.max(0.01),
            ),
        )),
        viewports: std::iter::once((
            ViewportId::ROOT,
            ViewportInfo {
                native_pixels_per_point: Some(overlay.pixels_per_point),
                inner_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(
                        overlay.logical_size[0] / overlay.pixels_per_point.max(0.01),
                        overlay.logical_size[1] / overlay.pixels_per_point.max(0.01),
                    ),
                )),
                focused: Some(true),
                ..Default::default()
            },
        ))
        .collect(),
        time: Some(now.duration_since(overlay.start_time).as_secs_f64()),
        predicted_dt: delta.as_secs_f32().max(1.0 / 10000.0),
        events,
        focused: true,
        ..Default::default()
    }
}

fn egui_key(key_code: u32) -> Option<Key> {
    match key_code {
        0x08 => Some(Key::Backspace),
        0x09 => Some(Key::Tab),
        0x0D => Some(Key::Enter),
        0x1B => Some(Key::Escape),
        0x21 => Some(Key::PageUp),
        0x22 => Some(Key::PageDown),
        0x23 => Some(Key::End),
        0x24 => Some(Key::Home),
        0x25 => Some(Key::ArrowLeft),
        0x26 => Some(Key::ArrowUp),
        0x27 => Some(Key::ArrowRight),
        0x28 => Some(Key::ArrowDown),
        0x2D => Some(Key::Insert),
        0x2E => Some(Key::Delete),
        _ => None,
    }
}

pub(crate) fn sync_overlay_mouse_control(overlay: &mut Overlay) {
    if overlay.menu_open || overlay.menu_animations.visible() {
        overlay.mouse_regrab_on_close |= jni_bridge::release_mouse_for_overlay();
        overlay.mouse_control_active = true;
        hooks::set_menu_open(true);
        return;
    }

    hooks::set_menu_open(false);
    if overlay.mouse_control_active {
        if overlay.mouse_regrab_on_close {
            jni_bridge::grab_mouse_after_overlay();
        }
        overlay.mouse_control_active = false;
        overlay.mouse_regrab_on_close = false;
    }
}

fn push_pointer_button_event(
    events: &mut Vec<Event>,
    pos: egui::Pos2,
    button: PointerButton,
    previous: &mut bool,
    current: bool,
) {
    if *previous == current {
        return;
    }

    *previous = current;
    events.push(Event::PointerButton {
        pos,
        button,
        pressed: current,
        modifiers: Default::default(),
    });
}
