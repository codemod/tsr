//! Opt-in ownership/cost probe for a predetermined file-read plan.
//!
//! This does not perform discovery or enable production parallel loading.
//! Workers receive only paths and a shareable backing filesystem. Owned text
//! returns in manifest order; parsing and program identity stay on the caller.

use std::hash::Hasher;
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

use tsr_vfs::{FileSystem, OsFileSystem};

#[derive(Debug, Default)]
struct WorkerStats {
    reads: usize,
    failed: usize,
    decoded_bytes: usize,
    read_time: Duration,
}

#[derive(Debug)]
struct PreparationStats {
    workers: Vec<WorkerStats>,
    wall_time: Duration,
    max_batch_bytes: usize,
}

fn prepare_serial(
    fs: &dyn FileSystem,
    paths: &[String],
    mut consume: impl FnMut(usize, &str, Option<String>),
) -> PreparationStats {
    let started = Instant::now();
    let mut stats = WorkerStats::default();
    let mut max_batch_bytes = 0;
    for (index, path) in paths.iter().enumerate() {
        let read_started = Instant::now();
        let text = fs.read_file(path);
        stats.read_time += read_started.elapsed();
        stats.reads += 1;
        stats.failed += usize::from(text.is_none());
        let bytes = text.as_ref().map_or(0, String::len);
        stats.decoded_bytes += bytes;
        max_batch_bytes = max_batch_bytes.max(bytes);
        consume(index, path, text);
    }
    PreparationStats {
        workers: if paths.is_empty() { vec![] } else { vec![stats] },
        wall_time: started.elapsed(),
        max_batch_bytes,
    }
}

/// At most one read per worker is outstanding. The coordinator drains the
/// entire batch before submitting another, then consumes results in plan order.
/// The consumer may retain text; that memory is outside this queue bound.
fn prepare<F: FileSystem + Sync + ?Sized>(
    fs: &F,
    paths: &[String],
    workers: usize,
    mut consume: impl FnMut(usize, &str, Option<String>),
) -> Result<PreparationStats, Box<dyn std::error::Error>> {
    if !(1..=4).contains(&workers) {
        return Err("probe worker count must be 1 through 4".into());
    }
    let started = Instant::now();
    let (stats, max_batch_bytes) = std::thread::scope(|scope| {
        let mut endpoints = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..workers.min(paths.len()) {
            let (submit, tasks) = sync_channel::<&str>(1);
            let (complete, results) = sync_channel(1);
            handles.push(scope.spawn(move || {
                let mut stats = WorkerStats::default();
                while let Ok(path) = tasks.recv() {
                    let read_started = Instant::now();
                    let text = fs.read_file(path);
                    stats.read_time += read_started.elapsed();
                    stats.reads += 1;
                    stats.failed += usize::from(text.is_none());
                    stats.decoded_bytes += text.as_ref().map_or(0, String::len);
                    if complete.send(text).is_err() {
                        break;
                    }
                }
                stats
            }));
            endpoints.push((submit, results));
        }
        let mut max_batch_bytes = 0;
        // Empty plans create no workers; avoid chunks(0).
        for (batch_index, batch) in paths.chunks(workers).enumerate() {
            for ((submit, _), path) in endpoints.iter().zip(batch) {
                submit
                    .send(path.as_str())
                    .map_err(|_| std::io::Error::other("read worker stopped"))?;
            }
            let texts: Vec<_> = endpoints
                .iter()
                .take(batch.len())
                .map(|(_, results)| results.recv())
                .collect::<Result<_, _>>()?;
            let batch_bytes = texts.iter().filter_map(Option::as_ref).map(String::len).sum();
            max_batch_bytes = max_batch_bytes.max(batch_bytes);
            for (offset, (path, text)) in batch.iter().zip(texts).enumerate() {
                consume(batch_index * workers + offset, path, text);
            }
        }
        drop(endpoints);
        let stats = handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| std::io::Error::other("read worker panicked")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, Box<dyn std::error::Error>>((stats, max_batch_bytes))
    })?;
    Ok(PreparationStats { workers: stats, wall_time: started.elapsed(), max_batch_bytes })
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [manifest, workers] = args.as_slice() else {
        return Err("usage: loader_reads /absolute/read-manifest.txt direct|1|2|4".into());
    };
    let paths: Vec<_> = std::fs::read_to_string(manifest)?.lines().map(str::to_owned).collect();
    if paths.iter().any(|path| !std::path::Path::new(path).is_absolute() || path.contains('\t')) {
        return Err("manifest paths must be absolute and contain no tabs".into());
    }
    let fs = OsFileSystem;
    let consume = |index, path: &str, text: Option<String>| {
        if let Some(text) = text {
            // A same-build equality check, not a persistent cryptographic input
            // fingerprint. The external harness must fingerprint original bytes.
            let mut hash = rustc_hash::FxHasher::default();
            hash.write(text.as_bytes());
            println!("read\t{index}\t{}\t{:016x}\t{path}", text.len(), hash.finish());
        } else {
            println!("missing\t{index}\t{path}");
        }
    };
    let stats = if workers == "direct" {
        prepare_serial(&fs, &paths, consume)
    } else {
        prepare(&fs, &paths, workers.parse()?, consume)?
    };
    eprintln!(
        "preparation\t{}\t{}\t{}\t{}\t{:.9}\t{}",
        stats.workers.len(),
        stats.workers.iter().map(|w| w.reads).sum::<usize>(),
        stats.workers.iter().map(|w| w.failed).sum::<usize>(),
        stats.workers.iter().map(|w| w.decoded_bytes).sum::<usize>(),
        stats.wall_time.as_secs_f64(),
        stats.max_batch_bytes,
    );
    for (index, worker) in stats.workers.iter().enumerate() {
        eprintln!(
            "worker\t{index}\t{}\t{}\t{}\t{:.9}",
            worker.reads,
            worker.failed,
            worker.decoded_bytes,
            worker.read_time.as_secs_f64(),
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
#[path = "loader_reads/controls.rs"]
mod controls;
