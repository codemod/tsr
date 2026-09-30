//! What a property access answers on a reference to an alias whose body is a
//! CONDITIONAL. §823's probe, pinned.
//!
//! §822 was built on a guess about this and measured at zero corpus-wide. The
//! guess was that `evaluate_conditional_alias`'s frame could not reach the
//! branch's members; the refutation was that the failing lines never go through
//! that function at all — they are property accesses on a type REFERENCE, and
//! they resolve through `type_reference_targets` on the `bd tsr-4qx` member seam.
//!
//! This file is the probe that should have come first. It asserts what the port
//! answers today, so the next change to this road has a before-picture that is
//! measured rather than recalled.
//!
//! The corpus shape is `recursiveArrayNotCircular`:
//!
//! ```ts
//! type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P }
//! ```
//!
//! and upstream answers `number` for `payload` on
//! `Action<ActionType.Bar, number>` where this port answers `P`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the last expression statement, descending into
/// function bodies so a fixture can introduce type parameters.
fn type_of_last_expression(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = last_expression_statement(parsed.source_file.statements)
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

fn last_expression_statement<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::ExpressionStatement(node) => found = node.expression,
            Statement::Block(block) => {
                found = last_expression_statement(block.statements).or(found);
            }
            Statement::IfStatement(node) => {
                for branch in [node.then_statement, node.else_statement].into_iter().flatten() {
                    found = last_expression_statement(std::slice::from_ref(&branch)).or(found);
                }
            }
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_expression_statement(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

/// The type of the LAST arrow function in the source — §834's probe needs the
/// arrow itself, not the statement around it.
fn last_arrow_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let mut last = None;
    for raw in 0..u32::try_from(parsed.nodes.len()).expect("fits") {
        let id = tsr_ast::NodeId::new(raw);
        if parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ArrowFunction {
            last = Some(id);
        }
    }
    let id = last.expect("the fixture must contain an arrow function");
    let Some(node) = parsed.node_map.get(id) else { return "NO-NODE".to_string() };
    let Ok(expression) = tsr_ast::Expression::try_from(node) else {
        return "NOT-EXPRESSION".to_string();
    };
    let ty = checker.check_expression(expression);
    checker.type_to_string(ty)
}

const ACTION: &str = "enum ActionType { Foo, Bar }\n\
     type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P }\n";

/// The corpus shape, and the line `recursiveArrayNotCircular` gets wrong:
/// upstream answers `number`.
#[test]
fn a_property_of_a_conditional_alias_reference() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.payload;\n");
    // Pinned as it behaves: **`error`**, a gap. Upstream answers `number`.
    //
    // Note the corpus answers `P` rather than `error` for the same shape, and
    // that difference is itself informative: in `recursiveArrayNotCircular` the
    // access goes through a UNION of these references narrowed by
    // `switch (action.type)`, so the `P` arrives on the narrowed-union road. The
    // direct reference declines outright.
    assert_eq!(type_of_last_expression(&source), "error");
}

/// The sibling property fails identically — upstream answers `ActionType.Bar`.
#[test]
fn the_other_property_fails_identically() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.type;\n");
    assert_eq!(type_of_last_expression(&source), "error");
}

/// **The control that localises the defect.** The same member access on an alias
/// whose body is a plain type literal — no conditional — substitutes correctly.
///
/// This is what says the defect is the CONDITIONAL body rather than the member
/// seam: `bd tsr-4qx`'s substitution works, and it is the conditional that gives
/// it nothing to substitute through.
#[test]
fn a_plain_generic_alias_substitutes_its_member() {
    let source = "type Plain<T, P> = { type: T, payload: P };\n\
         declare const a: Plain<string, number>;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "number");
}

/// **§823's mechanism, working.** The same shape with ordinary arguments: the
/// conditional's chosen branch supplies the member table and substitution
/// proceeds exactly as it does for `Plain`.
///
/// This is the test that says §823 is correct even though it moved **zero**
/// corpus lines — every corpus instance is blocked by one of the two defects
/// pinned below, not by this road.
#[test]
fn a_conditional_alias_reference_answers_from_its_branch() {
    let source = "type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };\n\
         declare const a: Action<string, number>;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "number");
}

/// **A LIMIT OF THIS HARNESS, not of the checker — corrected after it was first
/// written up the other way.**
///
/// `Action<ActionType.Bar, number>` answers `error` here while
/// `Action<string, number>` answers `number`, and §823 first recorded that as a
/// checker defect: *"an enum member as a type argument does not resolve"*.
/// **That claim is false**, and the corpus refutes it flatly: **5,723 RIGHT
/// lines carry a dotted enum-member answer** (`ambientEnum1` → `E1.y`,
/// `assignToEnum` → `A.foo`, and so on), and 14,133 RIGHT lines carry a dotted
/// answer of any kind.
///
/// So what fails is this fixture, not the road it was meant to probe. This
/// harness is `Checker::new` over **one file with no `lib.d.ts` and no
/// `ModuleHost`**, and it does not go through `types_producer`'s position rules
/// either — so it is a usable oracle for *"does this arm fire"* and **not** for
/// *"can the port express this"*.
///
/// Kept, with the caveat, because the asymmetry against the test above is still
/// the thing to re-check if anyone touches the argument road — but the next
/// question is what THIS fixture lacks, not what the checker lacks.
#[test]
fn an_enum_argument_fails_in_this_harness_only() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.payload;\n");
    assert_eq!(type_of_last_expression(&source), "error");
}

