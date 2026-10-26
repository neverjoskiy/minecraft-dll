






use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::Write,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

use windows::Win32::Foundation::HINSTANCE;

static MODULE: Mutex<Option<PathBuf>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(true);



pub fn set_module(handle: HINSTANCE) {
    let mut buffer = vec![0u16; 512];
    
    let length = unsafe {
        windows::Win32::System::LibraryLoader::GetModuleFileNameW(
            Some(handle.into()),
            &mut buffer,
        )
    };
    if length == 0 {
        return;
    }
    buffer.truncate(length as usize);
    let path = PathBuf::from(OsString::from_wide(&buffer));
    if let Some(directory) = path.parent()
        && let Ok(mut slot) = MODULE.lock()
    {
        *slot = Some(directory.join("xz.log"));
    }
}

fn log_path() -> Option<PathBuf> {
    if let Ok(slot) = MODULE.lock()
        && let Some(path) = slot.as_ref()
    {
        return Some(path.clone());
    }
    
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("xz.log")))
}

fn started() -> &'static Instant {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    STARTED.get_or_init(Instant::now)
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}



pub fn log(message: impl AsRef<str>) {
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let Some(path) = log_path() else {
        return;
    };
    let elapsed = started().elapsed();
    let line = format!(
        "[{:>8.3}] {}\n",
        elapsed.as_secs_f64(),
        message.as_ref()
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}


pub fn log_once(key: &'static str, message: impl AsRef<str>) {
    use std::collections::HashSet;
    static SEEN: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut guard) = seen.lock() {
        if !guard.insert(key) {
            return;
        }
    }
    log(message);
}



pub fn log_every(counter: &'static AtomicU64, interval: u64, message: impl AsRef<str>) {
    let count = counter.fetch_add(1, Ordering::Relaxed);
    if interval == 0 || count % interval != 0 {
        return;
    }
    log(format!("{} (#{count})", message.as_ref()));
}


pub fn reset() {
    let Some(path) = log_path() else {
        return;
    };
    let _ = std::fs::write(&path, b"");
    log("xz log started");
}






pub fn dump_class_once(class_name: &str, bytes: &[u8]) {
    use std::collections::HashSet;
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut guard) = seen.lock() {
        if !guard.insert(class_name.to_string()) {
            return;
        }
    }
    let Some(log_file) = log_path() else {
        return;
    };
    let Some(directory) = log_file.parent() else {
        return;
    };
    let file_name = format!("{}.class", class_name.replace('/', "."));
    let path = directory.join(&file_name);
    match std::fs::write(&path, bytes) {
        Ok(()) => log(format!("cflh: dumped {file_name} ({} bytes)", bytes.len())),
        Err(error) => log(format!("cflh: dump {file_name} failed: {error}")),
    }
}
