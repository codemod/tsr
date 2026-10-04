//! Public physical read/decode/parse control for allocation characterization.
//! Allocation observations come from the separately retained VFS probe patch.
//! This helper implements no budget policy and enables no loader workers.

use tsr_core::Arena;
use tsr_vfs::{FileSystem, OsFileSystem};

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("formatting into a String");
    }
    encoded
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        return Err("usage: read_allocations /absolute/path/input.ts".into());
    };
    if !std::path::Path::new(path).is_absolute() {
        return Err("input path must be absolute".into());
    }
    let Some(text) = OsFileSystem.read_file(path) else {
        println!("missing");
        return Ok(());
    };
    println!("text\t{}", hex(text.as_bytes()));
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut map = tsr_ast::NodeMap::new();
    let parsed = tsr_parser::parse_into(
        &arena,
        arena.alloc_str(&text),
        tsr_parser::ParseOptions {
            script_kind: tsr_parser::ScriptKind::from_file_name(path),
            ..Default::default()
        },
        &mut nodes,
        &mut map,
    );
    let image = format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        parsed.node_range,
        parsed.source_file,
        parsed.diagnostics,
        parsed.jsdoc,
        parsed.file_references,
        nodes,
        map,
    );
    println!("parse\t{}", hex(image.as_bytes()));
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
