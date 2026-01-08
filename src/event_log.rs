//! Event log for tracking command execution
//!
//! This module provides a simple ring buffer for logging commands as they're
//! dispatched. The log is independent of App and can be tested in isolation.

use std::collections::VecDeque;
use std::time::Instant;

/// Maximum number of entries to keep in the log
const MAX_LOG_SIZE: usize = 100;

/// A single event log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Human-readable description (from AppCommand::description())
    pub description: &'static str,

    /// Timestamp when the event occurred
    pub timestamp: Instant,

    /// Whether this command was undoable
    pub is_undoable: bool,
}

/// Event log for tracking command execution
///
/// Uses a ring buffer (VecDeque) for O(1) push/pop operations.
/// When capacity is reached, oldest entries are evicted.
#[derive(Debug)]
pub struct EventLog {
    /// Ring buffer of log entries (newest at back)
    entries: VecDeque<LogEntry>,

    /// Maximum capacity
    capacity: usize,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLog {
    /// Create a new event log with default capacity
    pub fn new() -> Self {
        Self::with_capacity(MAX_LOG_SIZE)
    }

    /// Create a new event log with custom capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Log a command execution
    pub fn log(&mut self, description: &'static str, is_undoable: bool) {
        let entry = LogEntry {
            description,
            timestamp: Instant::now(),
            is_undoable,
        };

        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Get entries in reverse chronological order (newest first)
    pub fn entries_recent_first(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter().rev()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if log is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new_log_is_empty() {
        let log = EventLog::new();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_log_entry() {
        let mut log = EventLog::new();
        log.log("test command", true);

        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());

        let entries: Vec<_> = log.entries_recent_first().collect();
        assert_eq!(entries[0].description, "test command");
        assert!(entries[0].is_undoable);
    }

    #[test]
    fn test_entries_in_reverse_order() {
        let mut log = EventLog::new();
        log.log("first", true);
        log.log("second", true);
        log.log("third", true);

        let entries: Vec<_> = log.entries_recent_first().collect();
        assert_eq!(entries[0].description, "third");
        assert_eq!(entries[1].description, "second");
        assert_eq!(entries[2].description, "first");
    }

    #[test]
    fn test_capacity_limit() {
        let mut log = EventLog::with_capacity(3);
        log.log("a", true);
        log.log("b", true);
        log.log("c", true);
        log.log("d", true); // Should evict "a"

        assert_eq!(log.len(), 3);

        let entries: Vec<_> = log.entries_recent_first().collect();
        assert_eq!(entries[0].description, "d");
        assert_eq!(entries[1].description, "c");
        assert_eq!(entries[2].description, "b");
    }

    #[test]
    fn test_clear() {
        let mut log = EventLog::new();
        log.log("test", true);
        log.clear();

        assert!(log.is_empty());
    }

    #[test]
    fn test_undoable_flag_preserved() {
        let mut log = EventLog::new();
        log.log("undoable", true);
        log.log("not undoable", false);

        let entries: Vec<_> = log.entries_recent_first().collect();
        assert!(!entries[0].is_undoable);
        assert!(entries[1].is_undoable);
    }

    #[test]
    fn test_timestamps_are_monotonic() {
        let mut log = EventLog::new();
        log.log("first", true);
        thread::sleep(Duration::from_millis(10));
        log.log("second", true);

        let entries: Vec<_> = log.entries_recent_first().collect();
        assert!(entries[0].timestamp > entries[1].timestamp);
    }
}
