//! Print the TS2300s our binder raises for a snippet, with the node kinds involved.
//!
//! Diagnostic tool, not a suite. Answers "which construct produced this
//! duplicate?", which the corpus-wide classifier cannot: it reports positions
//! only, and a position tells you where the collision was *reported*, not what
//! declared the colliding names.

use tsr_binder::SymbolFlags;
use tsr_conformance::symbols_baseline::line_and_character;
use tsr_parser::{ParsedFile, ScriptKind};

const SNIPPETS: &[(&str, &str)] = &[
    (
        "destructuring assignment vs let",
        "class C {\n    x: string;\n    y: string;\n    constructor() {\n        let { x, y: y1 } = this;\n        ({ x, y: y1, \"y\": y1 } = this);\n    }\n}\n",
    ),
    ("plain object literal", "const o = { a: 1, b: 2 };\n"),
    ("duplicate object literal key", "const o = { a: 1, a: 2 };\n"),
    ("array destructuring assignment", "let a, b;\n[a, b] = [1, 2];\n"),
    ("shorthand assignment only", "let x;\n({ x } = { x: 1 });\n"),
    ("two classes, same type parameter name", "class A<T> { x: T; }\nclass B<T> { y: T; }\n"),
    ("two functions, same type parameter name", "function f<T>(a: T) {}\nfunction g<T>(a: T) {}\n"),
    ("one class, duplicate type parameter", "class A<T, T> { }\n"),
];

fn main() {
    for (label, source) in SNIPPETS {
        println!("=== {label} ===");
        let parsed =
            ParsedFile::parse_with_script_kind((*source).to_string(), ScriptKind::TypeScript);
        parsed.with_ast(|file| {
            let bound = tsr_binder::bind(
                file,
                parsed.nodes(),
                tsr_binder::FileInfo { name: "probe.ts", text: parsed.source() },
            );
            for diagnostic in bound.diagnostics() {
                let (line, character) = line_and_character(source, diagnostic.span.start);
                println!("  TS{} at ({},{})", diagnostic.message.code(), line + 1, character + 1);
            }
            for (id, symbol) in bound.symbols().iter() {
                let flags = symbol.flags;
                if symbol.declarations.len() > 1 || flags.contains(SymbolFlags::ASSIGNMENT) {
                    println!(
                        "  symbol {:?} {:?} decls={} flags={flags:?}",
                        id,
                        symbol.name,
                        symbol.declarations.len()
                    );
                }
            }
        });
    }
}
