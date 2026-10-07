//! Ownership-cost probe, not a whole-project benchmark.
//!
//! `cargo run --release -p tsr-compiler --example front_end_costs -- DIR [WORKERS]`
//! DIR contains independent .d.ts files. Retains all private results to expose
//! an upper envelope; production uses bounded channels and ordered replay.

use std::{path::PathBuf, time::Instant};
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, PreparedNames};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, ParsedFile};

fn main() {
    let mut args = std::env::args().skip(1);
    let directory = PathBuf::from(args.next().expect("DIR is required"));
    let workers = args.next().map_or_else(
        || std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get),
        |value| value.parse::<usize>().expect("WORKERS must be positive"),
    );
    assert!(workers > 0);
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().ends_with(".d.ts"))
        .collect();
    paths.sort();
    let inputs: Vec<_> = paths
        .iter()
        .map(|path| (path.to_str().unwrap().to_owned(), std::fs::read_to_string(path).unwrap()))
        .collect();
    assert!(!inputs.is_empty());
    let workers = workers.min(inputs.len());

    let started = Instant::now();
    let mut private = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let inputs = &inputs;
                scope.spawn(move || {
                    (worker..inputs.len())
                        .step_by(workers)
                        .map(|index| {
                            let (name, text) = &inputs[index];
                            (
                                index,
                                ParsedFile::parse_with_options(
                                    text.clone(),
                                    ParseOptions::for_file(name),
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
    });
    private.sort_by_key(|(index, _)| *index);
    let parse_seconds = started.elapsed().as_secs_f64();
    let private_arena_bytes: usize = private
        .iter()
        .map(|(_, parsed)| parsed.with_ast_and_arena(|_, arena| arena.allocated_bytes()))
        .sum();

    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let started = Instant::now();
    let files: Vec<_> = private
        .iter()
        .map(|(index, private)| {
            let (name, text) = &inputs[*index];
            let name = &*arena.alloc_str(name);
            let text = &*arena.alloc_str(text);
            let parsed = private.publish(&arena, text, &mut nodes, &mut map);
            assert!(parsed.diagnostics.is_empty());
            (FileInfo { name, text }, parsed)
        })
        .collect();
    let publication_seconds = started.elapsed().as_secs_f64();
    let published_arena_bytes = arena.allocated_bytes();
    drop(private);

    let started = Instant::now();
    let names = PreparedNames::new(&arena, &map);
    let names_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let mut private = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let files = &files;
                let nodes = &nodes;
                let names = &names;
                scope.spawn(move || {
                    (worker..files.len())
                        .step_by(workers)
                        .map(|index| {
                            let (info, parsed) = &files[index];
                            let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
                            (
                                index,
                                tsr_binder::bind_file(
                                    names,
                                    nodes,
                                    parsed.source_file,
                                    *info,
                                    &jsdoc,
                                    parsed.node_range.clone(),
                                ),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
    });
    private.sort_by_key(|(index, _)| *index);
    let bind_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let mut bindings = BindResult::empty();
    for (_, private) in private {
        bindings = bindings.publish_file(&arena, &nodes, private);
    }
    let merge_seconds = started.elapsed().as_secs_f64();
    println!(
        "{{\"workers\":{workers},\"files\":{},\"nodes\":{},\"private_parse_seconds\":{parse_seconds},\"publication_seconds\":{publication_seconds},\"name_preparation_seconds\":{names_seconds},\"private_bind_seconds\":{bind_seconds},\"merge_seconds\":{merge_seconds},\"private_ast_requested_bytes\":{private_arena_bytes},\"canonical_ast_requested_bytes\":{published_arena_bytes},\"canonical_binder_heap_bytes\":{}}}",
        files.len(),
        nodes.len(),
        bindings.heap_bytes()
    );
}
