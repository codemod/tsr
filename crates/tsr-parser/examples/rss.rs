//! Peak resident set size for parsing and holding a file's AST.
//!
//! The counterpart to `benches/parse.rs`, for the axis
//! [ADR-0009](../../../docs/adr/0009-performance-gate.md) gates alongside wall
//! clock. Run it against the matching Go program to get a ratio; see
//! `docs/architecture/performance.md` for the method and the Go source.
//!
//! **Peak, not current.** `VmHWM` is a high-water mark the kernel never lowers,
//! which is the right measure here: a garbage-collected process that grows to 2 GB
//! and then returns memory to the OS still needed 2 GB to exist, and that is what
//! decides whether a large project can be checked at all.
//!
//! Three numbers are reported per mode, because only the difference is meaningful:
//! the baseline (source text read, nothing parsed) is subtracted from the loaded
//! figure to give the AST's own cost. A process's fixed overhead — runtime,
//! allocator arenas, the binary itself — is not something either side should get
//! credit or blame for.

use std::path::PathBuf;

use tsr_core::Arena;

/// Peak resident set size in kibibytes, from `/proc/self/status`.
///
/// `VmHWM` rather than `VmRSS`: the latter is whatever happens to be resident
/// when it is read, which for a GC'd process depends on when the last collection
/// ran.
fn peak_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("read /proc/self/status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("VmHWM in /proc/self/status")
}

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

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ has a grandparent")
        .to_path_buf();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let jsdoc = args.iter().any(|a| a == "--jsdoc");
    let only = args.iter().find(|a| !a.starts_with("--")).cloned();

    let selected: Vec<(String, PathBuf)> = fixtures(&root)
        .into_iter()
        .filter(|(name, _)| only.as_ref().is_none_or(|want| name == want))
        .collect();

    // Every fixture is read before anything is parsed, so the source text is in
    // the baseline rather than in the AST's column.
    let sources: Vec<(String, String)> = selected
        .iter()
        .filter_map(|(name, path)| {
            std::fs::read_to_string(path).ok().map(|text| (name.clone(), text))
        })
        .collect();
    let bytes: usize = sources.iter().map(|(_, text)| text.len()).sum();
    let baseline = peak_rss_kib();

    // One arena and one parse per fixture, all held live at once: a compiler holds
    // every file's tree, so measuring them one at a time and taking the max would
    // understate what a real run costs.
    let arenas: Vec<Arena> = sources.iter().map(|_| Arena::new()).collect();
    let mut nodes = 0usize;
    let parsed: Vec<_> = sources
        .iter()
        .zip(&arenas)
        .map(|((name, text), arena)| {
            let options =
                tsr_parser::ParseOptions { jsdoc, ..tsr_parser::ParseOptions::for_file(name) };
            let result = tsr_parser::parse_with_options(arena, text, options);
            nodes += result.nodes.len();
            result
        })
        .collect();

    let loaded = peak_rss_kib();
    // Nothing may be dropped before the measurement is taken.
    std::hint::black_box(&parsed);

    println!("files:        {}", sources.len());
    println!("source bytes: {bytes}");
    println!("nodes:        {nodes}");
    println!("jsdoc:        {jsdoc}");
    println!("baseline KiB: {baseline}");
    println!("loaded KiB:   {loaded}");
    println!("ast KiB:      {}", loaded.saturating_sub(baseline));
    #[allow(clippy::cast_precision_loss)]
    {
        println!(
            "bytes per source byte: {:.2}",
            (loaded.saturating_sub(baseline) * 1024) as f64 / bytes as f64
        );
    }
}
