//! TS2415 / TS2420 / TS2720 / TS2430 / TS2416 / TS2320 — a class or interface that does
//! not satisfy what it says it does.
//!
//! `checkClassLikeDeclaration`'s `extends` and `implements` arms and
//! `checkInterfaceDeclaration`'s `extends` loop (`checker.go`). A class runs
//! `checkTypeAssignableTo(typeWithThis, baseWithThis)` silently and, on
//! failure, `issueMemberSpecificError`: each own instance member whose type is
//! not assignable to the base's same-named member is TS2416 at the member's
//! name, and only when none is the broad class diagnostic reported at the
//! class name. An interface reports TS2430 at its name for each failing base.
//!
//! The verdict is §25's `relate_ternary`; a pair the relation cannot decide is
//! not reported, and neither is a broad diagnostic whose member walk met an
//! undecidable member, because upstream would have reported that member.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::{
    check::{has_modifier, modifiers_of},
    checker::Checker,
    relater::Relation,
    relater::Ternary,
    types::TypeId,
};

impl Checker<'_, '_> {
    /// The heritage conformance checks for one class or interface declaration.
    pub(crate) fn check_heritage_conformance(&mut self, node: NodeId) {
        // Neither `checkClassLikeDeclaration` nor `checkInterfaceDeclaration`
        // has a parse-error gate: both run on parse-recovered trees.
        if self.in_js_file(node) {
            // The JS arm is the implemented-type loop over the `@implements`
            // tags `reparseHosted` moves into the class's implements clause;
            // the extends arm and interfaces still decline in JS.
            if matches!(
                self.node_map.get(node),
                Some(Node::ClassDeclaration(_) | Node::ClassExpression(_))
            ) {
                self.check_class_implemented_types(node, true);
            }
            return;
        }
        match self.node_map.get(node) {
            Some(Node::ClassDeclaration(_) | Node::ClassExpression(_)) => {
                self.check_class_heritage_conformance(node);
            }
            Some(Node::InterfaceDeclaration(_)) => self.check_interface_heritage_conformance(node),
            _ => {}
        }
    }

    /// `checkClassLikeDeclaration`'s base-class and implemented-type arms
    /// (`checker.go:4293`).
    fn check_class_heritage_conformance(&mut self, node: NodeId) {
        let clauses = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return,
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        self.check_base_type_accessibility(symbol, clauses);
        // A class merged with an **interface** assembles its member table from
        // several declarations, and upstream's merge is not this port's for
        // private and inherited members (`mergedInterfacesWithInheritedPrivates3`).
        // A merged namespace or function adds no instance member, so a
        // clodule relates (`bluebirdStaticThis`; `docs/parity/notes/r5-heritage2.md` §1).
        if !self.has_single_type_declaration(symbol) {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        // The base declines when the default library declares it:
        // `class B extends Uint8Array` names a lib interface merged across
        // files whose inherited signatures this port's member table does not
        // carry (`subclassUint8Array`, `classExtendingBuiltinType`). A user
        // base merged from a class and an interface relates
        // (`interfaceClassMerging`; r5-heritage2 §1).
        let base_is_single = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
            .and_then(|entry| self.base_symbol_of_heritage_entry(entry, false))
            .is_some_and(|base| !self.has_default_library_declaration(base));
        // `checkClassLikeDeclaration` (`checker.go:4293`) relates against
        // `getBaseTypes(t)[0]` (`checker.go:19167`) and skips the arm when
        // that list is empty — a circular or otherwise invalid base.
        if base_is_single && let Some(&base) = self.get_base_types(symbol).first() {
            self.check_class_heritage_entry(
                node,
                source,
                base,
                &messages::CLASS_0_INCORRECTLY_EXTENDS_BASE_CLASS_1,
            );
        }
        self.check_class_implemented_types(node, false);
    }

    /// `checkBaseTypeAccessibility` (`checker.go:4481`), from
    /// `checkClassLikeDeclaration` when `getBaseTypes` is not empty: the
    /// first construct signature of the base constructor type is declared by
    /// the nearest class on the base chain that declares a constructor. When
    /// that declaration is `private` and the `extends` entry is not within
    /// that class, it is TS2675 at the entry, naming the base fully
    /// qualified (r7-grammar §83).
    fn check_base_type_accessibility(
        &mut self,
        symbol: SymbolId,
        clauses: &[&tsr_ast::HeritageClause<'_>],
    ) {
        let Some(entry) = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
        else {
            return;
        };
        let Some(at) = entry.node_id else { return };
        let Some(base) = self.base_symbol_of_heritage_entry(entry, false) else { return };
        if self.get_base_types(symbol).is_empty() {
            return;
        }
        let mut class_symbol = base;
        for _ in 0..16 {
            let Some(class) =
                self.binder.symbols().get(class_symbol).declarations.iter().copied().find(|&d| {
                    matches!(
                        self.nodes.kind(d),
                        SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                    )
                })
            else {
                return;
            };
            let (members, heritage) = match self.node_map.get(class) {
                Some(Node::ClassDeclaration(c)) => (c.members, c.heritage_clauses),
                Some(Node::ClassExpression(c)) => (c.members, c.heritage_clauses),
                _ => return,
            };
            let constructor = members.iter().find_map(|member| match member {
                tsr_ast::ClassElement::ConstructorDeclaration(constructor) => Some(*constructor),
                _ => None,
            });
            if let Some(constructor) = constructor {
                if !has_modifier(constructor.modifiers, SyntaxKind::PrivateKeyword) {
                    return;
                }
                // `isNodeWithinClass(node, typeClassDeclaration)`.
                let Some(base_class) =
                    self.binder.symbols().get(base).declarations.iter().copied().find(|&d| {
                        matches!(
                            self.nodes.kind(d),
                            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                        )
                    })
                else {
                    return;
                };
                if self.nodes.ancestors(at).any(|ancestor| ancestor == base_class) {
                    return;
                }
                let name = self.fully_qualified_name(base);
                let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
                let span = self.error_span(at);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::CANNOT_EXTEND_A_CLASS_0_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE,
                        span,
                        [name],
                    ),
                );
                return;
            }
            // No constructor of its own: the construct signatures are the
            // next base's.
            let Some(next) = heritage
                .iter()
                .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
                .flat_map(|clause| clause.types.iter())
                .next()
                .and_then(|next| self.base_symbol_of_heritage_entry(next, false))
            else {
                return;
            };
            class_symbol = next;
        }
    }

