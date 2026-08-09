//! Where `JSX.IntrinsicElements` is looked for, and TS7026's silence.
//!
//! `getJsxNamespaceAt` (`internal/checker/jsx.go:1306`) tries three roads and
//! takes the first that answers: the implicit-import container, then
//! **`getJsxNamespace(location)` as a namespace and its `JSX` export**, then the
//! global `JSX`. This port shipped the third alone.
//!
//! # Why these tests exist and the conformance suites do not cover this
//!
//! The corpus declares JSX the way TypeScript 2 did: 82 of its JSX cases write
//! `declare namespace JSX` at global scope and essentially none use the
//! `React.JSX` road. So `checker_types`, `diagnostics` and `binder_symbols` are
//! **byte-identical** with and without the second road — measured on three
//! separate bases — while a 22-package repository on `@types/react` 19 went from
//! **1,642 false TS7026 to 0**. `docs/architecture/checker-notes-diag2.md` §221.
//!
//! A rule with no reachable oracle needs unit tests, and these are them. Each
//! fixture mirrors the shape `@types/react` actually ships:
//!
//! ```ts
//! declare namespace React { namespace JSX { interface IntrinsicElements { … } } }
//! export = React;
//! export as namespace React;
//! ```
//!
//! # The mutations
//!
//! Every positive test was confirmed red under the arm it exists for, applied
//! one at a time:
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | delete the `jsx_namespace_symbol` step | the UMD and both import tests |
//! | 2 | delete the UMD (`NamespaceExportDeclaration`) hop | [`the_umd_global_reaches_the_jsx_namespace`] **only** |
//! | 3 | delete the `NamespaceImport`/`ImportClause` hop | the two import tests **only** |
//! | 4 | delete `follow_export_assignment` | the UMD and both import tests |
//! | 5 | let step 2 answer with a container that has no `JSX` | [`a_react_namespace_without_jsx_falls_through_rather_than_answering`] **only** |
//!
//! Measured, not predicted: an earlier draft of this table claimed 1 and 4
//! reddened "all four positive tests", and they redden three.
//! [`the_global_jsx_namespace_still_answers`] is green under both **because it
//! is supposed to be** — it exercises the road those mutations do not touch,
//! which is what makes it the pair to mutations 1 and 4 rather than a
//! duplicate of them.
//!
//! [`nothing_declares_jsx_so_the_rule_still_reports`] is the control: green
//! under all five, red only if the rule stops firing at all. Without it every
//! test here could pass by the rule never reporting, which is the exact failure
//! mode of a change whose entire purpose is to make a rule report *less*.

use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_checker::resolution::ModuleHost;
use tsr_core::Arena;

/// The `@types/react` shape, reduced to what the lookup walks.
///
/// `export = React` **and** `export as namespace React` together, because that
/// pairing is what makes the name reachable with no import at all and is the
/// reason two hops are needed to get from the name to the exports.
const REACT_DTS: &str = "declare namespace React {\n    namespace JSX {\n        interface IntrinsicElements {\n            div: { className?: string };\n        }\n    }\n}\nexport = React;\nexport as namespace React;\n";

/// Resolves a bare specifier to the fixture whose stem matches it.
///
/// The two import tests need this and the UMD test does not, which is itself
/// the distinction: `import * as React from "react"` reaches the namespace
/// through *module resolution*, and `getTargetOfNamespaceImport` cannot run
/// without a host. Without one they failed while the UMD test passed — the
/// harness answering a question about itself rather than about the rule.
struct Fixtures {
    /// `SourceFile` node id per fixture path, in load order.
    files: Vec<(String, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        self.files
            .iter()
            .find(|(path, _)| path.trim_start_matches('/').split('.').next() == Some(specifier))
            .map(|&(_, id)| id)
    }

    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }
}

/// Bind the fixtures as one program and return the TS7026 count for the **last**
/// file, which is the one every test puts its JSX in.
fn ts7026_count(files: &[(&str, &str)]) -> usize {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();

    for (name, source) in files {
        let source: &str = arena.alloc_str(source);
        let name: &str = arena.alloc_str(name);
        // `for_file`, not `default`: a `.tsx` fixture has to be parsed as JSX,
        // and a `.d.ts` has to be ambient.
        let file = tsr_parser::parse_into(
            &arena,
            source,
            tsr_parser::ParseOptions::for_file(name),
            &mut nodes,
            &mut node_map,
        );
        assert!(
            file.diagnostics.is_empty(),
            "fixture {name} must parse: {:?}",
            file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        parsed.push((name, source, file.source_file));
    }

    let mut bound = BindResult::empty();
    let mut roots: Vec<(&str, NodeId)> = Vec::new();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let id = source_file.node_id.expect("a parsed file has an id");
        roots.push((name, id));
        host.files.push((name.to_string(), id));
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name, text: source },
        );
    }

    let module_host: Option<&dyn ModuleHost> = Some(&host);
    let mut checker = Checker::with_module_host(&bound, &nodes, &node_map, module_host);
    // The whole rule is inside `if c.noImplicitAny` (`jsx.go:1252`).
    checker.set_no_implicit_any(true);
    let (name, root) = *roots.last().expect("at least one file");
    checker.check_source_file(
        root,
        FileContext { ambient: name.ends_with(".d.ts"), has_parse_errors: false },
    );
    checker.diagnostics().iter().filter(|(_, diagnostic)| diagnostic.code() == "TS7026").count()
}

