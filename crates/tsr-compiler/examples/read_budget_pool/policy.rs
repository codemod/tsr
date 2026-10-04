//! Fixed-plan progress witness. This is not a production loader scheduler.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel};
use std::time::Instant;

use super::budget::{self, Budget, Error, PreparedText};

#[derive(Debug)]
pub(super) enum Failure {
    Read { index: usize, error: Error },
    WorkerPanic { index: usize },
    Channel,
}

pub(super) struct Attempt {
    pub text: Result<PreparedText, Failure>,
    pub retryable: bool,
}

#[derive(Default, Debug)]
pub(super) struct Report {
    pub attempts: usize,
    pub refusals: usize,
    pub decoded_bytes: usize,
    pub retry_batches: usize,
    pub retry_reads: usize,
    pub startup_ns: u128,
    pub wall_ns: u128,
    pub max_batch_text_bytes: usize,
}

#[derive(Default)]
struct Counts {
    attempts: AtomicUsize,
    refusals: AtomicUsize,
    decoded_bytes: AtomicUsize,
}

impl Counts {
    fn attempt(
        &self,
        index: usize,
        path: &str,
        budget: &Budget,
        read: &impl Fn(usize, &str, &Budget) -> Attempt,
    ) -> Attempt {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(index, path, budget)));
        let attempt = result.unwrap_or_else(|_| Attempt {
            text: Err(Failure::WorkerPanic { index }),
            retryable: false,
        });
        if matches!(attempt.text, Err(Failure::Read { error: Error::Budget { .. }, .. })) {
            self.refusals.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(text) = &attempt.text {
            self.decoded_bytes.fetch_add(text.text().len(), Ordering::Relaxed);
        }
        attempt
    }
}

fn read_file(index: usize, path: &str, budget: &Budget) -> Attempt {
    let mut retryable = false;
    let text = std::fs::File::open(path).map_err(Error::Io).and_then(|mut file| {
        // This classifies repeatable sources, never authorizes allocation.
        // A FIFO or other stream must not be reopened after consuming bytes.
        retryable = file.metadata().is_ok_and(|metadata| metadata.is_file());
        budget::prepare_file(&mut file, budget)
    });
    Attempt { text: text.map_err(|error| Failure::Read { index, error }), retryable }
}

pub(super) fn prepare_paths(
    paths: &[String],
    workers: usize,
    budget: &Budget,
    report: &mut Report,
    consume: impl FnMut(usize, &str, PreparedText),
) -> Result<(), Failure> {
    prepare(paths, workers, budget, report, consume, read_file)
}

