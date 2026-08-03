//! Peak resident set size for parsing *and binding* a file.
//!
//! The counterpart to `crates/tsr-parser/examples/rss.rs`, which stops at the
//! AST. Same method, same fixtures, three measurements instead of two:
//!
//! - **baseline** — source text read, nothing parsed.
//! - **parsed** — every file's AST live at once.
//! - **bound** — every file's [`BindResult`] live too.
//!
//! Only the differences mean anything; a process's fixed overhead is not
//! something either side should get credit or blame for. See
//! [ADR-0009](../../../docs/adr/0009-performance-gate.md) for why peak (`VmHWM`)
//! rather than current, and `docs/architecture/performance.md` for the Go
//! program this is compared against.
//!
//! Run with `cargo run -p tsr-binder --example rss --release`.
//!
//! [`BindResult`]: tsr_binder::BindResult

use std::path::PathBuf;

use tsr_core::Arena;

/// Peak resident set size in kibibytes, from `/proc/self/status`.
fn peak_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("VmHWM in /proc/self/status")
}

/// The same fixtures as the parser's RSS example, in the same order.
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

#[allow(clippy::cast_precision_loss)] // Reporting ratios for a human.
fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ has a grandparent")
        .to_path_buf();

    // Everything is read before anything is parsed, so the source text is in the
    // baseline rather than in the AST's column.
    let sources: Vec<(String, String)> = fixtures(&root)
        .iter()
        .filter_map(|(name, path)| {
            std::fs::read_to_string(path).ok().map(|text| (name.clone(), text))
        })
        .collect();
    let bytes: usize = sources.iter().map(|(_, text)| text.len()).sum();
    let baseline = peak_rss_kib();

    // JSDoc off, matching the parse+bind benchmark: typescript-go builds no
    // JSDoc nodes for `.ts`/`.tsx` (ADR-0010), so leaving it on would measure a
    // different amount of tree.
    let arenas: Vec<Arena> = sources.iter().map(|_| Arena::new()).collect();
    let mut nodes = 0usize;
    let parsed: Vec<_> = sources
        .iter()
        .zip(&arenas)
        .map(|((name, text), arena)| {
            let options = tsr_parser::ParseOptions {
                jsdoc: false,
                ..tsr_parser::ParseOptions::for_file(name)
            };
            let result = tsr_parser::parse_with_options(arena, text, options);
            nodes += result.nodes.len();
            result
        })
        .collect();
    let after_parse = peak_rss_kib();

    let mut symbols = 0usize;
    let mut flow_nodes = 0usize;
    let mut reported_heap = 0usize;
    let bound: Vec<_> = parsed
        .iter()
        .map(|file| {
            let result = tsr_binder::bind(file.source_file, &file.nodes);
            symbols += result.symbols().len();
            flow_nodes += result.flow().len();
            reported_heap += result.heap_bytes();
            result
        })
        .collect();
    let after_bind = peak_rss_kib();

    // Nothing may be dropped before the measurements are read back.
    std::hint::black_box((&parsed, &bound));

    let ast = after_parse.saturating_sub(baseline);
    let binder = after_bind.saturating_sub(after_parse);

    println!("files:         {}", sources.len());
    println!("source bytes:  {bytes}");
    println!("nodes:         {nodes}");
    println!("symbols:       {symbols}");
    println!("flow nodes:    {flow_nodes}");
    println!("baseline KiB:  {baseline}");
    println!("parsed KiB:    {after_parse}");
    println!("bound KiB:     {after_bind}");
    println!("ast KiB:       {ast}");
    println!("binder KiB:    {binder}");
    println!("binder KiB (self-reported): {}", reported_heap / 1024);
    println!("nodes per flow node:   {:.1}", nodes as f64 / flow_nodes.max(1) as f64);
    println!("binder bytes per source byte: {:.2}", (binder * 1024) as f64 / bytes as f64);
    println!("total bytes per source byte:  {:.2}", ((ast + binder) * 1024) as f64 / bytes as f64);
}
