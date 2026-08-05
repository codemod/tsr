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
//! # The oracle is unproven, and that is a live risk
//!
//! `checker_types` has reported 0% since it was added, which means **its judging
//! path has never executed**. `docs/architecture/checker-oracle.md` is explicit
//! that the first time that suite is non-zero it must be mutation-tested — fed
//! deliberately wrong types and confirmed to go red — before any number from it is
//! believed. That is the same failure `file_loader` was caught by. Nothing in this
//! crate has changed that yet.

pub mod checker;
pub mod flags;
pub mod intrinsics;
pub mod printing;
pub mod types;

pub use checker::Checker;
pub use flags::TypeFlags;
pub use intrinsics::Intrinsics;
pub use types::{Type, TypeData, TypeId, TypeStore};
