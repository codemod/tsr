//! TS18050 — `The value '{0}' cannot be used here.`
//!
//! `checkArithmeticOperandType` (`checker.go:12799`), reached from
//! `checkBinaryLikeExpression` for every operator whose operands must be
//! numeric: an operand whose type *is* `null` or `undefined` is not a number and
//! cannot become one.
//!
//! Error node the **operand**: `binaryArithmatic1.ts(1,13)` is the `null` of
//! `var v = 4 | null;`.
//!
//! # `+` was excluded on an argument, and the measurement overturned it
//!
//! §32 declined `+` because it is overloaded with string concatenation; §40.5
//! measured that `null` and `undefined` are not string-like either, so the `+`
//! arm reaches the same TS18050. The `+` arm now reports through
//! [`Checker::check_non_null_type_reporting`] from its own port
//! (`crate::operator_operands`, `docs/parity/notes/operators.md` §5), as the
//! relational, arithmetic and unary arms already did; this module keeps the
//! reporter they share.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl Checker<'_, '_> {
    /// Does the class enclosing this node have `extends null`? §910.
    fn enclosing_class_extends_null(&self, node: NodeId) -> bool {
        let Some(class) = self.nodes.ancestors(node).find(|&ancestor| {
            matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        }) else {
            return false;
        };
        let clauses = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(declaration)) => declaration.heritage_clauses,
            Some(Node::ClassExpression(expression)) => expression.heritage_clauses,
            _ => return false,
        };
        clauses.iter().any(|clause| {
            clause.token.kind == SyntaxKind::ExtendsKeyword
                && clause.types.iter().any(|base| {
                    // **`extends null` parses its base as an `Identifier`
                    // named `null`**, not as the null-keyword node — a probe
                    // printed `first_kind=Some(Identifier)` and that is the
                    // whole of why §910's first attempt measured `+0`. §891.
                    base.expression.and_then(|e| e.node_id()).is_some_and(|id| {
                        self.nodes.kind(id) == SyntaxKind::NullKeyword
                            || self.identifier_text(id) == Some("null")
                    })
                })
        })
    }

    /// The same report against a type the caller has already adjusted.
    ///
    /// A receiver's facts are read from `checkNonNullType`'s *argument*, and on
    /// an optional chain that argument is `getOptionalExpressionType`'s answer,
    /// not `checkExpression`'s (`checkPropertyAccessChain`, `checker.go:11253`).
    /// Recomputing the type inside the reporter loses that distinction, which
    /// is what made every `a?.b` report — §5 of `checker-notes-nnaccess.md`.
    pub(crate) fn report_nullable_operand_of_type(
        &mut self,
        operand: tsr_ast::Expression<'_>,
        ty: TypeId,
    ) {
        let Some(id) = operand.node_id() else { return };
        // `getTypeFacts(t, IsUndefinedOrNull)` (`checker.go:7425`): the
        // type **may be** nullish, which for a union is any constituent.
        let (mut maybe_null, maybe_undefined) = self.nullish_facts(ty);
        // **`super` under a `null` heritage base.** §852 proved the diagnostics
        // half complete — the reporter is entered at the right node the right
        // number of times — and that `check_expression(super)` answers no
        // nullable type here. That is the type side's (`bd tsr-gjze`, still
        // open); the fact this rule needs is syntactic and exact, one keyword in
        // one clause, and nothing else in the language produces a `null` base.
        // A stand-in, as §830's and §867's were, not a fix. §910.
        if !maybe_null
            && self.nodes.kind(id) == SyntaxKind::SuperKeyword
            && self.enclosing_class_extends_null(id)
        {
            maybe_null = true;
        }
        if !maybe_null && !maybe_undefined {
            return;
        }
        // **This code is chosen by the node, not by the type.**
        // `reportObjectPossiblyNullOrUndefinedError` (`checker.go:7455`)
        // emits `The value '{0}' cannot be used here` only for a `null`
        // keyword or for an identifier literally spelled `undefined`;
        // every other nullable operand gets TS18048 / TS18049 / TS2531 /
        // TS2532 with the same *facts*. `var x: typeof undefined; t < x`
        // is upstream's TS18048 and was 64 wrong lines of this rule —
        // `checker-notes-diag2.md` §50.3.
        let written = match self.node_map.get(id) {
            _ if self.nodes.kind(id) == SyntaxKind::NullKeyword => Some("null"),
            Some(Node::Identifier(identifier)) if identifier.text == "undefined" => {
                Some("undefined")
            }
            _ => None,
        };
        let Some(file) = self.source_file_of_for_diagnostics(id) else { return };
        let span = self.error_span(id);
        if let Some(written) = written {
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::THE_VALUE_0_CANNOT_BE_USED_HERE,
                    span,
                    [written.to_string()],
                ),
            );
            return;
        }
        // The other five branches. `entityNameToString` gives the printed
        // name for an identifier or a dotted name; anything else takes the
        // `Object is possibly …` twin at the same position with the same
        // facts (`checker.go:7455`, `checker-notes-diag2.md` §51).
        let named = self.operand_entity_name_text(id).filter(|text| text.len() < 100);
        match (named, maybe_null, maybe_undefined) {
            (Some(text), true, true) => self.report(
                file,
                Diagnostic::with_args(&messages::_0_IS_POSSIBLY_NULL_OR_UNDEFINED, span, [text]),
            ),
            (Some(text), false, true) => self.report(
                file,
                Diagnostic::with_args(&messages::_0_IS_POSSIBLY_UNDEFINED, span, [text]),
            ),
            (Some(text), true, false) => self
                .report(file, Diagnostic::with_args(&messages::_0_IS_POSSIBLY_NULL, span, [text])),
            (None, true, true) => self.report(
                file,
                Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_NULL_OR_UNDEFINED, span),
            ),
            (None, false, true) => {
                self.report(file, Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_UNDEFINED, span));
            }
            (None, true, false) => {
                self.report(file, Diagnostic::new(&messages::OBJECT_IS_POSSIBLY_NULL, span));
            }
            (_, false, false) => {}
        }
    }

    /// `checkNonNullType` (`checker.go:7409`) **with** its reporter
    /// (`reportObjectPossiblyNullOrUndefinedError`, `checker.go:7455`): the
    /// checked operand's type with `null`/`undefined` removed, `errorType`
    /// for an `unknown` operand under `strictNullChecks` (whose TS18046 /
    /// TS2571 is not yet reported, see below) or for one that is nothing but
    /// nullable.
    pub(crate) fn check_non_null_type_reporting(
        &mut self,
        ty: TypeId,
        operand: tsr_ast::Expression<'_>,
    ) -> TypeId {
        if self.strict_null_checks && self.type_of(ty).flags.intersects(TypeFlags::UNKNOWN) {
            // TS18046 / TS2571 are **not reported yet**. A context-sensitive
            // arrow argument of a generic call (`Map.groupBy([0], x => x < 5)`)
            // has its parameter read as `unknown` by the diagnostics walk while
            // the type dump answers `number` — a node type cached during an
            // inference pass, outside this file. Reporting here measured two
            // EMPTY_RIGHT losses (`mapGroupBy`, `nonInferrableTypePropagation2`);
            // `docs/parity/notes/operators.md`.
            return self.intrinsics.error;
        }
        // The reporter runs on `getTypeFacts(t, IsUndefinedOrNull)` whatever
        // `strictNullChecks` says: `null` and `undefined` keep `IsNull` /
        // `IsUndefined` in the non-strict fact sets (`checker.go:471-472`), so
        // `+null` is TS18050 under `@strict: false` too
        // (`docs/parity/notes/operators.md` §10).
        self.report_nullable_operand_of_type(operand, ty);
        self.non_null_operand_type(ty)
    }

    /// The type half of `checkNonNullType` (`checker.go:7409`) for an
    /// operator operand, with no reporter: `errorType` for `unknown` under
    /// `strictNullChecks`, the non-nullable remainder otherwise, and
    /// upstream's tail (`checker.go:7429`) — `errorType` for a nullable or
    /// `never` result — in both modes. The `+` arm's *type*
    /// (`binary.rs` `check_addition`) asks this; its diagnostics go through
    /// [`Checker::check_non_null_type_reporting`].
    ///
    /// Each of those `errorType` answers is upstream's own (ADR-0048), so
    /// they are [`Intrinsics::native_error`](crate::Intrinsics), not the gap:
    /// verified line by line against the native identity probe, 256 of 256
    /// lines (`docs/parity/notes/r5-errorsplit5.md` §3). The gap is kept only
    /// for a gap operand, which [`Checker::check_non_null_type`] hands back
    /// unchanged.
    pub(crate) fn non_null_operand_type(&mut self, ty: TypeId) -> TypeId {
        if self.strict_null_checks && self.type_of(ty).flags.intersects(TypeFlags::UNKNOWN) {
            return self.intrinsics.native_error;
        }
        let non_null = self.check_non_null_type(ty);
        // `check_non_null_type` (`members.rs`) spells its two `errorType`
        // exits (`checker.go:7411`, `:7429`) as the gap; neither is one.
        if non_null == self.intrinsics.error && !self.is_gap(ty) {
            return self.intrinsics.native_error;
        }
        // Non-strict `GetNonNullableType` is the identity, and upstream's tail
        // (`checker.go:7429`) still answers `errorType` for a nullable or
        // never result; `check_non_null_type` returns the type unchanged there.
        if !self.strict_null_checks
            && non_null == ty
            && self.type_of(ty).flags.intersects(TypeFlags::NULLABLE | TypeFlags::NEVER)
            && self
                .get_type_facts(ty)
                .intersects(crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL)
        {
            return self.intrinsics.native_error;
        }
        non_null
    }

    /// `getTypeFacts(t, TypeFactsIsUndefinedOrNull)` reduced to the two bits
    /// the reporter branches on — union-aware, since that is the whole of what
    /// "may be" means here.
    pub(crate) fn nullish_facts(&self, ty: TypeId) -> (bool, bool) {
        let of = |checker: &Self, id: TypeId| {
            let flags = checker.type_of(id).flags;
            (flags.contains(TypeFlags::NULL), flags.contains(TypeFlags::UNDEFINED))
        };
        let (mut null, mut undefined) = of(self, ty);
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(ty).data {
            for &constituent in types {
                let (n, u) = of(self, constituent);
                null |= n;
                undefined |= u;
            }
        }
        (null, undefined)
    }

    /// `entityNameToString` (`checker.go`), for the shapes an operand takes:
    /// an identifier, or a dotted name of identifiers.
    fn operand_entity_name_text(&self, node: NodeId) -> Option<String> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::PropertyAccessExpression(access) => {
                let target = self.operand_entity_name_text(access.expression?.node_id()?)?;
                let member = match access.name? {
                    tsr_ast::MemberName::Identifier(identifier) => identifier.text,
                    tsr_ast::MemberName::PrivateIdentifier(private) => private.text,
                };
                Some(format!("{target}.{member}"))
            }
            _ => None,
        }
    }
}
