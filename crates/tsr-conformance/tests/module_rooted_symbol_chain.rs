//! `getSymbolChain` rooted at an external module prints an import type
//! (`docs/parity/notes/r7-printer.md` §1). Every expected line was read from
//! pinned native tsgo 5b1047d's compiler test runner on the same source.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn assertions(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "a.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

fn assert_has(lines: &[String], wanted: &[&str]) {
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// An export specifier is no local name for its target (`trySymbolTable`,
/// `symbolaccessibility.go:573`), so a file that re-exports `c` without
/// importing it names `c` through its declaring module, although the file
/// mentions that module's specifier.
#[test]
fn a_reexported_name_is_spelled_through_its_module() {
    let lines = assertions(
        "probe/module_rooted_reexport",
        r#"// @target: es2015
// @filename: server.ts
export class c {
}
export namespace m {
    export var x = 10;
}
// @filename: client.ts
export { c } from "./server";
export { c as c2 } from "./server";
export { m as instantiatedModule } from "./server";
"#,
    );
    assert_has(
        &lines,
        &[
            "c : typeof import(\"./server\").c",
            "c2 : typeof import(\"./server\").c",
            "instantiatedModule : typeof import(\"./server\").m",
        ],
    );
}

/// `getAlternativeContainingModules`' program-wide fallback
/// (`symbolaccessibility.go:217`): `c.ts` imports nothing that exports `I`,
/// so every external module that does is a container, and `sortByBestName`
/// prefers the re-exporting `./lib` over the declaring `./lib/impl/a`. In
/// `other.ts` the first query (the import line's) resolves the file's own
/// import and the per-file link keeps that answer for `v`'s line, whose own
/// location would miss it (`resolveExternalModule`'s default mode).
#[test]
fn a_container_no_import_reaches_comes_from_every_program_module() {
    let lines = assertions(
        "probe/module_rooted_extended_containers",
        r#"// @module: commonjs
// @target: es2015
// @filename: lib/impl/a.ts
export interface I { x: number }
export declare function make(): I;
// @filename: lib/index.ts
export * from "./impl/a";
// @filename: other.ts
import { make } from "./lib/impl/a";
export const v = make();
// @filename: c.ts
import { v } from "./other";
export const w = v;
"#,
    );
    assert_has(
        &lines,
        &[
            "w : import(\"./lib\").I",
            "make : () => import(\"./lib/impl/a\").I",
            "v : import(\"./lib/impl/a\").I",
        ],
    );
}

/// `symbolToTypeNode`'s import-type arm (`nodebuilderimpl.go:651`) tests the
/// chain's root for a string-named module declaration: an `export =` target
/// a module augmentation merged into is spelled through its module even in
/// its own file, where its name is accessible (`augmentExportEquals4`).
#[test]
fn an_augmented_export_equals_target_is_spelled_through_its_module() {
    let lines = assertions(
        "probe/module_rooted_augmented_target",
        r#"// @target: es2015
// @module: commonjs
// @filename: file1.ts
class foo {}
namespace foo {
    export var v = 1;
}
export = foo;
// @filename: file2.ts
import x = require("./file1");
x.b = 1;
declare module "./file1" {
    interface A { a }
    let b: number;
}
"#,
    );
    assert_has(
        &lines,
        &["foo : import(\"./file1\")", "foo : typeof import(\"./file1\")", "x : typeof x"],
    );
}
