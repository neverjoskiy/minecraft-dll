use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES,
    TOKEN_QUERY,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_AMD64;
use windows::Win32::System::Threading::{
    GetCurrentProcess, IsWow64Process2, OpenProcess, OpenProcessToken, PROCESS_ALL_ACCESS,
};

pub fn find_pid_by_name(name: &str) -> Result<u32, String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; 260],
        };
        if Process32FirstW(snapshot, &mut entry).is_err() {
            let _ = CloseHandle(snapshot);
            return Err("Process32First failed".to_string());
        }
        loop {
            let current = String::from_utf16_lossy(&entry.szExeFile);
            let current = current.trim_end_matches('\0');
            if current.eq_ignore_ascii_case(name) {
                let pid = entry.th32ProcessID;
                let _ = CloseHandle(snapshot);
                return Ok(pid);
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
        Err(format!("process not found: {}", name))
    }
}

pub fn enable_debug_privilege() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return;
        }
        let mut luid = LUID::default();
        let name: Vec<u16> = "SeDebugPrivilege\0".encode_utf16().collect();
        if LookupPrivilegeValueW(None, windows::core::PCWSTR(name.as_ptr()), &mut luid).is_err() {
            let _ = CloseHandle(token);
            return;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            ..Default::default()
        };
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = windows::Win32::Security::TOKEN_PRIVILEGES_ATTRIBUTES(0x00000002);
        let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        let _ = CloseHandle(token);
    }
}

pub fn open_process(pid: u32) -> Result<HANDLE, String> {
    unsafe {
        match OpenProcess(PROCESS_ALL_ACCESS, false, pid) {
            Ok(h) => Ok(h),
            Err(_) => {
                enable_debug_privilege();
                OpenProcess(PROCESS_ALL_ACCESS, false, pid).map_err(|e| e.to_string())
            }
        }
    }
}

pub fn is_x64(pid: u32) -> Result<bool, String> {
    let handle = open_process(pid)?;
    unsafe {
        let mut machine = windows::Win32::System::SystemInformation::IMAGE_FILE_MACHINE(0);
        let mut native = machine;
        let result = IsWow64Process2(handle, &mut machine, Some(&mut native));
        let _ = CloseHandle(handle);
        match result {
            Ok(()) => Ok(machine == IMAGE_FILE_MACHINE_AMD64),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub fn wait_for_process(name: &str, timeout_secs: u64) -> Result<u32, String> {
    let start = std::time::Instant::now();
    loop {
        if let Ok(pid) = find_pid_by_name(name) {
            return Ok(pid);
        }
        if start.elapsed().as_secs() >= timeout_secs {
            return Err(format!("timeout waiting for process: {}", name));
        }
        thread::sleep(Duration::from_millis(500));
    }
}
