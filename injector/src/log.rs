use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("injector")
}

pub fn init() {
    let dir = temp_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("injector.log");
    let _ = fs::write(&path, b"");
    if let Ok(mut guard) = LOG_PATH.lock() {
        *guard = Some(path);
    }
    info("injector log started");
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn log_line(level: &str, message: &str) {
    let path = match LOG_PATH.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => None,
    };
    let Some(path) = path else {
        return;
    };
    let line = format!("[{:.3}] [{}] {}\n", now(), level, message);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn info(message: &str) {
    log_line("INFO", message);
    println!("{}", message);
}

pub fn error(message: &str) {
    log_line("ERROR", message);
    eprintln!("{}", message);
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|g| g.clone())
}