/// **Blocker 2, pinned: the alias-declared road answers the UNINSTANTIATED
/// branch.**
///
/// Reached through an intermediate alias, the same reference answers a bare `P`
/// rather than `number` — so `in_alias_declared_position`'s road
/// (`declared.rs:2143`) evaluates the conditional and hands back a branch whose
/// parameters were never substituted. This is the road that gives the corpus its
/// bare `P`/`T`. Unlike the enum fixture above, this one is corroborated by the
/// corpus: `recursiveArrayNotCircular`'s five wrong lines answer exactly this
/// bare `P`/`T`, so the road really does hand back an uninstantiated branch.
#[test]
fn blocker_the_alias_declared_road_does_not_substitute() {
    let source = "type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };\n\
         type Bar = Action<string, number>;\n\
         declare const a: Bar;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "P");
}

/// §830: a GENERIC METHOD reached through an instantiated reference keeps its own
/// type parameters — the class's substitute, the method's shadow and survive.
///
/// `instantiate_for_reference` mapped the class's parameters by NAME, so `foo`'s
/// own `U` was substituted with the class's argument. +214 lines corpus-wide.
#[test]
fn a_generic_method_keeps_its_own_type_parameters() {
    let source = "class C<T, U> { foo<U>(t: T, u: U): T { return t; } }\n\
         declare const c: C<string, number>;\n\
         c.foo;\n";
    assert_eq!(type_of_last_expression(source), "<U>(t: string, u: U) => string");
}

/// §830.1: the same shadowing rule at the PROPERTY spelling —
/// `foo: <U>(t: T, u: U) => T` is the same two sets of parameters written the
/// other way. Measured at **zero** corpus-wide; this test is what says the
/// mechanism is correct rather than merely unexercised.
#[test]
fn a_generic_function_typed_property_keeps_its_own_type_parameters() {
    let source = "class C<T, U> { foo: <U>(t: T, u: U) => T; }\n\
         declare const c: C<string, number>;\n\
         c.foo;\n";
    assert_eq!(type_of_last_expression(source), "<U>(t: string, u: U) => string");
}

/// §830.2: the INHERITED half — a generic method reached through a generic BASE.
/// `instantiate_for_reference` was called without the shadowed names on that road
/// too. Measured at zero corpus-wide; this is what says the arm fires.
#[test]
fn an_inherited_generic_method_keeps_its_own_type_parameters() {
    let source = "class B<T> { m<U>(t: T, u: U): T { return t; } }\n\
         class D extends B<string> { }\n\
         declare const d: D;\n\
         d.m;\n";
    assert_eq!(type_of_last_expression(source), "<U>(t: string, u: U) => string");
}

/// §832: the PROPERTY spelling of an optional function-typed member already
/// answers correctly — the control that localised §832's defect to the METHOD
/// spelling, and the reason placing optionality on the symbol's type was refused.
#[test]
fn an_optional_function_typed_property_carries_undefined() {
    let source = "interface I { f?: () => void; g?: string; }\n\
         declare const i: I;\n\
         i.f;\n";
    assert_eq!(type_of_last_expression(source), "(() => void) | undefined");
}

/// §885 **supersedes §832's refusal.** An optional METHOD's type carries
/// `| undefined`, from `getTypeOfFuncClassEnumModuleWorker`'s tail
/// (`checker.go:16930`) — the same road the optional *property* above takes.
///
/// §832 built this and measured −4 (28 gained, 32 right lines lost), then
/// refused it with the reasoning that *"the correct placement is the
/// property-ACCESS road, which is also where upstream puts it."* **That
/// reasoning was wrong**: upstream puts it on the symbol's type for a method
/// (`checker.go:16930`) and for a property (`addOptionality`), and nowhere on
/// the access road. Re-measured at §885 on a checker many sessions further
/// along, the identical placement gives **+143 W→R against 24 adverse**.
#[test]
fn an_optional_method_carries_undefined() {
    let source = "interface I { f?(): void; g?: string; }\n\
         declare const i: I;\n\
         i.f;\n";
    assert_eq!(type_of_last_expression(source), "(() => void) | undefined");
}

/// §834: an EXPLICITLY WRITTEN type argument reaches the contextually typed
/// parameter. `f<number>(n => n)` answered `(n: unknown) => unknown` before —
/// the FIXING mapper overwriting a type argument the programmer had already
/// decided. +105 corpus lines at zero `RIGHT→WRONG`.
#[test]
fn a_written_type_argument_reaches_the_contextual_parameter() {
    let source = "function f<A>(a: (x: A) => A): void { }\n\
         f<number>(n => n);\n";
    assert_eq!(last_arrow_type(source), "(n: number) => number");
}

