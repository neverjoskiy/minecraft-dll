use std::{
    ffi::c_void,
    mem::transmute,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU32, AtomicU64, Ordering},
    },
};

use min_hook_rs::{
    HookError, create_hook, disable_hook, enable_hook, initialize, is_supported, remove_hook,
    uninitialize,
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::HDC,
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA},
        UI::{
            Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_RSHIFT},
            WindowsAndMessaging::{
                CallWindowProcW, GWLP_WNDPROC, SetWindowLongPtrW, WM_CHAR, WM_INPUT, WM_KEYDOWN,
                WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK,
                WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
                WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
                WM_XBUTTONDBLCLK, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDPROC,
            },
        },
    },
    core::{BOOL, s},
};

use crate::{gui, gui_sky_shader};

type WglSwapBuffers = extern "system" fn(HDC) -> BOOL;
type GlDrawArraysInstanced = extern "system" fn(u32, i32, i32, i32);
type GlDrawArraysInstancedBaseInstance = extern "system" fn(u32, i32, i32, i32, u32);
type GlDrawElementsInstancedBaseVertex = extern "system" fn(u32, i32, u32, *const c_void, i32, i32);
type GlDrawElementsInstancedBaseVertexBaseInstance =
    extern "system" fn(u32, i32, u32, *const c_void, i32, i32, u32);

static SWAP_BUFFERS_TARGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIGINAL_SWAP_BUFFERS: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DRAW_INSTANCED_TARGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIGINAL_DRAW_INSTANCED: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DRAW_BASE_INSTANCE_TARGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIGINAL_DRAW_BASE_INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DRAW_ELEMENTS_BASE_VERTEX_TARGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIGINAL_DRAW_ELEMENTS_BASE_VERTEX: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DRAW_ELEMENTS_BASE_INSTANCE_TARGET: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIGINAL_DRAW_ELEMENTS_BASE_INSTANCE: AtomicPtr<c_void> =
    AtomicPtr::new(std::ptr::null_mut());
static DRAW_HOOKS_TRIED: AtomicBool = AtomicBool::new(false);
static HOOKED_HWND: AtomicIsize = AtomicIsize::new(0);
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static MENU_TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
static UNLOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static MENU_OPEN: AtomicBool = AtomicBool::new(false);
static MOUSE_LEFT_DOWN: AtomicBool = AtomicBool::new(false);
static MOUSE_RIGHT_DOWN: AtomicBool = AtomicBool::new(false);
static MOUSE_MIDDLE_DOWN: AtomicBool = AtomicBool::new(false);
static LAST_KEY_PRESSED: AtomicU32 = AtomicU32::new(0);
static KEY_DOWN_BITS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static KEY_PRESSED_BITS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static TEXT_INPUT: OnceLock<Mutex<Vec<char>>> = OnceLock::new();
static SCROLL_DELTA: OnceLock<Mutex<(f32, f32)>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default)]
pub struct MouseState {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    if !SWAP_BUFFERS_TARGET.load(Ordering::Acquire).is_null() {
        return Ok(());
    }

    if !is_supported() {
        return Err("min_hook_rs supports only x64 Windows targets".into());
    }

    let opengl32 = match unsafe { GetModuleHandleA(s!("opengl32.dll")) } {
        Ok(module) => module,
        Err(_) => unsafe { LoadLibraryA(s!("opengl32.dll"))? },
    };

    let proc = unsafe { GetProcAddress(opengl32, s!("wglSwapBuffers")) }
        .ok_or("GetProcAddress(opengl32!wglSwapBuffers) failed")?;
    let target = proc as *mut c_void;

    initialize_min_hook()?;

    let trampoline = create_hook(target, hooked_wgl_swap_buffers as *mut c_void)?;
    ORIGINAL_SWAP_BUFFERS.store(trampoline, Ordering::Release);
    SWAP_BUFFERS_TARGET.store(target, Ordering::Release);
    if let Err(error) = enable_hook(target) {
        ORIGINAL_SWAP_BUFFERS.store(std::ptr::null_mut(), Ordering::Release);
        SWAP_BUFFERS_TARGET.store(std::ptr::null_mut(), Ordering::Release);
        let _ = remove_hook(target);
        return Err(error.into());
    }

