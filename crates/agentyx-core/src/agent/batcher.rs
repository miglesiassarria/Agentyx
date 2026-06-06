//! Delta batcher — accumulates `ContentDelta` text and flushes
//! either at a wall-clock interval (50 ms) or when the buffer
//! exceeds a character count (100). Implements F01.AC12.
//!
//! Used by the agent loop to avoid emitting one
//! `chat.content.delta.v1` per token when the provider is
//! producing them at high rates (Ollama local can do > 1000
//! tokens/s).
//!
//! The batcher is a single-consumer type: the loop creates one
//! per `run_id`, calls `push(text)` as deltas arrive, and
//! `flush()` either explicitly (e.g. before a `ToolUse` event
//! that needs a "natural" cut) or implicitly via the timeout
//! thread. The batcher is **not** async; it runs in a
//! `std::thread` that sleeps for `interval` and drains.
//!
//! For the v0.1 minimal slice, the batcher is a **synchronous
//! accumulator** owned by the agent loop; the loop polls its
//! `take()` from a Tokio task. The simpler design (no background
//! thread) is preferred — Tokio timers handle the timing.

use std::time::{Duration, Instant};

/// Tunables for the batcher. `interval` is the wall-clock target
/// between flushes; `max_chars` is the buffer cap that triggers
/// an early flush.
#[derive(Debug, Clone, Copy)]
pub struct BatcherConfig {
    /// Maximum time between two flushes.
    pub interval: Duration,
    /// Maximum number of chars to accumulate before forcing a
    /// flush.
    pub max_chars: usize,
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_millis(50),
            max_chars: 100,
        }
    }
}

/// Accumulator of `ContentDelta` text. The agent loop pushes
/// incoming chunks; `take()` returns the accumulated string
/// (concatenated) and resets the buffer.
///
/// The batcher also tracks **elapsed time since the last
/// flush** so the loop can decide whether to emit even if the
/// buffer is short (e.g. a small delta arrived 60 ms ago).
pub struct DeltaBatcher {
    config: BatcherConfig,
    buffer: String,
    last_flush: Instant,
    chars_since_flush: usize,
}

impl DeltaBatcher {
    /// Create a new batcher. The first `take()` call returns an
    /// empty string even if no `push` has happened.
    #[must_use]
    pub fn new(config: BatcherConfig) -> Self {
        Self {
            config,
            buffer: String::new(),
            last_flush: Instant::now(),
            chars_since_flush: 0,
        }
    }

    /// Append a delta to the buffer.
    pub fn push(&mut self, text: &str) {
        self.buffer.push_str(text);
        self.chars_since_flush = self.chars_since_flush.saturating_add(text.chars().count());
    }

    /// Returns `true` if the batcher has accumulated enough text
    /// or time to justify a flush. The agent loop calls this on
    /// each event to decide whether to emit now.
    #[must_use]
    pub fn should_flush(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        if self.chars_since_flush >= self.config.max_chars {
            return true;
        }
        self.last_flush.elapsed() >= self.config.interval
    }

    /// Take the accumulated text, reset the buffer, and update
    /// the last-flush timestamp. Returns `None` if the buffer is
    /// empty.
    pub fn take(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            return None;
        }
        let out = std::mem::take(&mut self.buffer);
        self.last_flush = Instant::now();
        self.chars_since_flush = 0;
        Some(out)
    }

    /// Current buffer length in characters (for tests / logs).
    #[must_use]
    pub fn buffered_chars(&self) -> usize {
        self.chars_since_flush
    }

    /// Reset the batcher (e.g. between turns or after a tool
    /// call).
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.last_flush = Instant::now();
        self.chars_since_flush = 0;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn empty_take_returns_none() {
        let mut b = DeltaBatcher::new(BatcherConfig::default());
        assert!(b.take().is_none());
    }

    #[test]
    fn push_then_take_returns_concatenated() {
        let mut b = DeltaBatcher::new(BatcherConfig::default());
        b.push("hello ");
        b.push("world");
        let out = b.take().unwrap();
        assert_eq!(out, "hello world");
        assert!(b.take().is_none());
    }

    #[test]
    fn chars_trigger_immediate_flush() {
        let cfg = BatcherConfig {
            interval: Duration::from_secs(10), // disable time-based
            max_chars: 10,
        };
        let mut b = DeltaBatcher::new(cfg);
        b.push("x".repeat(10).as_str());
        assert!(b.should_flush());
        let out = b.take().unwrap();
        assert_eq!(out.len(), 10);
        assert!(!b.should_flush());
    }

    #[test]
    fn interval_triggers_flush_after_delay() {
        let cfg = BatcherConfig {
            interval: Duration::from_millis(20),
            max_chars: 10_000,
        };
        let mut b = DeltaBatcher::new(cfg);
        b.push("hi");
        assert!(!b.should_flush());
        sleep(Duration::from_millis(30));
        assert!(b.should_flush());
    }

    #[test]
    fn reset_clears_buffer() {
        let mut b = DeltaBatcher::new(BatcherConfig::default());
        b.push("hello");
        b.reset();
        assert!(!b.should_flush());
        assert!(b.take().is_none());
    }

    #[test]
    fn default_config_is_50ms_100_chars() {
        let cfg = BatcherConfig::default();
        assert_eq!(cfg.interval, Duration::from_millis(50));
        assert_eq!(cfg.max_chars, 100);
    }
}
