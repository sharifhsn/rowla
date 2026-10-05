#[cfg(target_os = "macos")]
fn main() {
    let snapshot = taskbar_rs::macos::window_snapshot();
    if !snapshot.trusted {
        eprintln!("This executable needs its own Accessibility permission.");
        std::process::exit(1);
    }
    // Titles are intentionally omitted; integrations can opt into them.
    for window in snapshot.windows {
        println!("{}\t{}\t{}", window.id, window.pid, window.bundle);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Window discovery requires macOS.");
}
