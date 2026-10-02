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
//! The original fixes left the conformance suites byte-identical, making a
//! real repository their only instrument. Later flow fixes also affect corpus
//! types; the diagnostic pairs still prevent a suppression from passing merely
//! by disabling the rule.
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

// ---------------------------------------------------------------------------
// TS2438 — `Import name cannot be '{0}'.`
//
// `checkTypeNameIsReserved` has six callers upstream (`checker.go:2623`,
// `:4999`, `:5488`, `:6879`, `:10454`, `:10459`) and the `Import_name_cannot_be_0`
// one is `checkImportEqualsDeclaration` (`:5488`) — an **internal** module
// reference whose target has a type meaning. This port had it on `ImportClause`,
// `NamespaceImport` and `ImportSpecifier`, three kinds upstream never reaches
// with this message, so every `import { boolean } from "drizzle-orm/pg-core"`
// was an error: 31 on a 22-package repository against `tsc`'s zero.
//
// | mutation | reddens |
// |---|---|
// | restore the `ImportSpecifier` arm | [`a_named_import_of_a_reserved_type_name_is_silent`] |
// | delete the `ImportEqualsDeclaration` arm | [`an_import_equals_of_a_reserved_type_name_still_reports`] |
// | drop the `getSymbolFlags & TYPE` gate | [`an_import_equals_of_a_value_is_silent`] |
//
// `docs/architecture/checker-notes-diag2.md` §660.
// ---------------------------------------------------------------------------

#[test]
fn a_named_import_of_a_reserved_type_name_is_silent() {
    // A schema library exporting column constructors called `boolean` and
    // `bigint` is the ordinary case, not an exotic one.
    assert_eq!(
        codes(&[
            ("/m.ts", "export function boolean(): void {}\nexport function bigint(): void {}\n"),
            (
                "/a.ts",
                "import { boolean, bigint } from \"./m\";\nexport const x = [boolean, bigint];\n"
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn an_import_equals_of_a_reserved_type_name_still_reports() {
    // **The true positive**, and it is the only shape upstream fires on:
    // an internal module reference whose target has a *type* meaning.
    //
    // It did not fire on the first attempt at this fix, because
    // `Checker::resolve_alias` declines a qualified module reference — a
    // decline about *printing*. Resolving it through `resolve_entity_name`
    // instead is what keeps the rule alive rather than trading one wrong answer
    // for silence.
    assert_eq!(
        codes(&[(
            "/b.ts",
            "namespace N { export type SomeType = string; }\nimport boolean = N.SomeType;\nexport type Q = boolean;\n"
        )]),
        vec!["TS2438".to_string()]
    );
}

#[test]
fn an_import_equals_of_a_value_is_silent() {
    // `getSymbolFlags(target)&SymbolFlagsType != 0` (`checker.go:5487`). An
    // alias to something with no type meaning is not a reserved *type* name,
    // however it is spelled.
    assert_eq!(
        codes(&[(
            "/c.ts",
            "namespace N { export const someValue = 1; }\nimport boolean = N.someValue;\nexport const q = boolean;\n"
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn a_reserved_type_alias_name_still_reports() {
    // The neighbouring caller (`checker.go:6879`), untouched by this change and
    // asserted so: moving the import arm must not disturb the four that were
    // already right.
    //
    // The expectation here was first written as `[TS2456, TS2457]` from
    // memory and corrected to what `tsc` actually reports on this fixture —
    // TS2457 alone, which is also what this port reports. The port was right
    // and the guess was not.
    assert_eq!(codes(&[("/d.ts", "export type boolean = string;\n")]), vec!["TS2457".to_string()]);
}

// ---------------------------------------------------------------------------
// TS2454 and the exhaustive switch
//
// `isReachableFlowNodeWorker` (`flow.go:2572`) and `getTypeAtFlowBranchLabel`
// (`:1292`) both drop the **bypass** antecedent — the "no clause matched" edge,
// which the binder records with an empty clause range — when the switch covers
// every value of its discriminant. Without it every exhaustive switch left the
// pre-switch `undefined` in the join, and `let x: T` assigned in every case was
// "used before being assigned": 10 on a 22-package repository.
//
// | mutation | reddens |
// |---|---|
// | drop the bypass skip in `get_type_at_flow_branch_label` | [`an_exhaustive_switch_definitely_assigns`] |
// | make `is_exhaustive_switch_statement` return `true` unconditionally | [`a_non_exhaustive_switch_still_reports`] and [`a_switch_over_a_non_literal_still_reports`] |
//
// `docs/architecture/checker-notes-diag2.md` §686.
// ---------------------------------------------------------------------------

#[test]
fn an_exhaustive_switch_definitely_assigns() {
    // Every member of the union has a case, so there is no path on which the
    // variable is unassigned.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "export function f(i: \"day\" | \"week\" | \"month\"): string {\n  let g: string;\n  switch (i) {\n    case \"day\": g = \"d\"; break;\n    case \"week\": g = \"w\"; break;\n    case \"month\": g = \"m\"; break;\n  }\n  return g;\n}\n"
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn a_non_exhaustive_switch_still_reports() {
    // **The true positive.** One member of the union has no case, so the bypass
    // edge is real and the variable genuinely may be unassigned.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "export function g(i: \"day\" | \"week\"): string {\n  let x: string;\n  switch (i) {\n    case \"day\": x = \"d\"; break;\n  }\n  return x;\n}\n"
        )]),
        vec!["TS2454".to_string()]
    );
}

#[test]
fn normal_try_finally_completion_definitely_assigns() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare function compute(): number;
             declare function cleanup(): void;
             function f() {
                 let result: number;
                 try { result = compute(); } finally { cleanup(); }
                 return result;
             }"
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn an_unassigned_catch_path_after_finally_still_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare function compute(): number;
             function f() {
                 let result: number;
                 try { result = compute(); } catch {} finally {}
                 return result;
             }"
        )]),
        vec!["TS2454".to_string()]
    );
}

#[test]
fn an_exception_path_inside_finally_still_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare function compute(): number;
             function f() {
                 let result: number;
                 try { result = compute(); } finally { result; }
             }"
        )]),
        vec!["TS2454".to_string()]
    );
}