    /// `checkClassLikeDeclaration`'s implemented-type loop (`checker.go:4293`)
    /// over `ast.GetEffectiveImplementsTypeNodes` (`ast/utilities.go`): the
    /// written `implements` clause, followed in a JS file by each JSDoc
    /// `@implements` tag's class name — the order `reparseHosted`'s
    /// `KindJSDocImplementsTag` arm (`parser/reparser.go:563`) appends them in.
    /// `jsdoc_only` skips the written clause, which the JS arm of
    /// [`Checker::check_heritage_conformance`] still declines.
    fn check_class_implemented_types(&mut self, node: NodeId, jsdoc_only: bool) {
        let clauses = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return,
        };
        let mut entries: Vec<&tsr_ast::ExpressionWithTypeArguments<'_>> = Vec::new();
        if !jsdoc_only {
            for clause in clauses {
                if clause.token.kind == SyntaxKind::ImplementsKeyword {
                    entries.extend(clause.types.iter().copied());
                }
            }
        }
        if self.in_js_file(node)
            && let Some(docs) = self.jsdoc_entries.get(&node)
        {
            for doc in *docs {
                for tag in doc.tags {
                    if let tsr_ast::JSDocTag::JSDocImplementsTag(tag) = tag
                        && let Some(class_name) = tag.class_name
                    {
                        entries.push(class_name);
                    }
                }
            }
        }
        if entries.is_empty() {
            return;
        }
        // `if !ast.IsEntityNameExpression(expr) || ast.IsOptionalChain(expr)`
        // (`checker.go:4369`): TS2500 at the expression, a `c.error` that
        // precedes the entry's relation and every decline below
        // (r7-grammar §55).
        for entry in &entries {
            let Some(expression) = entry.expression else { continue };
            let Some(at) = expression.node_id() else { continue };
            if is_entity_name_expression(expression)
                && !self.nodes.flags(at).contains(tsr_ast::NodeFlags::OPTIONAL_CHAIN)
            {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::new(
                    &messages::A_CLASS_CAN_ONLY_IMPLEMENT_AN_IDENTIFIER_SLASHQUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUMENTS,
                    span,
                ),
            );
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // A class merged with an interface assembles its member table from
        // several declarations, and upstream's merge is not this port's; a
        // merged namespace adds no instance member (r5-heritage2 §1).
        if !self.has_single_type_declaration(symbol) {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        for entry in entries {
            let Some(expression) = entry.expression else { continue };
            let Some(implemented) = self.heritage_entity_symbol(expression, SymbolFlags::TYPE)
            else {
                continue;
            };
            let flags = self.binder.symbols().get(implemented).flags;
            // A merged implemented interface is one declared type whose
            // members `resolveDeclaredMembers` gathers from every declaration
            // (`classWithMultipleBaseClasses`); only the class-extends arm
            // keeps the lib-merged base decline.
            if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
                // `getTypeFromTypeNode` answers any declaration's type; a
                // type alias's is related like an interface
                // (`implemented_alias.rs`).
                if let Some((target, is_class)) =
                    self.implemented_alias_type(implemented, entry.type_arguments)
                {
                    let message = if is_class {
                        &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_CLASS_1_DID_YOU_MEAN_TO_EXTEND_1_AND_INHERIT_ITS_MEMBERS_AS_A_SUBCLASS
                    } else {
                        &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1
                    };
                    self.check_class_heritage_entry(node, source, target, message);
                }
                continue;
            }
            let Some(target) =
                self.instantiated_heritage_base(implemented, entry.type_arguments, entry.node_id)
            else {
                continue;
            };
            // `t.symbol.Flags&ast.SymbolFlagsClass` picks the message.
            let message = if flags.contains(SymbolFlags::CLASS) {
                &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_CLASS_1_DID_YOU_MEAN_TO_EXTEND_1_AND_INHERIT_ITS_MEMBERS_AS_A_SUBCLASS
            } else {
                &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1
            };
            self.check_class_heritage_entry(node, source, target, message);
        }
    }

    /// `if !c.checkTypeAssignableTo(typeWithThis, baseWithThis, nil, nil) {
    /// c.issueMemberSpecificError(…) }` for one heritage entry.
    fn check_class_heritage_entry(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        broad: &'static Message,
    ) {
        if self.is_error(target) || !self.pair_is_reportable(source, target) {
            return;
        }
        if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated {
            return;
        }
        self.issue_member_specific_error(node, source, target, broad);
    }

