//! The seam where upstream calls the checker.
//!
//! Ported from typescript-go's `printer.EmitResolver`
//! (`internal/printer/emitresolver.go:77`), narrowed to the methods
//! `internal/transformers/declarations` actually calls. Upstream's interface has
//! 44 members because it also serves the JSX, decorator-metadata and const-enum
//! transforms; the 9 here are the declaration transform's whole dependency on the
//! checker.
//!
//! # Why this trait exists rather than being inlined
//!
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md)
//! established that upstream's declaration transform cannot ship before the
//! checker, because `EmitResolver`'s only implementation is
//! `internal/checker/emitresolver.go`. It named the risk this creates as its third
//! falsifier:
//!
//! > **Reconciliation at Phase 4 turns out to be a rewrite.** If, once
//! > `CreateTypeOfDeclaration` exists, `tsr-dts` shares nothing with the ported
//! > declaration transform, then the year of early shipping cost us a subsystem's
//! > worth of throwaway work.
//!
//! Naming the seam is what makes that falsifier answerable in advance. The
//! transform above it is a port of `transform.go` and does not know which
//! implementation it has; [`SyntacticResolver`] is a checker-free one, and Phase 4
//! adds a checker-backed one beside it. What has to be reconciled is then one
//! trait with two implementations, rather than two subsystems.
//!
//! # What the syntactic implementation can and cannot answer
//!
//! Every method below is a question upstream answers by *running inference*. The
//! syntactic implementation answers the ones that turn out to be decidable from
//! syntax and refuses the rest — and "the rest" is not a judgement call: it is
//! exactly the set [`tsr_dts::analyze`] reports a `TS9xxx` for. That equivalence
//! is the load-bearing claim of this crate and is asserted directly in
//! `tests/analysis_agreement.rs`: for every construct the analysis calls clean,
//! the resolver must produce a type; for every construct it reports, the emitter
//! must refuse rather than guess.

use tsr_ast::{
    ClassElement, Expression, ModifierFlags, ModifierLike, NodeId, ParameterDeclaration,
    PropertyDeclaration, Statement, SyntaxKind, TypeNode, VariableDeclaration,
};

use crate::{factory::Factory, type_builder};

/// A const context: whether a literal keeps its literal type or widens.
///
/// `const x = 1` emits `1`; `let x = 1` emits `number`. Upstream distinguishes
/// these through freshness on the checker's literal types; syntactically it is a
/// parameter threaded through the type builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Literal types are preserved, and object members are `readonly`.
    Const,
    /// Literals widen to their base primitive.
    Widening,
}

/// The declaration transform's dependency on the checker.
///
/// Ported from `printer.EmitResolver` (`internal/printer/emitresolver.go:77`),
/// restricted to what `internal/transformers/declarations` calls. Method names
/// keep upstream's spelling so a `grep` for `IsDeclarationVisible` lands here.
pub trait EmitResolver<'a> {
    /// `EmitResolver.IsDeclarationVisible`.
    fn is_declaration_visible(&self, statement: &Statement<'a>) -> bool;

    /// `EmitResolver.IsImplementationOfOverload`.
    fn is_implementation_of_overload(&self, node_id: Option<NodeId>) -> bool;

    /// `EmitResolver.IsOptionalParameter`.
    fn is_optional_parameter(&self, parameter: &ParameterDeclaration<'a>) -> bool;

    /// `EmitResolver.IsLiteralConstDeclaration`.
    ///
    /// True when the declaration emits `= value` in place of a type annotation.
    fn is_literal_const_declaration(&self, node: LiteralConstHost<'_, 'a>) -> bool;

    /// `EmitResolver.CreateLiteralConstValue`.
    fn create_literal_const_value(
        &self,
        factory: &mut Factory<'a, '_>,
        node: LiteralConstHost<'_, 'a>,
    ) -> Option<Expression<'a>>;

    /// `EmitResolver.CreateTypeOfDeclaration`.
    ///
    /// `None` means "inference was required and is unavailable" — upstream's
    /// `ensureType` turns that into `any`, and this port does the same so the
    /// shape of the output does not depend on which resolver answered.
    fn create_type_of_declaration(
        &self,
        factory: &mut Factory<'a, '_>,
        initializer: Option<&Expression<'a>>,
        freshness: Freshness,
    ) -> Option<TypeNode<'a>>;

    /// `EmitResolver.CreateReturnTypeOfSignatureDeclaration`.
    fn create_return_type_of_signature_declaration(
        &self,
        factory: &mut Factory<'a, '_>,
    ) -> Option<TypeNode<'a>>;

    /// `EmitResolver.GetEnumMemberValue`.
    ///
    /// Answered for the whole member list at once rather than per member, because
    /// a member's value depends on its predecessors — `enum E { A, B }` numbers
    /// `B` from `A`, and `C = A | B` folds through both. Upstream's per-node
    /// signature hides that behind the checker's own memoised evaluator; here the
    /// dependency has to be explicit.
    fn get_enum_member_values(
        &self,
        members: &[&tsr_ast::EnumMember<'a>],
        enum_name: &str,
    ) -> Vec<Option<crate::enum_value::EnumValue>>;

    /// `DeclarationEmitHost.GetEffectiveDeclarationFlags`.
    ///
    /// On the host rather than the resolver upstream, but it is the same kind of
    /// question and the same seam: `GetEffectiveDeclarationFlags` consults the
    /// *combined* modifier flags, which for a JS file are partly inferred.
    fn get_effective_declaration_flags(
        &self,
        modifiers: &[ModifierLike<'a>],
        mask: ModifierFlags,
    ) -> ModifierFlags;
}

/// The declarations `canHaveLiteralInitializer` admits
/// (`internal/transformers/declarations/util.go`).
///
/// Upstream passes `*ast.Node` and switches on the kind inside; the union is
/// spelled out here because the two arms carry different modifier rules —
/// a property must not be `private`, a variable must be in a `const` list.
#[derive(Debug, Clone, Copy)]
pub enum LiteralConstHost<'b, 'a> {
    /// A `const` variable declaration.
    Variable(&'b VariableDeclaration<'a>, /* is_const */ bool),
    /// A `readonly` property declaration.
    Property(&'b PropertyDeclaration<'a>),
    /// A bare expression hosted by a synthesized `const` — the binding a
    /// default-exported expression is rewritten into (`modulePreserve4` keeps
    /// `declare const _default = 0;` rather than widening to `number`).
    Expression(&'b Expression<'a>),
}

impl<'a> LiteralConstHost<'_, 'a> {
    fn initializer(&self) -> Option<&Expression<'a>> {
        match self {
            Self::Variable(declaration, _) => declaration.initializer.as_ref(),
            Self::Property(property) => property.initializer.as_ref(),
            Self::Expression(expression) => Some(expression),
        }
    }

    /// The written type annotation, which is what makes the declaration *not*
    /// literal-const.
    fn annotation(&self) -> Option<&tsr_ast::TypeNode<'a>> {
        match self {
            Self::Variable(declaration, _) => declaration.r#type.as_ref(),
            Self::Property(property) => property.r#type.as_ref(),
            Self::Expression(_) => None,
        }
    }
}