    Ok(())
}

pub unsafe fn shutdown() {
    destroy_draw_hooks();
    let target = SWAP_BUFFERS_TARGET.swap(std::ptr::null_mut(), Ordering::AcqRel);
    ORIGINAL_SWAP_BUFFERS.store(std::ptr::null_mut(), Ordering::Release);
    if !target.is_null() {
        ignore_min_hook_teardown_error(disable_hook(target));
        ignore_min_hook_teardown_error(remove_hook(target));
    }
    ignore_min_hook_teardown_error(uninitialize());
    unsafe { restore_wndproc_hook() };
}

pub fn take_menu_toggle_requested() -> bool {
    MENU_TOGGLE_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Ordering::Release);
}

pub fn request_unload() {
    UNLOAD_REQUESTED.store(true, Ordering::Release);
}

pub fn take_unload_requested() -> bool {
    UNLOAD_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn mouse_state() -> MouseState {
    MouseState {
        left: MOUSE_LEFT_DOWN.load(Ordering::Acquire),
        right: MOUSE_RIGHT_DOWN.load(Ordering::Acquire),
        middle: MOUSE_MIDDLE_DOWN.load(Ordering::Acquire),
    }
}

pub fn take_last_key_pressed() -> Option<u32> {
    let key_code = LAST_KEY_PRESSED.swap(0, Ordering::AcqRel);
    (key_code != 0).then_some(key_code)
}

pub fn clear_last_key_pressed() {
    LAST_KEY_PRESSED.store(0, Ordering::Release);
}

pub fn is_key_down(key_code: u32) -> bool {
    let Some((bucket, bit)) = key_bit(key_code) else {
        return false;
    };

    (KEY_DOWN_BITS[bucket].load(Ordering::Acquire) & bit) != 0
}

pub fn take_pressed_keys() -> Vec<u32> {
    let mut keys = Vec::new();
    for (bucket_index, bucket) in KEY_PRESSED_BITS.iter().enumerate() {
        let mut bits = bucket.swap(0, Ordering::AcqRel);
        while bits != 0 {
            let bit_index = bits.trailing_zeros();
            keys.push((bucket_index as u32 * 64) + bit_index);
            bits &= !(1_u64 << bit_index);
        }
    }

    keys
}

pub fn take_text_input() -> Vec<char> {
    TEXT_INPUT
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut chars| chars.drain(..).collect())
        .unwrap_or_default()
}


pub fn take_scroll_delta() -> (f32, f32) {
    SCROLL_DELTA
        .get_or_init(|| Mutex::new((0.0, 0.0)))
        .lock()
        .map(|mut d| std::mem::take(&mut *d))
        .unwrap_or((0.0, 0.0))
}

fn record_scroll(dx: f32, dy: f32) {
    if let Ok(mut d) = SCROLL_DELTA.get_or_init(|| Mutex::new((0.0, 0.0))).lock() {
        d.0 += dx;
        d.1 += dy;
    }
}

pub fn clear_bind_input() {
    for bucket in &KEY_DOWN_BITS {
        bucket.store(0, Ordering::Release);
    }
    for bucket in &KEY_PRESSED_BITS {
        bucket.store(0, Ordering::Release);
    }
}

pub fn install_wndproc_hook(hwnd: HWND) {
    let hwnd_value = hwnd.0 as isize;
    let current = HOOKED_HWND.load(Ordering::Acquire);
    if current == hwnd_value {
        return;
    }

    if current != 0 {
        return;
    }

    if HOOKED_HWND
        .compare_exchange(0, hwnd_value, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let original = unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            overlay_wnd_proc as *const () as usize as isize,
        )
    };
    if original == 0 {
        HOOKED_HWND.store(0, Ordering::Release);
        return;
    }

    ORIGINAL_WNDPROC.store(original, Ordering::Release);
}

extern "system" fn hooked_wgl_swap_buffers(hdc: HDC) -> BOOL {
    ensure_draw_hooks();
    
    gui::render(hdc);
    gui_sky_shader::clear_sky_suppress();

    let original = ORIGINAL_SWAP_BUFFERS.load(Ordering::Acquire);
    if original.is_null() {
        return BOOL(0);
    }

    let original: WglSwapBuffers = unsafe { transmute(original) };
    original(hdc)
}


