use std::path::Path;
use std::path::PathBuf;

pub fn launch(dll_path: &str, java: Option<&Path>, game_args: Option<&str>) -> Result<(), String> {
    let java_exe = match java {
        Some(p) => p.to_path_buf(),
        None => find_java(),
    };
    let mut cmd = std::process::Command::new(&java_exe);
    cmd.arg(format!("-agentpath:{}", dll_path));
    if let Some(args) = game_args {
        for token in args.split_whitespace() {
            cmd.arg(token);
        }
    }
    let status = cmd.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("java exited with status: {}", status));
    }
    Ok(())
}

fn find_java() -> PathBuf {
    if let Ok(home) = std::env::var("JAVA_HOME")
        && !home.is_empty()
    {
        let candidate = PathBuf::from(home).join("bin").join("java.exe");
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("java")
}
