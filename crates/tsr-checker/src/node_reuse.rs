//! Reuse of WRITTEN type nodes by the node builder — the reuse arm of
//! `serializeTypeForDeclaration` (`nodebuilderimpl.go:2181`) and of
//! `serializeReturnTypeForSignature` (`nodebuilderimpl.go:2023`), printed
//! through the existing-node visitor `tryReuseExistingNodeHelper`
//! (`nodecopy.go:222`).
//!
//! # The reuse decision
//!
//! Upstream asks the pseudochecker for the declaration's type: an annotated
//! declaration answers a `PseudoTypeDirect` holding the annotation node. The
//! node is reused when `pseudoTypeEquivalentToType`
//! (`pseudotypenodebuilder.go:362`) holds between
//! `getTypeFromTypeNode(annotation)` and the type being printed (identity,
//! error-type charity, regular-literal identity, identical unions, optional
//! `NEUndefined` strip). Otherwise the type is serialized fresh.
//!
//! # Port-convention boundary (`docs/conventions.md`, "Checker ports preserve
//! ownership and work boundaries")
//!
//! - **Native operation:** the two serializers above, at pinned `5b1047d`,
//!   consumed by `signatureToSignatureDeclarationHelper` for every parameter
//!   and return annotation of a signature with a declaration, and by
//!   `addPropertyToElementList` (`nodebuilderimpl.go:2486`) for a type
//!   literal's property signature, whose text this port bakes when the
//!   literal is built ([`Checker::reused_annotation_text`]).
//! - **Identity and owner:** no cache and no side table. The decision is keyed
//!   by the annotation node and the type it must be equivalent to, and its
//!   result is exactly that pair ([`WrittenAnnotation`], `Copy`), carried on
//!   the signature — [`crate::signatures::Parameter::written_text`] and
//!   [`crate::signatures::Signature::written_return`] — filled when the
//!   signature is built from its declaration. At that point the printed type
//!   *is* `getTypeFromTypeNode(annotation)`, so the equivalence holds by
//!   identity — unless the literal is being evaluated under an alias mapper,
//!   where the port's slot already holds the image and the annotation is
//!   refused when it names a moved type parameter
//!   ([`Checker::reuse_annotation`]). Every printer then asks the gate again
//!   of the type the slot holds at print time
//!   ([`WrittenAnnotation::is_equivalent_to`]): an instantiation whose image
//!   differs ends the reuse, the identity arm of `pseudoTypeEquivalentToType`
//!   for instantiated signatures.
//! - **Consumer context:** upstream reuses relative to the PRINT site
//!   (`ctx.enclosingDeclaration`). `trackExistingEntityName`
//!   (`nodecopy.go:317`) keeps a written entity name only when its leftmost
//!   identifier resolves to the same symbol at the print site
//!   (`nodecopy.go:347`); otherwise the reference is serialized from its type.
//!   A site-aware printer re-emits the node at its reference
//!   ([`Checker::written_annotation_text_at`]); a printer with no site emits
//!   it from the annotation's own file
//!   ([`Checker::site_free_annotation_text`]).
//!   Type-parameter renaming by `typeParameterToName` inside the reused node
//!   reads the render's name allocations (`ctx.typeParameterNames`); a
//!   print-only rename clone re-attaches the original's annotations
//!   ([`Checker::carry_written_annotations_through_rename`], r5-sigs §1).
//! - **Work boundary:** upstream decides reuse inside the node builder, at
//!   print time, and so does this port: building a signature stores two ids
//!   per annotated slot and walks nothing (the one exception is the
//!   alias-mapper check, which needs the frames live and runs only under
//!   one). Each print of a reused slot is one walk over the annotation
//!   subtree; a type literal's property is printed when the literal is built
//!   (its text is part of the literal's baked name), so that walk runs once
//!   per annotated property signature per build, never under an
//!   alias-evaluation frame. A sub-node the visitor refuses falls back to
//!   `typeToTypeNode(getTypeFromTypeNode(node))` (`nodecopy.go:866`), which
//!   here is [`Checker::get_type_from_type_node`] (node-cached) plus the
//!   site's renderer.
//!
//! # The printer half
//!
//! The visitor deep-clones the node and the emitter prints it single-line
//! (`EFSingleLine` on type literals, tuples and mapped types, `nodecopy.go:801`).
//! Parentheses are re-derived from `ast.GetTypeNodePrecedence`
//! (`ast/precedence.go:655`) by `Printer.emitTypeNode`
//! (`printer/printer.go:2274`); a reused clone is synthesized, so a `typeof`
//! operand of a postfix type is parenthesised (`(typeof C)[]`).

use crate::{Checker, types::TypeId};
use tsr_ast::{
    BindingName, EntityName, Expression, ModifierLike, Node, NodeId, ParameterDeclaration,
    PropertyName, SyntaxKind, TypeElement, TypeNode, TypeParameterDeclaration,
};
use tsr_binder::SymbolFlags;

/// A reused written annotation, as carried by
/// [`crate::signatures::Parameter::written_text`] and
/// [`crate::signatures::Signature::written_return`]: the annotation node and
/// the type it denotes, nothing printed. `Copy`, so cloning and instantiating
/// signatures carries two ids per slot; the text is emitted only when a
/// printer asks ([`Checker::written_annotation_text`],
/// [`Checker::written_annotation_text_at`]).
#[derive(Debug, Clone, Copy)]
pub struct WrittenAnnotation {
    /// The annotation node (`PseudoTypeDirect`'s node).
    node: NodeId,
    /// `getTypeFromTypeNode` of the annotation: the type the printed slot must
    /// still hold for the node to be reused. For a [`Self::renamed`]
    /// annotation, the rename clone's image of that type.
    r#type: TypeId,
    /// Carried onto a print-only rename clone
    /// ([`Checker::carry_written_annotations_through_rename`]): the node is
    /// the ORIGINAL signature's annotation, and its type-parameter names are
    /// spelled through the render's allocations, as `typeParameterToName`
    /// spells them inside the reused node (`nodecopy.go:301`).
    renamed: bool,
}

impl WrittenAnnotation {
    /// `pseudoTypeEquivalentToType` (`pseudotypenodebuilder.go:362`), its
    /// identity and error-type arms, asked of the type the slot holds NOW: an
    /// instantiation or rewrite that changed the slot's type ends the reuse
    /// whichever code path changed it.
    #[must_use]
    pub fn is_equivalent_to(&self, current: TypeId, error: TypeId) -> bool {
        current == self.r#type || current == error
    }
}

/// The visitor's context: the print site (`ctx.enclosingDeclaration`), if
/// known, and the root of the node being reused.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent findings of one walk, each read by a different refusal"
)]
struct ReuseContext {
    site: Option<NodeId>,
    root: NodeId,
    /// With no print site: a name in the node is only resolvable inside a
    /// scope narrower than its file's top level.
    scope_local: bool,
    /// An entity name in the node names a type parameter the active
    /// alias-evaluation mapper moves (see [`Checker::reuse_annotation`]).
    mapped: bool,
    /// A name upstream can spell at the site (a module member reached by an
    /// import route) that this port's namer cannot: the whole node is then
    /// left to the type's own serialization rather than to the visitor's
    /// per-node fallback, which answers only upstream's own failures.
    unnameable: bool,
    /// The node declares type parameters of its own (a function, constructor
    /// or member signature's list, a mapped type's key, an `infer`). Upstream
    /// gives each such declaration its own `enterNewScope`
    /// (`nodecopy.go:700-845`), whose `typeParameterToName` can rename it
    /// against the names already allocated; this visitor emits them as
    /// written. See [`Checker::renamed_annotation_in_scope`].
    declares_type_parameters: bool,
}

impl ReuseContext {
    fn new(site: Option<NodeId>, root: NodeId) -> Self {
        Self {
            site,
            root,
            scope_local: false,
            mapped: false,
            unnameable: false,
            declares_type_parameters: false,
        }
    }
}

/// `ast.TypePrecedence` (`ast/precedence.go:403`), lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Conditional,
    JsDoc,
    Function,
    Union,
    Intersection,
    TypeOperator,
    Postfix,
    NonArray,
}

/// `ast.GetTypeNodePrecedence` (`ast/precedence.go:655`).
fn node_precedence(node: TypeNode<'_>) -> Precedence {
    match node {
        TypeNode::ConditionalTypeNode(_) => Precedence::Conditional,
        TypeNode::JSDocOptionalType(_) | TypeNode::JSDocVariadicType(_) => Precedence::JsDoc,
        TypeNode::FunctionTypeNode(_) | TypeNode::ConstructorTypeNode(_) => Precedence::Function,
        TypeNode::UnionTypeNode(_) => Precedence::Union,
        TypeNode::IntersectionTypeNode(_) => Precedence::Intersection,
        TypeNode::TypeOperatorNode(_) | TypeNode::TypeQueryNode(_) => Precedence::TypeOperator,
        TypeNode::InferTypeNode(infer) => {
            if infer.type_parameter.is_some_and(|parameter| parameter.constraint.is_some()) {
                Precedence::Function
            } else {
                Precedence::TypeOperator
            }
        }
        TypeNode::IndexedAccessTypeNode(_)
        | TypeNode::ArrayTypeNode(_)
        | TypeNode::OptionalTypeNode(_) => Precedence::Postfix,
        _ => Precedence::NonArray,
    }
}

