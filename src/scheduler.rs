//! Bounded, fair application reconciliation and failure backoff.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};
#[derive(Default)]
struct Retry {
    failures: u32,
    ready: Option<Instant>,
    queued: bool,
}
#[derive(Default)]
pub(crate) struct FairQueue {
    live: HashMap<i32, Retry>,
    queue: VecDeque<i32>,
}
impl FairQueue {
    pub(crate) fn set_live(&mut self, pids: impl IntoIterator<Item = i32>) {
        let live: HashSet<_> = pids.into_iter().take(256).collect();
        self.live.retain(|pid, _| live.contains(pid));
        self.queue.retain(|pid| live.contains(pid));
        for pid in live {
            self.live.entry(pid).or_default();
        }
    }
    pub(crate) fn notify(&mut self, pid: i32) {
        if let Some(retry) = self.live.get_mut(&pid)
            && !retry.queued
        {
            retry.queued = true;
            self.queue.push_back(pid);
        }
    }
    pub(crate) fn notify_all(&mut self) {
        for (&pid, retry) in &mut self.live {
            if !retry.queued {
                retry.queued = true;
                self.queue.push_back(pid);
            }
        }
    }
    pub(crate) fn next(&mut self, now: Instant) -> Option<i32> {
        for _ in 0..self.queue.len() {
            let pid = self.queue.pop_front()?;
            let retry = self
                .live
                .get_mut(&pid)
                .expect("queue contains only live applications");
            if retry.ready.is_some_and(|ready| now < ready) {
                self.queue.push_back(pid);
                continue;
            }
            retry.queued = false;
            return Some(pid);
        }
        None
    }
    pub(crate) fn finish(&mut self, pid: i32, continuing: bool, failed: bool, now: Instant) {
        let Some(retry) = self.live.get_mut(&pid) else {
            return;
        };
        if failed {
            retry.failures = (retry.failures + 1).min(8);
            retry.ready =
                Some(now + Duration::from_millis((250u64 << (retry.failures - 1)).min(2_000)));
        } else {
            retry.failures = 0;
            retry.ready = None;
        }
        if continuing || failed {
            self.notify(pid);
        }
    }
    pub(crate) fn pending(&self) -> usize {
        self.queue.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_client_cannot_starve_a_healthy_client_or_reset_backoff() {
        let now = Instant::now();
        let mut q = FairQueue::default();
        q.set_live([1, 2]);
        q.notify(1);
        q.notify(2);
        assert_eq!(q.next(now), Some(1));
        q.finish(1, true, true, now);
        for _ in 0..10_000 {
            q.notify(1);
        }
        assert_eq!(q.pending(), 2);
        assert_eq!(q.next(now), Some(2));
        q.finish(2, false, false, now);
        assert_eq!(q.next(now), None);
        assert_eq!(q.next(now + Duration::from_millis(250)), Some(1));
        q.finish(1, true, true, now + Duration::from_millis(250));
        assert_eq!(q.next(now + Duration::from_millis(500)), None);
    }
    #[test]
    fn termination_removes_pending_jobs_and_large_event_bursts_are_bounded() {
        let mut q = FairQueue::default();
        q.set_live(0..10_000);
        q.notify_all();
        assert_eq!(q.pending(), 256);
        for pid in 0..10_000 {
            q.notify(pid);
        }
        assert_eq!(q.pending(), 256);
        q.set_live([5]);
        assert_eq!(q.pending(), 1);
        assert_eq!(q.next(Instant::now()), Some(5));
        assert_eq!(q.next(Instant::now()), None);
    }
    #[test]
    fn in_flight_events_keep_backoff_and_reused_pids_start_fresh() {
        let now = Instant::now();
        let mut q = FairQueue::default();
        q.set_live([1]);
        q.notify(1);
        assert_eq!(q.next(now), Some(1));
        // A new event arrives while this application is under reconciliation.
        q.notify(1);
        q.finish(1, true, true, now);
        assert_eq!(q.pending(), 1);
        assert_eq!(q.next(now), None);
        q.set_live([]);
        assert_eq!(q.pending(), 0);
        q.set_live([1]);
        q.notify_all();
        assert_eq!(q.next(now), Some(1));
        assert_eq!(q.pending(), 0);
        q.finish(1, false, false, now);
        q.notify_all();
        assert_eq!(q.next(now), Some(1));
    }
}