#[test]
fn the_umd_global_reaches_the_jsx_namespace() {
    // No import at all. `React` resolves to the UMD global — a
    // `NamespaceExportDeclaration` alias — and the namespace holding `JSX` is
    // two hops away: `getTargetOfNamespaceExportDeclaration` (`checker.go:15011`)
    // takes the alias's parent, the file's module symbol, and
    // `resolveExternalModuleSymbol`s it through `export =`.
    //
    // This is the shape that produced most of the 1,642: a Next.js-style
    // component that never mentions React.
    assert_eq!(
        ts7026_count(&[
            ("/react.d.ts", REACT_DTS),
            ("/a.tsx", "export const x = <div className=\"a\" />;\n"),
        ]),
        0
    );
}

#[test]
fn a_namespace_import_reaches_the_jsx_namespace() {
    // `getTargetOfNamespaceImport` (`checker.go:15053`). The hop
    // `Checker::resolve_alias` declines for *naming* reasons (`bd tsr-4jk`) —
    // an argument about printing that does not reach a rule which only asks
    // whether the namespace exists.
    assert_eq!(
        ts7026_count(&[
            ("/react.d.ts", REACT_DTS),
            (
                "/a.tsx",
                "import * as React from \"react\";\nexport const x = <div className=\"a\" />;\n"
            ),
        ]),
        0
    );
}

#[test]
fn a_default_import_reaches_the_jsx_namespace() {
    // `getTargetOfImportClause` (`checker.go:15020`), synthetic-default arm:
    // `@types/react` has no `default` export, so the target is its `export =`.
    // `import type React from "react"` binds the same clause.
    assert_eq!(
        ts7026_count(&[
            ("/react.d.ts", REACT_DTS),
            ("/a.tsx", "import React from \"react\";\nexport const x = <div className=\"a\" />;\n"),
        ]),
        0
    );
}

#[test]
fn the_global_jsx_namespace_still_answers() {
    // The third road, which is the one this rule shipped with and the one the
    // entire corpus exercises. It must keep working: `getJsxNamespaceAt` falls
    // through to it when the first two find nothing, and no fixture here
    // declares a `React`.
    assert_eq!(
        ts7026_count(&[
            (
                "/globals.d.ts",
                "declare namespace JSX {\n    interface IntrinsicElements {\n        div: { className?: string };\n    }\n}\n"
            ),
            ("/a.tsx", "export const x = <div className=\"a\" />;\n"),
        ]),
        0
    );
}

#[test]
fn nothing_declares_jsx_so_the_rule_still_reports() {
    // The control. Without it every test above could pass by the rule never
    // firing, and the whole point of the change is that it fires *less* — so a
    // test that it fires at all is what makes the others mean something.
    //
    // Two elements, two diagnostics: upstream reports per opening-like element.
    assert_eq!(
        ts7026_count(&[("/a.tsx", "export const x = <div />;\nexport const y = <span />;\n")]),
        2
    );
}

#[test]
fn a_react_namespace_without_jsx_falls_through_rather_than_answering() {
    // `getJsxNamespaceAt` takes the *first road that answers*, not the first
    // that resolves: step 2 yields nothing unless the container actually
    // exports a `JSX` namespace (`jsx.go:1321-1327`), and only then does the
    // global fallback run. A `React` with no `JSX` must therefore not shadow a
    // global `JSX` that does exist.
    assert_eq!(
        ts7026_count(&[
            (
                "/react.d.ts",
                "declare namespace React {\n    function createElement(): void;\n}\nexport = React;\nexport as namespace React;\n"
            ),
            (
                "/globals.d.ts",
                "declare namespace JSX {\n    interface IntrinsicElements {\n        div: { className?: string };\n    }\n}\n"
            ),
            ("/a.tsx", "export const x = <div className=\"a\" />;\n"),
        ]),
        0
    );
}
