//! Parse one file repeatedly, so `perf record` has something to sample.
//!
//! Deliberately minimal — no custom allocator, no timing, no output inside the
//! loop — so the profile shows the parser and nothing else.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: parse_loop <file> [iterations]");
    let iterations: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(200);
    let source = std::fs::read_to_string(&path).expect("read");
    let script_kind = tsr_parser::ScriptKind::from_file_name(&path);

    let mut nodes = 0usize;
    for _ in 0..iterations {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse_with_script_kind(&arena, &source, script_kind);
        // Consume something so nothing can be optimised away.
        nodes = nodes.wrapping_add(parsed.nodes.len());
    }
    println!("{nodes}");
}