    /// `issueMemberSpecificError` (`checker.go`): TS2416 on every own
    /// non-static member whose type is not assignable to the base member of
    /// the same name; the broad diagnostic at the class name when none is.
    fn issue_member_specific_error(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        broad: &'static Message,
    ) {
        let members: Vec<NodeId> = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
            }
            Some(Node::ClassExpression(class)) => {
                class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
            }
            _ => return,
        };
        let Some(class_symbol) = self.binder.symbol_of(node) else { return };
        let this_type = self.class_instance_this_type(class_symbol);
        // The late-bound names of the class's own instance members.
        let late_bound = self.late_bound_members_of(self.binder.merged_symbol(class_symbol), false);
        let mut issued = false;
        let mut undecided = false;
        for member in members {
            let is_static = self
                .node_map
                .get(member)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword));
            if is_static {
                continue;
            }
            // `declaredProp.Name != ast.InternalSymbolNameComputed`: a member
            // without a name is skipped, and so is a computed one unless
            // `lateBindMember` gave it a name (`[Symbol.toPrimitive]`,
            // `["literal"]`; `symbolProperty24`).
            let Some(name_node) = self.declaration_name_of(member) else { continue };
            let (name, printed) = match self.node_map.get(name_node) {
                Some(Node::Identifier(name)) => (name.text.to_string(), None),
                // `symbolToString(declaredProp)` prints the name as written
                // (`getNameOfSymbolAsWritten`'s `DeclarationNameToString`,
                // `nodebuilderimpl.go:1002`): `'2.0'` keeps its quotes and
                // `0b11` its spelling (r7-grammar §55).
                Some(Node::StringLiteral(literal)) => {
                    (literal.text.to_string(), self.declaration_name_source_text(name_node))
                }
                Some(Node::NumericLiteral(literal)) => {
                    (literal.text.to_string(), self.declaration_name_source_text(name_node))
                }
                Some(Node::PrivateIdentifier(name)) => (name.text.to_string(), None),
                Some(Node::ComputedPropertyName(_)) => {
                    let Some(name) = late_bound.iter().find_map(|(name, declaration)| {
                        (*declaration == member).then(|| name.clone())
                    }) else {
                        continue;
                    };
                    (name, self.computed_member_name_text(member))
                }
                _ => continue,
            };
            if self.binder.symbol_of(member).is_none() {
                continue;
            }
            // `typeWithThis`/`baseWithThis`: both members read with the
            // class's own `this` as the this argument.
            let (Some(property), Some(base_property)) = (
                self.get_type_of_property_with_this_argument(source, &name, this_type, false),
                self.get_type_of_property_with_this_argument(target, &name, this_type, false),
            ) else {
                continue;
            };
            match self.relate_ternary(property, base_property, Relation::Assignable) {
                Ternary::Related => continue,
                Ternary::NotRelated
                    if self.assignability_pair_is_reportable(property, base_property) => {}
                _ => {
                    undecided = true;
                    continue;
                }
            }
            issued = true;
            let Some(file) = self.source_file_of_for_diagnostics(name_node) else { continue };
            let span = self.error_span(name_node);
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            let args = [printed.unwrap_or(name), source_text, target_text];
            // `NewDiagnosticChain(diags[0], Property_0_in_type_1_is_not_assignable…)`
            // over `checkTypeAssignableToEx(propType, basePropType, name, nil, &diags)`
            // (`checker.go:4472`), r7-grammar §41.
            let diagnostic = match self.relation_failure_diagnostic(name_node, property, base_property)
            {
                Some(child) => Diagnostic::new_chain(
                    Some(child),
                    &messages::PROPERTY_0_IN_TYPE_1_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE_2,
                    args,
                ),
                None => Diagnostic::with_args(
                    &messages::PROPERTY_0_IN_TYPE_1_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE_2,
                    span,
                    args,
                ),
            };
            self.report(file, diagnostic);
        }
        if issued || undecided {
            return;
        }
        // `core.OrElse(node.Name(), node)`.
        let at = self.declaration_name_of(node).unwrap_or(node);
        let span = self.error_span(at);
        // `checkTypeAssignableTo(typeWithThis, baseWithThis, name, broadDiag)`:
        // `isRelatedToEx`'s common-property check (`relater.go:2676`) fails a
        // weak target first and reports TS2559 with no head message.
        if self.report_weak_type_failure(at, span, source, target) {
            return;
        }
        self.report_heritage_relation(at, source, target, broad);
    }

    /// `checkTypeAssignableTo(source, target, errorNode, headMessage)` for a
    /// heritage relation (TS2415/TS2430): the relater's report under the head
    /// message, with its elaboration chain, through `report_relation_failure`.
    /// Both implements heads (`Class_0_incorrectly_implements_interface_1`,
    /// TS2420, and the `…implements_class_1_Did_you_mean_to_extend…` variant,
    /// TS2720) are `isConversionOrInterfaceImplementationMessage` heads, which
    /// keep themselves over a missing-property chain (`relater.go:4816`). The
    /// reporter makes that exception since r7-reports `ec17ba88`, so all four
    /// heads take this road (r7-grammar §57; §41 held the implements heads
    /// back until then). A report the relater declines keeps the bare head.
    fn report_heritage_relation(
        &mut self,
        at: NodeId,
        source: TypeId,
        target: TypeId,
        head: &'static Message,
    ) {
        let span = self.error_span(at);
        if self.report_relation_failure(at, span, None, source, target, Some(head)) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(file, Diagnostic::with_args(head, span, [source_text, target_text]));
    }

    /// `checkTypeAssignableToEx(source, target, errorNode, nil, &diags)`'s
    /// `diags[0]`: the relater's report at `at` with no head message,
    /// collected instead of reported (the error output container), with its
    /// file attached for nesting. `None` when the reporter declines.
    fn relation_failure_diagnostic(
        &mut self,
        at: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<Diagnostic> {
        let span = self.error_span(at);
        let mark = self.diagnostics.len();
        let reported = self.report_relation_failure(at, span, None, source, target, None);
        let mut collected: Vec<(NodeId, Diagnostic)> = self.diagnostics.drain(mark..).collect();
        if !reported || collected.is_empty() {
            return None;
        }
        let (file, mut first) = collected.remove(0);
        if let Some(image) = self.diagnostic_file_image(file) {
            first.set_file(image);
        }
        Some(first)
    }

    /// `checkMembersForOverrideModifier` / `checkMemberForOverrideModifier`
    /// (`checker.go:4704`, `checker.go:4729`): TS4112/TS4121 when the class
    /// has no base, TS4127/TS4128 for a non-bindable dynamic name, TS4113/
    /// TS4117 (JS: TS4122/TS4123) when the base type has no property of the
    /// member's name, and under `noImplicitOverride` TS4114/TS4115 (JS:
    /// TS4119/TS4120) for an unmarked override and TS4116 for an abstract one.
    /// In a JS file the `override` modifier is the reparsed `@override` tag
    /// ([`Checker::has_effective_modifier`]).
    ///
    /// A class whose `extends` entry this port cannot resolve to a base type
    /// declines, as does a base whose property table cannot be enumerated for
    /// the spelling suggestion. `ambient` is the class's ambient context
    /// (`node.Flags&ast.NodeFlagsAmbient`).
    pub(crate) fn check_members_for_override_modifier(&mut self, node: NodeId, ambient: bool) {
        let is_js = self.in_js_file(node);
        let (members, clauses) = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => (class.members, class.heritage_clauses),
            Some(Node::ClassExpression(class)) => (class.members, class.heritage_clauses),
            _ => return,
        };
        let base_node = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next();
        let mut checked: Vec<NodeId> = Vec::new();
        for member in members {
            let Some(id) = Node::from(*member).node_id() else { continue };
            // `!ast.HasAmbientModifier(member)`.
            if self.has_effective_modifier(id, SyntaxKind::DeclareKeyword) {
                continue;
            }
            if let tsr_ast::ClassElement::ConstructorDeclaration(constructor) = member {
                for parameter in constructor.parameters {
                    let Some(parameter_id) = parameter.node_id else { continue };
                    if parameter.modifiers.iter().any(|modifier| {
                        matches!(
                            modifier,
                            tsr_ast::ModifierLike::Token(token)
                                if matches!(
                                    token.kind,
                                    SyntaxKind::PublicKeyword
                                        | SyntaxKind::PrivateKeyword
                                        | SyntaxKind::ProtectedKeyword
                                        | SyntaxKind::ReadonlyKeyword
                                        | SyntaxKind::OverrideKeyword
                                )
                        )
                    }) {
                        checked.push(parameter_id);
                    }
                }
            } else {
                checked.push(id);
            }
        }
        // Without `noImplicitOverride` only members carrying `override` can
        // report; the rest return before any type is read.
        let no_implicit_override = self.no_implicit_override;
        let checked: Vec<(NodeId, bool)> = checked
            .into_iter()
            .map(|member| {
                (member, self.has_effective_modifier(member, SyntaxKind::OverrideKeyword))
            })
            .filter(|&(_, has_override)| has_override || no_implicit_override)
            .collect();
        if checked.is_empty() {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let class_type = self.get_declared_type_of_class_or_interface(symbol);
        let base = match base_node {
            Some(_) => match self.first_base_type_of_class_symbol(symbol) {
                Some(base) => Some(base),
                None => return,
            },
            None => None,
        };
        let static_type = self.get_type_of_symbol(symbol);
        let base_static = base_node
            .and_then(|entry| entry.expression)
            .map(|expression| self.check_expression(expression));
        for (member, has_override) in checked {
            let Some(base) = base else {
                if has_override {
                    let class_text = self.type_to_string(class_type);
                    self.report_override_error(
                        member,
                        if is_js {
                            &messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_CONTAINING_CLASS_0_DOES_NOT_EXTEND_ANOTHER_CLASS
                        } else {
                            &messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_0_DOES_NOT_EXTEND_ANOTHER_CLASS
                        },
                        vec![class_text],
                    );
                }
                continue;
            };
            if has_override
                && self.nodes.kind(member) != SyntaxKind::Parameter
                && self.non_bindable_computed_name(member).is_some()
            {
                self.report_override_error(
                    member,
                    if is_js {
                        &messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_ITS_NAME_IS_DYNAMIC
                    } else {
                        &messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC
                    },
                    Vec::new(),
                );
                continue;
            }
            let Some(member_symbol) = self.binder.symbol_of(member) else { continue };
            let is_static = self
                .node_map
                .get(member)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword));
            // `getSymbolOfDeclaration(member)` is the *late-bound* symbol: a
            // computed name `lateBindMember` binds (`[foo]` for a unique
            // symbol, `[prop]` for a literal-typed const) is looked up under
            // the name it was bound to, not `__computed` (`override21`,
            // `overrideLateBindableName1`). A computed name that is not late
            // bindable — a plain `symbol`, which `late_bound_members_of` also
            // names for its index-info display — keeps `__computed` and finds
            // no property, as natively (`overrideLateBindableIndexSignature1`).
            let mut name = self.binder.symbols().get(member_symbol).name.to_string();
            if name == "__computed" && self.has_late_bindable_name(member) {
                let late_bound = self.late_bound_members_of(symbol, is_static);
                if let Some((late_name, _)) =
                    late_bound.iter().find(|(_, declaration)| *declaration == member)
                {
                    name.clone_from(late_name);
                }
            }
            let this_type = if is_static { static_type } else { class_type };
            if self.get_property_of_type(this_type, &name).is_none() {
                continue;
            }
            let base_type = if is_static {
                let Some(base_static) = base_static else { continue };
                base_static
            } else {
                base
            };
            if self.is_gap(base_type) {
                continue;
            }
            let base_property = self.get_property_of_type(base_type, &name);
            if let Some(base_property) = base_property {
                if has_override || !no_implicit_override || ambient {
                    continue;
                }
                let declarations = self.binder.symbols().get(base_property).declarations.clone();
                if declarations.is_empty() {
                    continue;
                }
                let base_has_abstract = declarations.iter().any(|&declaration| {
                    self.has_effective_modifier(declaration, SyntaxKind::AbstractKeyword)
                });
                let base_text = self.type_to_string(base);
                if !base_has_abstract {
                    let message = match (self.nodes.kind(member) == SyntaxKind::Parameter, is_js) {
                        (true, true) => &messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE_BASE_CLASS_0,
                        (true, false) => &messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_BASE_CLASS_0,
                        (false, true) => &messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE_BASE_CLASS_0,
                        (false, false) => &messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE_BASE_CLASS_0,
                    };
                    self.report_override_error(member, message, vec![base_text]);
                } else if self.has_effective_modifier(member, SyntaxKind::AbstractKeyword) {
                    self.report_override_error(
                        member,
                        &messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METHOD_THAT_IS_DECLARED_IN_THE_BASE_CLASS_0,
                        vec![base_text],
                    );
                }
                continue;
            }
            if !has_override {
                continue;
            }
            // `getSuggestedSymbolForNonexistentClassMember`: a spelling
            // suggestion among the base type's class members.
            let Some(candidates) = self.get_property_names_of_type(base_type) else { continue };
            let candidates: Vec<&str> = candidates
                .iter()
                .map(String::as_str)
                .filter(|candidate| *candidate != "prototype")
                .collect();
            let base_text = self.type_to_string(base);
            match crate::check::spelling_suggestion(&name, &candidates) {
                Some(suggestion) => {
                    let suggestion = suggestion.to_string();
                    self.report_override_error(
                        member,
                        if is_js {
                            &messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_BASE_CLASS_0_DID_YOU_MEAN_1
                        } else {
                            &messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_BASE_CLASS_0_DID_YOU_MEAN_1
                        },
                        vec![base_text, suggestion],
                    );
                }
                None => self.report_override_error(
                    member,
                    if is_js {
                        &messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_BASE_CLASS_0
                    } else {
                        &messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_BASE_CLASS_0
                    },
                    vec![base_text],
                ),
            }
        }
    }

    /// `hasLateBindableName` (`checker.go`) for a class member:
    /// `isLateBindableName` — a computed name whose expression is an entity
    /// name expression of a string-literal, number-literal or `unique symbol`
    /// type (`TypeFlagsStringOrNumberLiteralOrUnique`).
    fn has_late_bindable_name(&mut self, member: NodeId) -> bool {
        let Some(name) = self.declaration_name_of(member) else { return false };
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return false;
        };
        let Some(expression) = computed.expression else { return false };
        if !is_entity_name_expression(expression) {
            return false;
        }
        let name_type = self.check_expression(expression);
        self.store.get(name_type).flags.intersects(
            crate::flags::TypeFlags::STRING_LITERAL
                | crate::flags::TypeFlags::NUMBER_LITERAL
                | crate::flags::TypeFlags::UNIQUE_ES_SYMBOL,
        )
    }

    /// `c.error(member, …)` for the override diagnostics.
    fn report_override_error(
        &mut self,
        member: NodeId,
        message: &'static Message,
        args: Vec<String>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(member) else { return };
        let span = self.error_span(member);
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// `checkInterfaceDeclaration`'s once-per-symbol block (`checker.go:4991`):
    /// `checkInheritedPropertiesAreIdentical`, and only when that succeeds,
    /// `checkTypeAssignableTo(typeWithThis, baseWithThis, node.Name(),
    /// Interface_0_incorrectly_extends_interface_1)` for each base type and
    /// `checkIndexConstraints`.
    ///
    /// `links.interfaceChecked` is set by the first interface declaration that
    /// is **checked**, the one a file-order check reaches first. A bundled
    /// default-library declaration is never checked, so a user augmentation
    /// of a lib interface (`interface Object { … }`) runs the block even
    /// though lib.es5's declaration is first in the merged symbol
    /// (`objectTypeHidingMembersOfExtendedObject`).
    fn check_interface_heritage_conformance(&mut self, node: NodeId) {
        let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(node) else { return };
        let Some(name) = interface.name.and_then(|n| n.node_id) else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let first = declarations.iter().copied().find(|&declaration| {
            self.nodes.kind(declaration) == SyntaxKind::InterfaceDeclaration
                && !self.in_default_library(declaration)
        });
        if first != Some(node) {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        // `getBaseTypes(t)` (`base_types.rs`), which resolves each written
        // `extends` entry, aliases of object types included, and drops an
        // invalid one. A dropped entry is either native's invalid base or a
        // base this port cannot resolve; the two are indistinguishable here,
        // so a count short of the written entries declines the identity walk
        // (and with it the base loop, which upstream runs only after it).
        let written: usize = declarations
            .iter()
            .filter_map(|&declaration| match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(each)) => Some(each.heritage_clauses),
                _ => None,
            })
            .flat_map(|clauses| clauses.iter())
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .map(|clause| clause.types.len())
            .sum();
        let resolved = self.get_base_types(symbol);
        let bases: Option<Vec<TypeId>> = (resolved.len() == written).then_some(resolved);
        match self.check_inherited_properties_are_identical(symbol, source, bases.as_deref(), name)
        {
            Some(true) => {}
            Some(false) | None => return,
        }
        // A **merged** source assembles its member table from several
        // declarations, and upstream's merge is not this port's for private
        // and inherited members (`mergedInterfacesWithInheritedPrivates3`).
        if self.binder.symbols().get(symbol).declarations.len() == 1 {
            for target in bases.into_iter().flatten() {
                if !self.pair_is_reportable(source, target) {
                    continue;
                }
                if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
                {
                    continue;
                }
                self.report_heritage_relation(
                    name,
                    source,
                    target,
                    &messages::INTERFACE_0_INCORRECTLY_EXTENDS_INTERFACE_1,
                );
            }
        }
        // `c.checkIndexConstraints(t, symbol, false)` on the declared type.
        let declared = self.get_declared_type_of_class_or_interface(symbol);
        self.check_index_constraints_of_type(declared, symbol, false);
    }

    /// `checkInheritedPropertiesAreIdentical` (`checker.go`): two bases that
    /// contribute one name the interface does not redeclare must contribute
    /// identical properties, else TS2320 at the interface name.
    ///
    /// `None` when the answer is not decidable here: a base that cannot be
    /// resolved, or a pair of property types that are neither the same type
    /// nor shown non-identical. `isTypeIdenticalTo` is not ported; a pair is
    /// non-identical when either direction of assignability is `NotRelated`,
    /// since identity implies both.
    fn check_inherited_properties_are_identical(
        &mut self,
        symbol: SymbolId,
        source: TypeId,
        bases: Option<&[TypeId]>,
        name: NodeId,
    ) -> Option<bool> {
        let bases = bases?;
        if bases.len() < 2 {
            return Some(true);
        }
        // `seen`: the base that first contributed each inherited name.
        let mut seen: Vec<(String, SymbolId, TypeId)> = Vec::new();
        let mut identical = true;
        let mut undecided = false;
        for &base in bases {
            let names = self.get_property_names_of_type(base)?;
            for property_name in names {
                if self.binder.symbols().get(symbol).members.get(property_name.as_str()).is_some() {
                    continue;
                }
                let Some(property) = self.get_property_of_type(base, &property_name) else {
                    continue;
                };
                let Some(&(_, existing, existing_base)) =
                    seen.iter().find(|(seen_name, _, _)| *seen_name == property_name)
                else {
                    seen.push((property_name, property, base));
                    continue;
                };
                match self.is_property_identical_to(
                    existing,
                    existing_base,
                    property,
                    base,
                    &property_name,
                ) {
                    Some(true) => continue,
                    Some(false) => {}
                    None => {
                        undecided = true;
                        continue;
                    }
                }
                identical = false;
                let Some(file) = self.source_file_of_for_diagnostics(name) else { continue };
                let span = self.error_span(name);
                let interface_text = self.type_to_string(source);
                let first_text = self.type_to_string(existing_base);
                let second_text = self.type_to_string(base);
                // `NewDiagnosticChain(Named_property_0_of_types_1_and_2_are_not_identical
                // at typeNode, Interface_0_cannot_simultaneously_extend_types_1_and_2)`
                // (`checker.go:5059`), r7-grammar §40.
                let child = self.related_diagnostic_at(
                    name,
                    &messages::NAMED_PROPERTY_0_OF_TYPES_1_AND_2_ARE_NOT_IDENTICAL,
                    span,
                    vec![property_name.clone(), first_text.clone(), second_text.clone()],
                );
                self.report(
                    file,
                    Diagnostic::new_chain(
                        Some(child),
                        &messages::INTERFACE_0_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_1_AND_2,
                        [interface_text, first_text, second_text],
                    ),
                );
            }
        }
        if undecided && identical {
            return None;
        }
        Some(identical)
    }

    /// `isPropertyIdenticalTo` → `compareProperties(…, compareTypesIdentical)`
    /// (`checker.go:5066`). `None` when the type comparison is undecidable.
    fn is_property_identical_to(
        &mut self,
        source: SymbolId,
        source_owner: TypeId,
        target: SymbolId,
        target_owner: TypeId,
        name: &str,
    ) -> Option<bool> {
        // The same declared symbol read through two instantiations of one
        // generic base (`A<string>, A<number>`) is two properties upstream.
        if source == target && source_owner == target_owner {
            return Some(true);
        }
        let accessibility = |checker: &Self, symbol| {
            if checker.property_has_modifier(symbol, SyntaxKind::PrivateKeyword) {
                1
            } else if checker.property_has_modifier(symbol, SyntaxKind::ProtectedKeyword) {
                2
            } else {
                0
            }
        };
        let source_access = accessibility(self, source);
        if source_access != accessibility(self, target) {
            return Some(false);
        }
        if source_access != 0 {
            // `getTargetSymbol`: a non-public member is identical only to
            // instantiations of itself.
            if source != target {
                return Some(false);
            }
        } else if self.property_is_optional(source) != self.property_is_optional(target) {
            return Some(false);
        }
        if self.is_readonly_property(source) != self.is_readonly_property(target) {
            return Some(false);
        }
        let source_type = self.get_type_of_property_of_type(source_owner, name)?;
        let target_type = self.get_type_of_property_of_type(target_owner, name)?;
        let source_type = self.remove_missing_type(source_type);
        let target_type = self.remove_missing_type(target_type);
        if source_type == target_type {
            return Some(true);
        }
        if !self.pair_is_reportable(source_type, target_type) {
            return None;
        }
        // `compareTypesIdentical` — the identity relation
        // (`crate::identity`). Both operands are declared member types, the
        // written kind §2 of `docs/parity/notes/decls.md` trusts structurally.
        match self.is_type_identical_to(source_type, target_type) {
            Ternary::Related => Some(true),
            Ternary::NotRelated => Some(false),
            Ternary::Unknown => None,
        }
    }

    /// Whether any of the symbol's declarations is in a bundled default
    /// library file.
    fn has_default_library_declaration(&self, symbol: SymbolId) -> bool {
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .any(|&declaration| self.in_default_library(declaration))
    }

    /// One class or interface declaration among the symbol's declarations; a
    /// merged `var` (`declare var A: { new(): A }`) contributes no members.
    fn has_single_type_declaration(&self, symbol: SymbolId) -> bool {
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter(|&&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ClassDeclaration
                        | SyntaxKind::ClassExpression
                        | SyntaxKind::InterfaceDeclaration
                )
            })
            .count()
            == 1
    }
}

