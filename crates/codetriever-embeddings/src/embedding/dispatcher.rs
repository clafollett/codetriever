//! Request dispatcher for embedding model pool
//!
//! Provides strategies for distributing incoming requests across worker pool.
//! Different dispatch strategies optimize for different workload patterns.

use tokio::sync::mpsc;

/// Strategy for distributing requests to workers
///
/// Allows different load balancing approaches:
/// - RoundRobin: Simple, fair distribution
/// - LeastBusy: Send to worker with smallest queue (future)
/// - Random: Randomized distribution to avoid patterns (future)
#[async_trait::async_trait]
pub trait Dispatcher<T>: Send + Sync {
    /// Dispatch a request to an available worker
    ///
    /// Returns true if dispatched successfully, false if all workers are unavailable
    async fn dispatch(&mut self, request: T) -> bool;

    /// Get dispatcher statistics for monitoring
    fn stats(&self) -> DispatcherStats;
}

/// Statistics about request distribution
#[derive(Debug, Clone, Default)]
pub struct DispatcherStats {
    /// Total requests dispatched
    pub total_dispatched: u64,
    /// Requests failed to dispatch (all workers busy)
    pub failed_dispatches: u64,
    /// Current worker being used (for round-robin)
    pub current_worker: usize,
}

/// Round-robin dispatcher - distributes requests evenly across workers
///
/// Simple, deterministic strategy that ensures fair load distribution.
/// Good for homogeneous workloads where all requests have similar cost.
pub struct RoundRobinDispatcher<T> {
    /// Channels to send requests to each worker
    worker_channels: Vec<mpsc::UnboundedSender<T>>,
    /// Current worker index (cycles 0..N-1)
    current: usize,
    /// Statistics
    stats: DispatcherStats,
}

impl<T> RoundRobinDispatcher<T> {
    /// Create a new round-robin dispatcher
    ///
    /// # Arguments
    /// * `worker_channels` - Channels to each worker in the pool
    pub fn new(worker_channels: Vec<mpsc::UnboundedSender<T>>) -> Self {
        Self {
            worker_channels,
            current: 0,
            stats: DispatcherStats {
                current_worker: 0,
                ..Default::default()
            },
        }
    }
}

#[async_trait::async_trait]
impl<T: Send> Dispatcher<T> for RoundRobinDispatcher<T> {
    async fn dispatch(&mut self, request: T) -> bool {
        let worker_count = self.worker_channels.len();
        tracing::trace!(
            "RoundRobin: dispatch called, {worker_count} workers available, current={}",
            self.current
        );

        if worker_count == 0 {
            tracing::error!("RoundRobin: NO WORKERS AVAILABLE!");
            self.stats.failed_dispatches += 1;
            return false;
        }

        // Try to send to current worker
        tracing::trace!("RoundRobin: sending to worker {}", self.current);
        let result = self.worker_channels[self.current].send(request);

        if result.is_ok() {
            tracing::trace!("RoundRobin: sent to worker {}", self.current);
            self.stats.total_dispatched += 1;
            // Advance to next worker (round-robin)
            self.current = (self.current + 1) % worker_count;
            self.stats.current_worker = self.current;
            tracing::trace!("RoundRobin: next worker will be {}", self.current);
            true
        } else {
            tracing::warn!("RoundRobin: worker {} channel CLOSED!", self.current);
            // Worker channel closed - remove it and retry
            self.worker_channels.remove(self.current);
            self.stats.failed_dispatches += 1;
            false
        }
    }

    fn stats(&self) -> DispatcherStats {
        self.stats.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin_distribution() {
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        let (tx3, mut rx3) = mpsc::unbounded_channel();

        let mut dispatcher = RoundRobinDispatcher::new(vec![tx1, tx2, tx3]);

        // Dispatch 6 requests
        for i in 0..6 {
            assert!(dispatcher.dispatch(i).await);
        }

        // Should be distributed evenly: 0→worker1, 1→worker2, 2→worker3, 3→worker1...
        assert_eq!(rx1.recv().await, Some(0));
        assert_eq!(rx2.recv().await, Some(1));
        assert_eq!(rx3.recv().await, Some(2));
        assert_eq!(rx1.recv().await, Some(3));
        assert_eq!(rx2.recv().await, Some(4));
        assert_eq!(rx3.recv().await, Some(5));

        let stats = dispatcher.stats();
        assert_eq!(stats.total_dispatched, 6);
        assert_eq!(stats.failed_dispatches, 0);
    }
}