/// The checker-free implementation of the seam.
///
/// Everything it knows comes from two sources: the visibility set
/// [`tsr_dts::visibility`] computed for this file, and the syntax of the node in
/// front of it. It never looks at a type.
pub struct SyntacticResolver<'a> {
    visible: tsr_dts::visibility::Visible,
    strict_null_checks: bool,
    /// File-level annotated names, for the arrow-return identifier copy.
    scope: type_builder::FileScope<'a>,
    /// Every *top-level* statement of the file.
    ///
    /// Needed to tell "this declaration is not visible" from "this pass has
    /// nothing to say about this declaration", which are the same answer in
    /// [`tsr_dts::visibility::Visible`] and must not be here — see
    /// [`SyntacticResolver::is_declaration_visible`].
    top_level: rustc_hash::FxHashSet<NodeId>,
}

impl<'a> SyntacticResolver<'a> {
    /// Build the resolver for one file.
    #[must_use]
    pub fn new(file: &tsr_ast::SourceFile<'a>, strict_null_checks: bool) -> Self {
        Self {
            visible: tsr_dts::visibility::visible_declarations(file),
            strict_null_checks,
            scope: type_builder::FileScope::of(file),
            top_level: file.statements.iter().filter_map(Statement::node_id).collect(),
        }
    }
}

