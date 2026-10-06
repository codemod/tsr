//! Bounded workers with deterministic, coordinator-only publication.

use std::sync::atomic::{AtomicUsize, Ordering};
use tsr_core::CompilerOptions;

// Multiple Programs (for example corpus cases) may already be checked in
// parallel. Do not multiply the host thread count by every active Program.
static ACTIVE_WORKERS: AtomicUsize = AtomicUsize::new(0);

struct WorkerLease(usize);

impl WorkerLease {
    fn acquire(requested: usize) -> Self {
        let limit = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        let mut count = 0;
        ACTIVE_WORKERS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |active| {
                count = requested.min(limit.saturating_sub(active));
                if count < 2 {
                    count = 0;
                }
                Some(active + count)
            })
            .expect("worker lease update always succeeds");
        Self(count)
    }
}

impl Drop for WorkerLease {
    fn drop(&mut self) {
        ACTIVE_WORKERS.fetch_sub(self.0, Ordering::Relaxed);
    }
}

pub(crate) fn workers(options: &CompilerOptions, jobs: usize) -> usize {
    if options.single_threaded.is_true() || jobs < 8 {
        return 1;
    }
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(jobs)
}

/// Each worker can retain at most two completed results (one in its channel,
/// one waiting to send). The consumer runs on the caller and publishes in input
/// order, even when files finish in a different order. No shared allocator or
/// mutable result table crosses the worker boundary.
pub(crate) fn ordered<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    produce: impl Fn(usize, &T) -> R + Sync,
    consume: impl FnMut(usize, R),
) -> usize {
    let lease = WorkerLease::acquire(workers.max(1).min(items.len().max(1)));
    let workers = lease.0.max(1);
    run_ordered(items, workers, produce, consume);
    workers
}

fn run_ordered<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    produce: impl Fn(usize, &T) -> R + Sync,
    mut consume: impl FnMut(usize, R),
) {
    if workers == 1 {
        for (index, item) in items.iter().enumerate() {
            consume(index, produce(index, item));
        }
        return;
    }
    std::thread::scope(|scope| {
        let mut receivers = Vec::with_capacity(workers);
        for worker in 0..workers {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            receivers.push(receiver);
            let produce = &produce;
            scope.spawn(move || {
                for index in (worker..items.len()).step_by(workers) {
                    // A consumer panic drops its receivers, unblocking every
                    // sender before the scope joins the remaining workers.
                    if sender.send(produce(index, &items[index])).is_err() {
                        break;
                    }
                }
            });
        }
        for index in 0..items.len() {
            let result = receivers[index % workers].recv().expect("front-end worker failed");
            consume(index, result);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::run_ordered;
    use std::sync::{Mutex, mpsc};

    #[test]
    fn two_workers_finish_out_of_order_and_publish_in_order() {
        let (ready, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        let completed = Mutex::new(Vec::new());
        let mut published = Vec::new();
        run_ordered(
            &[0, 1, 2, 3, 4, 5, 6, 7],
            2,
            |index, &value| {
                if index == 0 {
                    wait.lock().unwrap().recv().unwrap();
                }
                completed.lock().unwrap().push(index);
                if index == 1 {
                    ready.send(()).unwrap();
                }
                value
            },
            |index, value| {
                published.push((index, value));
            },
        );
        assert_eq!(completed.lock().unwrap()[0], 1);
        assert_eq!(published, (0..8).map(|index| (index, index)).collect::<Vec<_>>());
    }

    #[test]
    fn consumer_panic_unblocks_workers_waiting_to_send() {
        let result = std::panic::catch_unwind(|| {
            run_ordered(&[0; 20], 2, |index, _| index, |_, _| panic!("consumer failed"));
        });
        assert!(result.is_err());
    }
}
