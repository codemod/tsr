//! Print one file and report the round trip, for diagnosing a corpus failure.
//!
//! ```text
//! cargo run -p tsr-printer --example roundtrip -- path/to/file.ts
//! ```
fn main() {
    let path = std::env::args().nth(1).expect("usage: roundtrip <file>");
    let source = std::fs::read_to_string(&path).expect("reading the input");
    let kind = tsr_parser::ScriptKind::from_file_name(&path);
    let parsed = tsr_parser::ParsedFile::parse_with_script_kind(source, kind);
    for diagnostic in parsed.diagnostics() {
        println!("input diagnostic: {}", diagnostic.text());
    }
    let printed = parsed.with_ast(|file| tsr_printer::print(file, parsed.nodes()));
    for kind in &printed.unsupported {
        println!("unsupported: {}", kind.name());
    }
    println!("----- printed -----\n{}", printed.text);
    let again = tsr_parser::ParsedFile::parse_with_script_kind(printed.text, kind);
    for diagnostic in again.diagnostics() {
        println!("REPARSE ERROR: {}", diagnostic.text());
    }
}
