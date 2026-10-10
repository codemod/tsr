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

/// A print baked with its written qualifier (`foo.Provide`) is re-spelled
/// from `getSymbolChain` at the site: the in-scope alias `provide` names the
/// namespace first (`aliasBug`'s native baseline).
#[test]
fn a_written_qualifier_is_respelled_by_the_accessible_chain() {
    let lines = assertions(
        "probe/module_rooted_written_qualifier",
        r"// @target: es2015
// @module: commonjs
namespace foo {
    export class Provide {
    }
}
import provide = foo;
function use() {
  var p1: provide.Provide;
  var p2: foo.Provide;
}
",
    );
    assert_has(&lines, &["p1 : provide.Provide", "p2 : provide.Provide"]);
}

/// A JavaScript file that only `require`s is a `CommonJS` module, so its
/// locals are a scope table (`ast.IsGlobalSourceFile`), and the `require`
/// alias names the module (`varRequireFromJavascript`'s native baseline).
#[test]
fn a_require_alias_in_a_commonjs_file_names_its_module() {
    let lines = assertions(
        "probe/module_rooted_require_alias",
        r"// @target: es2015
// @allowJs: true
// @checkJs: true
// @strict: true
// @noEmit: true
// @Filename: ex.js
export class Crunch {
    /** @param {number} n */
    constructor(n) {
        this.n = n
    }
}
// @Filename: use.js
var ex = require('./ex')
/**
 * @param {ex.Crunch} wrap
 */
function f(wrap) {
    wrap.n
}
",
    );
    assert_has(&lines, &["wrap : ex.Crunch"]);
}

/// `getSymbolChain` over a module object: no alias in scope names module
/// `0`, but `trySymbolTable`'s `getCandidateListForSymbol` reaches it through
/// `foo`'s exports, where `export * as ns` re-exports it
/// (`exportAsNamespace1`'s native baseline).
#[test]
fn a_module_reached_through_an_alias_export_is_named_by_that_route() {
    let lines = assertions(
        "probe/module_rooted_namespace_reexport",
        r"// @target: es2015
// @module: esnext
// @filename: 0.ts
export const a = 1;
export const b = 2;
// @filename: 1.ts
export * as ns from './0';
// @filename: 2.ts
import * as foo from './1'
foo.ns.a;
",
    );
    assert_has(&lines, &["foo.ns : typeof foo.ns"]);
}

/// A baked import type (`import("pkg").ImportInterface`, as written with a
/// `resolution-mode` attribute) is re-spelled from the chain's module root:
/// under the `CommonJS` importing file's conditions, `pkg`'s `exports` block
/// the `import` file, so the specifier is the relative path through
/// `node_modules` (`nodeModulesImportAttributesTypeModeDeclarationEmit`'s
/// native baseline).
#[test]
fn a_baked_import_type_is_respelled_from_the_module_root() {
    let lines = assertions(
        "probe/module_rooted_import_type",
        r#"// @target: es2022
// @noImplicitReferences: true
// @module: node16
// @declaration: true
// @outDir: out
// @filename: /node_modules/pkg/package.json
{
    "name": "pkg",
    "version": "0.0.1",
    "exports": {
        "import": "./import.js",
        "require": "./require.js"
    }
}
// @filename: /node_modules/pkg/import.d.ts
export interface ImportInterface {}
// @filename: /node_modules/pkg/require.d.ts
export interface RequireInterface {}
// @filename: /index.ts
export const a = (null as any as import("pkg", { with: {"resolution-mode": "require"} }).RequireInterface);
export const b = (null as any as import("pkg", { with: {"resolution-mode": "import"} }).ImportInterface);
"#,
    );
    assert_has(
        &lines,
        &[
            "a : import(\"pkg\").RequireInterface",
            "b : import(\"./node_modules/pkg/import\").ImportInterface",
        ],
    );
}
