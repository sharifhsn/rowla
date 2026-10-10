//! Copied thumbnails are bounded by both count and actual bitmap bytes.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(super) const MAX_ENTRIES: usize = 32;
pub(super) const MAX_BYTES: usize = 16 * 1024 * 1024;
const REFRESH_AGE: Duration = Duration::from_secs(30);

struct Entry<T> {
    id: u32,
    image: T,
    bytes: usize,
    captured: Instant,
}

pub(super) struct PreviewCache<T> {
    entries: VecDeque<Entry<T>>,
    bytes: usize,
}
impl<T> PreviewCache<T> {
    pub(super) fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
        }
    }
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(super) fn get(&self, id: u32) -> Option<&T> {
        self.entries.iter().find(|e| e.id == id).map(|e| &e.image)
    }
    pub(super) fn needs_refresh(&self, id: u32, now: Instant) -> bool {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .is_none_or(|e| now.saturating_duration_since(e.captured) >= REFRESH_AGE)
    }
    pub(super) fn can_prefetch(&self, id: u32, bytes: usize) -> bool {
        // Background warming must not evict another ready thumbnail and loop
        // forever when the desktop exceeds the cache's memory/count budget.
        self.entries.iter().any(|e| e.id == id)
            || (self.len() < MAX_ENTRIES && bytes <= MAX_BYTES.saturating_sub(self.bytes))
    }
    pub(super) fn insert(&mut self, id: u32, image: T, bytes: usize, now: Instant) {
        self.retain(|previous| previous != id);
        if bytes > MAX_BYTES {
            return;
        }
        while self.len() >= MAX_ENTRIES || self.bytes > MAX_BYTES - bytes {
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.push_back(Entry {
            id,
            image,
            bytes,
            captured: now,
        });
    }
    pub(super) fn retain(&mut self, mut alive: impl FnMut(u32) -> bool) {
        // A minimized source cannot supply fresh frames. Keep its last bitmap
        // until replacement, budget eviction or closure; age controls refresh only.
        self.entries.retain(|e| {
            let keep = alive(e.id);
            if !keep {
                self.bytes -= e.bytes;
            }
            keep
        });
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_thumbnail_survives_long_minimization_and_still_needs_refresh() {
        let now = Instant::now();
        let mut cache = PreviewCache::new();
        for id in 1..=9 {
            cache.insert(id, id, 1024, now);
        }
        assert_eq!(cache.get(1), Some(&1));
        assert!(!cache.needs_refresh(1, now + Duration::from_secs(4)));
        assert!(cache.needs_refresh(1, now + REFRESH_AGE));
        let next_day = now + Duration::from_secs(24 * 60 * 60);
        cache.retain(|_| true);
        assert_eq!((cache.len(), cache.bytes()), (9, 9 * 1024));
        assert_eq!(cache.get(1), Some(&1));
        assert!(cache.needs_refresh(1, next_day));
        cache.retain(|_| false);
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
    }
    #[test]
    fn memory_and_count_remain_bounded_and_background_does_not_thrash() {
        let now = Instant::now();
        let mut cache = PreviewCache::new();
        for id in 0..100 {
            cache.insert(id, id, MAX_BYTES / 4, now);
        }
        assert_eq!((cache.len(), cache.bytes()), (4, MAX_BYTES));
        assert_eq!(cache.get(99), Some(&99));
        assert_eq!(cache.get(95), None);
        assert!(!cache.can_prefetch(100, 1));
        assert!(cache.can_prefetch(99, MAX_BYTES / 4));
        cache.clear();
        for id in 0..100 {
            cache.insert(id, id, 1, now);
        }
        assert_eq!(cache.len(), MAX_ENTRIES);
        assert!(!cache.can_prefetch(100, 1));
        cache.insert(100, 100, MAX_BYTES + 1, now);
        assert_eq!(cache.len(), MAX_ENTRIES);
    }
    #[test]
    fn closed_sources_and_replacements_release_their_bitmap_budget() {
        let now = Instant::now();
        let mut cache = PreviewCache::new();
        cache.insert(1, 10, 100, now);
        cache.insert(1, 20, 200, now);
        cache.insert(2, 30, 300, now);
        assert_eq!((cache.len(), cache.bytes()), (2, 500));
        cache.retain(|id| id == 2);
        assert_eq!((cache.len(), cache.bytes()), (1, 300));
        cache.clear();
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
    }
}