// The reader seam is private and used only for fault/ordering tests. The
// externally callable preparation path always uses an ordinary OS File.
fn prepare(
    paths: &[String],
    workers: usize,
    budget: &Budget,
    report: &mut Report,
    mut consume: impl FnMut(usize, &str, PreparedText),
    read: impl Fn(usize, &str, &Budget) -> Attempt + Sync,
) -> Result<(), Failure> {
    assert!(matches!(workers, 0 | 1 | 2 | 4));
    *report = Report::default();
    let started = Instant::now();
    let counts = Counts::default();
    let result = if workers == 0 || paths.is_empty() {
        paths.iter().enumerate().try_for_each(|(index, path)| {
            let text = counts.attempt(index, path, budget, &read).text?;
            report.max_batch_text_bytes = report.max_batch_text_bytes.max(text.capacity());
            consume(index, path, text);
            Ok(())
        })
    } else {
        std::thread::scope(|scope| {
            // Endpoints are local to this closure, so consumer unwind drops
            // them before scope joins. Idle workers then see disconnection.
            let mut endpoints = Vec::new();
            let (ready, ready_rx) = channel();
            for _ in 0..workers.min(paths.len()) {
                let (submit, tasks) = sync_channel::<(usize, &str)>(1);
                let (complete, results) = sync_channel(1);
                let ready = ready.clone();
                let counts = &counts;
                let read = &read;
                scope.spawn(move || {
                    if ready.send(()).is_err() {
                        return;
                    }
                    while let Ok((index, path)) = tasks.recv() {
                        let result = counts.attempt(index, path, budget, read);
                        if complete.send(result).is_err() {
                            break;
                        }
                    }
                });
                endpoints.push((submit, results));
            }
            drop(ready);
            for _ in 0..endpoints.len() {
                ready_rx.recv().map_err(|_| Failure::Channel)?;
            }
            report.startup_ns = started.elapsed().as_nanos();
            for (batch_index, batch) in paths.chunks(workers).enumerate() {
                let first = batch_index * workers;
                for (offset, ((submit, _), path)) in endpoints.iter().zip(batch).enumerate() {
                    submit.send((first + offset, path)).map_err(|_| Failure::Channel)?;
                }
                // Finish all physical reads before callbacks, fallback or
                // cancellation. There is no speculative next batch.
                let attempts: Vec<_> = endpoints
                    .iter()
                    .take(batch.len())
                    .map(|(_, results)| results.recv().map_err(|_| Failure::Channel))
                    .collect::<Result<_, _>>()?;
                let batch_bytes = attempts
                    .iter()
                    .filter_map(|a| a.text.as_ref().ok())
                    .map(PreparedText::capacity)
                    .sum();
                report.max_batch_text_bytes = report.max_batch_text_bytes.max(batch_bytes);
                let refused = attempts.iter().any(|a| {
                    matches!(a.text, Err(Failure::Read { error: Error::Budget { .. }, .. }))
                });
                if refused && attempts.iter().all(|a| a.retryable) {
                    // A later queued success could monopolize credits needed
                    // by an earlier retry. Drop EVERY speculative result first,
                    // then retry the entire stable regular-file batch once.
                    drop(attempts);
                    report.retry_batches += 1;
                    for (offset, path) in batch.iter().enumerate() {
                        report.retry_reads += 1;
                        let index = first + offset;
                        let text = counts.attempt(index, path, budget, &read).text?;
                        report.max_batch_text_bytes =
                            report.max_batch_text_bytes.max(text.capacity());
                        consume(index, path, text);
                    }
                } else {
                    // Non-repeatable streams and read/panic failures receive
                    // no implicit reread. Errors remain typed, never missing.
                    for (offset, (path, attempt)) in batch.iter().zip(attempts).enumerate() {
                        consume(first + offset, path, attempt.text?);
                    }
                }
            }
            drop(endpoints);
            Ok(())
        })
    };
    report.wall_ns = started.elapsed().as_nanos();
    report.attempts = counts.attempts.load(Ordering::Relaxed);
    report.refusals = counts.refusals.load(Ordering::Relaxed);
    report.decoded_bytes = counts.decoded_bytes.load(Ordering::Relaxed);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn fixture(size: usize) -> std::path::PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "tsr-read-pool-{}-{}.ts",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, vec![b' '; size]).unwrap();
        path
    }

    fn balanced(budget: &Budget, limit: usize) {
        let state = budget.stats();
        assert_eq!(state.live, 0);
        assert_eq!(state.acquired, state.released);
        assert!(state.peak <= limit);
    }

    #[test]
    fn queued_success_is_dropped_before_bounded_canonical_retry() {
        let file = fixture(32);
        let paths = vec![file.to_str().unwrap().to_owned(); 4];
        let budget = Budget::new(64);
        let mut report = Report::default();
        let consumed = Mutex::new(Vec::new());
        let later = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let later_read = later.clone();
        // Force index1's retained 32B result to precede index0's 64B read/decode
        // peak. Index0 must refuse despite fitting when read in isolation.
        let read = |index, path: &str, budget: &Budget| {
            if index == 0 && !*later_read.0.lock().unwrap() {
                let guard = later_read.0.lock().unwrap();
                drop(later_read.1.wait_while(guard, |done| !*done).unwrap());
            }
            let result = read_file(index, path, budget);
            if index == 1 {
                *later_read.0.lock().unwrap() = true;
                later_read.1.notify_all();
            }
            result
        };
        prepare(
            &paths[..2],
            2,
            &budget,
            &mut report,
            |index, _, text| {
                assert_eq!(text.text().len(), 32);
                consumed.lock().unwrap().push(index);
            },
            read,
        )
        .unwrap();
        assert_eq!(*consumed.lock().unwrap(), vec![0, 1]);
        assert_eq!(report.attempts, 4);
        assert_eq!(report.retry_batches, 1);
        assert_eq!(report.retry_reads, 2);
        assert_eq!(report.refusals, 1);
        balanced(&budget, 64);
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn oversized_and_consumer_retention_fail_without_waiting_or_leaking() {
        let file = fixture(32);
        let paths = vec![file.to_str().unwrap().to_owned(); 4];
        for workers in [0, 1, 2, 4] {
            let budget = Budget::new(64);
            let mut report = Report::default();
            let mut retained = Vec::new();
            let result = prepare_paths(&paths, workers, &budget, &mut report, |_, _, text| {
                retained.push(text)
            });
            assert!(matches!(result, Err(Failure::Read { error: Error::Budget { .. }, .. })));
            assert_eq!(retained.len(), 1);
            assert_eq!(budget.stats().live, retained[0].capacity());
            drop(retained);
            balanced(&budget, 64);
            assert!(report.attempts <= paths.len() * 2);
            let budget = Budget::new(16);
            assert!(matches!(
                prepare_paths(&paths, workers, &budget, &mut report, |_, _, _| panic!(
                    "must not consume"
                )),
                Err(Failure::Read { error: Error::Budget { .. }, .. })
            ));
            balanced(&budget, 16);
        }
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn consumer_unwind_disconnects_idle_workers_and_drops_pending_text() {
        let file = fixture(32);
        let paths = vec![file.to_str().unwrap().to_owned(); 7];
        for workers in [0, 1, 2, 4] {
            let budget = Budget::new(10000);
            let mut report = Report::default();
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    prepare_paths(&paths, workers, &budget, &mut report, |_, _, _| {
                        panic!("consumer")
                    })
                }))
                .is_err()
            );
            balanced(&budget, 10000);
        }
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn worker_panic_and_nonrepeatable_refusal_are_not_retried() {
        for workers in [0, 1, 2, 4] {
            let paths = vec!["unused".into(); 4];
            let budget = Budget::new(100);
            let mut report = Report::default();
            assert!(matches!(
                prepare(
                    &paths,
                    workers,
                    &budget,
                    &mut report,
                    |_, _, _| {},
                    |_, _, _| panic!("worker")
                ),
                Err(Failure::WorkerPanic { index: 0 })
            ));
            assert_eq!(report.retry_reads, 0);
            assert!(matches!(
                prepare(
                    &paths,
                    workers,
                    &budget,
                    &mut report,
                    |_, _, _| {},
                    |index, _, _| Attempt {
                        text: Err(Failure::Read {
                            index,
                            error: Error::Budget { requested: 200, live: 0, limit: 100 }
                        }),
                        retryable: false
                    }
                ),
                Err(Failure::Read { index: 0, .. })
            ));
            assert_eq!(report.retry_reads, 0);
            balanced(&budget, 100);
        }
    }

    #[test]
    fn empty_plan_has_no_startup_or_attempts() {
        for workers in [0, 1, 2, 4] {
            let mut report = Report::default();
            let budget = Budget::new(0);
            prepare_paths(&[], workers, &budget, &mut report, |_, _, _| unreachable!()).unwrap();
            assert_eq!(report.attempts, 0);
            assert_eq!(report.startup_ns, 0);
            balanced(&budget, 0);
        }
    }
}
