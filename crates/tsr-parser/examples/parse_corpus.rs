//! Parse every corpus file, printing each name *before* parsing it.
//!
//! Diagnostic tool: run under `timeout`, and the last line printed is the file
//! that hung. Not part of the test suite.

use std::io::Write as _;

fn main() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();
    let cases = root.join("vendor/typescript-go/_submodules/TypeScript/tests/cases");

    let mut files = Vec::new();
    collect(&cases, &mut files);
    files.sort();
    eprintln!("{} files", files.len());

    for path in files {
        println!("{}", path.display());
        let _ = std::io::stdout().flush();
        let Ok(source) = std::fs::read_to_string(&path) else { continue };
        let arena = tsr_core::Arena::new();
        let _ = tsr_parser::parse(&arena, &source);
    }
    println!("DONE");
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("ts" | "tsx")) {
            out.push(path);
        }
    }
}
