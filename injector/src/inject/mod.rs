pub mod manual;
pub mod remote_thread;

use crate::process;

pub fn inject(pid: u32, dll_path: &str, method: &str) -> Result<(), String> {
    match method {
        "remote-thread" => remote_thread::inject(pid, dll_path),
        "manual" => manual::inject(pid, dll_path),
        _ => Err("unknown method".to_string()),
    }
}

pub fn validate_target(pid: u32) -> Result<(), String> {
    if !process::is_x64(pid)? {
        return Err("target process is not x64".to_string());
    }
    Ok(())
}
