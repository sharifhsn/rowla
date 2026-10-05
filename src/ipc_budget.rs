//! A deadline shared by all native calls in one scan slice or control command.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Status {
    pub exhausted: bool,
    pub error: Option<i32>,
    pub calls: u32,
}
#[derive(Clone, Copy, Default)]
struct Context {
    deadline: Option<Instant>,
    status: Status,
}
thread_local! { static CONTEXT: Cell<Context> = Cell::new(Context::default()); }
pub(crate) fn run<T>(duration: Duration, f: impl FnOnce() -> T) -> (T, Status) {
    let previous = CONTEXT.replace(Context {
        deadline: Some(Instant::now() + duration),
        status: Status::default(),
    });
    struct Restore(Context);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONTEXT.set(self.0);
        }
    }
    let restore = Restore(previous);
    let result = f();
    let status = CONTEXT.get().status;
    drop(restore);
    (result, status)
}
pub(crate) fn timeout() -> Option<f32> {
    CONTEXT.with(|cell| {
        let mut c = cell.get();
        let remaining = c
            .deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(20));
        if remaining < Duration::from_millis(1) || crate::runtime::stopping() {
            c.status.exhausted = true;
            cell.set(c);
            return None;
        }
        c.status.calls += 1;
        cell.set(c);
        Some(remaining.min(Duration::from_millis(20)).as_secs_f32())
    })
}
pub(crate) fn record(error: i32, optional_attribute: bool) {
    // Unsupported/no-value optional attributes are not a stalled application.
    if error == 0 || (optional_attribute && [-25205, -25212].contains(&error)) {
        return;
    }
    CONTEXT.with(|cell| {
        let mut c = cell.get();
        c.status.error = Some(error);
        cell.set(c);
    });
}
pub(crate) fn expired() -> bool {
    CONTEXT.get().deadline.is_some_and(|d| Instant::now() >= d) || crate::runtime::stopping()
}
pub(crate) fn healthy() -> bool {
    let c = CONTEXT.get();
    !c.status.exhausted && c.status.error.is_none() && !expired()
}

/// A window closed during enumeration is normal churn, not a hung provider.
/// Discard just that element; genuine IPC failures keep their circuit breaker.
pub(crate) fn discard_destroyed_element() -> bool {
    CONTEXT.with(|cell| {
        let mut context = cell.get();
        if context.status.error != Some(-25202) {
            return false;
        }
        context.status.error = None;
        cell.set(context);
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_stops_calls_and_restores_nested_context() {
        let (_, outer) = run(Duration::from_secs(1), || {
            assert!(timeout().is_some());
            let (_, inner) = run(Duration::ZERO, || assert!(timeout().is_none()));
            assert!(inner.exhausted);
            assert!(timeout().is_some());
            record(-25205, true);
            assert!(healthy());
        });
        assert_eq!(outer.calls, 2);
        assert!(!outer.exhausted);
        let (_, bad) = run(Duration::from_secs(1), || record(-25204, true));
        assert!(!bad.exhausted && bad.error == Some(-25204));
    }
    #[test]
    fn closed_window_does_not_back_off_a_healthy_provider_but_timeout_does() {
        let (_, status) = run(Duration::from_secs(1), || {
            record(-25202, false);
            assert!(discard_destroyed_element());
            assert!(healthy());
            record(-25204, false);
            assert!(!discard_destroyed_element());
            assert!(!healthy());
        });
        assert_eq!(status.error, Some(-25204));
    }
}
