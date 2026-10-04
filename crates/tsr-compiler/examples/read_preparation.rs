//! Opt-in leased read/decode API witness. No production loading or pool policy.
#[path = "read_preparation/budget.rs"]
mod budget;

use budget::{Budget, Error, PreparedText};
use tsr_core::Arena;

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("formatting into String");
    }
    text
}

fn payload(text: &PreparedText, path: &str) {
    println!("text\t{}", hex(text.text().as_bytes()));
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut map = tsr_ast::NodeMap::new();
    let parsed = tsr_parser::parse_into(
        &arena,
        arena.alloc_str(text.text()),
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
        map
    );
    println!("parse\t{}", hex(image.as_bytes()));
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 && args.len() != 3 {
        return Err("usage: read_preparation /absolute/input.ts [budget-bytes audit]".into());
    }
    let path = &args[0];
    if !std::path::Path::new(path).is_absolute() {
        return Err("input path must be absolute".into());
    }
    let limit = if args.len() == 3 {
        if args[2] != "audit" {
            return Err("expected audit".into());
        }
        args[1].parse().map_err(|_| "invalid budget")?
    } else {
        1024 * 1024
    };
    let audit = args.len() == 3;
    let budget = Budget::new(limit);
    let result = std::fs::File::open(path)
        .map_err(Error::Io)
        .and_then(|mut file| budget::prepare_file(&mut file, &budget));
    let mut error = None;
    match result {
        Ok(text) => {
            if audit {
                eprintln!("retained\t{}\t{}", text.capacity(), budget.stats().live);
            }
            payload(&text, path);
            drop(text);
        }
        Err(Error::Io(io)) if !audit => {
            // The legacy read/parse witness uses missing for all read failures.
            // Detailed audit preserves the I/O error; budget failure never uses
            // this compatibility display branch.
            let _ = io;
            println!("missing");
        }
        Err(Error::Io(io)) => error = Some(format!("io\t{:?}", io.kind())),
        Err(Error::Budget { requested, live, limit }) => {
            error = Some(format!("budget\t{requested}\t{live}\t{limit}"));
        }
        Err(Error::Allocation(e)) => error = Some(format!("allocation\t{e}")),
        Err(Error::Overflow) => error = Some("overflow".into()),
        Err(Error::UnsupportedCapacity { requested, actual }) => {
            error = Some(format!("unsupported-capacity\t{requested}\t{actual}"));
        }
    }
    if audit {
        let stats = budget.stats();
        eprintln!(
            "released\t{}\t{}\t{}\t{}\t{}",
            stats.live, stats.peak, stats.acquired, stats.released, limit
        );
    }
    if let Some(error) = error {
        return Err(error);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
