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
    let count = args.get(2).and_then(|s| s.parse::<usize>().ok());
    let result = match args.get(1).map(String::as_str) {
        Some("--check-compatibility") => Ok(macos::compatibility_probe()),
        Some(command @ ("--check-fixture-activation" | "--check-fixture-related")) => {
            let Some(pid) = args.get(2).and_then(|s| s.parse().ok()) else {
                eprintln!("Pass a disposable native fixture PID.");
                std::process::exit(2);
            };
            Ok(if command == "--check-fixture-related" {
                macos::check_fixture_related(pid)
            } else {
                macos::check_fixture_activation(pid)
            })
        }
        Some("--check-fixture-close") => Ok(args
            .get(2)
            .and_then(|s| s.parse().ok())
            .is_some_and(macos::check_fixture_close)),
        Some("--benchmark-hover") => args
            .get(3)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| {
                "--benchmark-hover COUNT FIXTURE_PID requires the disposable interaction fixture"
                    .to_string()
            })
            .and_then(|pid| macos::benchmark_hover(count.unwrap_or(20), pid))
            .map(|_| true),
        Some("--benchmark-ui") => macos::benchmark_ui(count.unwrap_or(200)).map(|_| true),
        Some(command @ ("--benchmark" | "--benchmark-lifecycle")) => Ok(macos::benchmark(
            count.unwrap_or(300),
            command == "--benchmark-lifecycle",
        )),
        Some("--benchmark-latency") => Ok(macos::benchmark_latency(
            count.unwrap_or(10) as u64,
            args.get(3).and_then(|s| s.parse().ok()),
        )),
        Some("--benchmark-discovery") => Ok(macos::benchmark_discovery(count.unwrap_or(60) as u64)),
        Some("--benchmark-scan") => Ok(macos::benchmark_scan(count.unwrap_or(500))),
        Some("--gui-smoke") => macos::gui_smoke(count.unwrap_or(100)).map(|_| true),
        Some("--probe-updater") => Ok(macos::probe_updater()),
        _ => macos::run_app().map(|_| true),
    };
    let passed = result.unwrap_or_else(|error| {
        eprintln!("{error}");
        false
    });
    std::process::exit(if passed { 0 } else { 1 });
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!(
        "Rowla's GUI requires macOS 15.2+. Its config and model library can be used on other platforms."
    );
    std::process::exit(1);
}
