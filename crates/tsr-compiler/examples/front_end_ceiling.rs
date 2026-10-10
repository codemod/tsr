//! What a zero-copy parallel front end could save on a real project: the
//! measured input to the `tsr-2zk.17.4`/`.17.5` design record
//! (`docs/parity/notes/r7-perf.md` §10).
//!
//! ```text
//! cargo run --release -p tsr-compiler --example front_end_ceiling -- /abs/tsconfig.json [WORKERS] [ROUNDS]
//! ```
//!
//! It loads the project through the real loader (bundled libs, the CLI's
//! parse options), then times each program file on its own, single-threaded,
//! taking the minimum over `ROUNDS` (default 5):
//!
//! - `parse`: `parse_into` straight into fresh shared tables (today's serial
//!   path);
//! - `private_parse` and `publish`: a worker-style private parse, and the copy
//!   of it into shared tables (today's parallel path);
//! - `bind`: the accumulating serial bind of the file into the program's store,
//!   in program order;
//! - `private_bind` and `merge`: a worker-style private bind, and
//!   `publish_file`'s relocation and global merge.
//!
//! It then models three critical paths over `WORKERS` (default: available
//! parallelism):
//!
//! - `serial`: the sum of `parse` and `bind`, which is today's path on a
//!   project whose largest file is more than half its nodes;
//! - `copy_parallel`: today's private-then-publish design with every file
//!   fanned out: the longest-processing-time makespan of the private work,
//!   plus every publication on the coordinator;
//! - `zero_copy`: the 17.4/17.5 design, where each file parses and binds
//!   privately and publication costs nothing. The result is the makespan of
//!   `parse + bind` per file plus the merge, reported as a range:
//!   `merge_floor` is zero and `merge_ceiling` is the whole measured `merge`.
//!   Global merging cannot be skipped. How much of `publish_file` is
//!   relocation, which rebasable ids remove, and how much is merge, which
//!   they do not, is what the range brackets.
//!
//! The model ignores loader and resolver work, thread start-up and memory
//! bandwidth, so it is an upper envelope on the saving, not a prediction.

use std::time::{Duration, Instant};

use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, PreparedNames};
use tsr_compiler::{LoadOptions, Program};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, ParsedFile};
use tsr_vfs::FileSystem;

struct Host<'a> {
    fs: &'a dyn FileSystem,
    directory: &'a str,
}

impl tsr_module::types::ResolutionHost for Host<'_> {
    fn fs(&self) -> &dyn FileSystem {
        self.fs
    }

    fn current_directory(&self) -> &str {
        self.directory
    }
}

#[derive(Default, Clone, Copy)]
struct FileCost {
    nodes: usize,
    parse: Duration,
    private_parse: Duration,
    publish: Duration,
    bind: Duration,
    private_bind: Duration,
    merge: Duration,
}

fn min_into(slot: &mut Duration, value: Duration) {
    if *slot == Duration::ZERO || value < *slot {
        *slot = value;
    }
}

