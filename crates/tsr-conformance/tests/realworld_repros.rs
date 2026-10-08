//! Real-world parity repros (`tsr-2zk.914`, lane r4-realworld).
//!
//! Each test is a minimal standalone reduction of a diagnostic difference
//! between TSR and native tsgo on TypeScript's own `src/jsTyping` and
//! `src/typingsInstallerCore` projects (`types: []`). The expected list is
//! native's output: pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb, run
//! on the same source with `strict`, `target`/`lib` `es2020`. Every test is
//! `#[ignore]`d with the root cause it reproduces; remove the attribute in the
//! commit that ports the fix. Method, counts and native/TSR owners:
//! `docs/parity/notes/r4-realworld.md`.

use tsr_conformance::{TestCase, diagnostics_suite};

/// `(line, column, code)` of every reported diagnostic, 1-based, sorted.
fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let case = TestCase::parse("probe/realworld", "a.ts", source);
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

const PRELUDE: &str = "// @strict: true\n// @target: es2020\n// @lib: es2020\n";

fn check(body: &str, expected: &[(u32, u32, u32)]) {
    let source = format!("{PRELUDE}{body}");
    assert_eq!(diagnostics(&source), expected, "source:\n{source}");
}

// The harness strips directive lines, so positions count from the first line
// of `body`.

#[test]
#[ignore = "tsr-2zk.914: union origin slice gate answers errorType for `NamedUnion | Object`"]
fn named_union_alias_with_object_constituent_is_a_real_union() {
    // `unions.rs` `build_origin_union`'s §53 gate returns `intrinsics.error`
    // for a union whose origin has an alias-named union entry beside an
    // object entry; native `getUnionTypeWorker` (checker.go:25653) keeps the
    // origin. The error type then silences checks and relations answer
    // Unknown (array subtype reduction, type-predicate narrowing).
    check(
        "interface Node { readonly parent: Node }
interface A extends Node {}
interface B extends Node {}
interface C extends Node {}
type XY = A | B;
declare const md: XY | C;
export const t1: number = md;
",
        &[(7, 14, 2322)],
    );
}

