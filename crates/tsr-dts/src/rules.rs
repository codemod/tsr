//! The `TS9xxx` rules.
//!
//! # Where each diagnostic is anchored, and why that is the hard part
//!
//! Every rule here names the `TS9xxx` code it implements and the upstream message
//! constant in `internal/diagnostics/`, per ADR-0021. What the codes alone do *not*
//! tell you is where the squiggle goes, and getting that wrong fails a case just as
//! completely as missing the diagnostic. The anchors below are read off the
//! baselines, not guessed:
//!
//! | Situation | Code | Anchored at | Evidence |
//! |---|---|---|---|
//! | Variable, no annotation, uninferable initialiser | 9010 | the **name** | `isolatedDeclarationErrorsExpressions.ts(3,14)` — col 14 is `numberConstBad1`, not the `1 + 1` at col 32 |
//! | Property, same | 9012 | the **name** | same baseline, `(59,12)` |
//! | Parameter, same | 9011 | the **initialiser** when there is one | `…FunctionDeclarations.ts(7,49)` is `1 + 1`, while `(3,35)` — a parameter with no default — is the name `p` |
//! | Function declaration, no return type | 9007 | the **name** | `…FunctionDeclarations.ts(1,17)` is `noReturn` |
//! | Function/arrow *expression*, no return type | 9007 | the **expression** | `isolatedDeclarationErrors.ts(7,30)` is the `() => {}` |
//! | Anything uninferable *nested* in a literal | 9013 | the offending sub-expression | `…FunctionDeclarations.ts(7,66)` is the `1 + 1` inside `{ a: 1 + 1 }` |
//!
//! The last row is the rule that makes the rest cohere: a failure reports the
//! *declaration's* code when the initialiser fails as a whole, and `TS9013` when
//! the failure is buried inside an object or array literal. That is why
//! [`Inferability`] distinguishes `Generic` from `Reported` rather than being a
//! bool — the caller, not the callee, knows which of those two situations it is in.
//!
//! A specific diagnostic also *replaces* the generic one rather than adding to it:
//! `export let arr = [1, 2, 3];` produces `TS9017` at the array and no `TS9010` at
//! the name.

use tsr_ast::{
    ArrowFunction, ClassElement, EnumDeclaration, Expression, FunctionExpression, ModifierLike,
    NodeId, NodeTable, ObjectLiteralElementLike, ParameterDeclaration, PropertyName, SourceFile,
    Statement, SyntaxKind, VariableDeclaration,
};
use tsr_core::Span;
use tsr_diagnostics::{Diagnostic, messages};

use crate::visibility::Visible;

/// Whether an expression's type can be written down without inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inferability {
    /// The type is syntactically apparent.
    Ok,
    /// It is not, and a specific diagnostic has already been reported.
    Reported,
    /// It is not, and the caller must report its own declaration-level code.
    Generic,
}

/// Run the rules over a file's visible declarations.
#[must_use]
pub fn check<'a>(
    file: &'a SourceFile<'a>,
    nodes: &NodeTable,
    visible: &Visible,
) -> Vec<Diagnostic> {
    let mut checker = Checker { nodes, out: Vec::new(), in_parameter_default: false };
    for statement in file.statements {
        if visible.contains(statement.node_id()) {
            checker.statement(statement);
        }
    }
    checker.out
}

struct Checker<'t> {
    nodes: &'t NodeTable,
    out: Vec<Diagnostic>,
    /// Whether the expression being judged is (inside) a parameter default.
    ///
    /// This is the only position in which a function expression's missing return
    /// type is an error. See `arrow` for the evidence, which is emphatic and
    /// contradicts the corpus's own comments.
    in_parameter_default: bool,
}

