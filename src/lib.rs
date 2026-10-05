//! Rowla's owned data model and macOS integration boundary.
//! Native capture and UI internals are deliberately not exposed as raw pointers.
pub mod config;
pub mod models;
mod window_order;

#[cfg(target_os = "macos")]
mod capture;
#[cfg(target_os = "macos")]
mod dock;
#[cfg(target_os = "macos")]
mod ipc_budget;
#[cfg(target_os = "macos")]
mod native_features;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "macos")]
mod private_api;
#[cfg(any(target_os = "macos", test))]
mod runtime;
#[cfg(any(target_os = "macos", test))]
mod scheduler;
#[cfg(any(target_os = "macos", test))]
mod system_actions;
#[cfg(target_os = "macos")]
mod ui;
#[cfg(target_os = "macos")]
mod updater;

#[cfg(target_os = "macos")]
pub mod macos {
    #[doc(hidden)]
    pub fn compatibility_probe() -> bool {
        crate::native_features::compatibility_probe()
    }
    /// Obtain a read-only snapshot. This can block on Accessibility IPC; call
    /// from a worker, not the host's UI thread. It never requests permissions.
    /// The embedding application needs its own Accessibility approval.
    pub fn window_snapshot() -> crate::models::Snapshot {
        crate::platform::window_snapshot()
    }

    /// Run the standalone AppKit application. Must run on the process main
    /// thread, once per process. Embedders should use `window_snapshot` instead
    /// of replacing their host's application delegate.
    pub fn run_app() -> Result<(), String> {
        crate::ui::run()
    }

    #[doc(hidden)]
    pub fn benchmark(count: usize, lifecycle: bool) -> bool {
        crate::platform::benchmark(count, lifecycle)
    }
    #[doc(hidden)]
    pub fn benchmark_scan(count: usize) -> bool {
        crate::platform::benchmark_scan(count)
    }
    #[doc(hidden)]
    pub fn benchmark_discovery(seconds: u64) -> bool {
        crate::platform::benchmark_discovery(seconds)
    }
    #[doc(hidden)]
    pub fn probe_updater() -> bool {
        crate::updater::probe()
    }
    #[doc(hidden)]
    pub fn gui_smoke(count: usize) -> Result<(), String> {
        crate::ui::gui_smoke(count)
    }
    #[doc(hidden)]
    pub fn benchmark_ui(count: usize) -> Result<(), String> {
        crate::ui::benchmark_ui(count)
    }
    #[doc(hidden)]
    pub fn benchmark_hover(count: usize, fixture: i32) -> Result<(), String> {
        crate::ui::benchmark_hover(count, fixture)
    }
    #[doc(hidden)]
    pub fn benchmark_latency(seconds: u64, fixture: Option<i32>) -> bool {
        crate::platform::benchmark_latency(seconds, fixture)
    }
    #[doc(hidden)]
    pub fn check_fixture_activation(pid: i32) -> bool {
        crate::platform::check_fixture_activation(pid)
    }
    #[doc(hidden)]
    pub fn check_fixture_close(pid: i32) -> bool {
        crate::platform::check_fixture_close(pid)
    }
}