const PROPERTY: u8 = 1;
const ACCESSOR: u8 = 2;
const METHOD: u8 = 4;

/// What `checkKindsOfPropertyMemberOverrides` reads off one side's member
/// symbol, gathered from the declarations that make it up.
#[derive(Clone, Copy)]
struct OverrideMember {
    /// `symbol.Flags & SymbolFlagsPropertyOrAccessor` (an auto-accessor
    /// binds as an accessor, `bindPropertyWorker`, `binder.go:747`), plus the
    /// method bit for `isPrototypeProperty`: [`PROPERTY`] | [`ACCESSOR`] |
    /// [`METHOD`].
    kinds: u8,
    /// `getDeclarationModifierFlagsFromSymbol`: the get accessor's modifiers
    /// when there is one, else the value declaration's.
    private: bool,
    abstract_: bool,
    /// `isPropertyAbstractOrInterface` holds for every declaration given
    /// the symbol's own abstract flag: none is a property with an initializer.
    no_initialized_property: bool,
    /// `GetNameOfDeclaration(derived.ValueDeclaration)`.
    name_at: NodeId,
}

/// One non-static member declaration of a class: its name, kind bits,
/// modifiers' private/abstract bits, whether it is a property with an
/// initializer, and its name node. Parameter properties are members.
fn instance_member_declarations<'a>(
    members: &'a [tsr_ast::ClassElement<'a>],
) -> Vec<(&'a str, u8, bool, bool, bool, bool, NodeId)> {
    let mut out = Vec::new();
    for member in members {
        let (name, kind, modifiers, initialized, getter) = match *member {
            tsr_ast::ClassElement::PropertyDeclaration(p) => {
                let auto =
                    tsr_ast::has_syntactic_modifier(p.modifiers, SyntaxKind::AccessorKeyword);
                (
                    p.name,
                    if auto { ACCESSOR } else { PROPERTY },
                    p.modifiers,
                    !auto && p.initializer.is_some(),
                    false,
                )
            }
            tsr_ast::ClassElement::GetAccessorDeclaration(a) => {
                (a.name, ACCESSOR, a.modifiers, false, true)
            }
            tsr_ast::ClassElement::SetAccessorDeclaration(a) => {
                (a.name, ACCESSOR, a.modifiers, false, false)
            }
            tsr_ast::ClassElement::MethodDeclaration(m) => {
                (m.name, METHOD, m.modifiers, false, false)
            }
            tsr_ast::ClassElement::ConstructorDeclaration(constructor) => {
                for parameter in constructor.parameters {
                    let modifiers = parameter.modifiers;
                    let is_parameter_property = [
                        SyntaxKind::PublicKeyword,
                        SyntaxKind::PrivateKeyword,
                        SyntaxKind::ProtectedKeyword,
                        SyntaxKind::ReadonlyKeyword,
                    ]
                    .into_iter()
                    .any(|keyword| tsr_ast::has_syntactic_modifier(modifiers, keyword));
                    if !is_parameter_property {
                        continue;
                    }
                    let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else {
                        continue;
                    };
                    let Some(at) = name.node_id else { continue };
                    let private =
                        tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::PrivateKeyword);
                    out.push((name.text, PROPERTY, private, false, false, false, at));
                }
                continue;
            }
            _ => continue,
        };
        if tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword) {
            continue;
        }
        let tsr_ast::PropertyName::Identifier(identifier) = name else { continue };
        let Some(at) = identifier.node_id else { continue };
        out.push((
            identifier.text,
            kind,
            tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::PrivateKeyword),
            tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AbstractKeyword),
            initialized,
            getter,
            at,
        ));
    }
    out
}