impl<'a> EmitResolver<'a> for SyntacticResolver<'a> {
    /// Upstream computes this in `PrecalculateDeclarationEmitVisibility` by
    /// walking symbols; here it is reachability from the exports, which is the
    /// approximation `docs/architecture/isolated-declarations.md` documents.
    ///
    /// **The approximation is only defined at the top level.** `visible_declarations`
    /// walks the file's own statement list and nothing else, so a declaration
    /// inside a namespace body is absent from the set — not because it is
    /// invisible, but because the set never had an opinion about it. Reading that
    /// absence as "not visible" elided the body of every namespace in the corpus
    /// and printed `declare namespace M {}` for all of them, which parses, which is
    /// why nothing but a byte comparison caught it.
    ///
    /// A nested declaration is therefore visible unless its enclosing namespace is
    /// not. That is the correct shape as well as the convenient one: an ambient
    /// namespace exports everything it contains.
    fn is_declaration_visible(&self, statement: &Statement<'a>) -> bool {
        match statement.node_id() {
            Some(id) if self.top_level.contains(&id) => self.visible.contains(Some(id)),
            _ => true,
        }
    }

    /// Upstream asks the checker whether this signature is the *implementation*
    /// of an overload set, which needs the symbol's declaration list. Nothing
    /// syntactic can see the other declarations of the same name from one node,
    /// so this always answers `false`.
    ///
    /// The direction is deliberate. Answering `false` keeps a declaration that
    /// upstream elides — a visible, diffable extra line — whereas answering
    /// `true` would silently drop a signature that should have been emitted.
    /// Overload sets are handled at the statement-list level instead, in
    /// [`crate::transform`], where the sibling declarations *are* in hand.
    fn is_implementation_of_overload(&self, _node_id: Option<NodeId>) -> bool {
        false
    }

    /// Upstream's `IsOptionalParameter` is true for a `?` token, for a parameter
    /// with an initializer, and for a JS `@param` with a bracketed name. The first
    /// two are syntactic; the third needs JSDoc and is not handled here.
    fn is_optional_parameter(&self, parameter: &ParameterDeclaration<'a>) -> bool {
        parameter.question_token.is_some() || parameter.initializer.is_some()
    }

    /// Upstream asks whether the declaration's *type* is a fresh literal type.
    /// Syntactically: a `const` variable or `readonly` property, **with no
    /// annotation**, whose initializer is written as a primitive literal.
    ///
    /// The annotation test is not an extra condition bolted on — it is what
    /// "fresh" means. `const answer: number = 42` has the declared type `number`,
    /// which is not a fresh literal type, so upstream emits
    /// `declare const answer: number` and not `declare const answer = 42`. Without
    /// this test the emitter drops a stated annotation in favour of a value, which
    /// is a different type and parses fine — the failure mode a round trip cannot
    /// see.
    fn is_literal_const_declaration(&self, node: LiteralConstHost<'_, 'a>) -> bool {
        if node.annotation().is_some() {
            return false;
        }
        let qualifies = match node {
            LiteralConstHost::Variable(_, is_const) => is_const,
            LiteralConstHost::Property(property) => {
                has_modifier(property.modifiers, SyntaxKind::ReadonlyKeyword)
                    && !has_modifier(property.modifiers, SyntaxKind::PrivateKeyword)
            }
            // The synthesized default-export binding is always a `const`.
            LiteralConstHost::Expression(_) => true,
        };
        qualifies && node.initializer().is_some_and(type_builder::is_primitive_literal_value)
    }

    fn create_literal_const_value(
        &self,
        factory: &mut Factory<'a, '_>,
        node: LiteralConstHost<'_, 'a>,
    ) -> Option<Expression<'a>> {
        type_builder::literal_const_value(factory, node.initializer()?)
    }

    fn create_type_of_declaration(
        &self,
        factory: &mut Factory<'a, '_>,
        initializer: Option<&Expression<'a>>,
        freshness: Freshness,
    ) -> Option<TypeNode<'a>> {
        type_builder::type_of_expression(
            factory,
            initializer?,
            freshness,
            self.strict_null_checks,
            &self.scope,
        )
    }

    /// A return type is never recoverable from syntax: it is the type of whatever
    /// the body returns, and the body is exactly what a `.d.ts` drops. This is
    /// `TS9007`/`TS9008` territory, so [`tsr_dts::analyze`] has already reported
    /// it and the case is not in the emitter's target.
    fn create_return_type_of_signature_declaration(
        &self,
        _factory: &mut Factory<'a, '_>,
    ) -> Option<TypeNode<'a>> {
        None
    }

    /// Upstream evaluates enum members in the checker. The syntactic fold covers
    /// the same constant-expression grammar `tsr_dts`'s `TS9020` rule recognises —
    /// see [`crate::enum_value`] for why that is the right boundary.
    fn get_enum_member_values(
        &self,
        members: &[&tsr_ast::EnumMember<'a>],
        enum_name: &str,
    ) -> Vec<Option<crate::enum_value::EnumValue>> {
        crate::enum_value::fold_members(members, enum_name)
    }

    fn get_effective_declaration_flags(
        &self,
        modifiers: &[ModifierLike<'a>],
        mask: ModifierFlags,
    ) -> ModifierFlags {
        crate::modifiers::modifier_flags(modifiers) & mask
    }
}

/// Whether a modifier list carries a given keyword.
pub(crate) fn has_modifier(modifiers: &[ModifierLike<'_>], kind: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == kind))
}

/// Whether a class member is `private`, which decides whether its type is emitted.
pub(crate) fn is_private_member(member: &ClassElement<'_>) -> bool {
    let modifiers = match member {
        ClassElement::PropertyDeclaration(node) => node.modifiers,
        ClassElement::MethodDeclaration(node) => node.modifiers,
        ClassElement::GetAccessorDeclaration(node) => node.modifiers,
        ClassElement::SetAccessorDeclaration(node) => node.modifiers,
        ClassElement::ConstructorDeclaration(node) => node.modifiers,
        ClassElement::IndexSignatureDeclaration(node) => node.modifiers,
        ClassElement::ClassStaticBlockDeclaration(node) => node.modifiers,
        ClassElement::SemicolonClassElement(_) => &[],
    };
    has_modifier(modifiers, SyntaxKind::PrivateKeyword)
}