/// §834's leg-3 control, and the reason the change is *informing* the fill rather
/// than widening it: with **no** written type argument there is nothing to infer
/// from and upstream's FIXING mapper still applies, so the answer stays `unknown`.
///
/// If this ever moves, §834 has become a guess about inference instead of a read
/// of the written source.
#[test]
fn a_call_without_type_arguments_keeps_the_unknown_fill() {
    let source = "function f<A>(a: (x: A) => A): void { }\n\
         f(n => n);\n";
    assert_eq!(last_arrow_type(source), "(n: unknown) => unknown");
}

/// §835: `x === v` narrows an `unknown` — upstream's `flow.go:581-588`, an arm
/// this port's equality road never had. `filter_type` cannot reach it, because a
/// non-union `unknown` either survives whole or becomes `never`.
#[test]
fn equality_narrows_an_unknown_to_the_compared_value() {
    let source = "declare const u: unknown;\nif (u === 1) { u; }\n";
    assert_eq!(type_of_last_expression(source), "1");
}

/// §835's control: `typeof` narrowing of an `unknown` **already worked**, so the
/// residue in `unknownType2` is neither this arm nor its typeof sibling. Probed
/// before assuming a sibling fix was needed — §830.2's "fix the pair" lesson does
/// not apply when the pair is already correct.
#[test]
fn typeof_already_narrows_an_unknown() {
    let source = "declare const u: unknown;\nif (typeof u === \"string\") { u; }\n";
    assert_eq!(type_of_last_expression(source), "string");
}

/// §886: a UNION contextual type yields the one constituent's call signature.
///
/// `getContextualSignature` (`checker.go:10264`) iterates a union's
/// constituents and collects each one's contextual call signature. With
/// `k?(a: string): number`, §885 makes the contextual type
/// `((a: string) => number) | undefined`; `undefined` contributes no call
/// signature, so exactly one survives and the arrow's parameter is `string`.
///
/// Before §886 a union context answered `None` — the refusal that turned §885's
/// gain into 24 adverse rows in `assignmentCompatBug2` and `objectLitGetterSetter`.
#[test]
fn a_union_contextual_type_yields_its_one_signature() {
    let source = "interface I { k?(a: string): number; }\n\
         const o: I = { k: (a) => 1 };\n";
    assert_eq!(last_arrow_type(source), "(a: string) => number");
}

/// The half §886 leaves refused: TWO constituents offering a call signature.
/// Upstream compares them with `compareSignaturesIdentical` and may build a
/// `createUnionSignature`; neither exists here, so the context declines and the
/// parameter falls back rather than guessing a member.
#[test]
fn two_signature_constituents_still_decline() {
    let source = "type F = ((a: string) => number) | ((a: number) => number);\n\
         const o: F = (a) => 1;\n";
    assert_ne!(last_arrow_type(source), "(a: string) => number");
}

/// §952: a homomorphic IDENTITY mapped type — `{ [P in keyof T]: T[P] }` with
/// any combination of `?` and `readonly` — reads the SOURCE's members and
/// applies the modifiers. Before this, every member of every mapped type
/// gapped.
#[test]
fn a_homomorphic_identity_mapped_type_reads_the_sources_members() {
    // `?` adds optionality, which a READ sees as `| undefined`.
    assert_eq!(
        type_of_last_expression(
            "type P<T> = { [K in keyof T]?: T[K] };\ndeclare let p: P<{ x: string }>;\np.x;"
        ),
        "string | undefined"
    );
    // No modifier is the identity: the source's own member type, unchanged.
    assert_eq!(
        type_of_last_expression(
            "type I<T> = { [K in keyof T]: T[K] };\ndeclare let i: I<{ x: string }>;\ni.x;"
        ),
        "string"
    );
    // `-?` REMOVES optionality, so the source's `| undefined` is stripped.
    assert_eq!(
        type_of_last_expression(
            "type R<T> = { [K in keyof T]-?: T[K] };\ndeclare let r: R<{ x?: string }>;\nr.x;"
        ),
        "string"
    );
}

/// §952's falsifier legs — the two shapes that must keep declining, because
/// answering from the source's members would be a confident WRONG answer rather
/// than a missing one. The same line `record_index_info` draws for a
/// literal-union `Record` key (§785).
#[test]
fn a_mapped_type_that_is_not_the_identity_still_declines() {
    // A TRANSFORMING template needs the template instantiated per key, which
    // this arm deliberately does not build. Answering `string` here where
    // upstream answers `Box<string>` would be worse than gapping.
    assert_eq!(
        type_of_last_expression(
            "type Box<V> = { v: V };\ntype B<T> = { [K in keyof T]: Box<T[K]> };\ndeclare let b: B<{ x: string }>;\nb.x;"
        ),
        "error"
    );
    // A key REMAPPING (`as`) changes the NAMES, which is exactly what reusing
    // the source's member owner cannot express.
    assert_eq!(
        type_of_last_expression(
            "type M<T> = { [K in keyof T as \"z\"]: T[K] };\ndeclare let m: M<{ x: string }>;\nm.x;"
        ),
        "error"
    );
}
