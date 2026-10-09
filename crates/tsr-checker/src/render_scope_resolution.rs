//! Name resolution at a print site, with the node builder's synthesized
//! signature scopes in front of the binder's walk.
//!
//! # The native operation
//!
//! While the node builder prints a signature it calls `enterNewScope`
//! (`nodebuilderimpl.go`), which points `ctx.enclosingDeclaration` at a
//! synthesized node whose locals are the signature's type parameters. Every
//! name the builder resolves inside that print (`needsQualification`,
//! `getAccessibleSymbolChain`'s `trySymbolTable`, `isTypeSymbolAccessible`)
//! therefore sees those type parameters first. `resolveNameHelper`
//! (`binder/nameresolver.go:64`) admits them through its
//! `lastLocation.Flags&NodeFlagsSynthesized` arm, so the function-like
//! visibility rule that hides a type parameter from a member's name or
//! computed name does not apply inside the print.
//!
//! So `function a4<A>(x: A) { return new A() }`, with a class `A` outside,
//! prints `<A>(x: A) => globalThis.A`: inside the signature's scope the name
//! `A` is the type parameter, so the class needs its qualifier.
//!
//! # This port
//!
//! The printer already keeps that scope:
//! [`Checker::render_type_parameter_scope`] holds `(written name, symbol)` for
//! every signature render on the stack (`push_render_type_parameter_scope`).
//! [`Checker::resolve_name_at_print_site`] consults it innermost-first for a
//! Type-meaning lookup, then falls back to the binder's walk from the site.
//!
//! # Checker port boundaries (`docs/conventions.md`)
//!
//! - **Native operation:** `enterNewScope`'s synthesized locals as seen by
//!   `resolveNameHelper`, pinned at `5b1047d`.
//! - **Key identity and owner:** no new table. The scope is the existing
//!   `render_type_parameter_scope`, owned by the signature renderers.
//! - **Publication states:** none; a read.
//! - **Receiver/alias context:** none.
//! - **Work boundary:** a linear scan of the render scope (a few entries),
//!   then the binder walk the callers already made.
//!
//! `docs/parity/notes/r6-typesroots2.md` §5.

use tsr_binder::{SymbolFlags, SymbolId};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `resolveName` from a print site, inside the node builder's signature
    /// scopes.
    #[expect(dead_code, reason = "consumer: r6-typesroots2-shadowed-type-parameter-rename.diff")]
    pub(crate) fn resolve_name_at_print_site(
        &self,
        reference: tsr_ast::NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        if meaning.intersects(SymbolFlags::TYPE)
            && let Some(&(_, owner)) =
                self.render_type_parameter_scope.iter().rev().find(|(text, _)| text == name)
        {
            return Some(owner);
        }
        self.binder.resolve_name(self.nodes, self.node_map, reference, name, meaning)
    }
}
