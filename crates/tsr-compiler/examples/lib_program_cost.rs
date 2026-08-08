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
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions};
use tsr_parser::{ParseOptions, parse_into};

/// Where the libs are mounted, matching the `.types` producer's own constant.
const LIB_DIRECTORY: &str = "/.ts-lib";

/// A host holding nothing but the lib directory.
struct LibHost {
    fs: tsr_vfs::InMemoryFileSystem,
}

impl tsr_module::types::ResolutionHost for LibHost {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &'static str {
        "/"
    }
}

/// Build a whole `Program` through the **loader**, as the `.types` producer
/// does, and return the time plus the node count.
///
/// This is the number that matters for a corpus run and the one
/// `docs/architecture/program.md` was missing. It differs from
/// [`time_program`] in three ways that all cost time, and the point of
/// measuring both is to say which:
///
/// - it goes through `FileLoader`, so the `/// <reference lib="…" />` closure is
///   *discovered* rather than precomputed;
/// - it copies every file's text and name into the arena with `Arena::alloc_str`
///   (ADR-0034);
/// - **it parses with JSDoc on**, because a real compiler must, where
///   [`time_program`] turns it off. The lib text is JSDoc-dense.
fn time_loader_program(
    libs: &[(String, String)],
    lib_option: &str,
) -> (std::time::Duration, usize) {
    let mut files: Vec<(String, String)> =
        libs.iter().map(|(name, text)| (format!("{LIB_DIRECTORY}/{name}"), text.clone())).collect();
    files.push(("/a.ts".to_string(), "const x = 1;\n".to_string()));
    let host = LibHost { fs: tsr_vfs::InMemoryFileSystem::new(files, [], true) };

    let start = Instant::now();
    let arena = Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: CompilerOptions {
                lib: vec![lib_option.to_string()],
                ..CompilerOptions::default()
            },
            root_file_names: vec!["/a.ts".to_string()],
            default_library_path: LIB_DIRECTORY.to_string(),
        },
    );
    let elapsed = start.elapsed();
    let nodes = program.nodes().len();
    (elapsed, nodes)
}

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
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            FileInfo { name, text },
        );
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

    // The loader path, which is what the `.types` producer actually pays since
    // the rewire. Same lib sets, reported beside the figures above so the
    // difference between them is attributable rather than mysterious.
    println!();
    for (root, lib_option) in
        [("lib.es5.d.ts", "es5"), ("lib.d.ts", ""), ("lib.esnext.full.d.ts", "esnext.full")]
    {
        let files = closure(root);
        if files.is_empty() {
            continue;
        }
        // The default lib has no `--lib` spelling, so it is selected by leaving
        // `lib` empty and letting the target decide, exactly as a corpus case
        // without a `@target` directive does.
        let option = if lib_option.is_empty() { "es5" } else { lib_option };
        let _ = time_loader_program(&files, option);
        let (elapsed, nodes) = time_loader_program(&files, option);
        let per_case = elapsed.as_secs_f64();
        println!(
            "loader {root:<15} {:>9} nodes  {:>8.1} ms/case  -> {:>7.1} s over {CORPUS_CASES} cases",
            nodes,
            per_case * 1000.0,
            per_case * f64::from(CORPUS_CASES),
        );
    }
}
