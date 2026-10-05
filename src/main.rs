#[cfg(target_os = "macos")]
use taskbar_rs::{config, macos};

#[cfg(target_os = "macos")]
fn main() {
    std::panic::set_hook(Box::new(|info| {
        if config::Config::load().is_ok_and(|c| c.local_crash_reports) {
            use std::io::Write;
            let _ = std::fs::create_dir_all(config::Config::directory());
            let log = config::Config::directory().join("crash.log");
            if std::fs::metadata(&log).is_ok_and(|m| m.len() > 1024 * 1024) {
                let _ = std::fs::rename(&log, log.with_extension("previous.log"));
            }
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)
            {
                let _ = writeln!(
                    f,
                    "{:?}\n{info}\n{}",
                    std::time::SystemTime::now(),
                    std::backtrace::Backtrace::force_capture()
                );
            }
        }
        eprintln!("{info}");
    }));
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--check-compatibility") {
        std::process::exit(if macos::compatibility_probe() { 0 } else { 1 });
    }
    if args
        .get(1)
        .is_some_and(|s| s == "--check-fixture-activation")
    {
        let Some(pid) = args.get(2).and_then(|value| value.parse().ok()) else {
            eprintln!("Pass a disposable native fixture PID.");
            std::process::exit(2);
        };
        std::process::exit(if macos::check_fixture_activation(pid) {
            0
        } else {
            1
        });
    }
    if args.get(1).is_some_and(|s| s == "--benchmark-hover") {
        let Some(fixture) = args.get(3).and_then(|s| s.parse().ok()) else {
            eprintln!(
                "--benchmark-hover COUNT FIXTURE_PID requires the disposable interaction fixture"
            );
            std::process::exit(1);
        };
        if let Err(error) = macos::benchmark_hover(
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20),
            fixture,
        ) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).is_some_and(|s| s == "--benchmark-ui") {
        if let Err(error) =
            macos::benchmark_ui(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200))
        {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args
        .get(1)
        .is_some_and(|s| s == "--benchmark" || s == "--benchmark-lifecycle")
    {
        let ok = macos::benchmark(
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300),
            args[1] == "--benchmark-lifecycle",
        );
        std::process::exit(if ok { 0 } else { 1 });
    }
    if args.get(1).is_some_and(|s| s == "--benchmark-latency") {
        let ok = macos::benchmark_latency(
            args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10),
            args.get(3).and_then(|s| s.parse().ok()),
        );
        std::process::exit(if ok { 0 } else { 1 });
    }
    if args.get(1).is_some_and(|s| s == "--check-fixture-close") {
        let ok = args
            .get(2)
            .and_then(|s| s.parse().ok())
            .is_some_and(macos::check_fixture_close);
        std::process::exit(if ok { 0 } else { 1 });
    }
    if args.get(1).is_some_and(|s| s == "--gui-smoke") {
        if let Err(error) =
            macos::gui_smoke(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100))
        {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).is_some_and(|s| s == "--probe-updater") {
        std::process::exit(if macos::probe_updater() { 0 } else { 1 });
    }
    if args.get(1).is_some_and(|s| s == "--benchmark-discovery") {
        let ok = macos::benchmark_discovery(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60));
        std::process::exit(if ok { 0 } else { 1 });
    }
    if args.get(1).is_some_and(|s| s == "--benchmark-scan") {
        let ok = macos::benchmark_scan(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500));
        std::process::exit(if ok { 0 } else { 1 });
    }
    if let Err(e) = macos::run_app() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "Rowla's GUI requires macOS 15.2+. Its config and model library can be used on other platforms."
    );
    std::process::exit(1);
}
