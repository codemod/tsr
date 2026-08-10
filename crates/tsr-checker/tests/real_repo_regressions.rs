//! Diagnostics whose false positives were found by pointing `tsr` at a real
//! repository, and — for every one of them — the case that must still report.
//!
//! # Why this file exists as a whole
//!
//! Each fix below removes a diagnostic, and a fix that removes diagnostics can
//! always be faked by removing the rule. So **every suppression here is paired
//! with a true positive**: the same rule, on the shape it exists to catch,
//! asserted to still fire. Those pairs are the point of the file; the mutation
//! tables under each section name which of the two a given mutation reddens.
//!
//! The conformance suites are byte-identical across all of these — that is what
//! made a real repository the only instrument, and it is also why the true
//! positives have to be written by hand here rather than left to the corpus.
//!
//! ---
//!
//! # A heritage clause that emits nothing is a type position
//!
//! `isIdentifierInNonEmittingHeritageClause` (`ast/utilities.go:3132`) names the
//! two: a class's `implements`, and **either clause of an `interface`**. Only a
//! class's `extends` is a value — it is the base constructor and it survives to
//! the output. The `extends` keyword does not separate them, because an
//! interface's clause is spelled `extends` too.
//!
//! # Two rules, two different gates, and that is the finding
//!
//! It is tempting to answer this once in `is_value_reference` and have every
//! rule inherit it. **That is wrong, and the corpus said so.**
//!
//! - **TS1361** (`import type` used as a value) is guarded upstream by
//!   `IsValidTypeOnlyAliasUseSite` (`:3124`), whose third clause is exactly this
//!   predicate. Silencing it here is upstream's own rule.
//! - **TS2686** (a UMD global in a module) is guarded by
//!   `meaning&SymbolFlagsValue == SymbolFlagsValue` (`checker.go:1841`), and an
//!   interface heritage entry resolves at type meaning — so it too is silent.
//! - **TS2304** (`Cannot find name`) is **not**. Upstream reports it for an
//!   unresolved interface-heritage name through `resolveEntityName`'s failure
//!   at type meaning. Declining in `is_value_reference` — which TS2304 is keyed
//!   on here — silently lost `compiler/protoAssignment`
//!   (`interface Number extends Comparable<number>`), reported by the suite
//!   only as `1,994 → 1,993`.
//!
//! # The mutations
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | drop the clause from `is_valid_type_only_alias_use_site` | [`an_interfaces_extends_over_a_type_only_import_is_silent`] and [`a_typeof_inside_heritage_type_arguments_is_still_a_value_position`] |
//! | 2 | decline in `is_value_reference` instead (the tempting one fix) | [`an_unresolved_name_in_an_interfaces_extends_still_reports`] **only** |
//!
//! `docs/architecture/checker-notes-diag2.md` §502.

use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_checker::resolution::ModuleHost;

/// Resolves `"./stem"` to the fixture whose file name has that stem, and
/// answers `is_declaration_file` from the fixture's own name.
///
/// Both are needed and neither is decoration: without `resolved_module` the
/// TS1192 tests report nothing at all — the rule returns before it can — and
/// the control passed vacuously on the first run. That is the **third** time
/// this session a missing host in a fixture harness first showed up as a green
/// test that should have been red.
struct Fixtures {
    files: Vec<(String, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let want = specifier.trim_start_matches("./");
        self.files
            .iter()
            .find(|(path, _)| path.trim_start_matches('/').split('.').next() == Some(want))
            .map(|&(_, id)| id)
    }

    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }

    fn is_declaration_file(&self, file: NodeId) -> bool {
        self.files
            .iter()
            .find(|&&(_, id)| id == file)
            .is_some_and(|(path, _)| tsr_binder::is_declaration_file(path))
    }
}