#[test]
#[ignore = "tsr-2zk.914: union origin slice gate breaks type-predicate narrowing to ModuleBlock"]
fn predicate_narrowing_through_named_union_parent_chain() {
    // The jsTyping shape: `ModuleDeclaration.parent: ModuleBody | SourceFile`
    // is `NamedUnion | Object`, so `ModuleBlock <: Node` is undecidable and
    // `getNarrowedType` keeps `Node` (TS2339 'statements' on 'Node').
    check(
        "interface Node { readonly parent: Node }
interface Identifier extends Node {}
interface SourceFile extends Node {}
interface ModuleDeclaration extends Node { readonly parent: ModuleBody | SourceFile }
type NamespaceBody = ModuleBlock | ModuleDeclaration;
type JSDocNamespaceBody = Identifier | ModuleDeclaration;
type ModuleBody = NamespaceBody | JSDocNamespaceBody;
interface ModuleBlock extends Node { readonly parent: ModuleDeclaration; readonly statements: Node[] }
declare function isModuleBlock(n: Node): n is ModuleBlock;
export function f(p: Node) { if (isModuleBlock(p)) { return p.statements; } }
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.914: any source to object target under (strict)subtype answers Unknown"]
fn any_property_source_is_not_a_subtype_of_an_object_property() {
    // `relater.rs` `is_related_to_with_flags` falls through to Unknown for
    // `any -> Node` under Subtype/StrictSubtype; native `isSimpleTypeRelatedTo`
    // (relater.go:209-262) and `structuredTypeRelatedTo` answer False, so
    // `removeSubtypes` drops `Node` and the literal is `SourceFile[]`.
    check(
        "interface Node { readonly parent: Node }
interface SourceFile extends Node { readonly parent: any }
declare const n: Node; declare const sf: SourceFile;
export const t: number = [sf, n];
",
        &[(4, 14, 2322)],
    );
}

#[test]
fn logical_assignment_narrows_its_target() {
    // Native `getAssignedType` (flow.go:2288) ->
    // `getAssignedTypeOfBinaryExpression` (flow.go:2314) answers the right
    // operand for `??=`, `||=`, `&&=` as for `=` (`flow.rs`
    // `get_initial_or_assigned_type`, tsr-2zk.923).
    check(
        "export function f(a: string | undefined, b: string) { a ??= b; return a.length; }
export function g(a: string | undefined, b: string) { a ||= b; return a.length; }
export function h(a?: string) { a ??= \"\"; return a.length; }
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.914: overload resolution loses the contextual tuple of a nested generic call"]
fn overloaded_generic_call_gives_nested_map_callback_a_tuple_context() {
    // `new Map(xs.map(x => [x, 1]))`: with two generic overloads the inner
    // array literal must still be contextually typed by `readonly [K, V]`
    // (native `chooseOverload` re-checks the argument per candidate).
    check(
        "declare const files: string[];
declare function mk<K, V>(e?: readonly (readonly [K, V])[] | null): [K, V];
declare function mk<K, V>(e?: Iterable<readonly [K, V]> | null): [K, V];
export const a = mk(files.map(f => [f, 1]));
export const b = new Map(files.map((f, i) => [f, i]));
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.914: inference to `T & X` is not demoted to NakedTypeVariable priority"]
fn intersection_target_inference_has_naked_type_variable_priority() {
    // Native `inferToMultipleTypes` (inference.go:523) infers to the single
    // naked variable of an intersection at `InferencePriorityNakedTypeVariable`,
    // so the callback's `string` loses to the argument's `string | undefined`.
    // TSR keeps both at one priority and picks the contravariant `string`
    // (`visitNode(x, visitor, isExpression)` -> TS2769 in the project).
    check(
        "interface F { f?: 1 }
declare function g<T>(a: T, cb: (n: T & F) => void): T;
declare const x: string | undefined; declare function cb(n: string): void;
export const r: number = g(x, cb);
",
        &[(4, 14, 2322)],
    );
}

#[test]
fn mapped_type_over_union_with_generic_base_member() {
    // `Mutable<HasContainerFlags>` in binder.ts; `Mutable<OLE>` alone is fine.
    check(
        "interface Node { kind: number }
interface Base<T> extends Node { p: T }
interface OLE extends Base<number> {}
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
declare const x: Mutable<OLE | Node>;
export const n: Node = x;
export const k: string = x.kind;
",
        &[(7, 14, 2322)],
    );
}

#[test]
#[ignore = "tsr-2zk.914: alias type-argument variance applied to a union alias"]
fn union_alias_relates_structurally_not_by_variance() {
    // Native `structuredTypeRelatedToWorker` probes alias variance only for
    // Object|Conditional sources; `SearchResult<T>` is a union alias, so
    // `SearchResult<undefined> -> SearchResult<string>` is structural (true).
    check(
        "type SearchResult<T> = { value: T | undefined } | undefined;
declare const x: SearchResult<undefined>;
export const y: SearchResult<string> = x;
",
        &[],
    );
}

#[test]
fn mapped_type_over_array_intersection_maps_each_constituent() {
    // `Readonly<PathPathComponents>` (`Path[] & { brand }`): native
    // `isArrayOrTupleOrIntersection` is false (not every constituent is an
    // array), so `resolveMappedTypeMembers` enumerates the intersection's
    // properties, `length` among them (`docs/parity/notes/r4-mapped.md` §2).
    check(
        "type P = string[] & { __brand: any };
declare const p: Readonly<P>;
export const n: number = p.length;
export const s: string = p.length;
",
        &[(4, 14, 2322)],
    );
}

#[test]
#[ignore = "tsr-2zk.914: destructuring does not read the narrowed property type"]
fn destructured_property_keeps_reference_narrowing() {
    // tsbuildPublic.ts `disableCache`: native `getFlowTypeOfDestructuring`
    // narrows the synthetic `s.c` reference at the binding element.
    check(
        "export function f(s: { c?: { x: number } }) { if (!s.c) return; const { c } = s; return c.x; }
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.914: closure narrowing of a never-reassigned destructured let"]
fn destructured_let_narrowing_reaches_closures() {
    // program.ts `let { oldProgram } = ...`: a binding-element `let` that is
    // never reassigned keeps its narrowing inside a later arrow (native
    // `isSymbolAssigned`/constant-reference rule); the `let p = o.p` form
    // already agrees.
    check(
        "export function outer(o: { p?: { x: number } }) {
    let { p } = o;
    function inner() { if (!p) return; [1].map(() => p.x); }
    return inner;
}
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.914: comparable relation rejects weak targets and optional source members"]
fn assertion_to_weak_or_optional_target_is_comparable() {
    // `(node as TracingNode)` and `{ annotatedNodes } as EmitNode`: native
    // skips the weak-type check under comparability (relater.go:2675) and
    // ignores optionality there; no TS2352.
    check(
        "declare const n: { kind: number };
export const a = n as { tracingPath?: string };
export const b = { a: [1] } as { a?: number[]; c: number };
",
        &[],
    );
}

#[test]
fn missing_import_suggests_re_exported_name() {
    // compiler/*.ts import `Diagnostics` from the `_namespaces/ts` barrel;
    // native TS2724 suggests `Diagnostic` found through `export *`.
    let source = "// @strict: true
// @filename: b.ts
export interface Diagnostic { code: number }
// @filename: ns.ts
export * from \"./b\";
// @filename: a.ts
import { Diagnostics } from \"./ns\";
export { Diagnostics };
";
    assert_eq!(diagnostics(source), [(1, 10, 2724)]);
}

#[test]
fn unresolved_node_core_module_reports_install_types_hint() {
    // Native `getCannotResolveModuleNameErrorForSpecificModule`
    // (checker.go:15110) substitutes TS2591 for TS2307 on an unresolved Node
    // core module (TSR `report_module_not_found`, tsr-2zk.933).
    check(
        "export let fs: typeof import(\"fs\");
",
        &[(1, 30, 2591)],
    );
}

// ---------------------------------------------------------------------------
// r4-realworld2 (`tsr-2zk.938`): TypeScript's own `src/compiler` and
// `src/services`. Method and counts: `docs/parity/notes/r4-realworld2.md`.

#[test]
#[ignore = "tsr-2zk.938: type alias reached through a namespace-qualified name is opaque to relations"]
fn namespace_qualified_alias_reference_is_the_aliased_type() {
    // Native `getTypeFromTypeReference` -> `getTypeReferenceType` ->
    // `getTypeFromTypeAliasReference` (checker.go) answers the alias's
    // declared type for `a.TA` exactly as for `TA`; TSR answers a type that
    // relates to nothing and prints `a.TA`. services: `textChanges.TypeAnnotatable`,
    // `codefix.ImportOrRequireAliasDeclaration`, `FindAllReferences.Entry`.
    check(
        "namespace a { export type TA = string | number; }
declare const v: a.TA;
export const n: string | number = v;
export const m: boolean = v;
",
        &[(4, 14, 2322)],
    );
}

#[test]
#[ignore = "tsr-2zk.938: enum literal property widened under a union contextual type reached indirectly"]
fn enum_literal_member_keeps_literal_under_indirect_union_context() {
    // Native `checkObjectLiteral` -> `getWidenedLiteralLikeTypeForContextualType`
    // keeps `K.R` because `isLiteralOfContextualType` sees `K.R | K.T` through
    // `getContextualTypeForObjectLiteralElement`; TSR widens to `K` when the
    // literal's contextual union comes via an array element, a conditional
    // branch or a rest argument. services: `{ kind: ChangeKind.Text, ... }`
    // pushed to `Change[]`, `{ type: DefinitionKind.Symbol, ... }`.
    check(
        "enum K { R, T }
type C = { kind: K.R } | { kind: K.T; text: string };
declare const cs: C[];
cs.push({ kind: K.R });
export function f(): C[] { return [{ kind: K.R }]; }
export function g(b: boolean): C | undefined { return b ? undefined : { kind: K.R }; }
",
        &[],
    );
}

#[test]
fn branded_string_intersection_has_string_number_index() {
    // Native `getIndexInfosOfType` reads `getReducedApparentType`, and
    // `getApparentTypeOfIntersectionType` maps `string` to `String`, whose
    // `[index: number]: string` applies. TSR `get_index_infos_of_type`
    // (index_signatures.rs) recursed into the raw `string` constituent and
    // found none; r4-index's TS7053 arm then reported it (compiler `Path`).
    // Fixed by r4-index3 (tsr-2zk.946).
    check(
        "type Path = string & { __pathBrand: any };
declare const p: Path;
declare const i: number;
export const c: number = p[i];
",
        &[(4, 14, 2322)],
    );
}

#[test]
fn index_signature_with_error_valued_info_is_not_an_implicit_any_access() {
    // Native `getPropertyTypeForIndexType` (checker.go:27129-27184) reaches
    // TS7053 only when no index info applies. Here the string index applies;
    // its value `V | boolean` is cause 1's `errorType` in TSR, and
    // `index_signatures.rs` `index_infos_of_declaration` dropped an info whose
    // value was `errorType`, so `report_implicit_any_element_access` found no
    // applicable info. compiler: `options[name]` on
    // `CompilerOptions`/`OptionsBase` (21 lines). Fixed by r4-index3
    // (tsr-2zk.945): the info is kept, as `getIndexInfosOfIndexSymbol` does.
    check(
        "type V = string | number;
interface O { [k: string]: V | boolean; }
export function f(o: O, n: string) { return o[n]; }
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.938: mapped type over an intersection loses its members once an object literal is related to it"]
fn mapped_intersection_keeps_members_after_object_literal_relation() {
    // `declare const v: W; v.u` resolves when it is checked before any
    // object literal is related to `W`; after `const w: W = { u: true }`,
    // `w.u`, `(w as W).u` and a later `v.u` all report TS2339. So it is not
    // flow narrowing but the member table of the mapped type over an
    // intersection, which depends on what was asked first. Native
    // `resolveMappedTypeMembers` / `instantiateMappedType`
    // (`isArrayOrTupleOrIntersection` arm) has no such order. TSR:
    // `mapped.rs` member resolution under the relater. services:
    // `Mutable<ImportsCollection & { useRequire: boolean }>` in
    // importFixes.ts (15 lines).
    check(
        "type M<T> = { [K in keyof T]: T[K] };
const w: M<{ d?: string } & { u: boolean }> = { u: true };
export const z = w.u;
",
        &[],
    );
}

#[test]
#[ignore = "tsr-2zk.938: assignment narrowing rejects a literal with a context-typed arrow in an optional function property"]
fn assignment_narrowing_accepts_context_typed_arrow_in_optional_property() {
    // Native `getAssignmentReducedType` (flow.go) narrows `H | undefined` to
    // `H` because the initializer is assignable to `H`; the declaration
    // itself reports nothing in either tool. TSR's `flow.rs`
    // `get_assignment_reduced_type` keeps the declared type: its final
    // assignability guard rejects the initializer's type when an
    // unannotated arrow sits in an optional function-typed property
    // (annotating `x`, or making `d` required, agrees). services.ts
    // `compilerHost` (`directoryExists`/`getDirectories` arrows), 7 lines.
    check(
        "interface H { d?: (x: string) => boolean; }
export function m() { let h: H | undefined = { d: x => true }; return h.d; }
",
        &[],
    );
}
