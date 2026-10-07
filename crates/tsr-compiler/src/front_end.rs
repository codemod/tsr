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

/// An owned job can outlive the coordinator's current DFS task. No host,
/// resolver, canonical arena or borrowed AST enters this pool. The loader caps
/// outstanding tickets, including completed results awaiting publication.
pub(crate) struct DynamicPool<T, R> {
    sender: Option<std::sync::mpsc::Sender<(T, std::sync::mpsc::Sender<WorkerResult<R>>)>>,
    threads: Vec<std::thread::JoinHandle<()>>,
    _lease: WorkerLease,
}

type WorkerResult<R> = Result<R, Box<dyn std::any::Any + Send>>;

pub(crate) struct Ticket<R>(std::sync::mpsc::Receiver<WorkerResult<R>>);

impl<R> Ticket<R> {
    pub(crate) fn wait(self) -> R {
        match self.0.recv().expect("dependency parser worker disconnected") {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}

impl<T: Send + 'static, R: Send + 'static> DynamicPool<T, R> {
    pub(crate) fn new(
        requested: usize,
        produce: impl Fn(T) -> R + Send + Sync + 'static,
    ) -> Option<Self> {
        let lease = WorkerLease::acquire(requested);
        if lease.0 == 0 {
            return None;
        }
        let (sender, receiver) =
            std::sync::mpsc::channel::<(T, std::sync::mpsc::Sender<WorkerResult<R>>)>();
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
        let produce = std::sync::Arc::new(produce);
        let mut threads = Vec::with_capacity(lease.0);
        for _ in 0..lease.0 {
            let receiver = receiver.clone();
            let produce = produce.clone();
            threads.push(
                std::thread::Builder::new()
                    .name("tsr-dependency-parse".to_owned())
                    .stack_size(8 * 1024 * 1024)
                    .spawn(move || {
                        loop {
                            let job =
                                receiver.lock().expect("dependency job receiver poisoned").recv();
                            let Ok((input, result)) = job else { break };
                            // Unbounded one-result channels cannot block shutdown when an
                            // unclaimed ticket is discarded. Propagate a worker panic at
                            // the canonical visit instead of silently omitting the file.
                            let output =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    produce(input)
                                }));
                            let _ = result.send(output);
                        }
                    })
                    .expect("create dependency parser worker"),
            );
        }
        Some(Self { sender: Some(sender), threads, _lease: lease })
    }

    pub(crate) fn workers(&self) -> usize {
        self.threads.len()
    }

    pub(crate) fn submit(&self, input: T) -> Ticket<R> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.sender
            .as_ref()
            .expect("live dependency pool")
            .send((input, sender))
            .expect("dependency parser pool disconnected");
        Ticket(receiver)
    }
}

impl<T, R> Drop for DynamicPool<T, R> {
    fn drop(&mut self) {
        self.sender.take();
        for thread in self.threads.drain(..) {
            // Drop also runs during coordinator unwinding. A parser panic is
            // already carried by its ticket; never cause a second panic here.
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DynamicPool, run_ordered};
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

    #[test]
    fn dynamic_pool_out_of_order_panic_and_unconsumed_shutdown() {
        let available = std::thread::available_parallelism().unwrap().get();
        if available < 2 {
            return;
        }
        for requested in [2, available] {
            let (ready, wait) = mpsc::channel();
            let wait = Mutex::new(wait);
            let pool = DynamicPool::new(requested, move |index| {
                if index == 0 {
                    wait.lock().unwrap().recv().unwrap();
                }
                if index == 1 {
                    ready.send(()).unwrap();
                }
                assert_ne!(index, 2, "worker panic is propagated by its ticket");
                index
            })
            .expect("actual parser workers are available");
            assert_eq!(pool.workers(), requested);
            let first = pool.submit(0);
            let second = pool.submit(1);
            assert_eq!(first.wait(), 0);
            assert_eq!(second.wait(), 1);
            let panic = pool.submit(2);
            assert!(std::panic::catch_unwind(|| panic.wait()).is_err());
            // Completing a result after its ticket drops must not block join.
            drop(pool.submit(3));
            assert_eq!(pool.submit(4).wait(), 4);
            drop(pool);
        }
    }
}
