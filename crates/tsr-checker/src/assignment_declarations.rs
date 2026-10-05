//! Assignment declaration values, ported from checker.go:
//! `getWidenedTypeForAssignmentDeclaration`, `getAssignmentDeclarationInitializerType`
//! and `getTypeFromPropertyDescriptor`. The binder has already classified and
//! attached these declarations to their property symbols.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    Checker,
    flags::TypeFlags,
    flow::TypeFacts,
    types::{TypeData, TypeId},
};

/// `thisAssignmentDeclarationKind` (`checker.go`), with upstream's separately
/// cached location folded into the variant that carries it.
#[derive(Clone, Copy)]
pub(crate) enum ThisAssignmentDeclaration<'a> {
    /// Not (all) `this.property` assignments.
    None,
    /// A declaration carries a type annotation; use it.
    Typed(TypeNode<'a>),
    /// At least one declaration is in this class constructor; use its flow.
    Constructor(NodeId),
    /// Methods only: the base class property, else the declaration union
    /// plus `undefined`.
    Method,
}

impl<'a> Checker<'a, '_> {
    /// `isConstructorDeclaredThisProperty` (`checker.go`): every declaration
    /// is a `this.x` (or literal `this["x"]`) assignment declaration.
    pub(crate) fn is_constructor_declared_this_property(
        &mut self,
        symbol: SymbolId,
    ) -> ThisAssignmentDeclaration<'a> {
        let record = self.binder.symbols().get(symbol);
        let Some(value_declaration) = record.value_declaration else {
            return ThisAssignmentDeclaration::None;
        };
        if self.nodes.kind(value_declaration) != SyntaxKind::BinaryExpression {
            return ThisAssignmentDeclaration::None;
        }
        if let Some(&cached) = self.this_expando_kinds.get(&symbol) {
            return cached;
        }
        let declarations = record.declarations.clone();
        let mut all_this = true;
        let mut annotation = None;
        for &declaration in &declarations {
            let Some(Node::BinaryExpression(binary)) = self.node_map.get(declaration) else {
                all_this = false;
                break;
            };
            let is_this_property = match binary.left {
                Some(Expression::PropertyAccessExpression(access)) => {
                    is_this_keyword(access.expression)
                }
                Some(Expression::ElementAccessExpression(access)) => {
                    is_this_keyword(access.expression)
                        && matches!(
                            access.argument_expression,
                            Some(
                                Expression::StringLiteral(_)
                                    | Expression::NoSubstitutionTemplateLiteral(_)
                                    | Expression::NumericLiteral(_)
                            )
                        )
                }
                _ => false,
            };
            if !is_this_property || !self.in_js_file(declaration) {
                all_this = false;
                break;
            }
            if let Some(node) = self.assignment_declaration_type_node(declaration) {
                annotation = Some(node);
            }
        }
        let kind = if !all_this {
            ThisAssignmentDeclaration::None
        } else if let Some(annotation) = annotation {
            ThisAssignmentDeclaration::Typed(annotation)
        } else if let Some(constructor) = self.get_declaring_constructor(&declarations) {
            ThisAssignmentDeclaration::Constructor(constructor)
        } else {
            ThisAssignmentDeclaration::Method
        };
        self.this_expando_kinds.insert(symbol, kind);
        kind
    }

    /// `BinaryExpression.Type`: the parser's JSDoc reparse moves a statement's
    /// `@type` tag onto an assignment declaration (`reparser.go`'s
    /// `KindExpressionStatement` arm). This tree is not reparsed, so the tag
    /// is read from the statement that hosts it.
    pub(crate) fn assignment_declaration_type_node(
        &self,
        declaration: NodeId,
    ) -> Option<TypeNode<'a>> {
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(declaration)
            && let Some(annotation) = binary.r#type
        {
            return Some(annotation);
        }
        if let Some(annotation) = self.jsdoc_cast_annotation(declaration) {
            return Some(annotation);
        }
        if !self.in_js_file(declaration) {
            return None;
        }
        let statement = self.nodes.parent(declaration)?;
        if self.nodes.kind(statement) != SyntaxKind::ExpressionStatement {
            return None;
        }
        self.jsdoc_cast_annotation(statement)
    }

    /// `getDeclaringConstructor` (`checker.go:27345`).
    fn get_declaring_constructor(&self, declarations: &[NodeId]) -> Option<NodeId> {
        declarations.iter().find_map(|&declaration| {
            self.get_this_container(declaration, false)
                .filter(|&container| self.nodes.kind(container) == SyntaxKind::Constructor)
        })
    }

    /// `ast.GetThisContainer` (`ast/utilities.go:1790`) without the class
    /// computed-name stop, which no caller here requests.
    pub(crate) fn get_this_container(
        &self,
        node: NodeId,
        include_arrow_functions: bool,
    ) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            match self.nodes.kind(id) {
                SyntaxKind::ComputedPropertyName => {
                    current = self.nodes.parent(id).and_then(|member| self.nodes.parent(member));
                    continue;
                }
                SyntaxKind::Decorator => {
                    let parent = self.nodes.parent(id);
                    let grandparent = parent.and_then(|p| self.nodes.parent(p));
                    if parent.is_some_and(|p| self.nodes.kind(p) == SyntaxKind::Parameter)
                        && grandparent.is_some_and(|g| self.is_class_element(g))
                    {
                        current = grandparent;
                    } else if parent.is_some_and(|p| self.is_class_element(p)) {
                        current = parent;
                    }
                }
                SyntaxKind::ArrowFunction if include_arrow_functions => return Some(id),
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::ClassStaticBlockDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::PropertySignature
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::MethodSignature
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::EnumDeclaration
                | SyntaxKind::SourceFile => return Some(id),
                _ => {}
            }
            current = current.and_then(|id| self.nodes.parent(id));
        }
        None
    }

    pub(crate) fn is_class_element(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::Constructor
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::IndexSignature
                | SyntaxKind::ClassStaticBlockDeclaration
                | SyntaxKind::SemicolonClassElement
        )
    }

    /// `getFlowTypeInConstructor` (`flow.go:2466`). The reference is an
    /// existing `this.x` declaration target in `constructor`, standing in for
    /// upstream's synthesized access parented to the constructor.
    fn get_flow_type_in_constructor(
        &mut self,
        symbol: SymbolId,
        constructor: NodeId,
    ) -> Option<TypeId> {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let reference = declarations.iter().find_map(|&declaration| {
            if self.get_this_container(declaration, false) != Some(constructor) {
                return None;
            }
            match self.node_map.get(declaration) {
                Some(Node::BinaryExpression(binary)) => binary.left.and_then(|l| l.node_id()),
                _ => None,
            }
        })?;
        let return_flow = self.binder.return_flow(constructor)?;
        let flow_type = self.get_flow_type_of_property_symbol(reference, Some(return_flow), symbol);
        // We don't infer a type if assignments are only null or undefined.
        if self.every_type_is_nullable(flow_type) {
            return None;
        }
        Some(flow_type)
    }

    /// `getFlowTypeOfProperty` (`checker.go:11444`): the initial type is the
    /// base class property when there is one, else `undefined`.
    pub(crate) fn get_flow_type_of_property_symbol(
        &mut self,
        reference: NodeId,
        start: Option<tsr_binder::FlowId>,
        symbol: SymbolId,
    ) -> TypeId {
        let initial =
            self.get_type_of_property_in_base_class(symbol).unwrap_or(self.intrinsics.undefined);
        self.get_flow_type_of_property(reference, start, initial)
    }

    /// `everyType(t, isNullableType)`.
    fn every_type_is_nullable(&mut self, t: TypeId) -> bool {
        let types = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        types.into_iter().all(|t| {
            self.get_type_facts(t).intersects(TypeFacts::IS_UNDEFINED | TypeFacts::IS_NULL)
        })
    }

    /// `getTypeOfPropertyInBaseClass` (`checker.go:11455`) through
    /// `getDeclaringClass`: the first base type of the class whose members
    /// hold the property.
    pub(crate) fn get_type_of_property_in_base_class(
        &mut self,
        symbol: SymbolId,
    ) -> Option<TypeId> {
        let record = self.binder.symbols().get(symbol);
        let name = record.name.to_owned();
        let parent = record.parent?;
        if !self.binder.symbols().get(parent).flags.intersects(SymbolFlags::CLASS) {
            return None;
        }
        let base = self.first_base_type_of_class_symbol(parent)?;
        self.get_type_of_property_of_type(base, &name)
    }

    /// `getBaseTypes(classType)[0]` for a class symbol: its class
    /// declaration's single `extends` entry, instantiated.
    pub(crate) fn first_base_type_of_class_symbol(&mut self, class: SymbolId) -> Option<TypeId> {
        let declaration = self.binder.symbols().get(class).value_declaration?;
        let clauses = match self.node_map.get(declaration)? {
            Node::ClassDeclaration(node) => node.heritage_clauses,
            Node::ClassExpression(node) => node.heritage_clauses,
            _ => return None,
        };
        let entry = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()?;
        let base = self.base_symbol_of_heritage_entry(entry, false)?;
        let t = self.instance_base_type_of_heritage_entry(base, entry);
        (t != self.intrinsics.error).then_some(t)
    }

    /// The instance half of `resolveBaseTypesOfClass` (checker.go:19220) for
    /// one resolved `extends` entry.
    pub(crate) fn instance_base_type_of_heritage_entry(
        &mut self,
        base: SymbolId,
        entry: &tsr_ast::ExpressionWithTypeArguments<'a>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // resolveBaseTypesOfClass (checker.go:19220): actual classes apply
        // heritage arguments directly; class-like values use the first
        // constructor with the matching type-argument arity.
        if self.binder.symbols().get(base).flags.contains(SymbolFlags::CLASS) {
            return self
                .instantiated_heritage_base(base, entry.type_arguments, entry.node_id)
                .unwrap_or(error);
        }
        let constructor = self.get_type_of_symbol(base);
        let Some(signatures) =
            self.signatures_of_type_kind(constructor, crate::signatures::SignatureKind::Construct)
        else {
            return error;
        };
        let count = entry.type_arguments.len();
        for signature in signatures {
            let minimum = signature
                .type_parameters
                .iter()
                .rposition(|p| p.default.is_none())
                .map_or(0, |index| index + 1);
            if count < minimum || count > signature.type_parameters.len() {
                continue;
            }
            if signature.type_parameters.is_empty() {
                return signature.r#type;
            }
            let Some(parameters) = self.type_parameter_types(&signature) else { return error };
            let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
            let mut arguments: Vec<_> = entry
                .type_arguments
                .iter()
                .map(|&argument| self.get_type_from_type_node(argument))
                .collect();
            arguments.resize(parameters.len(), error);
            for index in count..parameters.len() {
                let Some(default) = signature.type_parameters[index].default else { return error };
                let map: Vec<_> =
                    parameters.iter().copied().zip(arguments.iter().copied()).collect();
                arguments[index] = self.instantiate_type(default, &map, &parameters, &names);
            }
            let map: Vec<_> = parameters.iter().copied().zip(arguments).collect();
            return self.instantiate_type(signature.r#type, &map, &parameters, &names);
        }
        error
    }

    /// `containsSameNamedThisProperty` (`checker.go`): the right side reads
    /// the property being declared, outside nested functions.
    fn contains_same_named_this_property(&mut self, this_property: NodeId, node: NodeId) -> bool {
        if self.references_match(this_property, node) {
            return true;
        }
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        ) {
            return false;
        }
        let Some(typed) = self.node_map.get(node) else { return false };
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| children.push(child));
        children
            .into_iter()
            .any(|child| self.contains_same_named_this_property(this_property, child))
    }

    /// `isReadonlyAssignmentDeclaration` (checker.go). A value descriptor is
    /// readonly unless its writable property exists and is not literal false;
    /// an accessor descriptor is readonly when it has no setter.
    pub(crate) fn assignment_declaration_is_readonly(&mut self, symbol: SymbolId) -> bool {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        for declaration in declarations {
            let Some(Node::CallExpression(call)) = self.node_map.get(declaration) else { continue };
            let Some(&descriptor) = call.arguments.get(2) else { continue };
            let descriptor = self.check_expression(descriptor);
            if descriptor == self.intrinsics.error {
                continue;
            }
            if self.get_type_of_property_of_type(descriptor, "value").is_some() {
                let Some(writable) = self.get_property_of_type(descriptor, "writable") else {
                    return true;
                };
                let writable = match self
                    .binder
                    .symbols()
                    .get(writable)
                    .value_declaration
                    .and_then(|id| self.node_map.get(id))
                {
                    Some(Node::PropertyAssignment(property)) => {
                        property.initializer.map(|initializer| self.check_expression(initializer))
                    }
                    _ => Some(self.get_type_of_symbol(writable)),
                };
                if writable.is_some_and(|t| {
                    matches!(self.store.get(t).data, TypeData::BooleanLiteral(false))
                }) {
                    return true;
                }
            } else if self.get_type_of_property_of_type(descriptor, "set").is_none() {
                return true;
            }
        }
        false
    }

    /// `GetAssignmentDeclarationKind`'s named `CommonJS` export assignment arm.
    /// Syntax determines the kind; each consumer still resolves the receiver's
    /// actual symbol before inspecting declarations or their value types.
    pub(crate) fn is_commonjs_export_property_assignment(
        &self,
        binary: &tsr_ast::BinaryExpression<'_>,
    ) -> bool {
        if !binary.node_id.is_some_and(|id| self.in_js_file(id))
            || binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
        {
            return false;
        }
        let target = match binary.left {
            Some(Expression::PropertyAccessExpression(access))
                if matches!(access.name, Some(tsr_ast::MemberName::Identifier(_))) =>
            {
                access.expression
            }
            Some(Expression::ElementAccessExpression(access)) => {
                let mut argument = access.argument_expression;
                while let Some(Expression::ParenthesizedExpression(parentheses)) = argument {
                    argument = parentheses.expression;
                }
                if !matches!(
                    argument,
                    Some(
                        Expression::StringLiteral(_)
                            | Expression::NoSubstitutionTemplateLiteral(_)
                            | Expression::NumericLiteral(_)
                    )
                ) {
                    return false;
                }
                access.expression
            }
            _ => None,
        };
        matches!(target, Some(Expression::Identifier(name)) if name.text == "exports")
            || target.is_some_and(is_module_exports_access)
    }

    pub(crate) fn get_widened_type_for_assignment_declaration(
        &mut self,
        symbol: SymbolId,
    ) -> TypeId {
        let kind = self.is_constructor_declared_this_property(symbol);
        let resolved = match kind {
            ThisAssignmentDeclaration::Typed(annotation) => {
                Some(self.get_type_from_type_node(annotation))
            }
            ThisAssignmentDeclaration::Constructor(constructor) => {
                self.get_flow_type_in_constructor(symbol, constructor)
            }
            ThisAssignmentDeclaration::Method => self.get_type_of_property_in_base_class(symbol),
            ThisAssignmentDeclaration::None => None,
        };
        if let Some(resolved) = resolved {
            return self.finish_assignment_declaration_type(symbol, resolved);
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut types = Vec::new();
        for (index, &declaration) in declarations.iter().enumerate() {
            let Some(node) = self.node_map.get(declaration) else { continue };
            let assigned = match node {
                Node::BinaryExpression(binary) => {
                    if let Some(annotation) = self.assignment_declaration_type_node(declaration) {
                        let annotated = self.get_type_from_type_node(annotation);
                        return self.finish_assignment_declaration_type(symbol, annotated);
                    }
                    let Some(left) = binary.left else { return self.intrinsics.error };
                    let Some(right) = binary.right else { return self.intrinsics.error };
                    let target = match left {
                        Expression::PropertyAccessExpression(access) => access.expression,
                        Expression::ElementAccessExpression(access) => access.expression,
                        _ => None,
                    };
                    // `getAssignmentDeclarationInitializerType`'s
                    // JSDeclarationKindThisProperty arm: `this.x = this.x || …`
                    // contributes nothing of its own.
                    if is_this_keyword(target)
                        && let Some(left_id) = left.node_id()
                        && let Some(right_id) = right.node_id()
                        && self.contains_same_named_this_property(left_id, right_id)
                    {
                        continue;
                    }
                    // ast.GetAssignmentDeclarationKind: both module.exports
                    // itself and its named properties preserve regular literals.
                    // Only the named-property kind ignores initial undefined.
                    let exports = self.is_commonjs_export_property_assignment(binary);
                    let module_exports = self.in_js_file(declaration)
                        && is_module_exports_access(left)
                        && !matches!(right, Expression::Identifier(name) if name.text == "exports");
                    let assigned = if exports || module_exports {
                        let mut rightmost = right;
                        while let Expression::BinaryExpression(assignment) = rightmost {
                            if assignment
                                .operator_token
                                .is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
                            {
                                break;
                            }
                            let Some(right) = assignment.right else {
                                return self.intrinsics.error;
                            };
                            rightmost = right;
                        }
                        let checked = self.check_expression(rightmost);
                        self.get_regular_type_of_literal_type(checked)
                    } else {
                        self.check_expression_for_mutable_location(right)
                    };
                    // CommonJS commonly starts an export with undefined and
                    // fills it later. Only its first undefined declaration is
                    // ignored, and only when another declaration exists.
                    if exports
                        && index == 0
                        && declarations.len() > 1
                        && assigned == self.intrinsics.undefined
                    {
                        continue;
                    }
                    if self.is_empty_array_literal_type(assigned)
                        && !self.has_parent_with_type_annotation(symbol)
                    {
                        let Some(array) = self.global_type_symbol("Array") else {
                            return self.intrinsics.error;
                        };
                        self.create_type_reference(array, vec![self.intrinsics.any])
                    } else {
                        assigned
                    }
                }
                Node::CallExpression(call) => {
                    let Some(&descriptor) = call.arguments.get(2) else {
                        return self.intrinsics.error;
                    };
                    self.get_type_from_property_descriptor(descriptor)
                }
                _ => continue,
            };
            if assigned == self.intrinsics.error {
                return assigned;
            }
            if !types.contains(&assigned) {
                types.push(assigned);
            }
        }
        if matches!(kind, ThisAssignmentDeclaration::Method)
            && !types.is_empty()
            && self.strict_null_checks
            && !types.contains(&self.intrinsics.undefined)
        {
            types.push(self.intrinsics.undefined);
        }
        let t = if types.is_empty() { self.intrinsics.any } else { self.get_union_type(&types) };
        self.finish_assignment_declaration_type(symbol, t)
    }

    /// The tail of `getWidenedTypeForAssignmentDeclaration`, shared by every
    /// exit that upstream routes through it.
    fn finish_assignment_declaration_type(&mut self, symbol: SymbolId, t: TypeId) -> TypeId {
        if t == self.intrinsics.error {
            return t;
        }
        // Native getWidenedType descends inferred object/array images. Reuse
        // the checker-owned widening worker and its context/publication rules;
        // regular CommonJS literals are not fresh and keep their identity.
        let t = self.widen_object_literal_freshness(t);
        // getWidenedType does not widen regular CommonJS literal types. Mutable
        // assignment and descriptor values have already widened at their own
        // expression boundary. JS all-nullable assignment inference is any,
        // including when strictNullChecks is enabled.
        let nullable_only = match &self.store.get(t).data {
            TypeData::Union { types, .. } => {
                types.iter().all(|&t| self.store.get(t).flags.intersects(TypeFlags::NULLABLE))
            }
            _ => self.store.get(t).flags.intersects(TypeFlags::NULLABLE),
        };
        if nullable_only
            && self
                .binder
                .symbols()
                .get(symbol)
                .value_declaration
                .is_some_and(|id| self.in_js_file(id))
        {
            return self.intrinsics.any;
        }
        t
    }

    /// `isEmptyArrayLiteralType` / `isEmptyLiteralType` (checker.go).
    pub(crate) fn is_empty_array_literal_type(&mut self, id: TypeId) -> bool {
        let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned() else {
            return false;
        };
        let Some(array) = self.global_type_symbol("Array") else { return false };
        self.binder.merged_symbol(target) == self.binder.merged_symbol(array)
            && arguments.len() == 1
            && self.is_empty_literal_type(arguments[0])
    }

    pub(crate) fn is_empty_literal_type(&self, id: TypeId) -> bool {
        id == if self.strict_null_checks {
            self.intrinsics.implicit_never
        } else {
            self.intrinsics.undefined_widening
        }
    }

    /// `hasParentWithTypeAnnotation` (checker.go): the function initializer's
    /// containing declaration supplies the annotation, not the expando itself.
    fn has_parent_with_type_annotation(&self, symbol: SymbolId) -> bool {
        let Some(parent) = self.binder.symbols().get(symbol).parent else { return false };
        let Some(declaration) = self.binder.symbols().get(parent).value_declaration else {
            return false;
        };
        if !matches!(
            self.node_map.get(declaration),
            Some(Node::FunctionExpression(_) | Node::ArrowFunction(_))
        ) {
            return false;
        }
        self.nodes
            .parent(declaration)
            .is_some_and(|parent| self.type_annotation_of(parent).is_some())
    }

    fn get_type_from_property_descriptor(&mut self, descriptor: Expression<'a>) -> TypeId {
        let descriptor = self.check_expression(descriptor);
        if descriptor == self.intrinsics.error {
            return descriptor;
        }
        if let Some(value) = self.get_type_of_property_of_type(descriptor, "value") {
            return value;
        }
        if let Some(getter) = self.get_type_of_property_of_type(descriptor, "get")
            && let Some(signature) = self.single_call_signature(getter)
        {
            return signature.r#type;
        }
        if let Some(setter) = self.get_type_of_property_of_type(descriptor, "set")
            && let Some(signature) = self.single_call_signature(setter)
        {
            return if signature.parameters.is_empty() {
                self.intrinsics.never
            } else {
                self.signature_type_at_position(&signature, 0).unwrap_or(self.intrinsics.error)
            };
        }
        self.intrinsics.any
    }
}

fn is_this_keyword(expression: Option<Expression<'_>>) -> bool {
    matches!(expression, Some(Expression::KeywordExpression(keyword))
        if keyword.kind == SyntaxKind::ThisKeyword)
}

/// `ast.IsModuleExportsAccessExpression`, native 5b1047d1 utilities.go:2869.
pub(crate) fn is_module_exports_access(expression: Expression<'_>) -> bool {
    let (target, name) = match expression {
        Expression::PropertyAccessExpression(access) => (
            access.expression,
            match access.name {
                Some(tsr_ast::MemberName::Identifier(name)) => Some(name.text),
                _ => None,
            },
        ),
        Expression::ElementAccessExpression(access) => (access.expression, {
            let mut argument = access.argument_expression;
            while let Some(Expression::ParenthesizedExpression(parentheses)) = argument {
                argument = parentheses.expression;
            }
            match argument {
                Some(Expression::StringLiteral(name)) => Some(name.text),
                Some(Expression::NoSubstitutionTemplateLiteral(name)) => Some(name.text),
                _ => None,
            }
        }),
        _ => return false,
    };
    matches!(target, Some(Expression::Identifier(name)) if name.text == "module")
        && name == Some("exports")
}
