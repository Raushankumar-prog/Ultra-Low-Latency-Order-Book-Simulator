use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// A simple hybrid spin/yield queue for ultra-low latency single-producer, single-consumer scenarios.
pub struct SpinYieldQueue<T> {
    queue: crossbeam_queue::SegQueue<T>,
    size: AtomicUsize,
}

impl<T> SpinYieldQueue<T> {
    pub fn new() -> Self {
        Self {
            queue: crossbeam_queue::SegQueue::new(),
            size: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, value: T) {
        self.queue.push(value);
        self.size.fetch_add(1, Ordering::Release);
    }

    /// Hybrid spin/yield pop: spins for a short time, then yields if empty.
    pub fn pop(&self) -> Option<T> {
        // Spin for up to 100 iterations
        for _ in 0..100 {
            if let Some(val) = self.queue.pop() {
                self.size.fetch_sub(1, Ordering::Acquire);
                return Some(val);
            }
            std::hint::spin_loop();
        }
        // If still empty, yield the thread
        for _ in 0..10 {
            if let Some(val) = self.queue.pop() {
                self.size.fetch_sub(1, Ordering::Acquire);
                return Some(val);
            }
            thread::yield_now();
        }
        None
    }

    pub fn len(&self) -> usize {
        self.size.load(Ordering::Acquire)
    }
}
