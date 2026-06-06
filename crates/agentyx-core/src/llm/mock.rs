//! `MockProvider` — a programmable `Provider` impl for tests.
//!
//! Each call to `chat()` consumes the next pre-configured
//! [`MockSequence`] (a `Vec<ChatEvent>`) and yields it as a
//! stream. Tests assemble a sequence of sequences to drive
//! multi-step agent-loop scenarios without an actual HTTP
//! server.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use futures::stream;

use crate::AppError;

use super::provider::Provider;
use super::types::{ChatEvent, ChatRequest, ChatStream, ModelCapabilities};

/// A pre-recorded sequence of events the mock emits in order.
#[derive(Debug, Clone, Default)]
pub struct MockSequence {
    /// Events yielded in this order.
    pub events: Vec<ChatEvent>,
}

impl MockSequence {
    /// Create a new sequence from a list of events.
    #[must_use]
    pub fn new(events: Vec<ChatEvent>) -> Self {
        Self { events }
    }
}

/// A mock provider that returns a pre-configured sequence per
/// `chat()` call.
pub struct MockProvider {
    id: &'static str,
    name: &'static str,
    sequences: Mutex<Vec<MockSequence>>,
    call_count: AtomicUsize,
}

impl std::fmt::Debug for MockProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockProvider")
            .field("id", &self.id)
            .field("call_count", &self.call_count)
            .field(
                "pending_sequences",
                &self.sequences.lock().map(|g| g.len()).unwrap_or(0),
            )
            .finish()
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    /// Create a new empty mock. The first `chat()` call will
    /// emit an empty stream.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: "mock",
            name: "Mock Provider",
            sequences: Mutex::new(Vec::new()),
            call_count: AtomicUsize::new(0),
        }
    }

    /// Push a sequence. The next `chat()` call will consume it.
    pub fn push_sequence(&self, seq: MockSequence) {
        self.sequences
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(seq);
    }

    /// Number of `chat()` calls made so far.
    #[must_use]
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn capabilities(&self, _model_id: &str) -> ModelCapabilities {
        ModelCapabilities {
            tools: true,
            vision: false,
            context_window: 8192,
            max_output_tokens: 2048,
        }
    }

    async fn chat(&self, _req: ChatRequest) -> Result<ChatStream, AppError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let seq = {
            let mut g = self
                .sequences
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if g.is_empty() {
                // No more sequences: return an empty stream.
                Vec::new()
            } else {
                g.remove(0).events
            }
        };
        let stream = stream::iter(seq.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }

    async fn health(&self) -> Result<u64, AppError> {
        Ok(1)
    }
}