/// Folds a class's declarations of `name` into the member symbol upstream
/// would see; `None` when the class declares no instance member of that name.
fn override_member_named(
    declarations: &[(&str, u8, bool, bool, bool, bool, NodeId)],
    name: &str,
) -> Option<OverrideMember> {
    let mut found: Option<OverrideMember> = None;
    let mut flags_from_getter = false;
    for &(seen, kind, private, abstract_, initialized, getter, at) in declarations {
        if seen != name {
            continue;
        }
        let member = found.get_or_insert(OverrideMember {
            kinds: 0,
            private,
            abstract_,
            no_initialized_property: true,
            name_at: at,
        });
        member.kinds |= kind;
        member.no_initialized_property &= !initialized;
        if getter && !flags_from_getter {
            flags_from_getter = true;
            member.private = private;
            member.abstract_ = abstract_;
        }
    }
    found
}

impl Checker<'_, '_> {
    /// TS2515 / TS2653 / TS2654 / TS2655 / TS2656 / TS2650 — the
    /// `notImplementedInfo` half of `checkKindsOfPropertyMemberOverrides`
    /// (`checker.go:4536`-`:4590`, reported at `:4664`), for a class-like
    /// declaration with an `extends` clause and a resolved base
    /// (`checkClassLikeDeclaration`, `checker.go:4313`: `baseTypes[0]`).
    ///
    /// Each property of the base type (`getPropertiesOfType`, here
    /// [`Checker::get_property_names_of_type`] then
    /// [`Checker::get_property_of_type`]) that the class type inherits
    /// **unchanged** (`getTargetSymbol(derived) == getTargetSymbol(base)`;
    /// property symbols here are the declared ones for every instantiation,
    /// so the target is the symbol itself) and whose declaration is
    /// `abstract` (`getDeclarationModifierFlagsFromSymbol`: the get accessor
    /// for an accessor, else the value declaration) is missing, unless the
    /// class is `abstract` or another base type (an interface merged with the
    /// class) supplies a different symbol of that name. The list is
    /// reported once, at the class: one member names it unquoted, two to
    /// five quote them all, more quote four and count the rest.
    ///
    /// Declines (reports nothing) when the base's property list is not
    /// certified (`get_property_names_of_type` answers `None`), or when a
    /// missing member's name is not an identifier: `symbolToString` of a
    /// string, numeric or computed name has spellings this port does not
    /// produce. No cache: the check walk visits each class once; the property
    /// list is the shared memo `get_property_names_of_type` already owns
    /// (r7-grammar §26).
    fn check_inherited_abstract_members_implemented(&mut self, node: NodeId) {
        let (modifiers, clauses, is_expression) = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => (class.modifiers, class.heritage_clauses, false),
            Some(Node::ClassExpression(class)) => (class.modifiers, class.heritage_clauses, true),
            _ => return,
        };
        if !clauses.iter().any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            || has_modifier(modifiers, SyntaxKind::AbstractKeyword)
        {
            return;
        }
        let Some(own) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(own);
        let base_types = self.get_base_types(symbol);
        let Some(&base_type) = base_types.first() else { return };
        if self.is_error(base_type) {
            return;
        }
        let class_type = self.get_declared_type_of_class_or_interface(symbol);
        let Some(names) = self.get_property_names_of_type(base_type) else { return };
        let mut missed: Vec<String> = Vec::new();
        'base_property: for name in names {
            let Some(base) = self.get_property_of_type(base_type, &name) else { continue };
            if self.binder.symbols().get(base).flags.contains(SymbolFlags::PROTOTYPE) {
                continue;
            }
            if self.target_property_of(class_type, &name) != Some(base)
                || !self.symbol_declaration_is_abstract(base)
            {
                continue;
            }
            for &other in &base_types[1..] {
                if self.target_property_of(other, &name).is_some_and(|found| found != base) {
                    continue 'base_property;
                }
            }
            let named_by_identifier =
                self.binder.symbols().get(base).value_declaration.is_some_and(|declaration| {
                    self.declaration_name_of(declaration).is_some_and(|name| {
                        matches!(
                            self.nodes.kind(name),
                            SyntaxKind::Identifier | SyntaxKind::PrivateIdentifier
                        )
                    })
                });
            if !named_by_identifier {
                return;
            }
            missed.push(name);
        }
        if missed.is_empty() {
            return;
        }
        let at = if is_expression { node } else { self.declaration_name_of(node).unwrap_or(node) };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let base_text = self.type_to_string(base_type);
        let class_text = self.type_to_string(class_type);
        let quoted = |names: &[String]| {
            names.iter().map(|name| format!("'{name}'")).collect::<Vec<_>>().join(", ")
        };
        let diagnostic = if let [only] = missed.as_slice() {
            if is_expression {
                Diagnostic::with_args(
                    &messages::NON_ABSTRACT_CLASS_EXPRESSION_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_0_FROM_CLASS_1,
                    span,
                    [only.clone(), base_text],
                )
            } else {
                Diagnostic::with_args(
                    &messages::NON_ABSTRACT_CLASS_0_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_1_FROM_CLASS_2,
                    span,
                    [class_text, only.clone(), base_text],
                )
            }
        } else if missed.len() > 5 {
            let listed = quoted(&missed[..4]);
            let remaining = (missed.len() - 4).to_string();
            if is_expression {
                Diagnostic::with_args(
                    &messages::NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_0_COLON_1_AND_2_MORE,
                    span,
                    [base_text, listed, remaining],
                )
            } else {
                Diagnostic::with_args(
                    &messages::NON_ABSTRACT_CLASS_0_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_1_COLON_2_AND_3_MORE,
                    span,
                    [class_text, base_text, listed, remaining],
                )
            }
        } else if is_expression {
            Diagnostic::with_args(
                &messages::NON_ABSTRACT_CLASS_EXPRESSION_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_0_COLON_1,
                span,
                [base_text, quoted(&missed)],
            )
        } else {
            Diagnostic::with_args(
                &messages::NON_ABSTRACT_CLASS_0_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF_1_COLON_2,
                span,
                [class_text, base_text, quoted(&missed)],
            )
        };
        self.report(file, diagnostic);
    }

    /// `getTargetSymbol(getPropertyOfObjectType(t, name))`: the property
    /// symbol, or, for a member inherited through a generic heritage entry
    /// (which [`Checker::get_property_of_type`] leaves a gap, §202), the
    /// declaring symbol [`Checker::generic_heritage_property_symbol`] finds.
    /// That is the target symbol, which is all the caller compares.
    fn target_property_of(&mut self, t: TypeId, name: &str) -> Option<SymbolId> {
        self.get_property_of_type(t, name)
            .or_else(|| self.generic_heritage_property_symbol(t, name, &mut Vec::new()))
    }

    /// `getDeclarationModifierFlagsFromSymbol(s) & ModifierFlagsAbstract`
    /// (`utilities.go:717`): the first get accessor of an accessor symbol,
    /// else the value declaration, with the reparser's JSDoc modifiers.
    fn symbol_declaration_is_abstract(&self, symbol: SymbolId) -> bool {
        let record = self.binder.symbols().get(symbol);
        let getter = record.flags.contains(SymbolFlags::GET_ACCESSOR).then(|| {
            record
                .declarations
                .iter()
                .copied()
                .find(|&declaration| self.nodes.kind(declaration) == SyntaxKind::GetAccessor)
        });
        let Some(declaration) = getter.flatten().or(record.value_declaration) else {
            return false;
        };
        self.has_effective_modifier(declaration, SyntaxKind::AbstractKeyword)
    }

    /// TS2610 / TS2611 and the three method-kind mismatches —
    /// `checkKindsOfPropertyMemberOverrides` (`checker.go:4536`), the
    /// property-kind half; the abstract-member half is TS2515's own check.
    ///
    /// Upstream walks `getPropertiesOfType(baseType)` and compares each with
    /// `getPropertyOfObjectType(t, name)`. The condition is about the
    /// **declarations** that make up the two symbols, so this port reads them
    /// from the tree: the derived class's own instance members, and for each
    /// the nearest class up the `extends` chain that declares an instance
    /// member of the same name (§309, §708). `docs/parity/notes/r5-classfields.md` §1.
    pub(crate) fn check_kinds_of_property_member_overrides(&mut self, node: NodeId) {
        self.check_inherited_abstract_members_implemented(node);
        let (clauses, members, derived_name) = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                (class.heritage_clauses, class.members, class.name.map(|name| name.text))
            }
            _ => return,
        };
        let Some(base_at) = self.override_base_class(clauses) else { return };
        let derived = instance_member_declarations(members);
        let mut reported: Vec<&str> = Vec::new();
        for &(name, ..) in &derived {
            if reported.contains(&name) {
                continue;
            }
            reported.push(name);
            let Some(derived_member) = override_member_named(&derived, name) else { continue };
            let Some(base) = self.override_base_member(base_at, name) else { continue };
            // `either base or derived property is private - not override`.
            if base.private || derived_member.private {
                continue;
            }
            let base_property_or_accessor = base.kinds & (PROPERTY | ACCESSOR) != 0;
            let derived_property_or_accessor = derived_member.kinds & (PROPERTY | ACCESSOR) != 0;
            let message = if base_property_or_accessor && derived_property_or_accessor {
                // `arePropertiesAbstractOrInterface`: an abstract base whose
                // declarations are not initialized properties need not match.
                if base.abstract_ && base.no_initialized_property {
                    continue;
                }
                let base_is_property = base.kinds & (PROPERTY | ACCESSOR) == PROPERTY;
                let derived_is_property = derived_member.kinds & (PROPERTY | ACCESSOR) == PROPERTY;
                if !base_is_property && derived_is_property {
                    &messages::_0_IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_INSTANCE_PROPERTY
                } else if base_is_property && !derived_is_property {
                    &messages::_0_IS_DEFINED_AS_A_PROPERTY_IN_CLASS_1_BUT_IS_OVERRIDDEN_HERE_IN_2_AS_AN_ACCESSOR
                } else {
                    // TS2612 (`GetUseDefineForClassFields`) needs
                    // `isPropertyInitializedInConstructor`; not ported. §1.
                    continue;
                }
            } else if base.kinds & METHOD != 0 {
                if derived_member.kinds & (METHOD | PROPERTY) != 0 {
                    continue;
                }
                &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_FUNCTION_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_ACCESSOR
            } else if base.kinds & ACCESSOR != 0 {
                &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_ACCESSOR_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_FUNCTION
            } else {
                &messages::CLASS_0_DEFINES_INSTANCE_MEMBER_PROPERTY_1_BUT_EXTENDED_CLASS_2_DEFINES_IT_AS_INSTANCE_MEMBER_FUNCTION
            };
            let at = derived_member.name_at;
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.nodes.span(at);
            let base_text = self.override_class_text(base_at);
            let derived_text = derived_name.unwrap_or_default().to_string();
            let args = if base_property_or_accessor && derived_property_or_accessor {
                [name.to_string(), base_text, derived_text]
            } else {
                [base_text, name.to_string(), derived_text]
            };
            self.report(file, Diagnostic::with_args(message, span, args));
        }
    }

    /// The declaration of the class an `extends` clause names, when it is an
    /// identifier resolving to a class declaration.
    fn override_base_class(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) -> Option<NodeId> {
        let extends =
            clauses.iter().find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)?;
        let Some(tsr_ast::Expression::Identifier(name)) = extends.types.first()?.expression else {
            return None;
        };
        let at = name.node_id?;
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            at,
            name.text,
            SymbolFlags::CLASS,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        matches!(self.node_map.get(declaration), Some(Node::ClassDeclaration(_)))
            .then_some(declaration)
    }

    /// `getPropertyOfObjectType(baseType, name)` read from declarations: the
    /// nearest class up the chain that declares an instance member `name`.
    /// Bounded, because the corpus contains cyclic heritage. §708.
    fn override_base_member(&mut self, base: NodeId, name: &str) -> Option<OverrideMember> {
        let mut at = Some(base);
        for _ in 0..8 {
            let Some(Node::ClassDeclaration(class)) = self.node_map.get(at?) else { return None };
            if let Some(member) =
                override_member_named(&instance_member_declarations(class.members), name)
            {
                return Some(member);
            }
            at = self.override_base_class(class.heritage_clauses);
        }
        None
    }

    fn override_class_text(&self, class: NodeId) -> String {
        match self.node_map.get(class) {
            Some(Node::ClassDeclaration(class)) => {
                class.name.map(|name| name.text.to_string()).unwrap_or_default()
            }
            _ => String::new(),
        }
    }
}

/// `ast.IsEntityNameExpression`: an identifier, or a property access whose
/// name is an identifier on an entity name expression.
fn is_entity_name_expression(expression: tsr_ast::Expression<'_>) -> bool {
    match expression {
        tsr_ast::Expression::Identifier(_) => true,
        tsr_ast::Expression::PropertyAccessExpression(access) => {
            matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                && access.expression.is_some_and(is_entity_name_expression)
        }
        _ => false,
    }
}
