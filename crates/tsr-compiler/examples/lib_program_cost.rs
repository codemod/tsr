//! What it costs to parse and bind the bundled lib files into one program.
//!
//! This exists to settle one question with a number instead of an argument:
//! **is sharing a bound lib program across conformance cases (`bd tsr-6av`) a
//! hard precondition for the `.types` producer building a program at all, or an
//! optimisation?**
//!
//! The producer runs over 12,444 corpus cases. If a case has to parse and bind
//! the default lib set itself, the per-case cost below multiplied by 12,444 is
//! what a corpus run gains. That number decides the ordering of `bd tsr-0e9`
//! against `bd tsr-6av`, and the reasoning is recorded in
//! `docs/architecture/program.md`.
//!
//! Run with `--release`; a debug number would be measuring the wrong thing.
//!
//! ```text
//! cargo run --release -p tsr-compiler --example lib_program_cost
//! ```

use std::time::Instant;

use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

/// The bundled lib directory.
fn lib_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-compiler is two below the repo root")
        .join("vendor/typescript-go/internal/bundled/libs")
}

/// A lib file and everything its `/// <reference lib="…" />` directives pull in,
/// in the order they are reached. The same transitive closure the loader walks.
fn closure(root: &str) -> Vec<(String, String)> {
    let dir = lib_dir();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut queue = vec![root.to_string()];
    while let Some(name) = queue.pop() {
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        let Ok(text) = std::fs::read_to_string(dir.join(&name)) else { continue };
        let references = tsr_parser::parse_file_references(&text);
        for reference in &references.lib_reference_directives {
            if let Some(file) = tsr_tsoptions::libs::get_lib_file_name(&reference.file_name) {
                queue.push(file.to_string());
            }
        }
        out.push((name, text));
    }
    out
}

/// Parse and bind a set of files into one identity space, returning the time.
fn time_program(files: &[(String, String)]) -> (std::time::Duration, usize, usize) {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut bound = BindResult::empty();

    let start = Instant::now();
    for (name, text) in files {
        let parsed = parse_into(
            &arena,
            text,
            ParseOptions { jsdoc: false, ..ParseOptions::for_file(name) },
            &mut nodes,
            &mut node_map,
        );
        bound = tsr_binder::bind_into(bound, parsed.source_file, &nodes, FileInfo { name, text });
    }
    let elapsed = start.elapsed();
    (elapsed, nodes.len(), bound.globals().len())
}

fn main() {
    // The corpus the `.types` producer runs over; see `docs/architecture/conformance.md`.
    const CORPUS_CASES: u32 = 12_444;

    if !lib_dir().exists() {
        println!("vendor/typescript-go is not checked out; nothing to measure");
        return;
    }

    for root in ["lib.es5.d.ts", "lib.d.ts", "lib.es2015.d.ts", "lib.esnext.full.d.ts"] {
        let files = closure(root);
        if files.is_empty() {
            continue;
        }
        // `u32` first, so the cast to `f64` is exact rather than merely
        // probably-exact: the lib set is a few megabytes and cannot overflow it.
        let bytes = u32::try_from(files.iter().map(|(_, text)| text.len()).sum::<usize>())
            .expect("the bundled libs are a few megabytes");
        // Twice, reporting the second: the first pass warms the file cache, and
        // this is measuring parse and bind, not the disk.
        let _ = time_program(&files);
        let (elapsed, node_count, globals) = time_program(&files);

        let per_case = elapsed.as_secs_f64();
        println!(
            "{root:<22} {:>3} files  {:>7.2} MB  {:>9} nodes  {:>6} globals  \
             {:>8.1} ms/case  -> {:>7.1} s over {CORPUS_CASES} cases",
            files.len(),
            f64::from(bytes) / 1_048_576.0,
            node_count,
            globals,
            per_case * 1000.0,
            per_case * f64::from(CORPUS_CASES),
        );
    }
}
