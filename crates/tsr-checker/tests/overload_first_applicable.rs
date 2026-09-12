//! Generic construct OVERLOADS are alternatives, not agreements. §788.
//!
//! The `new` road's generic-candidate walk answered only when **every**
//! candidate produced the same return. That is right for a set that is really
//! one signature seen several ways — `DateConstructor`'s four construct
//! signatures all return `Date` — and wrong for a genuine overload set.
//!
//! `SetConstructor` is two signatures:
//!
//! ```ts
//! new <T = any>(values?: readonly T[] | null): Set<T>;
//! new <T = any>(iterable?: Iterable<T> | null): Set<T>;
//! ```
//!
//! `new Set([0, 1, 2])` fits the first and not the second, so the second
//! answered `errorType`, the agreement walk went false, and the whole call
//! declined to a gap. `compiler/setMethods` is 180 wrong lines against 37 right
//! for that reason, and STATUS §787 had recorded it as blocked on "generic
//! overload selection, 926 lines" — the blocker is real but the *first*
//! applicable candidate is decidable without any of that machinery.
//!
//! Upstream's `chooseOverload` (`checker.go`) takes the first candidate the
//! arguments fit and never consults the rest. This is that rule, restricted to
//! what this port can decide: arity accepts the argument count, and the generic
//! resolution does not answer `errorType`. **Assignability-ranked selection
//! among several fitting candidates stays refused.**
//!
//! The arm is purely additive — it runs only where the agreement walk already
//! declined — so its failure direction is gap→wrong and it can take no right
//! line away. Measured +265 (69 GAP→RIGHT, 196 WRONG→RIGHT) against 6
//! GAP→WRONG, with zero RIGHT→WRONG and zero RIGHT→GAP.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's declaration.
fn type_of(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|list| list.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let symbol = bound
                .symbol_of(declaration.node_id.expect("registered"))
                .expect("the declaration must be bound");
            let id = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

/// The real `SetConstructor` shape, reduced. Both overloads are generic and
/// only the first fits an array argument.
///
/// Two things the real signatures carry are dropped here on purpose, because
/// each is a SEPARATE blocker that would fire before this arm and make the test
/// about something else. Both are refused with their numbers in STATUS §5
/// under §787:
///
/// - the `= any` DEFAULTS, which trigger §44's all-defaulted shortcut in
///   `get_signature_of_named_type` — it instantiates with the defaults and
///   answers `S<any>` before any of this runs;
/// - the `| null` UNION on the parameter, which the union strike-out in
///   `infer_from_types_within` strikes whole, so `T` collects no candidate and
///   BOTH overloads answer `errorType`.
///
/// That those two sit in front of this arm and it still converts 265 corpus
/// lines is the measurement's own comment on how much is stacked here.
const SET_LIKE: &str = "interface Array<T> { length: number }\n\
                        interface ReadonlyArray<T> { length: number }\n\
                        interface Iterable<T> { i: T }\n\
                        interface S<T> { size: number }\n\
                        interface SCtor {\n\
                          new <T>(values: readonly T[]): S<T>;\n\
                          new <T>(iterable: Iterable<T>): S<T>;\n\
                        }\n\
                        declare var SCtor: SCtor;\n";

/// The head case. Before §788 the disagreeing second overload declined the
/// whole call.
#[test]
fn the_first_fitting_overload_answers() {
    assert_eq!(type_of(&format!("{SET_LIKE}const a = new SCtor([0, 1, 2]);"), "a"), "S<number>");
}

/// The agreement walk is not replaced — a set whose candidates agree is still
/// answered by it, and this fixture has no second overload to disagree.
#[test]
fn a_single_generic_candidate_still_answers() {
    let source = "interface Array<T> { length: number }\n\
                  interface ReadonlyArray<T> { length: number }\n\
                  interface S<T> { size: number }\n\
                  interface One { new <T>(v: readonly T[]): S<T>; }\n\
                  declare var One: One;\nconst a = new One([0, 1, 2]);";
    assert_eq!(type_of(source, "a"), "S<number>");
}

/// Arity is what makes the pick decidable without assignability ranking: a
/// candidate that cannot accept the argument count is skipped rather than
/// tried.
#[test]
fn a_candidate_whose_arity_refuses_is_skipped() {
    let source = "interface S<T> { size: number }\n\
                  interface Two {\n\
                    new <T>(a: T, b: T): S<T>;\n\
                    new <T>(a: T): S<T>;\n\
                  }\n\
                  declare var Two: Two;\nconst a = new Two(5);";
    assert_eq!(type_of(source, "a"), "S<number>");
}

/// A set where NO candidate fits stays a gap — the arm answers `errorType`
/// rather than picking something the arguments do not match.
#[test]
fn no_fitting_candidate_stays_a_gap() {
    let source = "interface S<T> { size: number }\n\
                  interface Three { new <T>(a: T, b: T, c: T): S<T>; }\n\
                  declare var Three: Three;\nconst a = new Three();";
    assert_ne!(type_of(source, "a"), "S<number>");
}
