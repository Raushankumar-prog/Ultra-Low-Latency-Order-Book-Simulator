use std::sync::atomic::{
    AtomicU64,
    Ordering,
};

pub struct Sequencer {
    counter: AtomicU64,
}

impl Sequencer {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }

    pub fn next(&self) -> u64 {
        self.counter
            .fetch_add(1, Ordering::Relaxed)
    }
}