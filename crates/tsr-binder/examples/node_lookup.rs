//! What it costs to reach a declaration's *typed node* from a `NodeId`.
//!
//! The checker cannot compute a declaration's type without this: a `Symbol`
//! holds `value_declaration: Option<NodeId>`, and the annotation and initialiser
//! live in the typed node, while [`NodeTable`] stores only kind, span, flags and
//! parent. See `bd tsr-4sc.4` and
//! [ADR-0032](../../../docs/adr/0032-reaching-a-typed-node-from-an-id.md).
//!
//! Three candidate structures, each measured in **its own process** so the peak
//! RSS reading is not the sum of whatever was built before it:
//!
//! ```text
//! cargo run -p tsr-binder --example node_lookup --release -- none
//! cargo run -p tsr-binder --example node_lookup --release -- dense
//! cargo run -p tsr-binder --example node_lookup --release -- per-symbol
//! cargo run -p tsr-binder --example node_lookup --release -- sparse
//! ```
//!
//! - `none` — parse and bind only. The baseline every other run subtracts.
//! - `dense` — `Vec<Option<Node>>` indexed by `NodeId`, every node. Answers any
//!   id, including a `parent()` id, which is the query the other two cannot.
//! - `per-symbol` — one `Node` per symbol, its value declaration. Answers
//!   "the declaration of this symbol" and nothing else.
//! - `sparse` — `FxHashMap<NodeId, Node>` holding only nodes that are some
//!   symbol's declaration. Same answers as `per-symbol`, keyed by node instead.
//!
//! Reports exact bytes (from capacity, not estimated), peak RSS, populate time
//! over three readings, and entry counts.

use std::{path::PathBuf, time::Instant};

use rustc_hash::FxHashMap;
use tsr_ast::{Node, NodeId, push_children};
use tsr_core::Arena;

/// Peak resident set size in kibibytes, from `/proc/self/status`.
///
/// Same source as `examples/rss.rs`, and peak rather than current for the reason
/// [ADR-0009](../../../docs/adr/0009-performance-gate.md) gives.
fn peak_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("VmHWM in /proc/self/status")
}

/// The same four fixtures as `examples/rss.rs`, in the same order, so the
/// numbers here sit beside the ones already recorded for the binder.
fn fixtures(root: &std::path::Path) -> Vec<(String, PathBuf)> {
    let ts = root.join("vendor/typescript-go/_submodules/TypeScript");
    [
        ("checker.ts", "src/compiler/checker.ts"),
        ("dom.generated.d.ts", "src/lib/dom.generated.d.ts"),
        ("Herebyfile.mjs", "Herebyfile.mjs"),
        (
            "jsxComplexSignatureHasApplicabilityError.tsx",
            "tests/cases/compiler/jsxComplexSignatureHasApplicabilityError.tsx",
        ),
    ]
    .iter()
    .map(|(name, relative)| ((*name).to_string(), ts.join(relative)))
    .collect()
}

/// Every node of one file, by a pre-order walk from the source file root.
///
/// This is the walk any of the three options pays for; only what it *keeps*
/// differs. Iterative rather than recursive: the corpus contains trees deep
/// enough to overflow a thread stack, which is the whole of ADR-0029.
fn walk<'a>(root: Node<'a>, mut visit: impl FnMut(Node<'a>)) {
    let mut stack = vec![root];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        visit(node);
        children.clear();
        push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
}

