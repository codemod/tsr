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
//!
//! # What does not exist
//!
//! Everything else, and the list is the honest measure of distance:
//!
//! - **No declaration types.** `getTypeOfSymbol` is unported, so a variable,
//!   parameter or property has no type — only the *expression* forms above do.
//!   This is why `checker_types` still reads 0%.
//! - **No object, union, intersection, generic, conditional or indexed-access
//!   types.** `boolean` is an intrinsic here; upstream builds it as the union
//!   `false | true`, which will change how it prints in some positions.
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
//! # What blocks the next slice
//!
//! `getTypeOfSymbol` cannot be written yet: **nothing maps a `NodeId` back to a
//! typed node**, so the checker can reach a symbol's `value_declaration` position
//! but not the declaration's annotation or initialiser. Measured cost of the
//! obvious fix and the cheaper alternatives are in
//! `docs/architecture/checker.md`; the decision is `bd tsr-4sc.4`, which blocks
//! `bd tsr-4sc.2`.
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

pub mod checker;
pub mod flags;
pub mod intrinsics;
pub mod printing;
pub mod types;

pub use checker::Checker;
pub use flags::TypeFlags;
pub use intrinsics::Intrinsics;
pub use types::{Type, TypeData, TypeId, TypeStore};