#[test]
fn a_switch_over_a_non_literal_still_reports() {
    // `isLiteralType(t)` (`flow.go:1968`). A `string` discriminant can never be
    // covered by a finite set of cases, so exhaustiveness must not be claimed —
    // and this is the arm that a "return true" mutation walks straight past.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "export function h(i: string): string {\n  let x: string;\n  switch (i) {\n    case \"day\": x = \"d\"; break;\n    case \"week\": x = \"w\"; break;\n  }\n  return x;\n}\n"
        )]),
        vec!["TS2454".to_string()]
    );
}

#[test]
fn a_switch_with_a_default_is_unaffected() {
    // A `default` clause means the binder builds no bypass edge at all, so this
    // path never consults exhaustiveness. Asserted so that a future change to
    // the clause-range test cannot quietly start reporting here.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "export function k(i: \"day\" | \"week\"): string {\n  let x: string;\n  switch (i) {\n    case \"day\": x = \"d\"; break;\n    default: x = \"o\";\n  }\n  return x;\n}\n"
        )]),
        Vec::<String>::new()
    );
}

// ---------------------------------------------------------------------------
//
// # An optional chain asks the nullable question of an already-stripped receiver
//
// `checkPropertyAccessExpression` (`checker.go:11249`) sends a chain link to
// `checkPropertyAccessChain`, which hands `checkNonNullType` the answer of
// `getOptionalExpressionType` (`checker.go:29064`) rather than the receiver's
// own type. At a chain **root** that is `getNonNullableType`, so no nullable
// fact survives to report on; at an inner link it is
// `removeOptionalTypeMarker`, which takes back exactly what the chain added.
//
// This port reported on the receiver unconditionally, so **every** `a?.b` in a
// real repository produced TS18048 — 117 of the unexpected diagnostics in
// `conformance/controlFlowOptionalChain` alone.
//
// # The marker is subtracted by identity, not by flag
//
// Upstream's marker is an `undefined` distinct from the real one; this port has
// a single `undefined` (`checker-notes-nnaccess.md` §2). Filtering `undefined`
// out of an inner link's receiver therefore removes genuine ones too, and
// `privateIdentifierChain.1` measured it: `this?.a.#b` with `a?: A` must still
// report and stopped. The link's pre-union type is remembered instead
// (`Checker::pre_optional_marker`), which subtracts the same union member
// upstream subtracts.
//
// # The mutations
//
// | # | mutation | reddens |
// |---|---|---|
// | 1 | drop the `is_chain_root` early return | all four silence tests, plus the two chain true positives — which gain a *second* code |
// | 2 | inner link returns the receiver type unchanged | [`a_deeper_link_is_silent_when_only_the_marker_is_undefined`] and [`a_null_behind_a_chain_still_reports`] |
// | 3 | subtract the marker by filtering the `UNDEFINED` flag | [`a_genuine_undefined_behind_a_chain_still_reports`] **only** |
// | 3b | subtract every nullable at an inner link | that one **and** [`a_null_behind_a_chain_still_reports`] |
// | 4 | delete the receiver rule outright | the four true positives, and nothing else |
//
// Rows 3 and 3b are separate because they redden different tests, and the first
// draft of this table claimed row 3 reddened both. It does not: a filter that
// removes `UNDEFINED` leaves a `null` alone, so the `null` test cannot tell that
// mutation from the fix. Only row 3b can, which is why it is written down.
//
// # Two fixtures here were vacuous on the first run
//
// [`a_chain_link_does_not_report_on_the_marker`] and
// [`an_element_access_chain_root_is_silent_too`] were written with `string[]`,
// mirroring the real repository. This harness loads no lib, so the array type
// answered `errorType` and the rule declined for a reason that had nothing to do
// with chains — both stayed green under mutation 1. Rewritten to lib-free
// shapes, both redden. Every silence test in this file is only as good as its
// fixture actually reaching the rule.
// `docs/architecture/checker-notes-nnaccess.md` §5.