fn main() {
    let option = std::env::args().nth(1).unwrap_or_else(|| "none".to_string());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ has a grandparent")
        .to_path_buf();

    let sources: Vec<(String, String)> = fixtures(&root)
        .iter()
        .filter_map(|(name, path)| {
            std::fs::read_to_string(path).ok().map(|text| (name.clone(), text))
        })
        .collect();

    // JSDoc off, matching `examples/rss.rs` and the parse+bind benchmark.
    let arenas: Vec<Arena> = sources.iter().map(|_| Arena::new()).collect();
    let parsed: Vec<_> = sources
        .iter()
        .zip(&arenas)
        .map(|((name, text), arena)| {
            let options = tsr_parser::ParseOptions {
                jsdoc: false,
                ..tsr_parser::ParseOptions::for_file(name)
            };
            tsr_parser::parse_with_options(arena, text, options)
        })
        .collect();

    let bound: Vec<_> = parsed
        .iter()
        .zip(&sources)
        .map(|(file, (name, text))| {
            tsr_binder::bind(file.source_file, &file.nodes, tsr_binder::FileInfo { name, text })
        })
        .collect();

    let nodes: usize = parsed.iter().map(|file| file.nodes.len()).sum();
    let symbols: usize = bound.iter().map(|result| result.symbols().len()).sum();
    let after_bind = peak_rss_kib();

    // Three readings of the populate cost, reported as the fastest: the two
    // slower ones carry scheduler noise this is not trying to measure. The
    // structures from the first two rounds are dropped, so only the last is live
    // when RSS is read.
    let mut timings = Vec::new();
    let mut bytes = 0usize;
    let mut entries = 0usize;
    // Typed rather than `Box<dyn Any>`: the structures borrow from the arenas, so
    // erasing them would demand `'static`.
    let mut indexed_kept: Vec<Vec<Option<Node<'_>>>> = Vec::new();
    let mut sparse_kept: Vec<FxHashMap<NodeId, Node<'_>>> = Vec::new();

    for round in 0..3 {
        indexed_kept.clear();
        sparse_kept.clear();
        bytes = 0;
        entries = 0;
        let start = Instant::now();

        for (file, result) in parsed.iter().zip(&bound) {
            let root_node = Node::from(file.source_file);
            match option.as_str() {
                // Does the walk actually reach every registered node? Any table
                // built by walking is only as complete as this, so it bounds all
                // three options rather than only one.
                //
                // Answer, on these fixtures: 7 of 419,572 rows are never reached,
                // and all 7 are **orphans** rather than visitor gaps — their spans
                // land inside a comment and inside a string literal, so they are
                // rows registered during an abandoned speculative parse that
                // `NodeTable::truncate` did not remove. `push_children` reaches
                // every node that is actually in the tree. See `bd tsr-pum.12`.
                "coverage" if round == 0 => {
                    let mut visited = 0usize;
                    let mut with_id = 0usize;
                    let mut seen = vec![false; file.nodes.len()];
                    walk(root_node, |node| {
                        visited += 1;
                        if let Some(id) = node.node_id() {
                            with_id += 1;
                            seen[id.as_u32() as usize] = true;
                        }
                    });
                    let missing: Vec<usize> =
                        seen.iter().enumerate().filter(|(_, s)| !**s).map(|(i, _)| i).collect();
                    println!(
                        "  {}: registered {}, visited {visited}, with id {with_id}, unreached {}",
                        file.nodes.len(),
                        file.nodes.len(),
                        missing.len()
                    );
                    for &index in missing.iter().take(8) {
                        let id = NodeId::new(u32::try_from(index).expect("fits"));
                        println!(
                            "    unreached #{index}: {:?} {:?}",
                            file.nodes.kind(id),
                            file.nodes.span(id)
                        );
                    }
                }
                "dense" => {
                    // Indexed by NodeId. `Option<Node>` is 16 bytes, the same as
                    // `Node`, because the enum's niche absorbs the discriminant —
                    // so a sparse *dense* table costs exactly what a full one does.
                    let mut table: Vec<Option<Node<'_>>> = vec![None; file.nodes.len()];
                    walk(root_node, |node| {
                        if let Some(id) = node.node_id() {
                            table[id.as_u32() as usize] = Some(node);
                        }
                    });
                    bytes += table.capacity() * std::mem::size_of::<Option<Node<'_>>>();
                    entries += table.iter().filter(|slot| slot.is_some()).count();
                    indexed_kept.push(table);
                }
                "per-symbol" => {
                    // One node per symbol: its value declaration. Needs the walk
                    // anyway, to find the node that owns each declaration id.
                    let wanted: FxHashMap<NodeId, usize> = result
                        .symbols()
                        .iter()
                        .filter_map(|(id, symbol)| {
                            symbol.value_declaration.map(|decl| (decl, id.index()))
                        })
                        .collect();
                    let mut table: Vec<Option<Node<'_>>> = vec![None; result.symbols().len()];
                    walk(root_node, |node| {
                        if let Some(id) = node.node_id()
                            && let Some(&slot) = wanted.get(&id)
                        {
                            table[slot] = Some(node);
                        }
                    });
                    bytes += table.capacity() * std::mem::size_of::<Option<Node<'_>>>();
                    entries += table.iter().filter(|slot| slot.is_some()).count();
                    indexed_kept.push(table);
                }
                "sparse" => {
                    // Only nodes that are some symbol's declaration, keyed by id.
                    let wanted: std::collections::HashSet<NodeId> = result
                        .symbols()
                        .iter()
                        .flat_map(|(_, symbol)| symbol.declarations.iter().copied())
                        .collect();
                    let mut table: FxHashMap<NodeId, Node<'_>> = FxHashMap::default();
                    walk(root_node, |node| {
                        if let Some(id) = node.node_id()
                            && wanted.contains(&id)
                        {
                            table.insert(id, node);
                        }
                    });
                    // A hashbrown table is one allocation of (key, value) pairs
                    // plus a byte of control data per bucket.
                    bytes += table.capacity() * (std::mem::size_of::<(NodeId, Node<'_>)>() + 1);
                    entries += table.len();
                    sparse_kept.push(table);
                }
                "none" | "coverage" => {}
                other => panic!("unknown option {other}"),
            }
        }

        let elapsed = start.elapsed();
        if round > 0 {
            // Round 0 is discarded: it faults in the pages the walk touches.
            timings.push(elapsed);
        }
    }

    std::hint::black_box((&parsed, &bound, &indexed_kept, &sparse_kept));
    let after_build = peak_rss_kib();

    let fastest = timings.iter().min().copied().unwrap_or_default();
    println!("option:            {option}");
    println!("nodes:             {nodes}");
    println!("symbols:           {symbols}");
    println!("entries:           {entries}");
    println!("exact bytes:       {bytes}");
    println!("exact KiB:         {}", bytes / 1024);
    println!("rss after bind:    {after_bind} KiB");
    println!("rss after build:   {after_build} KiB");
    println!("rss delta:         {} KiB", after_build.saturating_sub(after_bind));
    println!("populate (best of 2 after warm-up): {fastest:?}");
}
