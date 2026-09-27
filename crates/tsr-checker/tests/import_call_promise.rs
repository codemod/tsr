//! A dynamic `import()` is a `Promise` even when the specifier does not
//! resolve. §818.
//!
//! `checkImportCallExpression` (`checker.go:8267`) has three returns and **all
//! three are a promise**:
//!
//! ```go
//! if len(args) == 0 { return c.createPromiseReturnType(node, c.anyType) }     // :8273
//! …
//! if moduleSymbol != nil { … return c.createPromiseReturnType(node, syntheticType) }  // :8311
//! return c.createPromiseReturnType(node, c.anyType)                           // :8314
//! ```
//!
//! §140 built the middle one and declined the other two, so an `import(expr)`
//! whose specifier is not a string literal — and one whose module does not
//! resolve — answered `errorType`. Upstream's own comment at `:8302` says the
//! fall-through is exactly for that case: *"resolveExternalModuleName will
//! return undefined if the moduleReferenceExpression is not a string literal"*.
//!
//! `createPromiseType` (`:20348`) then answers `Promise<any>` for `anyType`,
//! because `getAwaitedTypeNoAlias(any)` is `any` and no unwrapping is
//! observable.
//!
//! The one decline that stays is faithful: with **no `Promise` global**,
//! `createPromiseReturnType` (`:20374`) reports
//! `A_dynamic_import_call_returns_a_Promise…` and answers `errorType`.
//!
//! These fixtures declare their own global `Promise<T>` because this harness
//! parses files with no `lib.d.ts`.

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

struct Fixtures {
    files: Vec<(&'static str, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }

    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, _)| *fixture == name).map(|&(_, id)| id)
    }
}

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let file = tsr_parser::parse_into(
            arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(
            file.diagnostics.is_empty(),
            "fixture {name} must parse: {:?}",
            file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        parsed.push((*name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound, host }
}

/// The rendered type of the variable named `variable`.
fn variable_type(fixture: &Fixture<'_>, variable: &str) -> String {
    let host: Option<&dyn ModuleHost> = Some(&fixture.host);
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        .find(|&id| {
            fixture
                .bound
                .symbol_of(id)
                .is_some_and(|s| fixture.bound.symbols().get(s).name == variable)
        })
        .unwrap_or_else(|| panic!("no variable named `{variable}`"));
    let symbol = fixture.bound.symbol_of(declaration).expect("bound");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string_at(id, declaration).unwrap_or_else(|| "error".to_string())
}

const PROMISE: &str = "interface Promise<T> { then(): T; }\n";

/// §818, the `:8314` fall-through: a specifier that is not a string literal.
/// `importCallExpressionSpecifierNotStringTypeError` is the corpus case, and it
/// answered `error` on twelve lines.
#[test]
fn a_non_literal_specifier_is_still_a_promise() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("lib", PROMISE), ("a", "declare const s: string;\nconst p = import(s);\n")],
    );
    assert_eq!(variable_type(&fixture, "p"), "Promise<any>");
}

/// §818, the same return reached the other way: the specifier IS a string
/// literal and names no module the host knows.
#[test]
fn an_unresolvable_module_is_still_a_promise() {
    let arena = Arena::new();
    let fixture = program(&arena, &[("lib", PROMISE), ("a", "const p = import(\"./nope\");\n")]);
    assert_eq!(variable_type(&fixture, "p"), "Promise<any>");
}

/// §140's return, unchanged by §818: a resolvable specifier keeps the module's
/// own `typeof import("…")` spelling rather than collapsing to `any`.
#[test]
fn a_resolvable_specifier_keeps_the_module_namespace() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("lib", PROMISE),
            ("m", "export const x: number = 1;\n"),
            ("a", "const p = import(\"./m\");\n"),
        ],
    );
    assert_eq!(variable_type(&fixture, "p"), "Promise<typeof import(\"./m\")>");
}

/// The decline that stays, and the reason it is faithful rather than lazy:
/// with no `Promise` global, `createPromiseReturnType` (`checker.go:20374`)
/// reports a diagnostic and answers `errorType`.
#[test]
fn without_a_promise_global_the_call_declines() {
    let arena = Arena::new();
    let fixture = program(&arena, &[("a", "declare const s: string;\nconst p = import(s);\n")]);
    assert_eq!(variable_type(&fixture, "p"), "error");
}
