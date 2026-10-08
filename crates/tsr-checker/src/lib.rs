//! The type checker.
//!
//! Ported from `internal/checker` at the pinned commit — 60,269 lines upstream,
//! the largest single workstream in the port (`bd tsr-4sc`, PLAN.md §3.2).
//!
//! # What exists today
//!
//! **STALE — do not size an item from this section.** The two lists below were
//! written on the day `checker_types` read 0% and have not been maintained; at
//! the time of writing this warning the gradient is 71.06% and most of "what
//! does not exist" does. Only the assignability bullet has been corrected, and
//! only because a stale reading of it cost a session (see
//! `docs/architecture/checker-notes-assign.md` §1). `STATUS.md` is the
//! authority on what is ported; `bd tsr-7wkn` is the bullet-by-bullet rewrite.
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
//! - **No type references.** `interface I {}` cannot be used as an annotation:
//!   `getTypeFromTypeNode` covers the keyword, literal and parenthesised forms
//!   and yields `errorType` for everything else.
//! - **No functions, classes, enums, accessors, aliases or destructuring.**
//!   Their symbol shapes fall through `getTypeOfSymbol` to `errorType`.
//! - **No intersection, conditional or indexed-access types.** Unions exist
//!   ([`unions`]), and with them `boolean` is what upstream makes it — the union
//!   `false | true` — rather than an intrinsic.
//! - **Assignability** ([`relater`]) over primitives, literals, unions,
//!   intersections **and object types, compared structurally** — the last
//!   landed in `e24b7ca` and is exercised by
//!   `tests/relater.rs::two_structurally_identical_interfaces_relate`. Both
//!   callers it was built for now call it: the assignment reduction in [`flow`]
//!   and overload resolution in [`calls`]. What remains is not the *comparison*
//!   but its **trustworthiness**: a `false` between two object types may mean
//!   "unrelated" or "this port could not tell", and the relation has no third
//!   answer to distinguish them. That is why [`calls`]'s `SELECTABLE` still
//!   excludes object types. See `docs/architecture/checker-notes-assign.md`.
//! - **No inference, and no overload resolution.**
//! - **Control-flow narrowing, but only the truthiness guards.** [`flow`]
//!   walks the binder's flow graph, so `if (x)` drops the falsy constituents of
//!   a union. `typeof`, equality, discriminant, `instanceof` and `in` guards are
//!   not ported, and neither is the assignment reduction, which needs an
//!   assignability relation this crate does not have. Each unported form leaves
//!   the declared type — upstream's `narrowType` default arm — so they are gaps
//!   rather than wrong answers.
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

pub mod array_literals;
pub mod assertion_overlap;
pub mod assertions;
mod assignment_declarations;
pub mod assignreport;
mod base_types;
pub mod binary;
mod binding_patterns;
pub mod call_arity;
mod callable_expandos;
pub mod calls;
pub mod check;
pub mod checker;
mod circular_alias;
mod class_fields;
mod class_function_merge;
pub mod comparison_overlap;
pub mod computed_name;
mod const_inference;
mod constraints;
mod context_sensitive;
pub mod contextual;
mod debug_env;
pub mod declared;
mod decorators;
mod delete_operand;
pub mod destructure;
mod emit_helpers;
mod enum_initializer;
pub mod enum_member_name;
pub mod expressions;
pub mod flags;
pub mod flow;
pub mod function_types;
mod grammar;
pub mod heritage_conformance;
mod identity;
pub mod implicit_any;
mod import_attributes;
mod import_call;
mod import_meta;
mod index_access_reports;
pub mod index_constraint;
pub mod index_signatures;
pub mod indexed;
pub mod inference;
pub mod intersections;
pub mod intrinsics;
mod iteration;
mod js_case_data;
mod jsdoc_annotations;
mod jsdoc_full_signature;
mod jsdoc_links;
mod jsdoc_modifiers;
mod jsdoc_params;
mod jsx_attributes;
mod jsx_component;
mod jsx_factory;
pub mod jsx_intrinsic;
pub mod literals;
pub mod mapped;
pub mod meaning_mismatch;
pub mod member_completeness;
pub mod members;
pub mod merge_conflicts;
mod merged_export_spaces;
mod module_format;
mod name_suggestion;
pub mod node_reuse;
pub mod nonexistent_property;
pub mod nullable_operand;
mod nullish;
pub mod objects;
pub mod operator_operands;
pub mod optionality;
mod parameter_self_reference;
mod perf_links;
pub mod printing;
mod private_setter_read;
pub mod readonly_target;
mod reference_target;
pub mod relater;
mod relation_cache;
pub mod resolution;
mod rest_parameter_type;
mod satisfies;
mod signature_positions;
pub mod signatures;
mod spread_overrides;
mod spreads;
pub mod strict_mode;
mod string_mapping;
mod super_expression;
pub mod symbol_access;
pub mod symbols;
mod template_match;
mod templates;
mod this_expression;
pub mod truthiness;
mod tuples;
pub mod type_argument_arity;
pub mod types;
mod union_signatures;
pub mod unions;
pub mod unused;
mod using_declaration;
mod variances;
mod widening;
#[cfg(feature = "work-trace")]
pub mod work_trace;

pub use checker::Checker;
pub use flags::TypeFlags;
pub use intrinsics::Intrinsics;
pub use resolution::{PropertyName, Resolutions};
pub use types::{Type, TypeData, TypeId, TypeStore};
