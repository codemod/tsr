//! The type checker.
//!
//! Ported from `internal/checker` at the pinned commit — 60,269 lines upstream,
//! the largest single workstream in the port (`bd tsr-4sc`, PLAN.md §3.2).
//!
//! # What exists today
//!
//! **This crate is a foundation, not a checker.** Stated plainly because the
//! difference matters to anyone reading the conformance table:
//!
//! - [`TypeFlags`](flags::TypeFlags), ported one-for-one including bit positions,
//!   which are load-bearing for union constituent order.
//! - [`TypeStore`](types::TypeStore) and [`TypeId`](types::TypeId), the handle
//!   model [ADR-0013](../../../docs/adr/0013-checker-memoisation.md) settled.
//! - The [`intrinsics`], created in upstream's order, including the several
//!   distinct types that print the same string.
//! - Literal and keyword expression types, with interning.
//! - [`type_to_string`](Checker::type_to_string) for those types, matching the
//!   form a `.types` baseline compares against.
//! - [`get_type_of_symbol`](Checker::get_type_of_symbol) for variables,
//!   parameters and properties, with the lazy memo and circularity detection.
//! - [`get_type_from_type_node`](Checker::get_type_from_type_node) for the
//!   keyword, literal and parenthesised type forms.
//! - Literal **freshness** — the `freshType`/`regularType` pair that makes
//!   `const x = "a"` be `"a"` while `let x = "a"` is `string`.
//!
//! # What does not exist
//!
//! Everything else, and the list is the honest measure of distance:
//!
//! - **No narrowing.** An identifier reference yields the *declared* type;
//!   upstream reaches `getFlowTypeOfReference` and would narrow it. The binder
//!   builds the flow graph and nothing reads it yet.
//! - **No type references.** `interface I {}` cannot be used as an annotation:
//!   `getTypeFromTypeNode` covers the keyword, literal and parenthesised forms
//!   and yields `errorType` for everything else.
//! - **No functions, classes, enums, accessors, aliases or destructuring.**
//!   Their symbol shapes fall through `getTypeOfSymbol` to `errorType`.
//! - **No intersection, conditional or indexed-access types.** Unions exist
//!   ([`unions`]), and with them `boolean` is what upstream makes it — the union
//!   `false | true` — rather than an intrinsic.
//! - **No assignability, inference, or overload resolution.**
//! - **No control-flow narrowing.** The binder builds the flow graph
//!   (`tsr-binder`) and nothing reads it yet.
//! - **No diagnostics.** Not one of the checker's error messages is ported,
//!   including the algorithmic recursion limits (`bd tsr-el3.2`), which must be
//!   ported as the checker is written rather than retrofitted.
//!
//! # How types are computed
//!
//! Every method takes `&mut self` and returns [`TypeId`](types::TypeId). Nothing
//! hands out a reference into checker state, so the lazy-memoisation pattern that
//! the borrow checker rejects in its naive form becomes read, drop, recurse,
//! write. That decision was settled by three working implementations before it
//! constrained anything; see
//! [ADR-0013](../../../docs/adr/0013-checker-memoisation.md) and
//! `crates/tsr-checker-spike`.
//!
//! # Why `checker_types` still reads 0%
//!
//! Because nothing renders these types in `.types` baseline form. The checker can
//! now answer "what is the type of `x`"; the walker that emits
//! `>x : string` at the positions upstream emits them is `bd tsr-4sc.3`, and it
//! is what will first move the number.
//!
//! # The oracle is proved; the producer is not
//!
//! `checker_types` reported 0% from the day it was added and had no comparison
//! code at all, which made its judging path exactly as unproven as a suite
//! reading 100% on its first run. That was closed on 2026-08-05 — before any
//! checker work was measured with it — by writing `types_suite::compare` and
//! verifying its eight tests against seven deliberately weakened judges
//! (`docs/architecture/checker-oracle.md`).
//!
//! What is still unproven is the **producer**: nothing renders this crate's types
//! in `.types` baseline form (`bd tsr-4sc.3`), so the judge has only ever been fed
//! an empty left-hand side. The first non-zero `checker_types` number is not
//! evidence until something deliberately wrong has been pushed through the whole
//! path and seen to go red.

pub mod assertions;
pub mod binary;
pub mod calls;
pub mod checker;
pub mod declared;
pub mod expressions;
pub mod flags;
pub mod indexed;
pub mod intersections;
pub mod intrinsics;
pub mod literals;
pub mod members;
pub mod objects;
pub mod optionality;
pub mod printing;
pub mod resolution;
pub mod signatures;
pub mod symbols;
pub mod types;
pub mod unions;

pub use checker::Checker;
pub use flags::TypeFlags;
pub use intrinsics::Intrinsics;
pub use resolution::{PropertyName, Resolutions};
pub use types::{Type, TypeData, TypeId, TypeStore};