/// `stage?.toLowerCase()` — the receiver of a chain root.
#[test]
fn a_chain_root_does_not_report_on_its_receiver() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const stage: string | undefined;\nexport const s = stage?.toLowerCase();\n"
        )]),
        Vec::<String>::new()
    );
}

/// `result.logs?.map(…)` — the receiver is itself a property access, and it is
/// the shape that produced the report on a real repository.
#[test]
fn a_chain_link_does_not_report_on_the_marker() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const result: { logs?: { m(): number } };\nexport const m = result.logs?.m();\n"
        )]),
        Vec::<String>::new()
    );
}

/// `chain.a?.b.c` — the `.c` link's receiver is `{ c: number } | undefined`
/// where the `undefined` is **only** the propagated marker.
#[test]
fn a_deeper_link_is_silent_when_only_the_marker_is_undefined() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const chain: { a?: { b: { c: number } } };\nexport const c = chain.a?.b.c;\n"
        )]),
        Vec::<String>::new()
    );
}

/// An element access root, `arr?.[0]`, which travels the twin in `indexed.rs`.
#[test]
fn an_element_access_chain_root_is_silent_too() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const arr: { [k: number]: number } | undefined;\nexport const e = arr?.[0];\n"
        )]),
        Vec::<String>::new()
    );
}

/// **True positive.** No chain at all: the rule must still fire.
#[test]
fn an_unchained_possibly_undefined_receiver_still_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const nope: string | undefined;\nexport const s = nope.toLowerCase();\n"
        )]),
        vec!["TS18048".to_string()]
    );
}

/// **True positive.** A chain whose inner link carries a *genuine* `undefined`
/// — `b?: { c }` — reports, because only the marker is subtracted.
#[test]
fn a_genuine_undefined_behind_a_chain_still_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const g: { a?: { b?: { c: number } } };\nexport const c = g.a?.b.c;\n"
        )]),
        vec!["TS18048".to_string()]
    );
}

/// **True positive.** `null` is never the marker, so an inner link over a
/// nullable-by-`null` property keeps reporting — the arm a flag filter that
/// only removes `UNDEFINED` would still pass, and a filter of all nullables
/// would not.
#[test]
fn a_null_behind_a_chain_still_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const n: { a?: { b: { c: number } | null } };\nexport const c = n.a?.b.c;\n"
        )]),
        vec!["TS18047".to_string()]
    );
}

/// **True positive.** A parenthesis ends the chain — upstream's
/// `NodeFlagsOptionalChain` does not propagate through one — so the access on
/// `(p.a?.b)` is an ordinary nullable receiver and reports. The message is the
/// unnamed twin, because `entityNameToString` has no spelling for a
/// parenthesised expression.
#[test]
fn a_parenthesis_ends_the_chain_and_the_access_reports() {
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const p: { a?: { b: number } };\nexport const t = (p.a?.b).toFixed(2);\n"
        )]),
        vec!["TS2532".to_string()]
    );
}

