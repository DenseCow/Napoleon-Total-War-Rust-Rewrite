//! Once-per-key reports: the set a warning about bad data keeps so that it is logged once per key per
//! process, not on every read of that data.

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

/// The keys a warning has already been logged for. One `static` per warning.
#[derive(Debug, Default)]
pub(crate) struct ReportedKeys(Mutex<BTreeSet<String>>);

impl ReportedKeys {
    pub(crate) const fn new() -> Self {
        Self(Mutex::new(BTreeSet::new()))
    }

    /// Whether `key` is new to the set (and now in it): the caller logs when it is. A key already in the
    /// set is not cloned. A lock poisoned by a panic elsewhere still holds a usable set, so the report is
    /// neither lost nor repeated.
    pub(crate) fn first(&self, key: &str) -> bool {
        let mut reported = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        !reported.contains(key) && reported.insert(key.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::ReportedKeys;

    #[test]
    fn a_key_is_first_once_also_behind_a_poisoned_lock() {
        let reported = ReportedKeys::new();
        assert!(reported.first("a"));
        assert!(!reported.first("a"));
        assert!(reported.first("b"), "each key has its own report");
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = reported.0.lock().unwrap();
            panic!("poison the lock");
        }));
        assert!(poisoned.is_err() && reported.0.is_poisoned());
        assert!(!reported.first("b"), "still known");
        assert!(reported.first("c"), "still reports");
    }
}
