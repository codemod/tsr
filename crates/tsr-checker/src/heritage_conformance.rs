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
        // A **merged** declaration assembles its member table from several
        // declarations, and upstream's merge is not this port's for private
        // and inherited members (`mergedInterfacesWithInheritedPrivates3`).
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        // The merged-declaration decline applies to the **base** as well:
        // `class B extends Uint8Array` names a lib symbol merged across files.
        let base_is_single = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
            .and_then(|entry| self.base_symbol_of_heritage_entry(entry, false))
            .is_some_and(|base| self.has_single_type_declaration(base));
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
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // A **merged** declaration assembles its member table from several
        // declarations, and upstream's merge is not this port's.
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
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
                Some(Node::StringLiteral(name)) => (name.text.to_string(), None),
                Some(Node::NumericLiteral(name)) => (name.text.to_string(), None),
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
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_IN_TYPE_1_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE_2,
                    span,
                    [printed.unwrap_or(name), source_text, target_text],
                ),
            );
        }
        if issued || undecided {
            return;
        }
        // `core.OrElse(node.Name(), node)`.
        let at = self.declaration_name_of(node).unwrap_or(node);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(file, Diagnostic::with_args(broad, span, [source_text, target_text]));
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
        if self.file_has_parse_errors {
            return;
        }
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
            let name = self.binder.symbols().get(member_symbol).name.to_string();
            let is_static = self
                .node_map
                .get(member)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword));
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
        // `getBaseTypes(t)`, across every declaration; `None` marks a base
        // this port cannot resolve, which the identity walk cannot skip.
        let mut bases: Vec<Option<(SymbolId, TypeId)>> = Vec::new();
        for declaration in declarations {
            let Some(Node::InterfaceDeclaration(each)) = self.node_map.get(declaration) else {
                continue;
            };
            for clause in each.heritage_clauses {
                if clause.token.kind != SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for entry in clause.types {
                    let base = entry
                        .expression
                        .and_then(|e| self.heritage_entity_symbol(e, SymbolFlags::TYPE))
                        .filter(|&base| {
                            self.binder
                                .symbols()
                                .get(base)
                                .flags
                                .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                        })
                        .and_then(|base| {
                            let ty = self.instantiated_heritage_base(
                                base,
                                entry.type_arguments,
                                entry.node_id,
                            )?;
                            (!self.is_error(ty)).then_some((base, ty))
                        });
                    bases.push(base);
                }
            }
        }
        match self.check_inherited_properties_are_identical(symbol, source, &bases, name) {
            Some(true) => {}
            Some(false) | None => return,
        }
        // A **merged** source assembles its member table from several
        // declarations, and upstream's merge is not this port's for private
        // and inherited members (`mergedInterfacesWithInheritedPrivates3`).
        if self.binder.symbols().get(symbol).declarations.len() == 1 {
            for (_, target) in bases.into_iter().flatten() {
                if !self.pair_is_reportable(source, target) {
                    continue;
                }
                if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
                {
                    continue;
                }
                let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
                let span = self.error_span(name);
                let source_text = self.type_to_string(source);
                let target_text = self.type_to_string(target);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::INTERFACE_0_INCORRECTLY_EXTENDS_INTERFACE_1,
                        span,
                        [source_text, target_text],
                    ),
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
        bases: &[Option<(SymbolId, TypeId)>],
        name: NodeId,
    ) -> Option<bool> {
        if bases.len() < 2 {
            return Some(true);
        }
        let bases: Vec<(SymbolId, TypeId)> = bases.iter().copied().collect::<Option<_>>()?;
        // `seen`: the base that first contributed each inherited name.
        let mut seen: Vec<(String, SymbolId, TypeId)> = Vec::new();
        let mut identical = true;
        let mut undecided = false;
        for (_, base) in bases {
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
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::INTERFACE_0_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_1_AND_2,
                        span,
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