/// Every diagnostic code the last fixture produces, sorted and deduplicated.
fn codes(files: &[(&str, &str)]) -> Vec<String> {
    let arena = tsr_core::Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &str = arena.alloc_str(source);
        let name: &str = arena.alloc_str(name);
        let file = tsr_parser::parse_into(
            &arena,
            source,
            tsr_parser::ParseOptions::for_file(name),
            &mut nodes,
            &mut node_map,
        );
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
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
    let (name, root) = *roots.last().expect("at least one file");
    checker.check_source_file(
        root,
        FileContext { ambient: name.ends_with(".d.ts"), has_parse_errors: false },
    );
    let mut out: Vec<String> =
        checker.diagnostics().iter().map(|(_, d)| d.code()).collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

/// A type-only import of two names, one generic.
const LIB: (&str, &str) =
    ("/lib.ts", "export interface Base { a: number }\nexport type Variant<T> = { v: T };\n");
const TYPE_IMPORT: &str = "import type { Base, Variant } from \"./lib\";\n";

#[test]
fn an_interfaces_extends_over_a_type_only_import_is_silent() {
    // The shape every `cva`-styled React component is written in:
    // `interface P extends VariantProps<typeof variants>` over an
    // `import type { VariantProps }`. It reported TS1361 on all of them.
    //
    // Red under: dropping `identifier_in_non_emitting_heritage_clause` from
    // `is_valid_type_only_alias_use_site`.
    assert_eq!(
        codes(&[
            LIB,
            (
                "/a.ts",
                &format!(
                    "{TYPE_IMPORT}export interface X extends Base, Variant<string> {{ b: number }}\n"
                )
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_class_implements_over_a_type_only_import_is_silent() {
    // The other non-emitting clause. This one was already silent — its arm
    // keyed on the `implements` keyword — and it is here so that a future
    // simplification of the predicate cannot quietly drop it.
    assert_eq!(
        codes(&[
            LIB,
            ("/a.ts", &format!("{TYPE_IMPORT}export class C implements Base {{ a = 1; }}\n")),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_class_extends_over_a_type_only_import_still_reports() {
    // The value position, and the control for both tests above: a class's
    // `extends` is the base constructor and it *is* emitted, so a type-only
    // import there is exactly what TS1361 exists for.
    assert_eq!(
        codes(&[LIB, ("/a.ts", &format!("{TYPE_IMPORT}export class C extends Base {{}}\n")),]),
        vec!["TS1361".to_string()]
    );
}

#[test]
fn an_unresolved_name_in_an_interfaces_extends_still_reports() {
    // `compiler/protoAssignment`, reduced. Upstream reports `Cannot find name`
    // here through `resolveEntityName` at **type** meaning — a heritage clause
    // being a type position does not make an undeclared name legal.
    //
    // This is the test that says the two gates must stay separate: it is red
    // under the "obvious" fix of declining in `is_value_reference`, which is
    // what made TS1361 and TS2686 silent in one place.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "interface Number extends Comparable<number> { compareTo(o: number): void }\n"
        )]),
        vec!["TS2304".to_string()]
    );
}

#[test]
fn a_typeof_inside_heritage_type_arguments_is_still_a_value_position() {
    // The loop in `isIdentifierInNonEmittingHeritageClause` climbs
    // `PropertyAccessExpression` and `ExpressionWithTypeArguments` and nothing
    // else, which is what keeps the *type arguments* out of the exemption.
    // `typeof nope` is a value position (§79) and its name must still resolve.
    //
    // **Measured, and it is a guard rather than an arm test.** Widening the
    // walk to climb `TypeQueryNode` too does *not* redden it: TS2304 is keyed on
    // `is_value_reference`, whose own type-query arm answers independently of
    // the heritage walk. It stays because the invariant is worth pinning and
    // because a future rule keyed on the use-site predicate would need it — not
    // because a mutation was found for it. It *is* red under M1 (dropping the
    // clause), which adds a TS1361 for `Variant` beside the TS2304.
    assert_eq!(
        codes(&[
            LIB,
            (
                "/a.ts",
                &format!(
                    "{TYPE_IMPORT}export interface X extends Variant<typeof nope> {{ b: number }}\n"
                )
            ),
        ]),
        vec!["TS2304".to_string()]
    );
}

// ---------------------------------------------------------------------------
// TS1192 and the synthetic default
//
// `canHaveSyntheticDefault` (`checker.go:14818`) is the second of three
// conjuncts guarding the report (`:14566`); only the first was ported, so
// `import React from "react"` against `export = React` — the shape every
// `@types` package ships and every consumer writes — reported *"Module has no
// default export"*. 78 of them on a 22-package repository, 12 in one package.
//
// The conformance suites are **byte-identical** with and without this: the
// corpus has no `.d.ts` package written `export =` and imported as a default.
//
// | mutation | reddens |
// |---|---|
// | delete the `can_have_synthetic_default` conjunct | [`a_default_import_of_an_export_equals_module_is_silent`] |
// | make the declaration-file arm answer `false` | [`a_default_import_of_a_declaration_file_with_no_default_is_silent`] |
// | drop the `__esModule` escape hatch | [`a_declaration_file_that_declares_es_module_has_no_synthetic_default`] |
//
// One test per arm, and [`a_default_import_of_a_module_with_neither_still_reports`]
// is the control: a plain `.ts` with neither still reports, so none of the
// three can pass by the rule falling silent.
//
// `docs/architecture/checker-notes-diag2.md` §531.
// ---------------------------------------------------------------------------

/// A `.d.ts`-shaped module: `export =` and no `default`, like `@types/react`.
const EXPORT_EQUALS: (&str, &str) =
    ("/lib.d.ts", "declare namespace Lib {\n    const x: number;\n}\nexport = Lib;\n");

#[test]
fn a_default_import_of_an_export_equals_module_is_silent() {
    // `hasExportAssignmentSymbol` (`checker.go:14869`). Red under: deleting the
    // `can_have_synthetic_default` conjunct.
    assert_eq!(
        codes(&[EXPORT_EQUALS, ("/a.ts", "import Lib from \"./lib\";\nexport const q = Lib;\n")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_default_import_of_a_module_with_neither_still_reports() {
    // The control. A source file with no `default` and no `export =` has no
    // synthetic default, so TS1192 is exactly right — and without it the two
    // tests above could pass by the rule never firing.
    //
    // `/plain.ts` rather than `.d.ts`: the declaration-file arm is permissive
    // by design (`:14850`) and would grant a synthetic default here.
    assert_eq!(
        codes(&[
            ("/plain.ts", "export const x: number = 1;\n"),
            ("/a.ts", "import P from \"./plain\";\nexport const q = P;\n"),
        ]),
        vec!["TS1192".to_string()]
    );
}

#[test]
fn a_default_import_of_a_declaration_file_with_no_default_is_silent() {
    // The **declaration-file arm** (`checker.go:14850`), which the `export =`
    // arm does not cover: a `.d.ts` exporting only named declarations still
    // grants a synthetic default, because nothing in a `.d.ts` says whether the
    // JavaScript beside it sets `__esModule`. Upstream's own comment calls this
    // the permissive branch.
    //
    // Red under: making `can_have_synthetic_default`'s declaration-file arm
    // answer `false`, which leaves `export =` as the only road.
    assert_eq!(
        codes(&[
            ("/named.d.ts", "export declare function f(): void;\n"),
            ("/a.ts", "import N from \"./named\";\nexport const q = N;\n"),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_declaration_file_that_declares_es_module_has_no_synthetic_default() {
    // The escape hatch inside that arm: an explicit `__esModule` export means
    // someone said outright that this is an ES module, so there is no synthetic
    // default and TS1192 stands.
    assert_eq!(
        codes(&[
            (
                "/esm.d.ts",
                "export declare const __esModule: true;\nexport declare function f(): void;\n"
            ),
            ("/a.ts", "import E from \"./esm\";\nexport const q = E;\n"),
        ]),
        vec!["TS1192".to_string()]
    );
}

// ---------------------------------------------------------------------------
// TS2345 and an OPTIONAL parameter
//
// `getTypeOfParameter` (`checker.go:17042`) adds optionality to a parameter's
// declared type:
//
//   addOptionalityEx(getTypeOfSymbol(symbol), false,
//       declaration.Initializer() != nil || isOptionalDeclaration(declaration))
//
// so `b?: string` and `b: string = "d"` are both `string | undefined` under
// strictNullChecks. Taking the written annotation alone made every
// `string | undefined` argument at such a position a TS2345 — upstream's answer
// for a *required* parameter and nobody's for an optional one. 22 of them on a
// 22-package repository, and both conformance snapshots are byte-identical
// with and without the fix.
//
// | mutation | reddens |
// |---|---|
// | drop `add_optionality` from the call arm | [`an_optional_parameter_accepts_undefined`] and [`a_defaulted_parameter_accepts_undefined`] |
// | make `parameter_is_optional` test only `question_token` | [`a_defaulted_parameter_accepts_undefined`] **only** |
// | drop optionality from the `new` arm | [`a_new_expressions_optional_parameter_accepts_undefined`] **only** |
//
// [`a_required_parameter_still_rejects_undefined`] is the control.
// `docs/architecture/checker-notes-diag2.md` §541.
// ---------------------------------------------------------------------------

const MAYBE: &str = "declare const maybe: string | undefined;\n";

#[test]
fn an_optional_parameter_accepts_undefined() {
    // `isOptionalDeclaration` = `HasQuestionToken` (`utilities.go:299`).
    assert_eq!(
        codes(&[(
            "/a.ts",
            &format!("{MAYBE}function f(a: string, b?: string): void {{}}\nf(\"x\", maybe);\n")
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn a_defaulted_parameter_accepts_undefined() {
    // The other disjunct — `declaration.Initializer() != nil`. A parameter with
    // a default is optional at the call site even though it has no `?`.
    assert_eq!(
        codes(&[(
            "/a.ts",
            &format!(
                "{MAYBE}function g(a: string, b: string = \"d\"): void {{}}\ng(\"x\", maybe);\n"
            )
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn a_required_parameter_still_rejects_undefined() {
    // The control, and the reason the two above are not vacuous: without it
    // they would pass if TS2345 stopped firing altogether.
    assert_eq!(
        codes(&[(
            "/a.ts",
            &format!("{MAYBE}function h(a: string, b: string): void {{}}\nh(\"x\", maybe);\n")
        )]),
        vec!["TS2345".to_string()]
    );
}

#[test]
fn a_new_expressions_optional_parameter_accepts_undefined() {
    // The `new` arm reaches the same check through `sole_constructor_parameters`
    // and had the same defect; `resolveNewExpression` shares
    // `checkApplicableSignature` upstream.
    assert_eq!(
        codes(&[(
            "/a.ts",
            &format!(
                "{MAYBE}class C {{ constructor(a: string, b?: string) {{}} }}\nnew C(\"x\", maybe);\n"
            )
        )]),
        Vec::<String>::new()
    );
}

// ---------------------------------------------------------------------------
// TS1016 — a required parameter after an optional one
//
// `checkGrammarParameterList` (`grammarchecks.go:714`) is
// `seenOptionalParameter && parameter.Initializer == nil`, and the initialiser
// conjunct was missing. It is **not** the same test as the arm above it:
// `seenOptionalParameter` is set by a `?` alone (§288 corrected that one), and
// a parameter that merely has a default is not "required" and so never offends.
// `docs/architecture/checker-notes-diag2.md` §554.
// ---------------------------------------------------------------------------

#[test]
fn a_defaulted_parameter_may_follow_an_optional_one() {
    // `(items, toggle, onClick?, virtualizer?, autoFocus = false)` — the
    // ordinary shape of a React hook signature, and it was an error.
    assert_eq!(
        codes(&[("/a.ts", "export function f(a: string, b?: string, c = false): void {}\n")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_defaulted_parameter_before_a_required_one_is_still_legal() {
    // The other direction of §288's correction: an initialiser does not set
    // `seenOptionalParameter`, so `f(a = 1, b: string)` is legal — and must
    // stay legal, since the fix touches the same loop.
    assert_eq!(
        codes(&[("/a.ts", "export function g(a = 1, b: string): void {}\n")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_genuinely_required_parameter_after_an_optional_one_still_reports() {
    // **The true positive.** Without it the two above pass if TS1016 is deleted.
    assert_eq!(
        codes(&[("/a.ts", "export function h(a: string, b?: string, c: string): void {}\n")]),
        vec!["TS1016".to_string()]
    );
}

// ---------------------------------------------------------------------------
// The true positives for this session's other suppressions
//
// Each fix above and in `cross_file_aliases.rs` / `jsx_namespace.rs` removes a
// diagnostic. These are the paired cases that keep the rules honest, written at
// the diagnostic level rather than the type level so they fail if the rule is
// removed outright.
// ---------------------------------------------------------------------------

#[test]
fn a_missing_member_of_a_module_object_still_reports() {
    // Pairs with §400 (`symbolIsValue`'s alias half). That fix made every
    // export **specifier** a reachable member; a name the module does not
    // export at all must still be TS2339.
    assert_eq!(
        codes(&[
            ("/m.ts", "const inner: number = 1;\nexport { inner as outer };\n"),
            ("/a.ts", "import * as p from \"./m\";\nexport const v = p.nope;\n"),
        ]),
        vec!["TS2339".to_string()]
    );
}

#[test]
fn a_block_scoped_globalthis_member_records_a_known_divergence() {
    // **This is the true positive for §173's `globalThis` arm, and it does not
    // pass.** Recorded rather than dropped, because a suppression whose paired
    // positive cannot be written is exactly the thing worth writing down.
    //
    // Upstream silences TS2339 for a *missing* member of `globalThis` and
    // reports it for one that **is** a global and is block-scoped — `let`,
    // `const`, `class`, `enum` live in the global *scope* without being
    // properties of the global *object* (`checker.go:11337-11340`,
    // `SymbolFlagsBlockScoped`). So upstream answers `TS2339` here.
    //
    // **The guard added in §173 is not what silences it**, measured by deleting
    // the guard and re-running this fixture: still nothing. `typeof globalThis`
    // is minted by §33 of `checker-notes-narrow.md` when the *name* fails to
    // resolve, and a minted type carries no members table — so
    // `declared_members_are_complete` declines before the arm is reached. The
    // case was never reported, before §173 or after.
    //
    // The consequence is that the block-scoped branch of
    // `global_this_member_is_not_reported` is **unreachable today**. It is kept
    // because it is upstream's rule and because it becomes live the moment §33
    // grows a members table — which is the falsifier for this test: when that
    // lands, this assertion flips to `["TS2339"]` and the divergence closes.
    assert_eq!(
        codes(&[
            ("/g.ts", "let blockScoped = 1;\n"),
            ("/a.ts", "export {};\nexport const q = globalThis.blockScoped;\n"),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_umd_global_in_a_value_position_still_reports() {
    // Pairs with §502's TS2686 arm. The heritage exemption is about *type*
    // positions; a UMD global read as a value from inside a module is exactly
    // what the rule exists for.
    //
    // `/umd.d.ts` declares **only** the UMD name, so every declaration of the
    // symbol is a `NamespaceExportDeclaration` and upstream's `core.Every`
    // (`checker.go:1843`) holds — which is also the true positive for that
    // half of §502.
    assert_eq!(
        codes(&[
            ("/umd.d.ts", "export {};\nexport as namespace MyUmd;\n"),
            ("/a.ts", "export {};\nexport const q = MyUmd;\n"),
        ]),
        vec!["TS2686".to_string()]
    );
}