fn ensure_draw_hooks() {
    if DRAW_HOOKS_TRIED.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Err(_) = initialize_min_hook() {
        return;
    }

    install_one_draw_hook(
        "glDrawArraysInstanced",
        hooked_gl_draw_arrays_instanced as *mut c_void,
        &DRAW_INSTANCED_TARGET,
        &ORIGINAL_DRAW_INSTANCED,
    );
    install_one_draw_hook(
        "glDrawArraysInstancedBaseInstance",
        hooked_gl_draw_arrays_instanced_base_instance as *mut c_void,
        &DRAW_BASE_INSTANCE_TARGET,
        &ORIGINAL_DRAW_BASE_INSTANCE,
    );
    install_one_draw_hook(
        "glDrawElementsInstancedBaseVertex",
        hooked_gl_draw_elements_instanced_base_vertex as *mut c_void,
        &DRAW_ELEMENTS_BASE_VERTEX_TARGET,
        &ORIGINAL_DRAW_ELEMENTS_BASE_VERTEX,
    );
    install_one_draw_hook(
        "glDrawElementsInstancedBaseVertexBaseInstance",
        hooked_gl_draw_elements_instanced_base_vertex_base_instance as *mut c_void,
        &DRAW_ELEMENTS_BASE_INSTANCE_TARGET,
        &ORIGINAL_DRAW_ELEMENTS_BASE_INSTANCE,
    );
}

fn install_one_draw_hook(
    name: &str,
    detour: *mut c_void,
    target_slot: &AtomicPtr<c_void>,
    original_slot: &AtomicPtr<c_void>,
) {
    let proc = unsafe { gui_sky_shader::load_gl_proc(name) };
    if proc.is_null() {
        return;
    }
    match create_hook(proc, detour) {
        Ok(trampoline) => {
            original_slot.store(trampoline, Ordering::Release);
            target_slot.store(proc, Ordering::Release);
            if enable_hook(proc).is_err() {
                original_slot.store(std::ptr::null_mut(), Ordering::Release);
                target_slot.store(std::ptr::null_mut(), Ordering::Release);
                let _ = remove_hook(proc);
            }
        }
        Err(_) => {}
    }
}

fn destroy_draw_hooks() {
    for (target_slot, original_slot) in [
        (&DRAW_INSTANCED_TARGET, &ORIGINAL_DRAW_INSTANCED),
        (&DRAW_BASE_INSTANCE_TARGET, &ORIGINAL_DRAW_BASE_INSTANCE),
        (
            &DRAW_ELEMENTS_BASE_VERTEX_TARGET,
            &ORIGINAL_DRAW_ELEMENTS_BASE_VERTEX,
        ),
        (
            &DRAW_ELEMENTS_BASE_INSTANCE_TARGET,
            &ORIGINAL_DRAW_ELEMENTS_BASE_INSTANCE,
        ),
    ] {
        let target = target_slot.swap(std::ptr::null_mut(), Ordering::AcqRel);
        original_slot.store(std::ptr::null_mut(), Ordering::Release);
        if !target.is_null() {
            ignore_min_hook_teardown_error(disable_hook(target));
            ignore_min_hook_teardown_error(remove_hook(target));
        }
    }
    DRAW_HOOKS_TRIED.store(false, Ordering::Release);
}

extern "system" fn hooked_gl_draw_arrays_instanced(
    mode: u32,
    first: i32,
    count: i32,
    instance_count: i32,
) {
    if gui_sky_shader::intercept_sky_draw_arrays(mode, count, instance_count) {
        return;
    }
    let original = ORIGINAL_DRAW_INSTANCED.load(Ordering::Acquire);
    if original.is_null() {
        return;
    }
    let original: GlDrawArraysInstanced = unsafe { transmute(original) };
    original(mode, first, count, instance_count);
}

