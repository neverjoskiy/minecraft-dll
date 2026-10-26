#![allow(unsafe_op_in_unsafe_fn)]

mod cli;
mod config;
mod inject;
mod jvmti;
mod log;
mod process;

fn main() {
    log::init();
    let raw: Vec<String> = std::env::args().collect();
    let parsed = cli::parse(&raw[1..]);
    let args = match parsed {
        Ok(a) => a,
        Err(e) => {
            log::error(&e);
            std::process::exit(1);
        }
    };
    if let Err(e) = cli::validate(&args) {
        log::error(&e);
        std::process::exit(1);
    }

    let cfg = config::Config {
        process: args.process.clone(),
        pid: args.pid,
        dll: args.dll.as_ref().map(|p| p.to_string_lossy().to_string()),
        method: Some(args.method.clone()),
    };
    config::save(&cfg);

    if args.launch {
        let dll = args.dll.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let java = args.java.as_deref();
        match jvmti::launch(&dll, java, args.game_args.as_deref()) {
            Ok(()) => log::info("launch complete"),
            Err(e) => {
                log::error(&e);
                std::process::exit(1);
            }
        }
        return;
    }

    let pid = if let Some(p) = args.pid {
        p
    } else {
        let name = args.process.as_ref().unwrap();
        let result = if args.wait {
            process::wait_for_process(name, 60)
        } else {
            process::find_pid_by_name(name)
        };
        match result {
            Ok(p) => p,
            Err(e) => {
                log::error(&e);
                std::process::exit(1);
            }
        }
    };

    if let Err(e) = inject::validate_target(pid) {
        log::error(&e);
        std::process::exit(1);
    }

    let dll = args.dll.as_ref().unwrap().to_string_lossy().to_string();
    match inject::inject(pid, &dll, &args.method) {
        Ok(()) => log::info("injection succeeded"),
        Err(e) => {
            log::error(&e);
            std::process::exit(1);
        }
    }
}
