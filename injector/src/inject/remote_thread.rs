use std::ffi::CString;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
    WriteProcessMemory,
};
use windows::Win32::System::Threading::{CreateRemoteThread, GetExitCodeThread, WaitForSingleObject};

pub fn inject(pid: u32, dll_path: &str) -> Result<(), String> {
    let handle = crate::process::open_process(pid)?;
    let result = unsafe { inject_inner(handle, dll_path) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    result
}

unsafe fn inject_inner(handle: HANDLE, dll_path: &str) -> Result<(), String> {
    let path_bytes = dll_path.as_bytes();
    let size = path_bytes.len() + 1;
    let remote_mem = VirtualAllocEx(handle, None, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if remote_mem.is_null() {
        return Err("VirtualAllocEx failed".to_string());
    }
    let mut written: usize = 0;
    let ok = WriteProcessMemory(
        handle,
        remote_mem,
        path_bytes.as_ptr() as *const _,
        size - 1,
        Some(&mut written),
    );
    if ok.is_err() || written != size - 1 {
        let _ = VirtualFreeEx(handle, remote_mem, 0, MEM_RELEASE);
        return Err("WriteProcessMemory failed".to_string());
    }
    let null_byte = [0u8];
    let _ = WriteProcessMemory(
        handle,
        remote_mem.add(size - 1),
        null_byte.as_ptr() as *const _,
        1,
        None,
    );

    let kernel32 = GetModuleHandleA(windows::core::s!("kernel32.dll"))
        .map_err(|e| e.to_string())?;
    let load_library = GetProcAddress(kernel32, windows::core::s!("LoadLibraryA"))
        .ok_or_else(|| "LoadLibraryA not found".to_string())?;

    let thread = CreateRemoteThread(
        handle,
        None,
        0,
        Some(std::mem::transmute(load_library)),
        Some(remote_mem),
        0,
        None,
    )
    .map_err(|e| e.to_string())?;

    let _ = WaitForSingleObject(thread, 0xFFFFFFFF);
    let mut exit_code: u32 = 0;
    let _ = GetExitCodeThread(thread, &mut exit_code);
    let _ = CloseHandle(thread);
    let _ = VirtualFreeEx(handle, remote_mem, 0, MEM_RELEASE);

    if exit_code == 0 {
        return Err("LoadLibraryA returned null".to_string());
    }
    Ok(())
}