extern "system" fn hooked_gl_draw_arrays_instanced_base_instance(
    mode: u32,
    first: i32,
    count: i32,
    instance_count: i32,
    base_instance: u32,
) {
    if gui_sky_shader::intercept_sky_draw_arrays(mode, count, instance_count) {
        return;
    }
    let original = ORIGINAL_DRAW_BASE_INSTANCE.load(Ordering::Acquire);
    if original.is_null() {
        return;
    }
    let original: GlDrawArraysInstancedBaseInstance = unsafe { transmute(original) };
    original(mode, first, count, instance_count, base_instance);
}

extern "system" fn hooked_gl_draw_elements_instanced_base_vertex(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const c_void,
    instance_count: i32,
    base_vertex: i32,
) {
    if gui_sky_shader::intercept_sky_draw_elements() {
        return;
    }
    let original = ORIGINAL_DRAW_ELEMENTS_BASE_VERTEX.load(Ordering::Acquire);
    if original.is_null() {
        return;
    }
    let original: GlDrawElementsInstancedBaseVertex = unsafe { transmute(original) };
    original(mode, count, type_, indices, instance_count, base_vertex);
}

extern "system" fn hooked_gl_draw_elements_instanced_base_vertex_base_instance(
    mode: u32,
    count: i32,
    type_: u32,
    indices: *const c_void,
    instance_count: i32,
    base_vertex: i32,
    base_instance: u32,
) {
    if gui_sky_shader::intercept_sky_draw_elements() {
        return;
    }
    let original = ORIGINAL_DRAW_ELEMENTS_BASE_INSTANCE.load(Ordering::Acquire);
    if original.is_null() {
        return;
    }
    let original: GlDrawElementsInstancedBaseVertexBaseInstance = unsafe { transmute(original) };
    original(
        mode,
        count,
        type_,
        indices,
        instance_count,
        base_vertex,
        base_instance,
    );
}

fn initialize_min_hook() -> Result<(), HookError> {
    match initialize() {
        Ok(()) | Err(HookError::AlreadyInitialized) => Ok(()),
        Err(error) => Err(error),
    }
}

fn ignore_min_hook_teardown_error(result: Result<(), HookError>) {
    match result {
        Ok(()) | Err(HookError::Disabled | HookError::NotCreated | HookError::NotInitialized) => {}
        Err(_) => {}
    }
}

unsafe fn restore_wndproc_hook() {
    let hwnd_value = HOOKED_HWND.swap(0, Ordering::AcqRel);
    let original = ORIGINAL_WNDPROC.swap(0, Ordering::AcqRel);
    if hwnd_value == 0 || original == 0 {
        return;
    }

    let hwnd = HWND(hwnd_value as *mut c_void);
    let _ = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, original) };
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let rshift = is_rshift_message(lparam);
            if !key_was_down(lparam) && !rshift {
                LAST_KEY_PRESSED.store(wparam.0 as u32, Ordering::Release);
                record_key_pressed(wparam.0 as u32);
            }
            if !rshift {
                set_key_down(wparam.0 as u32, true);
            }
            if rshift && !key_was_down(lparam) {
                MENU_TOGGLE_REQUESTED.store(true, Ordering::Release);
            }
        }
        WM_KEYUP | WM_SYSKEYUP => set_key_down(wparam.0 as u32, false),
        WM_CHAR => record_text_input(wparam.0 as u32),
        WM_LBUTTONDOWN => {
            MOUSE_LEFT_DOWN.store(true, Ordering::Release);
            record_mouse_button(0x01);
        }
        WM_LBUTTONUP => {
            MOUSE_LEFT_DOWN.store(false, Ordering::Release);
            set_key_down(0x01, false);
        }
        WM_RBUTTONDOWN => {
            MOUSE_RIGHT_DOWN.store(true, Ordering::Release);
            record_mouse_button(0x02);
        }
        WM_RBUTTONUP => {
            MOUSE_RIGHT_DOWN.store(false, Ordering::Release);
            set_key_down(0x02, false);
        }
        WM_MBUTTONDOWN => {
            MOUSE_MIDDLE_DOWN.store(true, Ordering::Release);
            record_mouse_button(0x04);
        }
        WM_MBUTTONUP => {
            MOUSE_MIDDLE_DOWN.store(false, Ordering::Release);
            set_key_down(0x04, false);
        }
        WM_XBUTTONUP => {
            let xbutton = (wparam.0 >> 16) & 0xffff;
            let vk = match xbutton {
                1 => 0x05,
                2 => 0x06,
                _ => 0,
            };
            if vk != 0 {
                set_key_down(vk, false);
            }
        }
        WM_XBUTTONDOWN => {
            let xbutton = (wparam.0 >> 16) & 0xffff;
            let vk = match xbutton {
                1 => 0x05,
                2 => 0x06,
                _ => 0,
            };
            if vk != 0 {
                record_mouse_button(vk);
            }
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
            record_scroll(0.0, delta);
        }
        WM_MOUSEHWHEEL => {
            let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
            record_scroll(delta, 0.0);
        }
        _ => {}
    }

    if MENU_OPEN.load(Ordering::Acquire) && should_block_game_input(msg) {
        return LRESULT(0);
    }

    let original = ORIGINAL_WNDPROC.load(Ordering::Acquire);
    if original == 0 {
        return LRESULT(0);
    }

    let original: WNDPROC = unsafe { transmute(original) };
    unsafe { CallWindowProcW(original, hwnd, msg, wparam, lparam) }
}