impl Checker<'_> {
    fn span(&self, id: Option<NodeId>) -> Span {
        id.map_or(Span::new(0, 0), |id| self.nodes.span(id))
    }

    fn report(&mut self, message: &'static tsr_diagnostics::Message, span: Span) {
        self.out.push(Diagnostic::new(message, span));
    }

    // ----- statements ---------------------------------------------------------

    fn statement(&mut self, statement: &Statement<'_>) {
        match statement {
            Statement::VariableStatement(node) => {
                let Some(list) = node.declaration_list else { return };
                let is_const = list
                    .node_id
                    .is_some_and(|id| self.nodes.flags(id).contains(tsr_ast::NodeFlags::CONST));
                for declaration in list.declarations {
                    self.variable_declaration(declaration, is_const);
                }
            }
            Statement::FunctionDeclaration(node) => {
                self.parameters(node.parameters);
                if node.r#type.is_none() && node.full_signature.is_none() {
                    // TS9007 `Function_must_have_an_explicit_return_type_annotation_with_isolatedDeclarations`.
                    let span = self.span(node.name.and_then(|n| n.node_id));
                    self.report(
                        &messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                        span,
                    );
                }
            }
            Statement::ClassDeclaration(node) => {
                let mut accessors = Vec::new();
                for member in node.members {
                    self.class_element(member);
                    self.collect_accessor(member, &mut accessors);
                }
                self.accessors(&accessors);
            }
            Statement::EnumDeclaration(node) => self.enum_declaration(node),
            Statement::ExportAssignment(node) => {
                // `export default <expr>`. A bare identifier is fine: the `.d.ts`
                // references the declaration rather than restating its type, which
                // is why `isolatedDeclarationErrorsDefault`'s `f.ts` is clean while
                // its `a.ts` (`export default 1 + 1`) is not.
                if let Some(expression) = &node.expression
                    && !matches!(expression, Expression::Identifier(_))
                {
                    let span = self.span(expression.node_id());
                    // TS9037 `Default_exports_can_t_be_inferred_with_isolatedDeclarations`.
                    if self.infer(expression, false) == Inferability::Generic {
                        self.report(
                            &messages::DEFAULT_EXPORTS_CAN_T_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                            span,
                        );
                    }
                }
            }
            Statement::ModuleDeclaration(node) => {
                // Everything inside an emitted namespace is emitted with it.
                if let Some(tsr_ast::ModuleBody::ModuleBlock(block)) = &node.body {
                    for inner in block.statements {
                        self.statement(inner);
                    }
                }
            }
            Statement::ExpressionStatement(node) => {
                if let Some(Expression::BinaryExpression(assignment)) = &node.expression
                    && assignment
                        .operator_token
                        .is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
                    && let Some(Expression::PropertyAccessExpression(target)) = &assignment.left
                    && matches!(target.expression, Some(Expression::Identifier(_)))
                {
                    // TS9023 `Assigning_properties_to_functions_without_declaring_them_is_not_supported_with_isolatedDeclarations…`.
                    //
                    // Upstream reaches this through `IsExpandoFunctionDeclaration`,
                    // which is checker-backed. The syntactic stand-in is "a
                    // top-level assignment to a property of a top-level name", which
                    // is what `isolatedDeclarationErrors` exercises. It cannot tell
                    // a function from any other binding, so it is deliberately
                    // scoped to top-level statements only.
                    let span = self.span(target.node_id);
                    self.report(
                        &messages::ASSIGNING_PROPERTIES_TO_FUNCTIONS_WITHOUT_DECLARING_THEM_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS_ADD_AN_EXPLICIT_DECLARATION_FOR_THE_PROPERTIES_ASSIGNED_TO_THIS_FUNCTION,
                        span,
                    );
                }
            }
            _ => {}
        }
    }

    fn variable_declaration(&mut self, declaration: &VariableDeclaration<'_>, is_const: bool) {
        if declaration.r#type.is_some() {
            // An annotated declaration emits its annotation; the initialiser never
            // reaches the `.d.ts`, so nothing inside it can fail.
            return;
        }
        let name_span =
            self.span(declaration.name.as_ref().and_then(tsr_ast::BindingName::node_id));
        let Some(initializer) = &declaration.initializer else {
            // TS9010 `Variable_must_have_an_explicit_type_annotation_with_isolatedDeclarations`.
            self.report(
                &messages::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                name_span,
            );
            return;
        };
        if self.infer(initializer, is_const) == Inferability::Generic {
            self.report(
                &messages::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                name_span,
            );
        }
    }

    fn enum_declaration(&mut self, node: &EnumDeclaration<'_>) {
        for member in node.members {
            let Some(initializer) = &member.initializer else { continue };
            if !is_constant_enum_initializer(initializer) {
                // TS9020 `Enum_member_initializers_must_be_computable_without_references_to_external_symbols_with_isolatedDeclarations`.
                let span = self.span(initializer.node_id());
                self.report(
                    &messages::ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBOLS_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
        }
    }

    // ----- class and object members -------------------------------------------

    fn class_element(&mut self, member: &ClassElement<'_>) {
        if is_private(member) {
            // `private` and `#name` members are not emitted, so they cannot fail.
            return;
        }
        match member {
            ClassElement::PropertyDeclaration(property) => {
                self.property_name(&property.name);
                if property.r#type.is_some() {
                    return;
                }
                let name_span = self.span(property.name.node_id());
                let generic = match &property.initializer {
                    None => true,
                    Some(initializer) => self.infer(initializer, false) == Inferability::Generic,
                };
                if generic {
                    // TS9012 `Property_must_have_an_explicit_type_annotation_with_isolatedDeclarations`.
                    self.report(
                        &messages::PROPERTY_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                        name_span,
                    );
                }
            }
            ClassElement::MethodDeclaration(method) => {
                self.property_name(&method.name);
                self.parameters(method.parameters);
                if method.r#type.is_none() && method.full_signature.is_none() {
                    // TS9008 `Method_must_have_an_explicit_return_type_annotation_with_isolatedDeclarations`.
                    let span = self.span(method.name.node_id());
                    self.report(
                        &messages::METHOD_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                        span,
                    );
                }
            }
            ClassElement::ConstructorDeclaration(constructor) => {
                self.parameters(constructor.parameters);
            }
            // Accessors are judged as a pair, in `accessors`; index signatures,
            // static blocks and stray semicolons carry nothing to infer.
            _ => {}
        }
    }

    /// Judge a class or object literal's accessors, which are annotated as a pair.
    ///
    /// TS9009 `At_least_one_accessor_must_have_an_explicit_type_annotation_with_isolatedDeclarations`:
    /// a getter's return type and a setter's parameter type describe the same
    /// property, so *either* suffices. `isolatedDeclarationErrorsClasses` pins this
    /// down with all four combinations.
    fn accessors(&mut self, members: &[AccessorInfo]) {
        for member in members {
            if member.annotated {
                continue;
            }
            if members.iter().any(|other| other.name == member.name && other.annotated) {
                continue;
            }
            self.report(
                &messages::AT_LEAST_ONE_ACCESSOR_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                member.span,
            );
        }
    }

    /// Record a class accessor for the pair rule.
    fn collect_accessor(&mut self, member: &ClassElement<'_>, out: &mut Vec<AccessorInfo>) {
        if is_private(member) {
            return;
        }
        match member {
            ClassElement::GetAccessorDeclaration(getter) => out.push(AccessorInfo {
                name: accessor_key(&getter.name),
                annotated: getter.r#type.is_some() || getter.full_signature.is_some(),
                span: self.span(getter.name.node_id()),
            }),
            ClassElement::SetAccessorDeclaration(setter) => out.push(AccessorInfo {
                name: accessor_key(&setter.name),
                annotated: setter.parameters.iter().any(|p| p.r#type.is_some()),
                span: self.span(setter.name.node_id()),
            }),
            _ => {}
        }
    }

    fn property_name(&mut self, name: &PropertyName<'_>) {
        if let PropertyName::ComputedPropertyName(computed) = name
            && let Some(expression) = &computed.expression
            && !is_simple_computed_name(expression)
        {
            // TS9038 `Computed_property_names_on_class_or_object_literals_cannot_be_inferred_with_isolatedDeclarations`.
            let span = self.span(computed.node_id);
            self.report(
                &messages::COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                span,
            );
        }
    }

    fn parameters(&mut self, parameters: &[&ParameterDeclaration<'_>]) {
        for parameter in parameters {
            if parameter.r#type.is_some() {
                continue;
            }
            // A parameter anchors its diagnostic at the initialiser when it has
            // one, and at the name otherwise. See the table in the module docs.
            let (span, generic) = match &parameter.initializer {
                Some(initializer) => {
                    let outer = std::mem::replace(&mut self.in_parameter_default, true);
                    let result = self.infer(initializer, false);
                    self.in_parameter_default = outer;
                    (self.span(initializer.node_id()), result == Inferability::Generic)
                }
                None => (
                    self.span(parameter.name.as_ref().and_then(tsr_ast::BindingName::node_id)),
                    true,
                ),
            };
            if generic {
                // TS9011 `Parameter_must_have_an_explicit_type_annotation_with_isolatedDeclarations`.
                self.report(
                    &messages::PARAMETER_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
        }
    }

    // ----- expressions --------------------------------------------------------

    /// Whether `expression`'s type is syntactically apparent.
    ///
    /// `is_const` is the const-assertion context: `const x = [1]` and
    /// `let x = [1] as const` differ only in this, and the array rule turns on it.
    fn infer(&mut self, expression: &Expression<'_>, is_const: bool) -> Inferability {
        match expression {
            expression if is_apparent_literal(expression) => Inferability::Ok,

            // `-1` and `+1` are literal types too; anything else prefixed is not.
            Expression::PrefixUnaryExpression(unary) => {
                let numeric = matches!(
                    unary.operand,
                    Some(Expression::NumericLiteral(_) | Expression::BigIntLiteral(_))
                );
                let sign =
                    matches!(unary.operator.kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken);
                if numeric && sign { Inferability::Ok } else { Inferability::Generic }
            }

            // A template with substitutions widens to `string` for a mutable
            // binding, which is writable; under a const assertion it would need the
            // substitutions' types. `isolatedDeclarationErrorsExpressions` contrasts
            // `templateLetOk2` against `templateConstNotOk3` on exactly this.
            Expression::TemplateExpression(_) => {
                if is_const {
                    Inferability::Generic
                } else {
                    Inferability::Ok
                }
            }

            Expression::ParenthesizedExpression(inner) => match &inner.expression {
                Some(expression) => self.infer(expression, is_const),
                None => Inferability::Generic,
            },

            // `x as const` enters a const context; `x as T` states the type outright.
            Expression::AsExpression(as_expression) => {
                if is_const_assertion(as_expression.r#type.as_ref()) {
                    match &as_expression.expression {
                        Some(expression) => self.infer(expression, true),
                        None => Inferability::Generic,
                    }
                } else {
                    Inferability::Ok
                }
            }
            Expression::TypeAssertion(_) => Inferability::Ok,

            // `satisfies` constrains without naming the type, so inference is still
            // required for the operand.
            Expression::SatisfiesExpression(satisfies) => match &satisfies.expression {
                Some(expression) => self.infer(expression, is_const),
                None => Inferability::Generic,
            },

            Expression::ArrayLiteralExpression(array) => {
                if !is_const {
                    // TS9017 `Only_const_arrays_can_be_inferred_with_isolatedDeclarations`.
                    let span = self.span(array.node_id);
                    self.report(
                        &messages::ONLY_CONST_ARRAYS_CAN_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                        span,
                    );
                    return Inferability::Reported;
                }
                let mut result = Inferability::Ok;
                for element in array.elements {
                    if let Expression::SpreadElement(spread) = element {
                        // TS9018 `Arrays_with_spread_elements_can_t_inferred_with_isolatedDeclarations`.
                        let span = self.span(spread.node_id);
                        self.report(
                            &messages::ARRAYS_WITH_SPREAD_ELEMENTS_CAN_T_INFERRED_WITH_ISOLATEDDECLARATIONS,
                            span,
                        );
                        result = Inferability::Reported;
                    } else if self.nested(element, true) {
                        result = Inferability::Reported;
                    }
                }
                result
            }

            Expression::ObjectLiteralExpression(object) => {
                let mut result = Inferability::Ok;
                let mut accessors = Vec::new();
                for property in object.properties {
                    if self.object_member(property, is_const, &mut accessors) {
                        result = Inferability::Reported;
                    }
                }
                let before = self.out.len();
                self.accessors(&accessors);
                if self.out.len() != before {
                    result = Inferability::Reported;
                }
                result
            }

            Expression::ArrowFunction(arrow) => self.arrow(arrow),
            Expression::FunctionExpression(function) => self.function_expression(function),

            Expression::ClassExpression(class) => {
                // TS9022 `Inference_from_class_expressions_is_not_supported_with_isolatedDeclarations`.
                let span = self.span(class.node_id);
                self.report(
                    &messages::INFERENCE_FROM_CLASS_EXPRESSIONS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
                Inferability::Reported
            }

            // Identifiers, calls, `new`, arithmetic, property access, conditionals:
            // all need the checker to say what they evaluate to.
            _ => Inferability::Generic,
        }
    }

    /// Infer a sub-expression of a literal, reporting `TS9013` if it fails.
    ///
    /// Returns whether anything was reported. This is the `Generic` → `TS9013`
    /// conversion described in the module docs: nested failures name themselves,
    /// top-level ones defer to the declaration.
    fn nested(&mut self, expression: &Expression<'_>, is_const: bool) -> bool {
        match self.infer(expression, is_const) {
            Inferability::Ok => false,
            Inferability::Reported => true,
            Inferability::Generic => {
                // TS9013 `Expression_type_can_t_be_inferred_with_isolatedDeclarations`.
                let span = self.span(expression.node_id());
                self.report(
                    &messages::EXPRESSION_TYPE_CAN_T_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
                true
            }
        }
    }

    /// One object-literal member. Returns whether it reported anything.
    fn object_member(
        &mut self,
        property: &ObjectLiteralElementLike<'_>,
        is_const: bool,
        accessors: &mut Vec<AccessorInfo>,
    ) -> bool {
        let before = self.out.len();
        match property {
            ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                self.property_name(&assignment.name);
                if let Some(initializer) = &assignment.initializer {
                    self.nested(initializer, is_const);
                }
            }
            ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                // TS9016 `Objects_that_contain_shorthand_properties_can_t_be_inferred_with_isolatedDeclarations`.
                let span = self.span(shorthand.node_id);
                self.report(
                    &messages::OBJECTS_THAT_CONTAIN_SHORTHAND_PROPERTIES_CAN_T_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
            ObjectLiteralElementLike::SpreadAssignment(spread) => {
                // TS9015 `Objects_that_contain_spread_assignments_can_t_be_inferred_with_isolatedDeclarations`.
                let span = self.span(spread.node_id);
                self.report(
                    &messages::OBJECTS_THAT_CONTAIN_SPREAD_ASSIGNMENTS_CAN_T_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
            ObjectLiteralElementLike::MethodDeclaration(method) => {
                self.property_name(&method.name);
                self.parameters(method.parameters);
                if method.r#type.is_none() && method.full_signature.is_none() {
                    let span = self.span(method.name.node_id());
                    self.report(
                        &messages::METHOD_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                        span,
                    );
                }
            }
            ObjectLiteralElementLike::GetAccessorDeclaration(getter) => {
                self.property_name(&getter.name);
                accessors.push(AccessorInfo {
                    name: accessor_key(&getter.name),
                    annotated: getter.r#type.is_some() || getter.full_signature.is_some(),
                    span: self.span(getter.name.node_id()),
                });
            }
            ObjectLiteralElementLike::SetAccessorDeclaration(setter) => {
                self.property_name(&setter.name);
                accessors.push(AccessorInfo {
                    name: accessor_key(&setter.name),
                    annotated: setter.parameters.iter().any(|p| p.r#type.is_some()),
                    span: self.span(setter.name.node_id()),
                });
            }
        }
        self.out.len() != before
    }

    /// A function or arrow *expression*.
    ///
    /// # The rule is narrower than the corpus's comments claim
    ///
    /// `isolatedDeclarationErrorsReturnTypes` labels a block of exported
    /// `const fn = function foo() { return 0; }` declarations `// Should Error`,
    /// and a block of class fields likewise. **Neither errors** — not in
    /// typescript-go's baseline and not in TypeScript's own
    /// (`_submodules/TypeScript/tests/baselines/reference/…`, identical, 31
    /// diagnostics, first at line 13). Every `TS9007` in that 170-line file is
    /// anchored on a function used as a **parameter default**.
    ///
    /// So a missing return type is an error only there. A function expression
    /// bound to a variable or a property is inferable as far as this analysis is
    /// concerned. Taking the comments at face value cost 54 false positives in
    /// that one case, which is what surfaced this.
    ///
    /// The known exception is checker-driven and out of scope: in
    /// `isolatedDeclarationErrors.ts` an arrow *does* get `TS9007` at (7,30) — but
    /// only because an expando assignment (`errorOnMissingReturn.a = ""`) forces
    /// upstream to build the function's type through `IsExpandoFunctionDeclaration`.
    /// That is exactly the checker dependence ADR-0021 accepted diverging on.
    fn arrow(&mut self, arrow: &ArrowFunction<'_>) -> Inferability {
        self.parameters(arrow.parameters);
        self.return_type(arrow.r#type.is_some() || arrow.full_signature.is_some(), arrow.node_id)
    }

    fn function_expression(&mut self, function: &FunctionExpression<'_>) -> Inferability {
        self.parameters(function.parameters);
        self.return_type(
            function.r#type.is_some() || function.full_signature.is_some(),
            function.node_id,
        )
    }

    fn return_type(&mut self, annotated: bool, node_id: Option<NodeId>) -> Inferability {
        if annotated || !self.in_parameter_default {
            return Inferability::Ok;
        }
        // TS9007 `Function_must_have_an_explicit_return_type_annotation_with_isolatedDeclarations`.
        let span = self.span(node_id);
        self.report(
            &messages::FUNCTION_MUST_HAVE_AN_EXPLICIT_RETURN_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
            span,
        );
        Inferability::Reported
    }
}

/// A getter or setter, reduced to what the pair rule needs.
struct AccessorInfo {
    /// The property name as written. Computed names collapse to their source text,
    /// which pairs `get [k]()` with `set [k]()` and nothing else.
    name: String,
    annotated: bool,
    span: Span,
}

/// The key that pairs a getter with its setter.
fn accessor_key(name: &PropertyName<'_>) -> String {
    match name {
        PropertyName::Identifier(identifier) => identifier.text.to_string(),
        PropertyName::StringLiteral(literal) => literal.text.to_string(),
        PropertyName::NumericLiteral(literal) => literal.text.to_string(),
        PropertyName::PrivateIdentifier(identifier) => format!("#{}", identifier.text),
        PropertyName::BigIntLiteral(literal) => literal.text.to_string(),
        PropertyName::NoSubstitutionTemplateLiteral(literal) => literal.text.to_string(),
        PropertyName::ComputedPropertyName(computed) => {
            format!("[computed:{:?}]", computed.node_id)
        }
    }
}

/// Whether a class member is `private` or `#private`, and so not emitted.
fn is_private(member: &ClassElement<'_>) -> bool {
    let (modifiers, name) = match member {
        ClassElement::PropertyDeclaration(node) => (node.modifiers, Some(&node.name)),
        ClassElement::MethodDeclaration(node) => (node.modifiers, Some(&node.name)),
        ClassElement::GetAccessorDeclaration(node) => (node.modifiers, Some(&node.name)),
        ClassElement::SetAccessorDeclaration(node) => (node.modifiers, Some(&node.name)),
        _ => return false,
    };
    if matches!(name, Some(PropertyName::PrivateIdentifier(_))) {
        return true;
    }
    modifiers.iter().any(|modifier| {
        matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::PrivateKeyword)
    })
}

/// Whether an expression is a literal whose type is its own text.
///
/// `true`, `false` and `null` are keyword expressions rather than literal nodes,
/// which is why they cannot simply join the pattern list.
fn is_apparent_literal(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_)
        | Expression::RegularExpressionLiteral(_) => true,
        Expression::KeywordExpression(keyword) => matches!(
            keyword.kind,
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword
        ),
        _ => false,
    }
}

/// Whether a type node is the `const` of `x as const`.
///
/// The parser gives a const assertion a `TypeReferenceNode` with **no type name**
/// (`crates/tsr-parser/src/types.rs:403`) rather than one named `const`, because
/// `const` is a keyword token and never becomes an identifier. Matching on the
/// name alone silently accepted every `as const` as an ordinary `as T` — which
/// reads as "the type was stated", so nothing was ever reported inside one. Both
/// spellings are matched here so the rule survives a change on either side.
fn is_const_assertion(node: Option<&tsr_ast::TypeNode<'_>>) -> bool {
    let Some(tsr_ast::TypeNode::TypeReferenceNode(reference)) = node else { return false };
    match &reference.type_name {
        None => true,
        Some(tsr_ast::EntityName::Identifier(identifier)) => identifier.text == "const",
        Some(tsr_ast::EntityName::QualifiedName(_)) => false,
    }
}

/// Whether a computed property name is one the emitter can restate verbatim.
///
/// Upstream decides this from the *type* of the name expression — a string or
/// number literal type, or a `unique symbol`. That is checker knowledge, and this
/// is the syntactic stand-in: literals and dotted names pass, everything else does
/// not. `isolatedDeclarationErrorsObjects` shows where the two part company —
/// `[str]` (a `string`-typed const) is an error upstream and accepted here.
/// A known and deliberate divergence, in the direction ADR-0021 anticipates.
fn is_simple_computed_name(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_)
        | Expression::Identifier(_) => true,
        Expression::PropertyAccessExpression(access) => {
            access.expression.as_ref().is_some_and(is_simple_computed_name)
        }
        _ => false,
    }
}

/// Whether an enum member initialiser is computable without external symbols.
///
/// TS9020's condition. Literals, signed literals, and arithmetic over them are
/// fine; a call or an imported name is not. A bare identifier is accepted because
/// the overwhelmingly common case is a reference to an earlier member of the same
/// enum, which is computable — distinguishing that from an external name needs
/// scope information this pass does not have.
fn is_constant_enum_initializer(expression: &Expression<'_>) -> bool {
    match expression {
        // A bare identifier is accepted because it is nearly always an earlier
        // member of the same enum, which is computable; telling that apart from an
        // imported name needs scope information this pass does not have.
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::Identifier(_) => true,
        Expression::ParenthesizedExpression(inner) => {
            inner.expression.as_ref().is_some_and(is_constant_enum_initializer)
        }
        Expression::PrefixUnaryExpression(unary) => {
            unary.operand.as_ref().is_some_and(is_constant_enum_initializer)
        }
        Expression::BinaryExpression(binary) => {
            binary.left.as_ref().is_some_and(is_constant_enum_initializer)
                && binary.right.as_ref().is_some_and(is_constant_enum_initializer)
        }
        _ => false,
    }
}
