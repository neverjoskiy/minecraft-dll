use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static mut HWND_PROCESS: HWND = HWND(std::ptr::null_mut());
static mut HWND_PID: HWND = HWND(std::ptr::null_mut());
static mut HWND_DLL: HWND = HWND(std::ptr::null_mut());
static mut HWND_METHOD: HWND = HWND(std::ptr::null_mut());
static mut HWND_WAIT: HWND = HWND(std::ptr::null_mut());
static mut HWND_STATUS: HWND = HWND(std::ptr::null_mut());

const ID_PROCESS: i32 = 101;
const ID_PID: i32 = 102;
const ID_DLL: i32 = 103;
const ID_METHOD: i32 = 104;
const ID_WAIT: i32 = 105;
const ID_INJECT: i32 = 106;
const ID_STATUS: i32 = 107;

fn get_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn set_status(text: &str) {
    unsafe {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = SetWindowTextW(HWND_STATUS, windows::core::PCWSTR(wide.as_ptr()));
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hinst = GetModuleHandleW(None).unwrap_or_default();
            let hinst_opt = Some(hinst.into());
            let parent = Some(hwnd);
            let make_edit = |x, y, id| {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    windows::core::PCWSTR("Edit\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                    windows::core::PCWSTR("\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                    WS_CHILD | WS_VISIBLE | WS_BORDER,
                    x, y, 250, 22,
                    parent,
                    Some(windows::Win32::UI::WindowsAndMessaging::HMENU(id as *mut _)),
                    hinst_opt,
                    None,
                )
                .unwrap_or_default()
            };
            HWND_PROCESS = make_edit(100, 20, ID_PROCESS);
            HWND_PID = make_edit(100, 50, ID_PID);
            HWND_DLL = make_edit(100, 80, ID_DLL);

            HWND_METHOD = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR("ComboBox\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                windows::core::PCWSTR("\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 3),
                100, 110, 250, 80,
                parent,
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(ID_METHOD as *mut _)),
                hinst_opt,
                None,
            )
            .unwrap_or_default();
            let s1: Vec<u16> = "remote-thread\0".encode_utf16().collect();
            let s2: Vec<u16> = "manual\0".encode_utf16().collect();
            let _ = SendMessageW(HWND_METHOD, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(s1.as_ptr() as isize)));
            let _ = SendMessageW(HWND_METHOD, CB_ADDSTRING, Some(WPARAM(0)), Some(LPARAM(s2.as_ptr() as isize)));
            let _ = SendMessageW(HWND_METHOD, CB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));

            HWND_WAIT = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR("Button\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                windows::core::PCWSTR("Wait\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 3),
                100, 140, 80, 22,
                parent,
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(ID_WAIT as *mut _)),
                hinst_opt,
                None,
            )
            .unwrap_or_default();

            let _ = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR("Button\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                windows::core::PCWSTR("Inject\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                WS_CHILD | WS_VISIBLE,
                200, 140, 150, 22,
                parent,
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(ID_INJECT as *mut _)),
                hinst_opt,
                None,
            );

            HWND_STATUS = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::PCWSTR("Static\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                windows::core::PCWSTR("Ready\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
                WS_CHILD | WS_VISIBLE,
                20, 175, 330, 22,
                parent,
                Some(windows::Win32::UI::WindowsAndMessaging::HMENU(ID_STATUS as *mut _)),
                hinst_opt,
                None,
            )
            .unwrap_or_default();
            LRESULT(0)
        }
        WM_COMMAND => {
            if (wparam.0 & 0xFFFF) as i32 == ID_INJECT {
                let process = get_text(HWND_PROCESS);
                let pid_text = get_text(HWND_PID);
                let dll = get_text(HWND_DLL);
                let wait = SendMessageW(HWND_WAIT, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == 1;
                let sel = SendMessageW(HWND_METHOD, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0 as usize;
                let method = if sel == 1 { "manual" } else { "remote-thread" };

                if dll.trim().is_empty() {
                    set_status("missing dll");
                    return LRESULT(0);
                }

                let mut pid: Option<u32> = None;
                if !pid_text.trim().is_empty() {
                    match pid_text.trim().parse::<u32>() {
                        Ok(v) => pid = Some(v),
                        Err(_) => {
                            set_status("invalid pid");
                            return LRESULT(0);
                        }
                    }
                }

                if pid.is_none() && process.trim().is_empty() {
                    set_status("missing process or pid");
                    return LRESULT(0);
                }

                let target_pid = if let Some(p) = pid {
                    p
                } else {
                    let name = process.trim().to_string();
                    if wait {
                        match crate::process::wait_for_process(&name, 60) {
                            Ok(p) => p,
                            Err(e) => {
                                set_status(&e);
                                return LRESULT(0);
                            }
                        }
                    } else {
                        match crate::process::find_pid_by_name(&name) {
                            Ok(p) => p,
                            Err(e) => {
                                set_status(&e);
                                return LRESULT(0);
                            }
                        }
                    }
                };

                if let Err(e) = crate::inject::validate_target(target_pid) {
                    set_status(&e);
                    return LRESULT(0);
                }

                match crate::inject::inject(target_pid, dll.trim(), method) {
                    Ok(()) => set_status("injection succeeded"),
                    Err(e) => set_status(&e),
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn run() -> i32 {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let class_name: Vec<u16> = "InjectorGui\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            lpszClassName: windows::core::PCWSTR(class_name.as_ptr()),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(
                windows::Win32::Graphics::Gdi::GetStockObject(windows::Win32::Graphics::Gdi::WHITE_BRUSH).0,
            ),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let title: Vec<u16> = "Injector\0".encode_utf16().collect();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::PCWSTR(class_name.as_ptr()),
            windows::core::PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100, 100, 400, 230,
            None,
            None,
            Some(hinst.into()),
            None,
        );

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        msg.wParam.0 as i32
    }
}
