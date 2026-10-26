use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub process: Option<String>,
    pub pid: Option<u32>,
    pub dll: Option<PathBuf>,
    pub method: String,
    pub wait: bool,
    pub launch: bool,
    pub java: Option<PathBuf>,
    pub game_args: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            process: None,
            pid: None,
            dll: None,
            method: "remote-thread".to_string(),
            wait: false,
            launch: false,
            java: None,
            game_args: None,
        }
    }
}

pub fn parse(input: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0;
    while i < input.len() {
        let key = &input[i];
        match key.as_str() {
            "--process" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --process".to_string());
                }
                args.process = Some(input[i].clone());
            }
            "--pid" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --pid".to_string());
                }
                let value: u32 = input[i]
                    .parse()
                    .map_err(|_| "invalid pid".to_string())?;
                args.pid = Some(value);
            }
            "--dll" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --dll".to_string());
                }
                args.dll = Some(PathBuf::from(&input[i]));
            }
            "--method" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --method".to_string());
                }
                let value = &input[i];
                if value != "remote-thread" && value != "manual" {
                    return Err("method must be remote-thread or manual".to_string());
                }
                args.method = value.clone();
            }
            "--wait" => {
                args.wait = true;
            }
            "--launch" => {
                args.launch = true;
            }
            "--java" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --java".to_string());
                }
                args.java = Some(PathBuf::from(&input[i]));
            }
            "--game-args" => {
                i += 1;
                if i >= input.len() {
                    return Err("missing value for --game-args".to_string());
                }
                args.game_args = Some(input[i].clone());
            }
            "--help" | "-h" => {
                return Err(usage());
            }
            other => {
                return Err(format!("unknown argument: {}", other));
            }
        }
        i += 1;
    }
    Ok(args)
}

pub fn usage() -> String {
    "Usage: injector [--process <name> | --pid <id>] --dll <path> [--method <remote-thread|manual>] [--wait] [--launch --java <path> --game-args <args>]".to_string()
}

pub fn validate(args: &Args) -> Result<(), String> {
    if args.launch {
        return Ok(());
    }
    if args.dll.is_none() {
        return Err("missing --dll".to_string());
    }
    match (args.process.is_some(), args.pid.is_some()) {
        (true, false) | (false, true) => Ok(()),
        (false, false) => Err("missing --process or --pid".to_string()),
        (true, true) => Err("use either --process or --pid".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        let input = vec![
            "--process".to_string(),
            "javaw.exe".to_string(),
            "--dll".to_string(),
            "C:\\x.dll".to_string(),
            "--method".to_string(),
            "manual".to_string(),
            "--wait".to_string(),
        ];
        let args = parse(&input).unwrap();
        assert_eq!(args.process.as_deref(), Some("javaw.exe"));
        assert_eq!(args.method, "manual");
        assert!(args.wait);
    }

    #[test]
    fn validate_requires_dll() {
        let args = Args::default();
        assert!(validate(&args).is_err());
    }

    #[test]
    fn reject_unknown_method() {
        let input = vec![
            "--method".to_string(),
            "foo".to_string(),
        ];
        assert!(parse(&input).is_err());
    }
}