/// The precedence of a FALLBACK print. The fallback is
/// `typeToTypeNode(getTypeFromTypeNode(node))`, whose node kind this port
/// only has as text, so the top-level operator is read off the text: the
/// spellings `type_to_string` produces for each node kind of
/// [`node_precedence`] are distinguishable at bracket depth zero.
fn text_precedence(text: &str) -> Precedence {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let (mut union, mut intersection, mut function, mut conditional) = (false, false, false, false);
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' | b'`' => quote = Some(byte),
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b'>' if index > 0 && bytes[index - 1] == b'=' => function |= depth == 0,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b'|' if depth == 0 => union = true,
            b'&' if depth == 0 => intersection = true,
            b'?' if depth == 0 && text[..index].contains(" extends ") => conditional = true,
            _ => {}
        }
        index += 1;
    }
    if conditional {
        Precedence::Conditional
    } else if function || text.starts_with("new ") || text.starts_with("abstract new ") {
        Precedence::Function
    } else if union {
        Precedence::Union
    } else if intersection {
        Precedence::Intersection
    } else if ["keyof ", "unique ", "readonly ", "typeof ", "infer "]
        .iter()
        .any(|operator| text.starts_with(operator))
    {
        Precedence::TypeOperator
    } else {
        Precedence::NonArray
    }
}

/// `emitTypeNode(node, TypePrecedenceIntersection)` (`printer.go:2274`)
/// parenthesises a constituent whose node binds below `Intersection`. A
/// constituent's node kind is only available here as its printed text, so it
/// is read by [`text_precedence`], the same reader the fallback print uses.
/// A union's text is its ORIGIN's print when it has one: `keyof NameMap` is a
/// `TypeOperator` node (getIndexType's `newIndexType` origin), an alias
/// spelling a `TypeReference`, and an intersection origin an
/// `IntersectionType`; none of those is parenthesised.
pub(crate) fn binds_below_intersection(text: &str) -> bool {
    text_precedence(text) < Precedence::Intersection
}

/// `emitTypeOperator` emits its operand at `TypePrecedenceTypeOperator`
/// (`printer.go:2274`), so `keyof (A | B)` and `keyof (T extends U ? X : Y)`
/// keep their parentheses. Same text reader as
/// [`binds_below_intersection`].
pub(crate) fn binds_below_type_operator(text: &str) -> bool {
    text_precedence(text) < Precedence::TypeOperator
}

/// A string literal printed with its WRITTEN quote character — the clone keeps
/// `TokenFlagsSingleQuote` (`nodecopy.go:811`), and the emitter's
/// `getLiteralText` escapes for that quote.
fn quoted_literal(text: &str, single: bool) -> String {
    if !single {
        return crate::printing::quote(text);
    }
    let double = crate::printing::quote(text);
    let inner = &double[1..double.len() - 1];
    let mut out = String::with_capacity(inner.len() + 2);
    out.push('\'');
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '\'' => out.push_str("\\'"),
            other => out.push(other),
        }
    }
    out.push('\'');
    out
}

fn entity_name_text(name: EntityName<'_>) -> Option<String> {
    match name {
        EntityName::Identifier(identifier) => Some(identifier.text.to_string()),
        EntityName::QualifiedName(qualified) => {
            let left = entity_name_text(qualified.left?)?;
            Some(format!("{left}.{}", qualified.right?.text))
        }
    }
}

/// An entity-name expression (`a`, `a.b.c`) as written.
fn entity_name_expression_text(expression: Expression<'_>) -> Option<String> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.text.to_string()),
        Expression::PropertyAccessExpression(access) => {
            let left = entity_name_expression_text(access.expression?)?;
            match access.name? {
                tsr_ast::MemberName::Identifier(name) => Some(format!("{left}.{}", name.text)),
                tsr_ast::MemberName::PrivateIdentifier(_) => None,
            }
        }
        _ => None,
    }
}

fn modifier_text(modifier: &ModifierLike<'_>) -> Option<&'static str> {
    let ModifierLike::Token(token) = modifier else { return None };
    Some(match token.kind {
        SyntaxKind::ReadonlyKeyword => "readonly",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::OutKeyword => "out",
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::StaticKeyword => "static",
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::ProtectedKeyword => "protected",
        _ => return None,
    })
}

fn modifiers_prefix(modifiers: &[ModifierLike<'_>]) -> Option<String> {
    let mut out = String::new();
    for modifier in modifiers {
        out.push_str(modifier_text(modifier)?);
        out.push(' ');
    }
    Some(out)
}

impl<'a> Checker<'a, '_> {
    /// The reuse DECISION of `serializeTypeForDeclaration`: the written
    /// annotation is kept on the slot, as a node and the type it denotes,
    /// for a printer to re-emit ([`Checker::written_annotation_text_at`]).
    /// Nothing is walked or printed here — signature construction is the hot
    /// path and most signatures are never printed.
    ///
    /// Callers own the `pseudoTypeEquivalentToType` gate; see the module docs
    /// for why it holds by identity where this port builds the signature —
    /// except under an alias-evaluation mapper, checked here.
    ///
    /// Upstream builds a type literal once and instantiates it: the printed
    /// slot holds the mapper's image while `pseudoTypeToType` of the written
    /// node is `getTypeFromTypeNode(annotation)`, the type before mapping, so the
    /// identity arm of `pseudoTypeEquivalentToType`
    /// (`pseudotypenodebuilder.go:369`) fails whenever the mapper moved the
    /// annotation. This port evaluates such a literal by re-resolving its
    /// nodes under [`Checker::alias_evaluation_bindings`], so `equivalent`
    /// is already the image. The image differs from the annotation's own
    /// type exactly when the annotation names a type parameter the frames
    /// bind to something other than itself (`instantiateType` moves nothing
    /// else); the visitor already resolves every entity name, so it records
    /// that ([`ReuseContext::mapped`]) rather than resolving the annotation a
    /// second time with the frames lifted — which, for a recursive alias
    /// (`recursiveResolveTypeMembers`), re-enters the evaluation unbounded.
    /// The frames exist only while the literal is evaluated, so that walk is
    /// the one piece of the decision that runs here, and only under a frame.
    pub(crate) fn reuse_annotation(
        &mut self,
        node: TypeNode<'a>,
        equivalent: TypeId,
    ) -> Option<WrittenAnnotation> {
        let root = Node::from(node).node_id()?;
        if !self.alias_evaluation_bindings.is_empty() && !self.is_error(equivalent) {
            let mut cx = ReuseContext::new(None, root);
            self.try_reuse_type_node(node, false, &mut cx)?;
            if cx.mapped {
                return None;
            }
        }
        Some(WrittenAnnotation { node: root, r#type: equivalent, renamed: false })
    }

    /// Native prints a shadowing signature UNINSTANTIATED: the renaming is
    /// `typeParameterToName`'s, inside the node builder's scope
    /// (`signatureToSignatureDeclarationHelper` → `enterNewScope`,
    /// `nodebuilderscopes.go:59`), so every slot's reuse decision
    /// (`serializeTypeForDeclaration`, `serializeReturnTypeForSignature`) is
    /// asked of the ORIGINAL signature's types, and a reused node's
    /// type-parameter identifiers print their allocated names
    /// (`attachSymbolToLeftmostIdentifier`, `nodecopy.go:292`).
    /// `complexRecursiveCollections`' `Collection.Indexed<Z_1>` is that reuse.
    ///
    /// This port prints a print-only CLONE instantiated to fresh parameters
    /// (`rename_type_parameters_for_site`), and the instantiation ends every
    /// slot's reuse because the image differs. This re-attaches each written
    /// annotation the original could reuse (its slot holds the annotation's
    /// own type), keyed to the clone's image so every printer's identity gate
    /// holds, and marked [`WrittenAnnotation::renamed`] so the visitor spells
    /// the renamed parameters. Slots the original could not reuse stay as the
    /// clone left them.
    pub(crate) fn carry_written_annotations_through_rename(
        &mut self,
        original: &crate::signatures::Signature,
        renamed: &mut crate::signatures::Signature,
    ) {
        let pairs = original
            .this_parameter
            .iter()
            .zip(renamed.this_parameter.iter_mut())
            .chain(original.parameters.iter().zip(renamed.parameters.iter_mut()));
        let mut carried = Vec::new();
        for (index, (source, target)) in pairs.enumerate() {
            if target.written_text.is_some() {
                continue;
            }
            let Some(written) = source.written_text else { continue };
            if self.parameter_type(source) != written.r#type {
                continue;
            }
            carried.push((index, written.node, self.parameter_type(target)));
        }
        for (index, node, image) in carried {
            let target = if renamed.this_parameter.is_some() {
                match index {
                    0 => renamed.this_parameter.as_mut(),
                    _ => renamed.parameters.get_mut(index - 1),
                }
            } else {
                renamed.parameters.get_mut(index)
            };
            if let Some(target) = target {
                target.written_text =
                    Some(WrittenAnnotation { node, r#type: image, renamed: true });
            }
        }
        if renamed.written_return.is_none()
            && let Some(written) = original.written_return
            && self.get_return_type_of_signature(original).unwrap_or(original.r#type)
                == written.r#type
        {
            let image = self.get_return_type_of_signature(renamed).unwrap_or(renamed.r#type);
            renamed.written_return =
                Some(WrittenAnnotation { node: written.node, r#type: image, renamed: true });
        }
    }

    /// `reuseTypeNode` (`nodecopy.go:56`) for a printer with no print site:
    /// the written annotation re-emitted through the existing-node visitor
    /// from the annotation's own scope, each refused sub-node replaced by its
    /// fresh serialization. Such a printer stands for a site at the top level
    /// of the annotation's own file, which is where a declaration's signature
    /// is printed; a name only resolvable inside a narrower scope cannot be
    /// reused without the real site. `None` when the slot no longer holds the
    /// annotation's type or the node is refused: the slot is then serialized
    /// from its type.
    pub fn site_free_annotation_text(
        &mut self,
        written: WrittenAnnotation,
        current: TypeId,
    ) -> Option<String> {
        let (text, scope_local) = self.emit_from_annotation_scope(written, current)?;
        (!scope_local).then_some(text)
    }

    /// The visitor run with no print site: the text as seen from the
    /// annotation's own scope, and whether a name in it resolves only inside
    /// a scope narrower than its file's top level.
    fn emit_from_annotation_scope(
        &mut self,
        written: WrittenAnnotation,
        current: TypeId,
    ) -> Option<(String, bool)> {
        if !written.is_equivalent_to(current, self.intrinsics.error)
            || !self.renamed_annotation_in_scope(written)
        {
            return None;
        }
        let node = TypeNode::try_from(self.node_map.get(written.node)?).ok()?;
        let mut cx = ReuseContext::new(None, written.node);
        let text = self.try_reuse_type_node(node, false, &mut cx)?;
        if written.renamed && cx.declares_type_parameters {
            return None;
        }
        Some((text, cx.scope_local))
    }

    /// The reuse decision and the print at once, for a member whose text this
    /// port bakes where the member is built, from the member's own scope (a
    /// type literal's property signature, printed by
    /// `addPropertyToElementList` through `serializeTypeForDeclaration`,
    /// `nodebuilderimpl.go:2486`). The baked text the alternative
    /// `type_to_string` gives is the same inside view, so a scope-local name
    /// does not refuse the reuse here.
    pub(crate) fn reused_annotation_text(
        &mut self,
        node: TypeNode<'a>,
        equivalent: TypeId,
    ) -> Option<String> {
        // Under an alias-evaluation frame the literal is an alias body being
        // instantiated by re-resolution; the visitor's per-node fallback
        // (`get_type_from_type_node`) would re-enter that evaluation from
        // inside it (`flatArrayNoExcessiveStackDepth`'s recursive
        // `FlatArray`), so the image keeps its serialized text.
        if !self.alias_evaluation_bindings.is_empty() {
            return None;
        }
        let written = self.reuse_annotation(node, equivalent)?;
        Some(self.emit_from_annotation_scope(written, equivalent)?.0)
    }

    /// `typeToTypeNodeHelperWithPossibleReusableTypeNode(constraint,
    /// getConstraintDeclaration(parameter))` (`nodebuilderimpl.go:1597`,
    /// called by `typeParameterToDeclaration`, `:1615`): a type parameter's
    /// constraint reuses the first written `extends` node of the parameter's
    /// declarations (`getConstraintDeclaration`, `checker.go:29132`) when
    /// `getTypeFromTypeNode` of that node is the constraint being printed.
    /// An instantiated parameter's constraint is a different type, so it is
    /// serialized fresh. Printed at `reference` when the printer has a site,
    /// else from the annotation's own scope. `None` when the node is not the
    /// constraint's or the visitor refuses it.
    pub(crate) fn reused_constraint_text(
        &mut self,
        parameter: TypeId,
        constraint: TypeId,
        reference: Option<NodeId>,
    ) -> Option<String> {
        let symbol = *self.type_parameter_symbols.get(&parameter)?;
        let node =
            self.binder.symbols().get(symbol).declarations.iter().find_map(|&id| {
                match self.node_map.get(id)? {
                    Node::TypeParameterDeclaration(declaration) => declaration.constraint,
                    _ => None,
                }
            })?;
        if self.get_type_from_type_node(node) != constraint {
            return None;
        }
        let written = WrittenAnnotation {
            node: Node::from(node).node_id()?,
            r#type: constraint,
            renamed: false,
        };
        match reference {
            Some(reference) => self.written_annotation_text_at(written, constraint, reference),
            None => self.site_free_annotation_text(written, constraint),
        }
    }

    /// The reused annotation as printed at `reference`
    /// (`ctx.enclosingDeclaration`), so `trackExistingEntityName` answers for
    /// this site. `None` when the slot no longer holds the annotation's type,
    /// or the node is refused at this site.
    pub fn written_annotation_text_at(
        &mut self,
        written: WrittenAnnotation,
        current: TypeId,
        reference: NodeId,
    ) -> Option<String> {
        if !written.is_equivalent_to(current, self.intrinsics.error)
            || !self.renamed_annotation_in_scope(written)
        {
            return None;
        }
        let node = TypeNode::try_from(self.node_map.get(written.node)?).ok()?;
        let mut cx = ReuseContext::new(Some(reference), written.node);
        let text = self.try_reuse_type_node(node, false, &mut cx)?;
        if written.renamed && cx.declares_type_parameters {
            return None;
        }
        (!cx.unnameable).then_some(text)
    }

    /// A [`WrittenAnnotation::renamed`] node names its renamed parameters
    /// from the render's allocations, which live only while the rename
    /// clone's render runs; printed outside one, the clone's image is
    /// serialized instead. A renamed node that declares type parameters of
    /// its own is likewise refused after the walk
    /// ([`ReuseContext::declares_type_parameters`]): its inner declarations
    /// would need their own allocation against the renamed scope
    /// (`jsxGenericComponentWithSpreadingResultOfGenericFunction`'s
    /// `<T_1>(obj: T_1) => Omit<T_1, K_1>`), which the visitor does not model,
    /// and the clone's serialization already renames them.
    fn renamed_annotation_in_scope(&self, written: WrittenAnnotation) -> bool {
        !written.renamed || !self.render_type_parameter_names.allocations.is_empty()
    }

    /// `trackExistingEntityName` (`nodecopy.go:317`) for the leftmost
    /// identifier of a written entity name: `false` is `introducesError`.
    ///
    /// A type parameter, or a name bound inside the reused node itself (a
    /// function type's parameter), resolves wherever the node is printed
    /// (`enterNewScope`). Otherwise the name must resolve to the same symbol
    /// at the print site (`getSymbolIfSameReference`, which compares merged
    /// alias targets); with no print site the annotation's own scope
    /// answers, and a non-global name marks the text site-dependent.
    fn track_existing_entity_name(
        &mut self,
        name: EntityName<'a>,
        is_type_of: bool,
        cx: &mut ReuseContext,
    ) -> bool {
        let mut leftmost = name;
        while let EntityName::QualifiedName(qualified) = leftmost {
            let Some(left) = qualified.left else { return true };
            leftmost = left;
        }
        let EntityName::Identifier(identifier) = leftmost else { return true };
        let Some(id) = identifier.node_id else { return true };
        // `getMeaningOfEntityNameReference` (`emitresolver.go:311`).
        let meaning = if is_type_of {
            SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE
        } else if matches!(name, EntityName::QualifiedName(_)) {
            SymbolFlags::NAMESPACE
        } else {
            SymbolFlags::TYPE
        };
        let text = identifier.text;
        let Some(symbol) = self.binder.resolve_name(self.nodes, self.node_map, id, text, meaning)
        else {
            return true;
        };
        if self.declared_inside_reused_node(symbol, cx.root) {
            return true;
        }
        // An active alias-evaluation frame that binds this name to anything
        // but its own declared type moves the annotation (see
        // `reuse_annotation`).
        cx.mapped |= !self.alias_evaluation_bindings.is_empty()
            && self
                .alias_evaluation_bindings
                .iter()
                .rev()
                .find_map(|frame| frame.get(&symbol))
                .is_some_and(|bound| self.declared_types.get(&symbol) != Some(bound));
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER) {
            // Named at the site by `typeParameterToName`; see the
            // `TypeReferenceNode` arm.
            return true;
        }
        let Some(site) = cx.site else {
            // With no print site the annotation's own file answers: a name
            // only resolvable inside a narrower scope marks the text
            // scope-local.
            if !self.is_global_name(symbol, text, meaning) {
                cx.scope_local |= !self.resolves_from_file_top_level(id, symbol, text, meaning);
            }
            return true;
        };
        let Some(at_site) =
            self.binder.resolve_name(self.nodes, self.node_map, site, text, meaning)
        else {
            return false;
        };
        // `getSymbolIfSameReference` compares merged alias targets; an
        // exported declaration's local marker and its export symbol are one
        // symbol upstream (`getExportSymbolOfValueSymbolIfExported`).
        let same = |checker: &mut Self, symbol| {
            let target = checker.resolve_alias_fully(symbol);
            let record = checker.binder.symbols().get(target);
            checker.binder.merged_symbol(record.export_symbol.unwrap_or(target))
        };
        if same(self, at_site) != same(self, symbol) {
            return false;
        }
        // "If a parameter is resolvable in the current context it is also
        // visible"; anything else must also be accessible
        // (`IsSymbolAccessible`), whose declaration half is
        // `hasVisibleDeclarations` on the symbol the name found.
        let flags = self.binder.symbols().get(at_site).flags;
        (flags.contains(SymbolFlags::FUNCTION_SCOPED_VARIABLE)
            && self
                .binder
                .symbols()
                .get(at_site)
                .value_declaration
                .is_some_and(|declaration| self.is_parameter_or_parameter_binding(declaration)))
            || self.has_visible_declarations(at_site)
    }

    /// `serializeTypeName` (`nodebuilderimpl.go:436`): the entity's symbol
    /// named from the print site through `symbolToTypeNode`'s
    /// `lookupSymbolChain`, or `None` when it cannot be. The naming is this
    /// port's site renderer's: a type through [`Checker::reference_text_at`]
    /// (accessible alias chains, renames, container qualification), a value
    /// through [`Checker::best_name`] then [`Checker::symbol_chain`] (with the
    /// `globalThis` arm of `trySymbolTable`), and a module member through the
    /// import-route namer [`Checker::parameter_source_symbol_name_at`]. A
    /// name the port cannot spell marks the context
    /// [`ReuseContext::unnameable`].
    fn serialize_type_name(
        &mut self,
        name: EntityName<'a>,
        is_type_of: bool,
        cx: &mut ReuseContext,
    ) -> Option<String> {
        let site = cx.site?;
        let meaning = if is_type_of { SymbolFlags::VALUE } else { SymbolFlags::TYPE };
        let symbol = self.resolve_entity_name(name, meaning)?;
        let symbol = self.resolve_alias_fully(symbol);
        // `getExportSymbolOfValueSymbolIfExported`: a namespace's exported
        // value resolves to its local marker, whose container is reached
        // through the export symbol (`getParentOfSymbol`).
        let record = self.binder.symbols().get(symbol);
        let symbol = self.binder.merged_symbol(record.export_symbol.unwrap_or(symbol));
        let record = self.binder.symbols().get(symbol);
        let text = record.name;
        // `IsSymbolAccessible` (`symbolaccessibility.go:26`): a symbol whose
        // declarations are not visible from outside (a function-local alias)
        // cannot be named; the visitor serializes the node from its type.
        if !self.has_visible_declarations(symbol) {
            return None;
        }
        let module_member = record.parent.is_some_and(|parent| {
            self.binder.symbols().get(parent).declarations.iter().any(|&declaration| {
                self.nodes.kind(declaration) == SyntaxKind::SourceFile
                    || matches!(self.node_map.get(declaration), Some(Node::ModuleDeclaration(module))
                        if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_))))
            })
        });
        let named = if module_member {
            self.parameter_source_symbol_name_at(symbol, site, meaning, 0)
        } else if !is_type_of {
            self.reference_text_at(symbol, &[], site)
        } else if !self.needs_qualification(symbol, text, site, meaning) {
            Some(text.to_string())
        } else if let Some(better) = self.best_name(symbol, site).filter(|better| better != text) {
            Some(better)
        } else {
            self.symbol_chain(symbol, site, meaning, 0).map(|prefix| format!("{prefix}{text}"))
        };
        cx.unnameable |= named.is_none();
        named
    }

    /// `IsParameterDeclaration(WalkUpBindingElementsAndPatterns(node))`.
    fn is_parameter_or_parameter_binding(&self, declaration: NodeId) -> bool {
        self.parameter_of_binding(declaration).is_some()
    }

    /// Whether a declaration carries `export` — on itself, or on the
    /// `VariableStatement` that owns a variable declaration
    /// (`getCombinedModifierFlags`).
    fn has_export_keyword(&self, declaration: NodeId) -> bool {
        let mut current = declaration;
        if matches!(self.nodes.kind(current), SyntaxKind::VariableDeclaration)
            && let Some(list) = self.nodes.parent(current)
            && let Some(statement) = self.nodes.parent(list)
        {
            current = statement;
        }
        self.node_map
            .get(current)
            .and_then(crate::check::modifiers_of)
            .is_some_and(|modifiers| {
                modifiers.iter().any(|modifier| {
                    matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::ExportKeyword)
                })
            })
    }

    /// `ast.GetDeclarationContainer`: the parent of the nearest ancestor that
    /// is not part of a variable or import declaration.
    fn declaration_container(&self, declaration: NodeId) -> Option<NodeId> {
        let mut current = declaration;
        while matches!(
            self.nodes.kind(current),
            SyntaxKind::VariableDeclaration
                | SyntaxKind::VariableDeclarationList
                | SyntaxKind::ImportSpecifier
                | SyntaxKind::NamedImports
                | SyntaxKind::NamespaceImport
                | SyntaxKind::ImportClause
                | SyntaxKind::BindingElement
                | SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern
        ) {
            current = self.nodes.parent(current)?;
        }
        self.nodes.parent(current)
    }

    fn is_global_source_file(&self, node: NodeId) -> bool {
        matches!(self.node_map.get(node), Some(Node::SourceFile(source))
            if !tsr_binder::is_external_module(source))
    }

    fn has_modifier_keyword(&self, declaration: NodeId, kinds: &[SyntaxKind]) -> bool {
        let modifiers = match self.node_map.get(declaration) {
            Some(Node::PropertySignatureDeclaration(node)) => node.modifiers,
            Some(Node::MethodSignatureDeclaration(node)) => node.modifiers,
            Some(node) => crate::check::modifiers_of(node).unwrap_or(&[]),
            None => &[],
        };
        modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if kinds.contains(&token.kind))
        })
    }

    /// `isDeclarationVisible` / `determineIfDeclarationIsVisible`
    /// (`emitresolver.go:111`, `emitresolver.go:131`).
    fn is_declaration_visible(&self, node: NodeId) -> bool {
        match self.nodes.kind(node) {
            SyntaxKind::BindingElement => self
                .nodes
                .parent(node)
                .and_then(|pattern| self.nodes.parent(pattern))
                .is_some_and(|owner| self.is_declaration_visible(owner)),
            SyntaxKind::VariableDeclaration
            | SyntaxKind::ModuleDeclaration
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::ImportEqualsDeclaration => {
                let Some(container) = self.declaration_container(node) else { return false };
                // `IsExternalModuleAugmentation`: `declare module "m"` inside a
                // module file is always visible.
                if let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node)
                    && matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                    && matches!(self.node_map.get(container), Some(Node::SourceFile(source))
                        if tsr_binder::is_external_module(source))
                {
                    return true;
                }
                let ambient_member = self.nodes.kind(node) != SyntaxKind::ImportEqualsDeclaration
                    && self.nodes.kind(container) != SyntaxKind::SourceFile
                    && self.nodes.flags(container).contains(tsr_ast::NodeFlags::AMBIENT);
                if !self.has_export_keyword(node) && !ambient_member {
                    return self.is_global_source_file(container);
                }
                self.is_declaration_visible(container)
            }
            SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::MethodSignature => {
                !self.has_modifier_keyword(
                    node,
                    &[SyntaxKind::PrivateKeyword, SyntaxKind::ProtectedKeyword],
                ) && self
                    .nodes
                    .parent(node)
                    .is_some_and(|parent| self.is_declaration_visible(parent))
            }
            SyntaxKind::Constructor
            | SyntaxKind::ConstructSignature
            | SyntaxKind::CallSignature
            | SyntaxKind::IndexSignature
            | SyntaxKind::Parameter
            | SyntaxKind::ModuleBlock
            | SyntaxKind::FunctionType
            | SyntaxKind::ConstructorType
            | SyntaxKind::TypeLiteral
            | SyntaxKind::TypeReference
            | SyntaxKind::ArrayType
            | SyntaxKind::TupleType
            | SyntaxKind::UnionType
            | SyntaxKind::IntersectionType
            | SyntaxKind::ParenthesizedType
            | SyntaxKind::NamedTupleMember => {
                self.nodes.parent(node).is_some_and(|parent| self.is_declaration_visible(parent))
            }
            SyntaxKind::TypeParameter
            | SyntaxKind::SourceFile
            | SyntaxKind::NamespaceExportDeclaration => true,
            _ => false,
        }
    }

    /// `hasVisibleDeclarations` (`emitresolver.go:384`), answering only
    /// whether the symbol is accessible: an unexported import, variable
    /// statement or top-level declaration is made visible when its container
    /// is.
    fn has_visible_declarations(&self, symbol: tsr_binder::SymbolId) -> bool {
        let record = self.binder.symbols().get(symbol);
        for &declaration in &record.declarations {
            let kind = self.nodes.kind(declaration);
            if kind == SyntaxKind::Identifier || self.is_declaration_visible(declaration) {
                continue;
            }
            // `getAnyImportSyntax`: the import statement owning an import
            // clause, namespace import, specifier or import-equals.
            let mut import_statement = None;
            let mut current = declaration;
            loop {
                match self.nodes.kind(current) {
                    // A JSDoc `@import` is native's reparsed
                    // `JSImportDeclaration`, an `ImportDeclaration` kind.
                    SyntaxKind::ImportEqualsDeclaration
                    | SyntaxKind::ImportDeclaration
                    | SyntaxKind::JSDocImportTag => {
                        import_statement = Some(current);
                        break;
                    }
                    SyntaxKind::ImportSpecifier
                    | SyntaxKind::NamedImports
                    | SyntaxKind::NamespaceImport
                    | SyntaxKind::ImportClause => match self.nodes.parent(current) {
                        Some(parent) => current = parent,
                        None => break,
                    },
                    _ => break,
                }
            }
            if let Some(statement) = import_statement
                && !self.has_export_keyword(statement)
                && if self.nodes.kind(statement) == SyntaxKind::JSDocImportTag {
                    self.jsdoc_import_declaration_parent(statement)
                } else {
                    self.nodes.parent(statement)
                }
                .is_some_and(|parent| self.is_declaration_visible(parent))
            {
                continue;
            }
            if kind == SyntaxKind::VariableDeclaration
                && let Some(list) = self.nodes.parent(declaration)
                && let Some(statement) = self.nodes.parent(list)
                && self.nodes.kind(statement) == SyntaxKind::VariableStatement
                && !self.has_export_keyword(statement)
                && self
                    .nodes
                    .parent(statement)
                    .is_some_and(|parent| self.is_declaration_visible(parent))
            {
                continue;
            }
            // `IsLateVisibilityPaintedStatement` (`ast/utilities.go:3548`).
            if matches!(
                kind,
                SyntaxKind::ClassDeclaration
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::ModuleDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::EnumDeclaration
                    | SyntaxKind::ImportEqualsDeclaration
            ) && !self.has_export_keyword(declaration)
                && self
                    .nodes
                    .parent(declaration)
                    .is_some_and(|parent| self.is_declaration_visible(parent))
            {
                continue;
            }
            if kind == SyntaxKind::BindingElement
                && record.flags.contains(SymbolFlags::BLOCK_SCOPED_VARIABLE)
            {
                let mut root = declaration;
                while matches!(
                    self.nodes.kind(root),
                    SyntaxKind::BindingElement
                        | SyntaxKind::ObjectBindingPattern
                        | SyntaxKind::ArrayBindingPattern
                ) {
                    let Some(parent) = self.nodes.parent(root) else { return false };
                    root = parent;
                }
                let statement = self.nodes.parent(root).and_then(|list| self.nodes.parent(list));
                if let Some(statement) = statement
                    && self.nodes.kind(statement) == SyntaxKind::VariableStatement
                    && (self.has_export_keyword(statement)
                        || self
                            .nodes
                            .parent(statement)
                            .is_some_and(|parent| self.is_declaration_visible(parent)))
                {
                    continue;
                }
                return false;
            }
            return false;
        }
        true
    }

    /// Whether `symbol` resolves wherever the reused node is printed without
    /// consulting the site: it is declared inside the node itself, or it is a
    /// parameter (or a binding element of one) of a function-like node that
    /// encloses the annotation — the printed signature's own parameters,
    /// which `enterSignatureScope`/`enterNewScope`
    /// (`nodebuilderscopes.go:53`) put in scope.
    fn declared_inside_reused_node(&self, symbol: tsr_binder::SymbolId, root: NodeId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return false;
        };
        if self.is_ancestor_or_self(root, declaration) {
            return true;
        }
        let Some(parameter) = self.parameter_of_binding(declaration) else { return false };
        self.nodes.parent(parameter).is_some_and(|owner| self.is_ancestor_or_self(owner, root))
    }

    /// `WalkUpBindingElementsAndPatterns`, answering the `Parameter` it
    /// reaches, if it reaches one.
    fn parameter_of_binding(&self, declaration: NodeId) -> Option<NodeId> {
        let mut current = declaration;
        while matches!(
            self.nodes.kind(current),
            SyntaxKind::BindingElement
                | SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern
        ) {
            current = self.nodes.parent(current)?;
        }
        (self.nodes.kind(current) == SyntaxKind::Parameter).then_some(current)
    }

    /// `ast.IsInJSDoc`: a JSDoc node encloses `node` within its file.
    fn has_jsdoc_ancestor(&self, node: NodeId) -> bool {
        let mut current = self.nodes.parent(node);
        while let Some(ancestor) = current {
            let kind = self.nodes.kind(ancestor);
            if (SyntaxKind::JSDocTypeExpression..=SyntaxKind::JSDocImportTag).contains(&kind) {
                return true;
            }
            if kind == SyntaxKind::SourceFile {
                return false;
            }
            current = self.nodes.parent(ancestor);
        }
        false
    }

    fn is_ancestor_or_self(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = node;
        loop {
            if current == ancestor {
                return true;
            }
            match self.nodes.parent(current) {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    /// Whether `text` names `symbol` from the top level of the file holding
    /// the annotation node `id`.
    fn resolves_from_file_top_level(
        &self,
        id: NodeId,
        symbol: tsr_binder::SymbolId,
        text: &str,
        meaning: SymbolFlags,
    ) -> bool {
        self.source_file_of(id)
            .and_then(|file| {
                self.binder.resolve_name(self.nodes, self.node_map, file, text, meaning)
            })
            .is_some_and(|found| {
                self.binder.merged_symbol(found) == self.binder.merged_symbol(symbol)
            })
    }

    /// Whether `symbol` is what `text` names from every file's top level: it
    /// is declared in a script (not a module) and resolves from that file's
    /// root to itself.
    fn is_global_name(
        &self,
        symbol: tsr_binder::SymbolId,
        text: &str,
        meaning: SymbolFlags,
    ) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return false;
        };
        let Some(file) = self.source_file_of(declaration) else { return false };
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return false };
        if tsr_binder::is_external_module(source) {
            return false;
        }
        self.binder.resolve_name(self.nodes, self.node_map, file, text, meaning).is_some_and(
            |global| self.binder.merged_symbol(global) == self.binder.merged_symbol(symbol),
        )
    }

    /// `Printer.emitTypeNode` (`printer/printer.go:2274`) over the visitor's
    /// result: parenthesise when the emitted node binds looser than its
    /// position allows. The `extends` clause of a conditional raises a
    /// conditional's requirement to function precedence.
    fn emit_reused_type(
        &mut self,
        node: TypeNode<'a>,
        precedence: Precedence,
        in_extends: bool,
        cx: &mut ReuseContext,
    ) -> String {
        let precedence = if in_extends && precedence <= Precedence::Conditional {
            Precedence::Function
        } else {
            precedence
        };
        let parenthesized = node_precedence(node) < precedence;
        if let Some(text) = self.try_reuse_type_node(node, in_extends && !parenthesized, cx) {
            return if parenthesized { format!("({text})") } else { text };
        }
        // The visitor's recovery arm (`nodecopy.go:866`): a type node
        // whose subtree marked an error is serialized from its type instead.
        let resolved = self.get_type_from_type_node(node);
        let text = if resolved == self.intrinsics.error {
            // `typeToTypeNode(errorType)` prints the `any` keyword.
            "any".to_string()
        } else if let Some(site) = cx.site {
            self.type_to_string_at(resolved, site).unwrap_or_else(|| self.type_to_string(resolved))
        } else {
            self.type_to_string(resolved)
        };
        if text_precedence(&text) < precedence { format!("({text})") } else { text }
    }

    /// One node through `visitExistingNodeTreeSymbolsWorker`
    /// (`nodecopy.go:468`) and the emitter. `None` is the recovery boundary's
    /// `markError`: the node is not reusable as written.
    fn try_reuse_type_node(
        &mut self,
        node: TypeNode<'a>,
        in_extends: bool,
        cx: &mut ReuseContext,
    ) -> Option<String> {
        const LOWEST: Precedence = Precedence::Conditional;
        Some(match node {
            TypeNode::KeywordTypeNode(keyword) => match keyword.kind {
                SyntaxKind::AnyKeyword => "any",
                SyntaxKind::UnknownKeyword => "unknown",
                SyntaxKind::NumberKeyword => "number",
                SyntaxKind::BigIntKeyword => "bigint",
                SyntaxKind::ObjectKeyword => "object",
                SyntaxKind::BooleanKeyword => "boolean",
                SyntaxKind::StringKeyword => "string",
                SyntaxKind::SymbolKeyword => "symbol",
                SyntaxKind::VoidKeyword => "void",
                SyntaxKind::UndefinedKeyword => "undefined",
                SyntaxKind::NeverKeyword => "never",
                SyntaxKind::IntrinsicKeyword => "intrinsic",
                _ => return None,
            }
            .to_string(),
            TypeNode::ThisTypeNode(_) => "this".to_string(),
            TypeNode::LiteralTypeNode(literal) => Self::reused_literal_text(literal.literal?)?,
            TypeNode::TypeReferenceNode(reference) => {
                let name = reference.type_name?;
                // `nodecopy.go:547`: a reference with an empty name (a
                // JSDoc `*`-like hole) becomes `any`.
                if let EntityName::Identifier(identifier) = name
                    && identifier.text.is_empty()
                {
                    return Some("any".to_string());
                }
                if !self.can_reuse_existing_js_type_node(reference) {
                    return None;
                }
                // `attachSymbolToLeftmostIdentifier` names a type parameter
                // through `typeParameterToName`, and `tryVisitTypeReference`
                // serializes one the context's mapper remaps
                // (`nodecopy.go:416`); both are the site's own rendering of
                // the parameter. A name the render already allocated
                // (`ctx.typeParameterNames`, a shadow rename) is that name
                // with or without a site.
                if reference.type_arguments.is_empty()
                    && let EntityName::Identifier(identifier) = name
                    && let Some(id) = identifier.node_id
                    && !self.render_type_parameter_names.allocations.is_empty()
                    && let Some(symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        identifier.text,
                        SymbolFlags::TYPE,
                    )
                    && self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER)
                    && !self.declared_inside_reused_node(symbol, cx.root)
                {
                    let parameter = self.get_type_from_type_node(node);
                    if let Some((_, allocated)) = self
                        .render_type_parameter_names
                        .allocations
                        .iter()
                        .find(|(owner, _)| *owner == parameter)
                    {
                        return Some(allocated.clone());
                    }
                }
                if let Some(site) = cx.site
                    && reference.type_arguments.is_empty()
                    && let EntityName::Identifier(identifier) = name
                    && let Some(id) = identifier.node_id
                    && let Some(symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        identifier.text,
                        SymbolFlags::TYPE,
                    )
                    && self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER)
                    && !self.declared_inside_reused_node(symbol, cx.root)
                {
                    let parameter = self.get_type_from_type_node(node);
                    return self.type_to_string_at(parameter, site);
                }
                // `tryVisitTypeReference` (`nodecopy.go:416`): an entity name
                // that does not survive the move is serialized from its type.
                let mut text = if self.track_existing_entity_name(name, false, cx) {
                    entity_name_text(name)?
                } else {
                    self.serialize_type_name(name, false, cx)?
                };
                self.push_reused_type_arguments(&mut text, reference.type_arguments, cx);
                text
            }
            // `tryVisitTypeQuery` (`nodecopy.go:400`).
            TypeNode::TypeQueryNode(query) => {
                let name = query.expr_name?;
                let written = if self.track_existing_entity_name(name, true, cx) {
                    entity_name_text(name)?
                } else {
                    self.serialize_type_name(name, true, cx)?
                };
                let mut text = format!("typeof {written}");
                self.push_reused_type_arguments(&mut text, query.type_arguments, cx);
                text
            }
            // `Printer.emitTypePredicate` (`printer/printer.go:1869`). The
            // parameter name is bound by the function-like node around it.
            TypeNode::TypePredicateNode(predicate) => {
                let mut text = String::new();
                if predicate.asserts_modifier.is_some() {
                    text.push_str("asserts ");
                }
                match predicate.parameter_name? {
                    tsr_ast::TypePredicateParameterName::Identifier(name) => {
                        text.push_str(name.text);
                    }
                    tsr_ast::TypePredicateParameterName::ThisTypeNode(_) => text.push_str("this"),
                }
                if let Some(r#type) = predicate.r#type {
                    text.push_str(" is ");
                    text.push_str(&self.emit_reused_type(r#type, LOWEST, false, cx));
                }
                text
            }
            TypeNode::ImportTypeNode(import) => {
                // `nodecopy.go:614`; an attributes clause is not
                // reproduced by this printer.
                if import.attributes.is_some() {
                    return None;
                }
                let TypeNode::LiteralTypeNode(argument) = import.argument? else { return None };
                let mut text = String::new();
                if import.is_type_of {
                    text.push_str("typeof ");
                }
                text.push_str("import(");
                text.push_str(&Self::reused_literal_text(argument.literal?)?);
                text.push(')');
                if let Some(qualifier) = import.qualifier {
                    text.push('.');
                    text.push_str(&entity_name_text(qualifier)?);
                }
                self.push_reused_type_arguments(&mut text, import.type_arguments, cx);
                text
            }
            TypeNode::ArrayTypeNode(array) => {
                let element =
                    self.emit_reused_type(array.element_type?, Precedence::Postfix, in_extends, cx);
                format!("{element}[]")
            }
            TypeNode::IndexedAccessTypeNode(access) => {
                let object =
                    self.emit_reused_type(access.object_type?, Precedence::Postfix, in_extends, cx);
                let index = self.emit_reused_type(access.index_type?, LOWEST, false, cx);
                format!("{object}[{index}]")
            }
            TypeNode::OptionalTypeNode(optional) => {
                let inner =
                    self.emit_reused_type(optional.r#type?, Precedence::Postfix, in_extends, cx);
                format!("{inner}?")
            }
            TypeNode::RestTypeNode(rest) => {
                format!("...{}", self.emit_reused_type(rest.r#type?, LOWEST, false, cx))
            }
            TypeNode::NamedTupleMember(member) => {
                let dots = if member.dot_dot_dot_token.is_some() { "..." } else { "" };
                let question = if member.question_token.is_some() { "?" } else { "" };
                let inner = self.emit_reused_type(member.r#type?, LOWEST, false, cx);
                format!("{dots}{}{question}: {inner}", member.name?.text)
            }
            TypeNode::TupleTypeNode(tuple) => {
                let mut parts = Vec::with_capacity(tuple.elements.len());
                for element in tuple.elements {
                    parts.push(self.emit_reused_type(*element, LOWEST, false, cx));
                }
                format!("[{}]", parts.join(", "))
            }
            TypeNode::UnionTypeNode(union) => {
                let mut parts = Vec::with_capacity(union.types.len());
                for constituent in union.types {
                    parts.push(self.emit_reused_type(
                        *constituent,
                        Precedence::TypeOperator,
                        in_extends,
                        cx,
                    ));
                }
                parts.join(" | ")
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                let mut parts = Vec::with_capacity(intersection.types.len());
                for constituent in intersection.types {
                    parts.push(self.emit_reused_type(
                        *constituent,
                        Precedence::TypeOperator,
                        in_extends,
                        cx,
                    ));
                }
                parts.join(" & ")
            }
            TypeNode::ParenthesizedTypeNode(paren) => {
                format!("({})", self.emit_reused_type(paren.r#type?, LOWEST, false, cx))
            }
            TypeNode::TypeOperatorNode(operator)
                if operator.operator.kind == SyntaxKind::UniqueKeyword
                    && matches!(operator.r#type, Some(TypeNode::KeywordTypeNode(keyword))
                        if keyword.kind == SyntaxKind::SymbolKeyword) =>
            {
                // `nodecopy.go:596`: `unique symbol` is reused only when the
                // node lies inside the print's enclosing declaration (the
                // `.types` writer's is the asserted node's parent); elsewhere
                // it is serialized from its type.
                let enclosing = cx.site.and_then(|site| self.nodes.parent(site))?;
                let id = operator.node_id?;
                self.is_ancestor_or_self(enclosing, id).then(|| "unique symbol".to_string())?
            }
            TypeNode::TypeOperatorNode(operator) => {
                let (keyword, operand_precedence) = match operator.operator.kind {
                    SyntaxKind::KeyOfKeyword => ("keyof", Precedence::TypeOperator),
                    SyntaxKind::ReadonlyKeyword => ("readonly", Precedence::Postfix),
                    // `nodecopy.go:596`: `unique symbol` is reused only
                    // inside its own declaration's scope, which a print of a
                    // signature is not.
                    _ => return None,
                };
                let operand =
                    self.emit_reused_type(operator.r#type?, operand_precedence, in_extends, cx);
                format!("{keyword} {operand}")
            }
            TypeNode::FunctionTypeNode(function) => {
                let mut text = self.reused_type_parameters(function.type_parameters, cx)?;
                text.push_str(&self.reused_parameters(function.parameters, cx)?);
                text.push_str(" => ");
                text.push_str(&self.reused_return_type(function.r#type, in_extends, cx));
                text
            }
            TypeNode::ConstructorTypeNode(constructor) => {
                let mut text = modifiers_prefix(constructor.modifiers)?;
                text.push_str("new ");
                text.push_str(&self.reused_type_parameters(constructor.type_parameters, cx)?);
                text.push_str(&self.reused_parameters(constructor.parameters, cx)?);
                text.push_str(" => ");
                text.push_str(&self.reused_return_type(constructor.r#type, in_extends, cx));
                text
            }
            TypeNode::TypeLiteralNode(literal) => self.reused_type_members(literal.members, cx)?,
            TypeNode::MappedTypeNode(mapped) => {
                if !mapped.members.is_empty() {
                    return None;
                }
                let parameter = mapped.type_parameter?;
                cx.declares_type_parameters = true;
                let readonly = match mapped.readonly_token.map(|token| token.kind) {
                    None => "",
                    Some(SyntaxKind::ReadonlyKeyword) => "readonly ",
                    Some(SyntaxKind::PlusToken) => "+readonly ",
                    Some(SyntaxKind::MinusToken) => "-readonly ",
                    Some(_) => return None,
                };
                let question = match mapped.question_token.map(|token| token.kind) {
                    None => "",
                    Some(SyntaxKind::QuestionToken) => "?",
                    Some(SyntaxKind::PlusToken) => "+?",
                    Some(SyntaxKind::MinusToken) => "-?",
                    Some(_) => return None,
                };
                let constraint = self.emit_reused_type(parameter.constraint?, LOWEST, false, cx);
                let remapped = match mapped.name_type {
                    Some(name_type) => {
                        format!(" as {}", self.emit_reused_type(name_type, LOWEST, false, cx))
                    }
                    None => String::new(),
                };
                let template = self.emit_reused_type(mapped.r#type?, LOWEST, false, cx);
                format!(
                    "{{ {readonly}[{} in {constraint}{remapped}]{question}: {template}; }}",
                    parameter.name?.text
                )
            }
            // `Printer.emitConditionalType` (`printer/printer.go:2058`).
            TypeNode::ConditionalTypeNode(conditional) => {
                let check =
                    self.emit_reused_type(conditional.check_type?, Precedence::Union, false, cx);
                let extends = self.emit_reused_type(conditional.extends_type?, LOWEST, true, cx);
                let true_type = self.emit_reused_type(conditional.true_type?, LOWEST, false, cx);
                let false_type = self.emit_reused_type(conditional.false_type?, LOWEST, false, cx);
                format!("{check} extends {extends} ? {true_type} : {false_type}")
            }
            TypeNode::InferTypeNode(infer) => {
                let parameter = infer.type_parameter?;
                cx.declares_type_parameters = true;
                let mut text = format!("infer {}", parameter.name?.text);
                if let Some(constraint) = parameter.constraint {
                    text.push_str(" extends ");
                    text.push_str(&self.emit_reused_type(constraint, LOWEST, true, cx));
                }
                text
            }
            TypeNode::TemplateLiteralTypeNode(template) => {
                let mut text = format!("`{}", Self::escape_template_text(template.head?.text));
                for span in template.template_spans {
                    text.push_str("${");
                    text.push_str(&self.emit_reused_type(span.r#type?, LOWEST, false, cx));
                    text.push('}');
                    text.push_str(&Self::escape_template_text(match span.literal? {
                        tsr_ast::TemplateMiddleOrTail::TemplateMiddle(node) => node.text,
                        tsr_ast::TemplateMiddleOrTail::TemplateTail(node) => node.text,
                    }));
                }
                text.push('`');
                text
            }
            // The JSDoc arms of `visitExistingNodeTreeSymbolsWorker`
            // (`nodecopy.go:475`).
            TypeNode::JSDocTypeExpression(expression) => {
                self.emit_reused_type(expression.r#type?, LOWEST, in_extends, cx)
            }
            TypeNode::JSDocAllType(_) => "any".to_string(),
            TypeNode::JSDocNonNullableType(non_null) => {
                self.emit_reused_type(non_null.r#type?, LOWEST, in_extends, cx)
            }
            TypeNode::JSDocNullableType(nullable) => {
                let inner = self.emit_reused_type(
                    nullable.r#type?,
                    Precedence::TypeOperator,
                    in_extends,
                    cx,
                );
                format!("{inner} | null")
            }
            TypeNode::JSDocOptionalType(optional) => {
                let inner = self.emit_reused_type(
                    optional.r#type?,
                    Precedence::TypeOperator,
                    in_extends,
                    cx,
                );
                format!("{inner} | undefined")
            }
            TypeNode::JSDocVariadicType(variadic) => {
                let inner =
                    self.emit_reused_type(variadic.r#type?, Precedence::Postfix, in_extends, cx);
                format!("{inner}[]")
            }
            _ => return None,
        })
    }

    /// `canReuseExistingJSTypeNode` (`nodebuilderimpl.go:501`): a JSDoc
    /// reference the checker remaps (`getIntendedTypeFromJSDocTypeReference`,
    /// `checker.go:23020`), or a JS reference written with fewer type
    /// arguments than its target requires (`Foo` meaning `Foo<any>`,
    /// `existingTypeNodeIsNotReferenceOrIsReferenceWithCompatibleTypeArgumentCount`,
    /// `nodebuilderimpl.go:518`), is serialized rather than reused.
    fn can_reuse_existing_js_type_node(
        &mut self,
        reference: &tsr_ast::TypeReferenceNode<'a>,
    ) -> bool {
        let Some(id) = reference.node_id else { return true };
        // `NodeFlagsJSDoc` is a parse flag this parser does not set: a JSDoc
        // type is the one with a JSDoc ancestor (as
        // `get_type_from_type_reference` asks it). A JSDoc node's parent chain
        // does not reach its file's flags, so `in_js_file` misses it.
        let jsdoc = self.has_jsdoc_ancestor(id);
        if !jsdoc && !self.in_js_file(id) {
            return true;
        }
        if jsdoc && let Some(EntityName::Identifier(identifier)) = reference.type_name {
            let arguments = reference.type_arguments.len();
            let remapped = match identifier.text {
                "String" | "Number" | "BigInt" | "Boolean" | "Void" | "Undefined" | "Null"
                | "Function" | "function" => true,
                "array" | "promise" => arguments == 0 && !self.no_implicit_any,
                "Object" => arguments == 2 || !self.no_implicit_any,
                _ => false,
            };
            if remapped {
                return false;
            }
        }
        let Some(symbol) = reference
            .type_name
            .and_then(|name| self.resolve_entity_name(name, tsr_binder::SymbolFlags::TYPE))
        else {
            return true;
        };
        let required = self
            .local_type_parameters_of(symbol)
            .iter()
            .rposition(|parameter| parameter.default_type.is_none())
            .map_or(0, |index| index + 1);
        reference.type_arguments.len() >= required
    }

    fn push_reused_type_arguments(
        &mut self,
        text: &mut String,
        arguments: &[TypeNode<'a>],
        cx: &mut ReuseContext,
    ) {
        if arguments.is_empty() {
            return;
        }
        let mut parts = Vec::with_capacity(arguments.len());
        for argument in arguments {
            parts.push(self.emit_reused_type(*argument, Precedence::Conditional, false, cx));
        }
        text.push('<');
        text.push_str(&parts.join(", "));
        text.push('>');
    }

    /// The literal of a `LiteralTypeNode`, as `Printer.emitLiteralType`
    /// prints the clone.
    fn reused_literal_text(literal: Node<'_>) -> Option<String> {
        Some(match literal {
            Node::KeywordExpression(keyword) => match keyword.kind {
                SyntaxKind::NullKeyword => "null".to_string(),
                SyntaxKind::TrueKeyword => "true".to_string(),
                SyntaxKind::FalseKeyword => "false".to_string(),
                _ => return None,
            },
            Node::StringLiteral(string) => quoted_literal(
                string.text,
                string.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE),
            ),
            Node::NumericLiteral(numeric) => numeric.text.to_string(),
            Node::BigIntLiteral(bigint) => bigint.text.to_string(),
            Node::NoSubstitutionTemplateLiteral(template) => {
                format!("`{}`", Self::escape_template_text(template.text))
            }
            Node::PrefixUnaryExpression(prefix) => {
                let operator = match prefix.operator.kind {
                    SyntaxKind::MinusToken => "-",
                    SyntaxKind::PlusToken => "+",
                    _ => return None,
                };
                let operand = match prefix.operand? {
                    Expression::NumericLiteral(numeric) => numeric.text,
                    Expression::BigIntLiteral(bigint) => bigint.text,
                    _ => return None,
                };
                format!("{operator}{operand}")
            }
            _ => return None,
        })
    }

    /// A function-like node's return annotation; a missing one is filled
    /// with `any` (`nodecopy.go:660`). `Printer.emitReturnType`
    /// (`printer/printer.go:1904`) keeps the `extends` context, and
    /// parenthesises a constrained `infer` there.
    fn reused_return_type(
        &mut self,
        return_type: Option<TypeNode<'a>>,
        in_extends: bool,
        cx: &mut ReuseContext,
    ) -> String {
        let Some(return_type) = return_type else { return "any".to_string() };
        let precedence = match return_type {
            TypeNode::InferTypeNode(infer)
                if in_extends && infer.type_parameter.is_some_and(|p| p.constraint.is_some()) =>
            {
                Precedence::NonArray
            }
            _ => Precedence::Conditional,
        };
        self.emit_reused_type(return_type, precedence, in_extends, cx)
    }

    /// `Printer.emitTypeParameters` over the cloned list.
    fn reused_type_parameters(
        &mut self,
        parameters: &[&TypeParameterDeclaration<'a>],
        cx: &mut ReuseContext,
    ) -> Option<String> {
        if parameters.is_empty() {
            return Some(String::new());
        }
        cx.declares_type_parameters = true;
        let mut parts = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            let mut text = modifiers_prefix(parameter.modifiers)?;
            text.push_str(parameter.name?.text);
            if let Some(constraint) = parameter.constraint {
                text.push_str(" extends ");
                text.push_str(&self.emit_reused_type(
                    constraint,
                    Precedence::Conditional,
                    false,
                    cx,
                ));
            }
            if let Some(default) = parameter.default_type {
                text.push_str(" = ");
                text.push_str(&self.emit_reused_type(default, Precedence::Conditional, false, cx));
            }
            parts.push(text);
        }
        Some(format!("<{}>", parts.join(", ")))
    }

    /// `Printer.emitParameters` over the cloned list; an untyped parameter
    /// without an initializer gains `: any` (`nodecopy.go:686`).
    fn reused_parameters(
        &mut self,
        parameters: &[&ParameterDeclaration<'a>],
        cx: &mut ReuseContext,
    ) -> Option<String> {
        let mut parts = Vec::with_capacity(parameters.len());
        for parameter in parameters {
            if !parameter.modifiers.is_empty() || parameter.initializer.is_some() {
                return None;
            }
            let mut text = String::new();
            if parameter.dot_dot_dot_token.is_some() {
                text.push_str("...");
            }
            match parameter.name? {
                BindingName::Identifier(identifier) => text.push_str(identifier.text),
                BindingName::BindingPattern(pattern) => {
                    text.push_str(&self.clone_binding_name_text(pattern)?);
                }
            }
            if parameter.question_token.is_some() {
                text.push('?');
            }
            text.push_str(": ");
            match parameter.r#type {
                Some(annotation) => {
                    text.push_str(&self.emit_reused_type(
                        annotation,
                        Precedence::Conditional,
                        false,
                        cx,
                    ));
                }
                None => text.push_str("any"),
            }
            parts.push(text);
        }
        Some(format!("({})", parts.join(", ")))
    }

    /// A property name as the clone prints it. A computed name is kept when
    /// its expression is a literal or an entity name (`nodecopy.go:648`,
    /// `nodecopy.go:751`); any other computed form is refused.
    fn reused_property_name(name: PropertyName<'_>) -> Option<String> {
        Some(match name {
            PropertyName::Identifier(identifier) => identifier.text.to_string(),
            PropertyName::PrivateIdentifier(identifier) => identifier.text.to_string(),
            PropertyName::StringLiteral(string) => quoted_literal(
                string.text,
                string.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE),
            ),
            PropertyName::NumericLiteral(numeric) => numeric.text.to_string(),
            PropertyName::BigIntLiteral(bigint) => bigint.text.to_string(),
            PropertyName::ComputedPropertyName(computed) => {
                let inner = match computed.expression? {
                    Expression::StringLiteral(string) => quoted_literal(
                        string.text,
                        string.token_flags.contains(tsr_ast::TokenFlags::SINGLE_QUOTE),
                    ),
                    Expression::NumericLiteral(numeric) => numeric.text.to_string(),
                    other => entity_name_expression_text(other)?,
                };
                format!("[{inner}]")
            }
            PropertyName::NoSubstitutionTemplateLiteral(_) => return None,
        })
    }

    /// A type literal's members, single-line (`Printer.emitTypeLiteral`
    /// with `LFSingleLineTypeLiteralMembers`).
    fn reused_type_members(
        &mut self,
        members: &[TypeElement<'a>],
        cx: &mut ReuseContext,
    ) -> Option<String> {
        if members.is_empty() {
            return Some("{}".to_string());
        }
        let mut parts = Vec::with_capacity(members.len());
        for member in members {
            parts.push(self.reused_type_member(*member, cx)?);
        }
        Some(format!("{{ {} }}", parts.join(" ")))
    }

    fn reused_type_member(
        &mut self,
        member: TypeElement<'a>,
        cx: &mut ReuseContext,
    ) -> Option<String> {
        const LOWEST: Precedence = Precedence::Conditional;
        let annotation =
            |checker: &mut Self, node: Option<TypeNode<'a>>, cx: &mut ReuseContext| match node {
                Some(node) => checker.emit_reused_type(node, LOWEST, false, cx),
                None => "any".to_string(),
            };
        Some(match member {
            TypeElement::PropertySignatureDeclaration(property) => {
                if property.initializer.is_some() {
                    return None;
                }
                let mut text = modifiers_prefix(property.modifiers)?;
                text.push_str(&Self::reused_property_name(property.name)?);
                if let Some(token) = property.postfix_token {
                    text.push_str(match token.kind {
                        SyntaxKind::QuestionToken => "?",
                        SyntaxKind::ExclamationToken => "!",
                        _ => return None,
                    });
                }
                format!("{text}: {};", annotation(self, property.r#type, cx))
            }
            TypeElement::MethodSignatureDeclaration(method) => {
                let mut text = modifiers_prefix(method.modifiers)?;
                text.push_str(&Self::reused_property_name(method.name)?);
                if method.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
                {
                    text.push('?');
                }
                text.push_str(&self.reused_type_parameters(method.type_parameters, cx)?);
                text.push_str(&self.reused_parameters(method.parameters, cx)?);
                format!("{text}: {};", annotation(self, method.r#type, cx))
            }
            TypeElement::CallSignatureDeclaration(call) => {
                let mut text = self.reused_type_parameters(call.type_parameters, cx)?;
                text.push_str(&self.reused_parameters(call.parameters, cx)?);
                format!("{text}: {};", annotation(self, call.r#type, cx))
            }
            TypeElement::ConstructSignatureDeclaration(construct) => {
                let mut text = String::from("new ");
                text.push_str(&self.reused_type_parameters(construct.type_parameters, cx)?);
                text.push_str(&self.reused_parameters(construct.parameters, cx)?);
                format!("{text}: {};", annotation(self, construct.r#type, cx))
            }
            TypeElement::IndexSignatureDeclaration(index) => {
                let mut text = modifiers_prefix(index.modifiers)?;
                let parameters = self.reused_parameters(index.parameters, cx)?;
                text.push('[');
                text.push_str(&parameters[1..parameters.len() - 1]);
                text.push(']');
                format!("{text}: {};", annotation(self, index.r#type, cx))
            }
            TypeElement::GetAccessorDeclaration(getter) => {
                let mut text = modifiers_prefix(getter.modifiers)?;
                text.push_str("get ");
                text.push_str(&Self::reused_property_name(getter.name)?);
                text.push_str(&self.reused_parameters(getter.parameters, cx)?);
                if let Some(node) = getter.r#type {
                    text.push_str(": ");
                    text.push_str(&self.emit_reused_type(node, LOWEST, false, cx));
                }
                text.push(';');
                text
            }
            TypeElement::SetAccessorDeclaration(setter) => {
                let mut text = modifiers_prefix(setter.modifiers)?;
                text.push_str("set ");
                text.push_str(&Self::reused_property_name(setter.name)?);
                text.push_str(&self.reused_parameters(setter.parameters, cx)?);
                text.push(';');
                text
            }
            TypeElement::NotEmittedTypeElement(_) => return None,
        })
    }

    /// `cloneBindingName` (`nodebuilderimpl.go:1713`): the WRITTEN binding
    /// pattern with every initializer elided — holes, property names of any
    /// kind and rests as written.
    pub(crate) fn clone_binding_name_text(
        &self,
        pattern: &tsr_ast::BindingPattern<'_>,
    ) -> Option<String> {
        let is_object = pattern
            .node_id
            .is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ObjectBindingPattern);
        let mut parts = Vec::with_capacity(pattern.elements.len());
        for element in pattern.elements {
            let bound = match element.name {
                Some(BindingName::Identifier(inner)) => inner.text.to_string(),
                Some(BindingName::BindingPattern(inner)) => self.clone_binding_name_text(inner)?,
                None => return None,
            };
            let mut text = String::new();
            if element.dot_dot_dot_token.is_some() {
                text.push_str("...");
            }
            if let Some(property) = element.property_name {
                text.push_str(&Self::reused_property_name(property)?);
                text.push_str(": ");
            }
            text.push_str(&bound);
            parts.push(text);
        }
        Some(match (is_object, parts.is_empty()) {
            (true, true) => "{}".to_string(),
            (true, false) => format!("{{ {} }}", parts.join(", ")),
            (false, true) => "[]".to_string(),
            (false, false) => format!("[{}]", parts.join(", ")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Precedence, binds_below_intersection, binds_below_type_operator, quoted_literal,
        text_precedence,
    };

    #[test]
    fn constituent_and_operand_parentheses_follow_the_printed_node() {
        // A `keyof` origin is a TypeOperator node; a conditional binds lowest.
        assert!(!binds_below_intersection("keyof NameMap"));
        assert!(binds_below_intersection("T extends C ? number : string"));
        assert!(binds_below_intersection("A | B"));
        assert!(!binds_below_intersection("A & B"));
        assert!(binds_below_type_operator("A & B"));
        assert!(binds_below_type_operator("keyof T extends never ? {} : { id: T; }"));
        assert!(!binds_below_type_operator("Foo<A | B>"));
        assert!(!binds_below_type_operator("keyof T"));
    }

    #[test]
    fn fallback_text_precedence_reads_the_top_level_operator() {
        assert_eq!(text_precedence("(x: number) => void"), Precedence::Function);
        assert_eq!(text_precedence("new () => C"), Precedence::Function);
        assert_eq!(text_precedence("string | number"), Precedence::Union);
        assert_eq!(text_precedence("A & B"), Precedence::Intersection);
        assert_eq!(text_precedence("Foo<A | B>"), Precedence::NonArray);
        assert_eq!(text_precedence("{ a: A | B; }"), Precedence::NonArray);
        assert_eq!(text_precedence("keyof T"), Precedence::TypeOperator);
        assert_eq!(text_precedence("\"a|b\""), Precedence::NonArray);
        assert_eq!(text_precedence("T extends U ? X : Y"), Precedence::Conditional);
        assert_eq!(text_precedence("Foo<() => void>"), Precedence::NonArray);
    }

    #[test]
    fn a_single_quoted_literal_keeps_its_quote() {
        assert_eq!(quoted_literal("a", true), "'a'");
        assert_eq!(quoted_literal("a'b\"c", true), "'a\\'b\"c'");
        assert_eq!(quoted_literal("a", false), "\"a\"");
    }
}
