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
    let mut checker = Checker {
        nodes,
        out: Vec::new(),
        in_parameter_default: false,
        namespace_imports: namespace_import_names(file.statements),
    };
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
    /// Names bound by `import * as N`: destructuring one of these emits
    /// `typeof N` without inference, while any other entity needs the checker.
    namespace_imports: std::collections::HashSet<String>,
}

/// The names a file binds through namespace imports.
fn namespace_import_names(statements: &[Statement<'_>]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for statement in statements {
        if let Statement::ImportDeclaration(import) = statement
            && let Some(clause) = import.import_clause
            && let Some(tsr_ast::NamedImportBindings::NamespaceImport(namespace)) =
                &clause.named_bindings
            && let Some(name) = namespace.name
        {
            names.insert(name.text.to_string());
        }
    }
    names
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
                self.heritage(node.heritage_clauses);
                let mut accessors = Vec::new();
                for member in node.members {
                    self.class_element(member);
                    self.collect_accessor(member, &mut accessors);
                }
                self.accessors(&accessors);
            }
            Statement::EnumDeclaration(node) => self.enum_declaration(node),
            Statement::InterfaceDeclaration(node) => {
                self.heritage(node.heritage_clauses);
                for member in node.members {
                    self.interface_member(member);
                }
            }
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
                    if self.infer(expression, Ctx::binding(false)) == Inferability::Generic {
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
                    && let Some(Expression::Identifier(base)) = &target.expression
                    // `module.exports = …` and `exports.x = …` are CommonJS export
                    // assignment, not properties bolted onto a function. Found by
                    // running this rule over the whole corpus rather than its
                    // 15-case oracle: it was the only false-positive class among
                    // the eight cases the expando rule blocked on its own.
                    && base.text != "module"
                    && base.text != "exports"
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

    /// `TS9021`, for an `extends` clause the emitter cannot restate.
    ///
    /// A `.d.ts` can write `extends Base` and `extends a.b.Base`; it cannot write
    /// `extends id(Base)`, because naming the resulting type means computing it.
    /// `implements` is not checked: it is emitted as written and never needs a
    /// synthesised name.
    fn heritage(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) {
        for clause in clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for base in clause.types {
                let Some(expression) = &base.expression else { continue };
                if is_entity_name_expression(expression) {
                    continue;
                }
                // TS9021 `Extends_clause_can_t_contain_an_expression_with_isolatedDeclarations`.
                let span = self.span(expression.node_id());
                self.report(
                    &messages::EXTENDS_CLAUSE_CAN_T_CONTAIN_AN_EXPRESSION_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
        }
    }

    fn variable_declaration(&mut self, declaration: &VariableDeclaration<'_>, is_const: bool) {
        // TS9019 is independent of the annotation: `export const [, , b = 1]: T = …`
        // is fully annotated and still reported, because the `.d.ts` would have to
        // restate the destructuring and cannot carry the default.
        if let Some(tsr_ast::BindingName::BindingPattern(pattern)) = &declaration.name {
            self.binding_pattern(pattern);
            // A pattern destructuring a namespace import emits
            // `typeof <entity>` without inference
            // (`declarationEmitExpressionInExtends6`'s `const { Foo } = A`
            // becomes `declare const { Foo }: typeof A;`); destructuring any
            // other entity needs the checker's member types.
            if declaration
                .initializer
                .as_ref()
                .is_some_and(|expression| self.is_namespace_entity(expression))
            {
                return;
            }
        }
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
        if self.infer(initializer, Ctx::binding(is_const)) == Inferability::Generic {
            self.report(
                &messages::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                name_span,
            );
        }
    }

    /// `TS9020`, which is three-valued and transitive rather than a simple predicate.
    ///
    /// `isolatedDeclarationErrorsEnums` separates the three outcomes deliberately,
    /// and reading it as a two-way "constant or not" gets every one of them wrong:
    ///
    /// - `A = computed(0)` — **not an error.** A call is not a constant expression,
    ///   so the member is emitted with no value at all (`declare enum E { A }`) and
    ///   nothing needed inferring. All four members of that file's `enum E` are
    ///   calls and the baseline is silent on every one.
    /// - `AB = A | B` where `A` and `B` are constant siblings — **not an error.**
    ///   `Flag.AB` and `Flag["A"]` are equally fine: a self-reference by any
    ///   spelling stays inside the enum.
    /// - `A = E.A` (another enum), `E = EV` (an outside `const`) — **errors.** A
    ///   constant-shaped expression that reaches outside cannot be folded without
    ///   resolving what it reaches.
    ///
    /// And it is transitive: `enum F { A = E.A, B = A }` errors *twice*. `B` names
    /// a sibling, but that sibling is not constant, so `B` is not either. Members
    /// are therefore folded in declaration order, each seeing the verdicts of the
    /// ones before it.
    ///
    /// Anchored at the member **name** — `isolatedDeclarationErrorsEnums.ts(12,5)`
    /// is the `A` of `A = E.A`, not its initialiser.
    /// An interface member the emitter cannot restate.
    ///
    /// A method signature with no return type is `TS9013`, anchored on the whole
    /// member. `isolatedDeclarationErrorsClasses`'s `interface I` is the evidence,
    /// and it is worth reading carefully because it looks like a computed-name rule
    /// and is not: `[noAnnotationStringName]: 10` — a computed name whose
    /// identifier is `let … : string` — is **clean**, while
    /// `[noAnnotationLiteralName]()` on the next line is reported. What separates
    /// them is the missing return type, not the name; upstream pairs the diagnostic
    /// with `TS7010`, "lacks return-type annotation".
    fn interface_member(&mut self, member: &tsr_ast::TypeElement<'_>) {
        let tsr_ast::TypeElement::MethodSignatureDeclaration(method) = member else { return };
        self.parameters(method.parameters);
        if method.r#type.is_some() || method.full_signature.is_some() {
            return;
        }
        // TS9013 `Expression_type_can_t_be_inferred_with_isolatedDeclarations`.
        let span = self.span(method.node_id);
        self.report(&messages::EXPRESSION_TYPE_CAN_T_BE_INFERRED_WITH_ISOLATEDDECLARATIONS, span);
    }

    /// `TS9019`, for a destructured binding whose element carries a default.
    fn binding_pattern(&mut self, pattern: &tsr_ast::BindingPattern<'_>) {
        for element in pattern.elements {
            if element.initializer.is_some() {
                // TS9019 `Binding_elements_with_initializers_can_t_be_exported_directly_with_isolatedDeclarations`.
                let span = self.span(element.node_id);
                self.report(
                    &messages::BINDING_ELEMENTS_WITH_INITIALIZERS_CAN_T_BE_EXPORTED_DIRECTLY_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
            if let Some(tsr_ast::BindingName::BindingPattern(nested)) = &element.name {
                self.binding_pattern(nested);
            }
        }
    }

    fn enum_declaration(&mut self, node: &EnumDeclaration<'_>) {
        let own_name = node.name.map_or("", |name| name.text);
        let mut folded: Vec<(String, Constness)> = Vec::new();

        for member in node.members {
            let constness = match &member.initializer {
                // No initialiser: auto-numbered, and constant by construction.
                None => Constness::Constant,
                Some(initializer) => fold_enum_initializer(initializer, own_name, &folded),
            };
            if constness == Constness::External {
                // TS9020 `Enum_member_initializers_must_be_computable_without_references_to_external_symbols_with_isolatedDeclarations`.
                let span = self.span(member.name.node_id());
                self.report(
                    &messages::ENUM_MEMBER_INITIALIZERS_MUST_BE_COMPUTABLE_WITHOUT_REFERENCES_TO_EXTERNAL_SYMBOLS_WITH_ISOLATEDDECLARATIONS,
                    span,
                );
            }
            folded.push((accessor_key(&member.name), constness));
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
                // `readonly` makes the initialiser a const context, exactly as
                // `const` does for a variable: `readonly t = ` + "`s${1}`" + ` `
                // keeps its template literal type instead of widening to `string`,
                // so it needs an annotation. `isolatedDeclarationErrorsExpressions`
                // has the same three template initialisers twice over, once on
                // `export let` (clean) and once on `readonly` fields (TS9012).
                let is_const = has_modifier(property.modifiers, SyntaxKind::ReadonlyKeyword);
                let generic = match &property.initializer {
                    None => true,
                    Some(initializer) => {
                        self.infer(initializer, Ctx::binding(is_const)) == Inferability::Generic
                    }
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
            // A *pair* never reports, even when neither half is annotated.
            // `isolatedDeclarationErrorsClasses` puts the four combinations side by
            // side and only the lone accessors are squiggled: `get getOnly()` and
            // `set setOnly(value)` error, while `get getSetBad()` /
            // `set getSetBad(value)` — neither annotated — is clean in both
            // typescript-go's baseline and TypeScript's own.
            if members.iter().any(|other| other.name == member.name && other.kind != member.kind) {
                continue;
            }
            // A lone *getter* reports only in a class. The same shape in an object
            // literal is silent: `isolatedDeclarationErrorsObjects` has
            // `get singleGetterBad() { return 0 }` with no diagnostic, while its
            // `set singleSetterBad(value)` on the next line has one. The asymmetry
            // is taken from the baselines rather than derived, and it is the kind of
            // thing a printer slice may well revise.
            if member.kind == AccessorKind::Get && !member.in_class {
                continue;
            }
            self.report(
                &messages::AT_LEAST_ONE_ACCESSOR_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
                member.span,
            );
        }
    }

    /// Where a setter's `TS9009` goes: its parameter, falling back to its name.
    fn setter_anchor(
        &self,
        parameters: &[&ParameterDeclaration<'_>],
        name: Option<NodeId>,
    ) -> Span {
        parameters
            .first()
            .and_then(|parameter| parameter.name.as_ref().and_then(tsr_ast::BindingName::node_id))
            .map_or_else(|| self.span(name), |id| self.span(Some(id)))
    }

    /// Record a class accessor for the pair rule.
    ///
    /// The accessor's name is checked here rather than in `class_element`, and a
    /// name that fails suppresses the pair rule: upstream reports `TS9038` at
    /// `get [noAnnotationStringName]()` and *not* `TS9009`, at the same position.
    /// Reporting both would put two diagnostics on one squiggle.
    fn collect_accessor(&mut self, member: &ClassElement<'_>, out: &mut Vec<AccessorInfo>) {
        if is_private(member) {
            return;
        }
        if let Some(name) = accessor_name(member) {
            let before = self.out.len();
            self.property_name(name);
            if self.out.len() != before {
                return;
            }
        }
        match member {
            ClassElement::GetAccessorDeclaration(getter) => out.push(AccessorInfo {
                name: accessor_key(&getter.name),
                kind: AccessorKind::Get,
                in_class: true,
                annotated: getter.r#type.is_some() || getter.full_signature.is_some(),
                span: self.span(getter.name.node_id()),
            }),
            ClassElement::SetAccessorDeclaration(setter) => out.push(AccessorInfo {
                name: accessor_key(&setter.name),
                kind: AccessorKind::Set,
                in_class: true,
                annotated: setter.parameters.iter().any(|p| p.r#type.is_some()),
                span: self.setter_anchor(setter.parameters, setter.name.node_id()),
            }),
            _ => {}
        }
    }

    fn property_name(&mut self, name: &PropertyName<'_>) {
        if let PropertyName::ComputedPropertyName(computed) = name
            && let Some(expression) = &computed.expression
            && !Self::is_restatable_computed_name(expression)
        {
            // TS9038 `Computed_property_names_on_class_or_object_literals_cannot_be_inferred_with_isolatedDeclarations`.
            let span = self.span(computed.node_id);
            self.report(
                &messages::COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOLATEDDECLARATIONS,
                span,
            );
        }
    }

    /// Whether a computed property name is one the emitter can restate.
    ///
    /// Only a literal is. Not an identifier, however it was declared; not an enum
    /// member; not a `unique symbol`.
    ///
    /// This is narrower than it looks like it should be, and the corpus is
    /// unambiguous about it. `isolatedDeclarationErrorsObjects` writes six computed
    /// names in one object literal — `[1]`, `[1 + 3]`, `[prop(2)]`, `[s]` where
    /// `const s: unique symbol`, `[E.V]` where `E` is an enum, and `[str]` — and
    /// **only `[1]` is accepted**. `isolatedDeclarationErrorsClasses` agrees for
    /// class members: every computed name in its `class C` is reported, including
    /// `[noAnnotationLiteralName]`, which is declared `const … = "…"` and therefore
    /// does have a literal type.
    ///
    /// An earlier version of this rule resolved the name through a top-level scope
    /// index to ask whether it was a `const` with a literal type, on the assumption
    /// that a literal-typed name would be restatable. It is not, the machinery
    /// bought nothing, and it was removed. The one place the corpus's own comment
    /// says otherwise — `[missing] = 1`, marked "Should not be reported as an
    /// isolated declaration error" — is reported by TypeScript too.
    fn is_restatable_computed_name(expression: &Expression<'_>) -> bool {
        match expression {
            Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::NoSubstitutionTemplateLiteral(_) => true,
            // `[-1]` is a literal type too. `computedPropertiesNarrowed` accepts
            // `{ [-1]: 1 }` and rejects `{ [1-1]: 1 }` on the next line, which is
            // the whole distinction: a signed literal, not an arithmetic result.
            Expression::PrefixUnaryExpression(unary) => {
                matches!(unary.operator.kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
                    && matches!(unary.operand, Some(Expression::NumericLiteral(_)))
            }
            _ => false,
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
                    let result = self.infer(initializer, Ctx::binding(false));
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
    /// See [`Ctx`] for why the const-ness of the *binding* and the const-ness of
    /// an *assertion* are two flags rather than one.
    fn infer(&mut self, expression: &Expression<'_>, ctx: Ctx) -> Inferability {
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
                if ctx.fresh {
                    Inferability::Generic
                } else {
                    Inferability::Ok
                }
            }

            Expression::ParenthesizedExpression(inner) => match &inner.expression {
                Some(expression) => self.infer(expression, ctx),
                None => Inferability::Generic,
            },

            // `x as const` enters a const context; `x as T` states the type outright.
            Expression::AsExpression(as_expression) => {
                if is_const_assertion(as_expression.r#type.as_ref()) {
                    match &as_expression.expression {
                        Some(expression) => self.infer(expression, Ctx::ASSERTED),
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
                Some(expression) => self.infer(expression, ctx),
                None => Inferability::Generic,
            },

            Expression::ArrayLiteralExpression(array) => {
                if !ctx.asserted {
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
                    } else if self.nested(element, Ctx::ASSERTED) {
                        result = Inferability::Reported;
                    }
                }
                result
            }

            Expression::ObjectLiteralExpression(object) => {
                let mut result = Inferability::Ok;
                let mut accessors = Vec::new();
                for property in object.properties {
                    if self.object_member(property, ctx, &mut accessors) {
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
    fn nested(&mut self, expression: &Expression<'_>, ctx: Ctx) -> bool {
        match self.infer(expression, ctx) {
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
        ctx: Ctx,
        accessors: &mut Vec<AccessorInfo>,
    ) -> bool {
        let before = self.out.len();
        match property {
            ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                self.property_name(&assignment.name);
                if let Some(initializer) = &assignment.initializer {
                    self.nested(initializer, ctx);
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
                let before = self.out.len();
                self.property_name(&getter.name);
                if self.out.len() != before {
                    return true;
                }
                accessors.push(AccessorInfo {
                    name: accessor_key(&getter.name),
                    kind: AccessorKind::Get,
                    in_class: false,
                    annotated: getter.r#type.is_some() || getter.full_signature.is_some(),
                    span: self.span(getter.name.node_id()),
                });
            }
            ObjectLiteralElementLike::SetAccessorDeclaration(setter) => {
                let before = self.out.len();
                self.property_name(&setter.name);
                if self.out.len() != before {
                    return true;
                }
                accessors.push(AccessorInfo {
                    name: accessor_key(&setter.name),
                    kind: AccessorKind::Set,
                    in_class: false,
                    annotated: setter.parameters.iter().any(|p| p.r#type.is_some()),
                    span: self.setter_anchor(setter.parameters, setter.name.node_id()),
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
        // A concise body *is* the return type when the expression is itself
        // apparent: `(cb = () => 1)` needs no annotation, while
        // `(cb = function(){ })` does. `isolatedDeclarationErrorsReturnTypes` pairs
        // the two spellings on consecutive lines nine times over and reports only
        // the block-bodied one each time.
        let concise = arrow.body.as_ref().is_some_and(concise_body_is_apparent_literal);
        let annotated = concise || arrow.r#type.is_some() || arrow.full_signature.is_some();
        self.return_type(annotated, arrow.node_id)
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

/// Which half of a property's accessor pair this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessorKind {
    Get,
    Set,
}

/// A getter or setter, reduced to what the pair rule needs.
struct AccessorInfo {
    /// The property name as written. Computed names collapse to a per-node key, so
    /// `get [k]()` pairs with nothing — which is the conservative direction.
    name: String,
    kind: AccessorKind,
    /// Whether this is a class member rather than an object-literal property.
    in_class: bool,
    annotated: bool,
    /// Where the diagnostic goes: a getter's *name*, but a setter's *parameter*.
    /// `isolatedDeclarationErrorsClasses.ts(11,9)` is `getOnly`; `(12,17)` is the
    /// `value` of `set setOnly(value)`, not the `setOnly`.
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

/// Whether an arrow's concise body is a literal, and so its own return type.
///
/// `ConciseBody` is `Block | Expression`, and the generated union spells out every
/// expression variant, so this mirrors [`is_apparent_literal`] over that union
/// rather than converting between the two.
fn concise_body_is_apparent_literal(body: &tsr_ast::ConciseBody<'_>) -> bool {
    use tsr_ast::ConciseBody as Body;
    match body {
        Body::NumericLiteral(_)
        | Body::StringLiteral(_)
        | Body::BigIntLiteral(_)
        | Body::NoSubstitutionTemplateLiteral(_)
        | Body::RegularExpressionLiteral(_) => true,
        Body::KeywordExpression(keyword) => matches!(
            keyword.kind,
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword
        ),
        _ => false,
    }
}

impl Checker<'_> {
    /// Whether an entity-name expression is rooted at a namespace import.
    fn is_namespace_entity(&self, expression: &Expression<'_>) -> bool {
        match expression {
            Expression::Identifier(identifier) => self.namespace_imports.contains(identifier.text),
            Expression::PropertyAccessExpression(access) => {
                access.expression.as_ref().is_some_and(|inner| self.is_namespace_entity(inner))
            }
            _ => false,
        }
    }
}

/// Whether an expression is a plain name or a dotted chain of them.
fn is_entity_name_expression(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Identifier(_) => true,
        Expression::PropertyAccessExpression(access) => {
            access.expression.as_ref().is_some_and(is_entity_name_expression)
        }
        _ => false,
    }
}

/// The property name of a class accessor, if the member is one.
fn accessor_name<'b, 'a>(member: &'b ClassElement<'a>) -> Option<&'b PropertyName<'a>> {
    match member {
        ClassElement::GetAccessorDeclaration(getter) => Some(&getter.name),
        ClassElement::SetAccessorDeclaration(setter) => Some(&setter.name),
        _ => None,
    }
}

/// Whether a modifier list carries a given keyword.
fn has_modifier(modifiers: &[ModifierLike<'_>], kind: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == kind))
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
    has_modifier(modifiers, SyntaxKind::PrivateKeyword)
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

/// The two kinds of const-ness, which are not the same kind.
///
/// `is_const` used to be one flag, passed the *declaration's* const-ness and
/// documented as the *assertion* context. Two rules read it and they want
/// different answers, which `compiler/isolatedDeclarationErrorsExpressions`
/// settles line by line:
///
/// | | `let` | `const` | `as const` |
/// |---|---|---|---|
/// | `` `s${1}` `` | clean | **TS9010** | **TS9010** |
/// | `[1, 2, 3]` | **TS9017** | ? | clean |
///
/// The template row is about **widening**: a `let` widens to `string`, which is
/// emittable, while a `const` keeps the template literal type and would need its
/// substitutions' types. A `readonly` property behaves like `const`, and the same
/// file shows it.
///
/// The array row is about **assertion**: `[1, 2, 3]` is `number[]` under both
/// `let` and `const`, and neither can be restated from the written syntax without
/// widening and unioning the elements. Only `as const` makes it a `readonly` tuple
/// of literal types, each of which *is* written down.
///
/// The `?` is the cell no corpus case covers — there is no plain-`const` array in
/// any `@isolatedDeclarations` case, which is why one flag served for both rules
/// without the conformance suite noticing. It is `TS9017` by the reasoning above:
/// const-ness of the binding does not make an array literal a tuple. That is a
/// deduction from the other three cells rather than a measurement, and it is
/// flagged as such here so the next person can overturn it with a baseline rather
/// than by re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ctx {
    /// The declared type keeps its literal types — a `const` binding, a `readonly`
    /// property, or anything inside an `as const`.
    fresh: bool,
    /// Inside an `as const`.
    asserted: bool,
}

impl Ctx {
    /// Inside a const assertion: both hold.
    const ASSERTED: Self = Self { fresh: true, asserted: true };

    /// The context a binding establishes. `is_const` is `const`/`readonly`.
    const fn binding(is_const: bool) -> Self {
        Self { fresh: is_const, asserted: false }
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

/// How an enum member's value can be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Constness {
    /// Foldable from literals and constant siblings.
    Constant,
    /// Not a constant expression at all — a call, say. Emitted without a value,
    /// which is legal, so this is *not* an error.
    Dynamic,
    /// Constant-shaped, but reaching a symbol outside this enum. This is TS9020.
    External,
}

/// Fold one enum member initialiser against the members already folded.
///
/// `siblings` are the earlier members of the same enum with their verdicts, which
/// is what makes the rule transitive — see `Checker::enum_declaration`.
fn fold_enum_initializer(
    expression: &Expression<'_>,
    own_name: &str,
    siblings: &[(String, Constness)],
) -> Constness {
    let sibling = |name: &str| {
        siblings
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map_or(Constness::External, |(_, constness)| *constness)
    };

    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_) => Constness::Constant,

        Expression::Identifier(identifier) => sibling(identifier.text),

        // `Self.Member` and `Self["Member"]` stay inside the enum; any other
        // qualifier reaches outside it.
        Expression::PropertyAccessExpression(access) => match (&access.expression, &access.name) {
            (Some(Expression::Identifier(target)), Some(tsr_ast::MemberName::Identifier(m)))
                if target.text == own_name =>
            {
                sibling(m.text)
            }
            _ => Constness::External,
        },
        Expression::ElementAccessExpression(access) => {
            match (&access.expression, &access.argument_expression) {
                (
                    Some(Expression::Identifier(target)),
                    Some(Expression::StringLiteral(argument)),
                ) if target.text == own_name => sibling(argument.text),
                _ => Constness::External,
            }
        }

        Expression::ParenthesizedExpression(inner) => inner
            .expression
            .as_ref()
            .map_or(Constness::Dynamic, |e| fold_enum_initializer(e, own_name, siblings)),
        Expression::PrefixUnaryExpression(unary) => unary
            .operand
            .as_ref()
            .map_or(Constness::Dynamic, |e| fold_enum_initializer(e, own_name, siblings)),
        Expression::BinaryExpression(binary) => {
            let left = binary
                .left
                .as_ref()
                .map_or(Constness::Dynamic, |e| fold_enum_initializer(e, own_name, siblings));
            let right = binary
                .right
                .as_ref()
                .map_or(Constness::Dynamic, |e| fold_enum_initializer(e, own_name, siblings));
            // Reaching outside anywhere in the expression poisons the whole of it;
            // otherwise a non-constant operand makes the result non-constant.
            match (left, right) {
                (Constness::External, _) | (_, Constness::External) => Constness::External,
                (Constness::Dynamic, _) | (_, Constness::Dynamic) => Constness::Dynamic,
                _ => Constness::Constant,
            }
        }

        // Calls and everything else: a computed member, emitted without a value.
        _ => Constness::Dynamic,
    }
}