/// Longest-processing-time makespan of `jobs` on `workers`.
fn makespan(mut jobs: Vec<Duration>, workers: usize) -> Duration {
    jobs.sort_unstable_by(|a, b| b.cmp(a));
    let mut loads = vec![Duration::ZERO; workers.max(1)];
    for job in jobs {
        let least = loads.iter_mut().min().expect("at least one worker");
        *least += job;
    }
    loads.into_iter().max().unwrap_or_default()
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn main() {
    let mut args = std::env::args().skip(1);
    let project = args.next().expect("usage: front_end_ceiling /abs/tsconfig.json [WORKERS] [ROUNDS]");
    let workers = args.next().map_or_else(
        || std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        |value| value.parse().expect("WORKERS must be a number"),
    );
    let rounds: usize = args.next().map_or(5, |value| value.parse().expect("ROUNDS must be a number"));

    let os = tsr_vfs::OsFileSystem::new();
    let fs = tsr_vfs::BundledFileSystem::new(os);
    let directory = std::env::current_dir().expect("cwd").to_string_lossy().into_owned();
    let text = fs.read_file(&project).expect("cannot read project");
    let parsed = tsr_tsoptions::parse_config_file(
        &project,
        &text,
        tsr_path::get_directory_path(&project),
        &fs,
    );
    let host = Host { fs: &fs, directory: &directory };
    let arena = Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: parsed.compiler_options,
            root_file_names: parsed.file_names,
            default_library_path: tsr_vfs::bundled::LIB_PATH.to_string(),
        },
    );
    let inputs: Vec<(String, String)> = program
        .source_files()
        .iter()
        .map(|file| (file.file_name().to_string(), file.text().to_string()))
        .collect();
    let options =
        |name: &str| ParseOptions::for_file(name).deferring_ts_jsdoc(name);

    let mut costs = vec![FileCost::default(); inputs.len()];
    for _ in 0..rounds {
        // Serial path: direct parse into one set of shared tables, then the
        // accumulating bind in program order.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut map = NodeMap::new();
        let mut parsed = Vec::with_capacity(inputs.len());
        for (index, (name, text)) in inputs.iter().enumerate() {
            let name = &*arena.alloc_str(name);
            let text = &*arena.alloc_str(text);
            let before = nodes.len();
            let started = Instant::now();
            let file = tsr_parser::parse_into(&arena, text, options(name), &mut nodes, &mut map);
            min_into(&mut costs[index].parse, started.elapsed());
            costs[index].nodes = nodes.len() - before;
            parsed.push((FileInfo { name, text }, file));
        }
        let mut bound = BindResult::empty();
        for (index, (info, file)) in parsed.iter().enumerate() {
            let jsdoc: Vec<_> = file.jsdoc.iter().collect();
            let started = Instant::now();
            bound = tsr_binder::bind_into_with_jsdoc(
                bound,
                &arena,
                file.source_file,
                &nodes,
                *info,
                &jsdoc,
            );
            min_into(&mut costs[index].bind, started.elapsed());
        }
        drop(bound);

        // Parallel path's pieces, timed one file at a time.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut map = NodeMap::new();
        let mut published = Vec::with_capacity(inputs.len());
        for (index, (name, text)) in inputs.iter().enumerate() {
            let started = Instant::now();
            let private = ParsedFile::parse_with_options(text.clone(), options(name));
            min_into(&mut costs[index].private_parse, started.elapsed());
            let name = &*arena.alloc_str(name);
            let text = &*arena.alloc_str(text);
            let started = Instant::now();
            let file = private.publish(&arena, text, &mut nodes, &mut map);
            min_into(&mut costs[index].publish, started.elapsed());
            published.push((FileInfo { name, text }, file));
        }
        let names = PreparedNames::new(&arena, &map);
        let mut bound = BindResult::empty();
        for (index, (info, file)) in published.iter().enumerate() {
            let jsdoc: Vec<_> = file.jsdoc.iter().collect();
            let started = Instant::now();
            let local = tsr_binder::bind_file(
                &names,
                &nodes,
                file.source_file,
                *info,
                &jsdoc,
                file.node_range.clone(),
            );
            min_into(&mut costs[index].private_bind, started.elapsed());
            let started = Instant::now();
            bound = bound.publish_file(&arena, &nodes, local);
            min_into(&mut costs[index].merge, started.elapsed());
        }
    }

    let sum = |pick: fn(&FileCost) -> Duration| costs.iter().map(pick).sum::<Duration>();
    let serial = sum(|c| c.parse) + sum(|c| c.bind);
    let copy_parallel = makespan(costs.iter().map(|c| c.private_parse).collect(), workers)
        + sum(|c| c.publish)
        + makespan(costs.iter().map(|c| c.private_bind).collect(), workers)
        + sum(|c| c.merge);
    let zero_copy_work = makespan(costs.iter().map(|c| c.private_parse + c.private_bind).collect(), workers);
    let largest = costs
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| c.nodes)
        .map(|(index, c)| (inputs[index].0.rsplit('/').next().unwrap_or_default().to_string(), *c))
        .expect("a program has files");
    let total_nodes: usize = costs.iter().map(|c| c.nodes).sum();
    println!(
        "{{\"project\":{:?},\"files\":{},\"nodes\":{},\"workers\":{},\"rounds\":{},\
         \"largest\":{{\"file\":{:?},\"nodes\":{},\"parse_ms\":{:.2},\"bind_ms\":{:.2}}},\
         \"sum_ms\":{{\"parse\":{:.2},\"private_parse\":{:.2},\"publish\":{:.2},\"bind\":{:.2},\"private_bind\":{:.2},\"merge\":{:.2}}},\
         \"critical_path_ms\":{{\"serial\":{:.2},\"copy_parallel\":{:.2},\"zero_copy_merge_floor\":{:.2},\"zero_copy_merge_ceiling\":{:.2}}}}}",
        project,
        inputs.len(),
        total_nodes,
        workers,
        rounds,
        largest.0,
        largest.1.nodes,
        ms(largest.1.parse),
        ms(largest.1.bind),
        ms(sum(|c| c.parse)),
        ms(sum(|c| c.private_parse)),
        ms(sum(|c| c.publish)),
        ms(sum(|c| c.bind)),
        ms(sum(|c| c.private_bind)),
        ms(sum(|c| c.merge)),
        ms(serial),
        ms(copy_parallel),
        ms(zero_copy_work),
        ms(zero_copy_work + sum(|c| c.merge)),
    );
}
