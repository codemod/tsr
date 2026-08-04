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
    // A static and an instance member of the same name are in *different* tables
    // upstream: `declareClassMember` splits on `IsStatic` (binder.go:415).
    (
        "static and instance accessor",
        "class C {\n    static get x() { return 1; }\n    static set x(v) {}\n    get x() { return 1; }\n    set x(v) {}\n}\n",
    ),
    ("static and instance method", "class C {\n    static m() {}\n    m() {}\n}\n"),
    ("static and instance property", "class C {\n    static p: number;\n    p: string;\n}\n"),
    ("get/set pair, instance only", "class C {\n    get x() { return 1; }\n    set x(v) {}\n}\n"),
    (
        "named imports with the same property name",
        "import { a as a1 } from \"m1\";\nimport { a as a2 } from \"m2\";\n",
    ),
    (
        "named re-exports with the same property name",
        "export { a as a1 } from \"m1\";\nexport { a as a2 } from \"m2\";\n",
    ),
    (
        "import and re-export of the same local name",
        "import { a as a1 } from \"m\";\nexport { a as a1 } from \"m\";\na1;\n",
    ),
    (
        "classes implementing primitives",
        "class C implements number { }\nclass C2 implements string { }\n\nconst C4 = class implements number {}\nconst C5 = class implements string {}\n\nconst C7 = class A implements number { }\nconst C8 = class B implements string { }\n",
    ),
    (
        "named class expression vs a class of the same name",
        "class C { }\nconst C9 = class C { };\n",
    ),
    ("two default exports", "export default class D { }\nexport default function g() { }\n"),
    (
        "export default class then export default object",
        "export default class D { }\nexport default { };\n",
    ),
    (
        "two getters, same name",
        "class C {\n    get x() { return 1; }\n    get x() { return 2; }\n}\n",
    ),
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
                println!(
                    "  TS{} at ({},{}) args={:?}",
                    diagnostic.message.code(),
                    line + 1,
                    character + 1,
                    diagnostic.args
                );
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
