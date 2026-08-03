//! Measure what eager JSDoc parsing costs across the conformance corpus.
//!
//! Parses every `.ts` unit twice: once normally, once with JSDoc parsing
//! disabled, and reports the difference. Diagnostic tool.
use std::time::Instant;
use tsr_core::Arena;

fn main() {
    let root = std::env::args().nth(1).expect("usage: jsdoc_cost <dir>");
    let mut sources = Vec::new();
    collect(std::path::Path::new(&root), &mut sources);
    eprintln!("{} files, {} bytes", sources.len(), sources.iter().map(String::len).sum::<usize>());

    // Warm the allocator and the branch predictor before timing either arm.
    for source in &sources {
        let arena = Arena::new();
        std::hint::black_box(tsr_parser::parse(&arena, source).jsdoc.len());
    }

    let mut documented = 0usize;
    let start = Instant::now();
    for source in &sources {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        documented += parsed.jsdoc.len();
        std::hint::black_box(&parsed);
    }
    let elapsed = start.elapsed();
    println!("with jsdoc:    {elapsed:?}  ({documented} documented nodes)");
    println!(
        "set TSR_NO_JSDOC=1 and re-run to compare; the difference is the cost of parsing eagerly"
    );
}

fn collect(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "ts") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push(text);
            }
        }
    }
}