// ---------------------------------------------------------------------------
//
// # `typeof x === "object"` must not manufacture a `null`
//
// `narrowTypeByTypeName` (`flow.go:670`):
//
// ```go
// case "object":
//     if t.flags&TypeFlagsAny != 0 { return t }
//     return c.getUnionType([]*Type{ …nonPrimitive…, …null… })
// ```
//
// A **flags** test. This port wrote `t == self.intrinsics.any` — an identity
// one — and upstream's `errorType` is `newIntrinsicType(TypeFlagsAny, "error")`,
// as are `wildcardType` and `blockedStringType` (`crate::intrinsics`). Every one
// of them is admitted by the flag and excluded by the identity comparison.
//
// # Why a transliteration slip cost three real-repo reports
//
// `errorType` is this port's answer for *anything it cannot resolve yet*, and it
// has far more gaps than upstream does — so a rule that treats `errorType` as an
// ordinary type gets exercised constantly here and almost never there. Falling
// through this arm turned an unresolved receiver into `object | null`, and the
// next property access reported `possibly 'null'` on a value whose type nobody
// had ever established. The `null` came from the narrowing itself.
//
// Three user reports, three unrelated libraries — a Playwright `page.evaluate`
// chain, a `Record`-defaulted generic from a wasm loader, a t3-env `createEnv` —
// all landed here. Two of them I had already attributed to their *producers*
// (`checker-notes-printseam.md` §9) and called blocked on the mapped-type
// subsystem. That attribution was right about the gap and wrong about the
// **diagnostic**: the gap is upstream-shaped and silent, and only this arm
// turned it into an error message.
//
// **A gap must stay a gap.** Any rule that converts `errorType` into a concrete
// type is a candidate for the same bug; this is the one that was found by being
// reported three times.
//
// Corpus: **zero rows changed** on `checker_types` and `diagnostics`,
// per-case — the corpus resolves what it writes, so nothing here reaches the
// arm. Real repository: **6 false positives gone, none new.**
//
// | # | mutation | reddens |
// |---|---|---|
// | 1 | back to `t == self.intrinsics.any` | [`an_unresolved_receiver_is_not_narrowed_into_a_null`] |
// | 2 | drop the guard entirely | that, and [`a_plain_any_receiver_is_left_alone`] |
// | 3 | return `t` for every type, not just the any-flagged ones | nothing in this file — `narrowing.rs::typeof_object_keeps_null_and_drops_the_primitives` |
//
// **Row 3 was written twice and wrong both times, and that is the finding.**
// It first named [`a_genuinely_nullable_receiver_still_reports_after_typeof_object`]:
// returning `t` unchanged *keeps* the `null`, so that test passes under the
// mutation. It then named [`typeof_object_still_removes_the_primitives_it_should`],
// on the theory that leaving `string` in the union would report TS2339 — it does
// not; this port answers a property access on such a union silently.
//
// **No diagnostic test in this file can see mutation 3**, because the mutation
// makes the checker narrow *less* and this file only ever asks whether a
// diagnostic fired. The assertion that catches it is a **type** assertion, and
// it lives in `narrowing.rs`. Worth stating plainly: a suite of "did it report?"
// tests is structurally blind to under-narrowing, and the only way to find that
// out is to run the mutation rather than reason about it. Two predictions, two
// losses.

#[test]
fn an_unresolved_receiver_is_not_narrowed_into_a_null() {
    // `Unresolved` is `errorType`, so the union is too. Before the fix the
    // guard minted `object | null` and the access reported TS18047.
    let codes = codes(&[(
        "/a.ts",
        "declare const x: Unresolved | null;\nexport function f() {\n  if (x && typeof x === \"object\") { return x.foo; }\n  return 0;\n}\n",
    )]);
    assert!(
        codes.contains(&"TS2304".to_string()),
        "the unresolved name must still report: {codes:?}"
    );
    assert!(!codes.contains(&"TS18047".to_string()), "a gap must not become a null: {codes:?}");
}

#[test]
fn a_plain_any_receiver_is_left_alone() {
    // The arm upstream's guard exists for, and the one the identity test did
    // cover. Kept so a rewrite cannot lose it while fixing the flag.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const x: any;\nexport function f() {\n  if (typeof x === \"object\") { return x.foo; }\n  return 0;\n}\n"
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn a_genuinely_nullable_receiver_still_reports_after_typeof_object() {
    // **True positive, and it is the whole point of the arm being there.**
    // `typeof null === "object"`, so narrowing a real union by `=== "object"`
    // *keeps* `null` — upstream builds `object | null` deliberately. A guard
    // that returned `t` for every type would silence this.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const x: { a: number } | null;\nexport function f() {\n  if (typeof x === \"object\") { return x.a; }\n  return 0;\n}\n"
        )]),
        vec!["TS18047".to_string()]
    );
}

#[test]
fn a_truthiness_guard_still_removes_the_null_it_should() {
    // The pair to the test above: with `x &&` in front, the `null` is gone and
    // nothing reports. This is what the real code was written to do, and it is
    // the assertion that fails if a future change over-corrects by making the
    // `object` arm drop `null` unconditionally.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const x: { a: number } | null;\nexport function f() {\n  if (x && typeof x === \"object\") { return x.a; }\n  return 0;\n}\n"
        )]),
        Vec::<String>::new()
    );
}

#[test]
fn typeof_object_still_removes_the_primitives_it_should() {
    // **True positive.** The arm does two things: it keeps `null` (because
    // `typeof null === "object"`) and it drops everything that is not
    // object-like. A guard that returned `t` for every type would leave
    // `string` in the union, and `x.a` would report TS2339 on it — so the codes
    // here name both halves of the arm at once.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "declare const x: string | { a: number } | null;\nexport function f() {\n  if (typeof x === \"object\") { return x.a; }\n  return 0;\n}\n"
        )]),
        vec!["TS18047".to_string()]
    );
}