fn record_text_input(value: u32) {
    let Some(ch) = char::from_u32(value) else {
        return;
    };
    if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
        return;
    }

    if let Ok(mut chars) = TEXT_INPUT.get_or_init(|| Mutex::new(Vec::new())).lock() {
        chars.push(ch);
    }
}

fn is_rshift_message(lparam: LPARAM) -> bool {
    let scan_code = ((lparam.0 >> 16) & 0xff) as u32;
    let rshift_scan_code = unsafe { MapVirtualKeyW(VK_RSHIFT.0 as u32, MAPVK_VK_TO_VSC) } & 0xff;
    scan_code == rshift_scan_code
}

fn key_was_down(lparam: LPARAM) -> bool {
    (lparam.0 & (1isize << 30)) != 0
}

fn key_bit(key_code: u32) -> Option<(usize, u64)> {
    if key_code > 255 {
        return None;
    }

    let bucket = (key_code / 64) as usize;
    let bit = 1_u64 << (key_code % 64);
    Some((bucket, bit))
}

fn record_key_pressed(key_code: u32) {
    let Some((bucket, bit)) = key_bit(key_code) else {
        return;
    };

    KEY_PRESSED_BITS[bucket].fetch_or(bit, Ordering::AcqRel);
    KEY_DOWN_BITS[bucket].fetch_or(bit, Ordering::AcqRel);
}

fn record_mouse_button(vk: u32) {
    LAST_KEY_PRESSED.store(vk, Ordering::Release);
    record_key_pressed(vk);
    set_key_down(vk, true);
}

pub fn clear_mouse_button_state(vk: u32) {
    set_key_down(vk, false);
    match vk {
        0x01 => MOUSE_LEFT_DOWN.store(false, Ordering::Release),
        0x02 => MOUSE_RIGHT_DOWN.store(false, Ordering::Release),
        0x04 => MOUSE_MIDDLE_DOWN.store(false, Ordering::Release),
        0x05 | 0x06 => {}
        _ => {}
    }
}

fn set_key_down(key_code: u32, down: bool) {
    let Some((bucket, bit)) = key_bit(key_code) else {
        return;
    };

    if down {
        KEY_DOWN_BITS[bucket].fetch_or(bit, Ordering::AcqRel);
    } else {
        KEY_DOWN_BITS[bucket].fetch_and(!bit, Ordering::AcqRel);
    }
}

fn should_block_game_input(msg: u32) -> bool {
    matches!(
        msg,
        WM_KEYDOWN
            | WM_KEYUP
            | WM_SYSKEYDOWN
            | WM_SYSKEYUP
            | WM_CHAR
            | WM_MOUSEMOVE
            | WM_INPUT
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
            | WM_LBUTTONDOWN
            | WM_LBUTTONUP
            | WM_LBUTTONDBLCLK
            | WM_RBUTTONDOWN
            | WM_RBUTTONUP
            | WM_RBUTTONDBLCLK
            | WM_MBUTTONDOWN
            | WM_MBUTTONUP
            | WM_MBUTTONDBLCLK
            | WM_XBUTTONDOWN
            | WM_XBUTTONUP
            | WM_XBUTTONDBLCLK
    )
}
