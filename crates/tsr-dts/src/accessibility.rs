//! Declaration-emit accessibility errors for names written in declarations.
//!
//! # This module *is* a port, unlike the rest of the crate
//!
//! [`crate::rules`] is written against the `isolatedDeclarations` specification
//! because upstream has no syntactic predicate to port (ADR-0021). The errors
//! here are the opposite case: upstream raises them from a syntactic walk —
//! `DeclarationTransformer` (`internal/transformers/declarations/transform.go`)
//! visits each declaration that reaches the `.d.ts` and, at every written
//! entity name, asks `EmitResolver.IsEntityNameVisible`
//! (`checkEntityNameVisibility`, `transform.go:1697`). A name the `.d.ts`
//! could not refer to is reported through the diagnostic context the walk
//! keeps (`setupDiagnosticContext`, `transform.go:551`, and
//! `createGetSymbolAccessibilityDiagnosticForNode`, `diagnostics.go:82`).
//! So these items name their `transform.go` / `diagnostics.go` counterparts,
//! and the checker half lives behind [`AccessibilityResolver`].
//!
//! Why the walk is here rather than in `tsr-declarations`, which ports the
//! same transform for its output: `docs/parity/notes/r4-declemit.md` §2.
//!
//! # What is walked, and what is declined
//!
//! Mostly the written-annotation half: `TypeReference`, `TypeQuery`, heritage
//! `ExpressionWithTypeArguments` and `import x = N.y`. Declarations whose type
//! upstream *infers* go through the node builder's `SymbolTracker`
//! (`transform.go:1667`, `CreateTypeOfDeclaration`); this port has the
//! tracker side ([`TrackerReport`]) and asks the resolver for the reports of
//! the node-builder arms it reaches (`docs/parity/notes/r5-declemit2.md` §3).
//! Every other inferred type's errors are missed rather than invented. The
//! same holds for every arm below marked *declined*: each skips a subtree
//! upstream would visit, never visits one upstream skips.

use rustc_hash::FxHashSet;
use tsr_ast::{
    BindingName, ClassElement, ModifierLike, ModuleBody, Node, NodeId, NodeMap, NodeTable,
    PropertyName, SyntaxKind as K, TypeElement,
};
use tsr_diagnostics::{Diagnostic, Message, messages as m};

/// `printer.SymbolAccessibilityResult`, as `IsEntityNameVisible` returns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityNameVisibility {
    /// `SymbolAccessibilityAccessible` with `AliasesToMakeVisible`.
    Accessible(Vec<NodeId>),
    /// `SymbolAccessibilityNotAccessible` with `ErrorSymbolName` and
    /// `ErrorNode`.
    NotAccessible {
        /// The name the `.d.ts` cannot write.
        symbol_name: String,
        /// Where the error is reported.
        error_node: NodeId,
    },
    /// `SymbolAccessibilityNotResolved`: the checker reports it instead.
    NotResolved,
}

/// The `printer.EmitResolver` calls this walk makes.
pub trait AccessibilityResolver {
    /// `PrecalculateDeclarationEmitVisibility`.
    fn precalculate_declaration_emit_visibility(&mut self, file: NodeId);
    /// `IsDeclarationVisible`.
    fn is_declaration_visible(&mut self, node: NodeId) -> bool;
    /// `IsEntityNameVisible`.
    fn is_entity_name_visible(
        &mut self,
        entity_name: NodeId,
        enclosing: NodeId,
    ) -> EntityNameVisibility;
    /// `IsImplementationOfOverload`.
    fn is_implementation_of_overload(&mut self, node: NodeId) -> bool;
    /// `IsImportRequiredByAugmentation`, asked of an `import` declaration.
    fn is_import_required_by_augmentation(&mut self, _import: NodeId) -> bool {
        false
    }
    /// The `SymbolTracker` calls the node builder makes serializing the
    /// inferred type of `node`: `CreateTypeOfDeclaration` for a variable
    /// declaration or an export assignment, `CreateTypeOfExpression` for an
    /// `ExpressionWithTypeArguments`.
    fn inferred_type_reports(&mut self, _node: NodeId) -> Vec<TrackerReport> {
        Vec::new()
    }
}

/// A `checker.SymbolTracker` call (`checker/symboltracker.go`) the walk turns
/// into a diagnostic (`SymbolTrackerImpl`, `tracker.go`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackerReport {
    /// `ReportPrivateInBaseOfClassExpression(propertyName)` (`tracker.go:131`).
    PrivateInBaseOfClassExpression(String),
}

/// The `SymbolTrackerSharedState` options the walk reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct WalkOptions {
    /// `isolatedDeclarations` (`tracker.go:238`).
    pub isolated_declarations: bool,
}

/// Report the accessibility errors for written names in one TypeScript file.
///
/// `getDeclarationDiagnostics` (`compiler/emitter.go:520`) over
/// `TransformSourceFile`; the caller applies `sourceFileMayBeEmitted`.
#[must_use]
pub fn written_name_diagnostics(
    file: NodeId,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    text: &str,
    resolver: &mut impl AccessibilityResolver,
) -> Vec<Diagnostic> {
    declaration_walk_diagnostics(file, nodes, map, text, WalkOptions::default(), resolver)
}

/// [`written_name_diagnostics`] with the transform's options, which add the
/// `isolatedDeclarations` errors the transform itself raises (TS9026).
#[must_use]
pub fn declaration_walk_diagnostics(
    file: NodeId,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    text: &str,
    options: WalkOptions,
    resolver: &mut impl AccessibilityResolver,
) -> Vec<Diagnostic> {
    let Some(Node::SourceFile(source)) = map.get(file) else { return Vec::new() };
    let mut walk = Walk {
        nodes,
        map,
        text,
        options,
        resolver,
        enclosing: file,
        context: None,
        suppress: false,
        error_name_node: None,
        fallback: Vec::new(),
        late_marked: Vec::new(),
        transformed: FxHashSet::default(),
        out: Vec::new(),
    };
    // `visitSourceFile` (`transform.go:280`).
    walk.resolver.precalculate_declaration_emit_visibility(file);
    // `transformSourceFile` (`transform.go:341`).
    for statement in source.statements {
        if let Some(id) = statement.node_id() {
            walk.visit(id);
        }
    }
    walk.transform_late_painted_statements();
    walk.out
}

/// Which `GetSymbolAccessibilityDiagnostic` closure is current.
#[derive(Clone, Copy)]
enum Context {
    /// `createGetSymbolAccessibilityDiagnosticForNode(node)`.
    ForNode(NodeId),
}

struct Walk<'a, 'n, 'r, R> {
    nodes: &'n NodeTable,
    map: &'n NodeMap<'a>,
    text: &'n str,
    options: WalkOptions,
    resolver: &'r mut R,
    /// `tx.enclosingDeclaration`.
    enclosing: NodeId,
    /// `tx.state.getSymbolAccessibilityDiagnostic`; `None` is
    /// `throwDiagnostic`.
    context: Option<Context>,
    /// `tx.suppressNewDiagnosticContexts`.
    suppress: bool,
    /// `tx.state.errorNameNode`.
    error_name_node: Option<NodeId>,
    /// `SymbolTrackerImpl.fallbackStack`.
    fallback: Vec<NodeId>,
    /// `tx.state.lateMarkedStatements`.
    late_marked: Vec<NodeId>,
    /// The keys of `tx.lateStatementReplacementMap`.
    transformed: FxHashSet<NodeId>,
    out: Vec<Diagnostic>,
}

impl<'a, R: AccessibilityResolver> Walk<'a, '_, '_, R> {
    fn kind(&self, id: NodeId) -> K {
        self.nodes.kind(id)
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.parent(id)
    }

    /// `visit` (`transform.go:226`).
    fn visit(&mut self, id: NodeId) {
        match self.kind(id) {
            K::FunctionDeclaration
            | K::ModuleDeclaration
            | K::ImportEqualsDeclaration
            | K::InterfaceDeclaration
            | K::ClassDeclaration
            | K::TypeAliasDeclaration
            | K::EnumDeclaration
            | K::VariableStatement
            | K::ImportDeclaration
            | K::ExportDeclaration
            | K::ExportAssignment => self.visit_declaration_statements(id),
            K::BreakStatement
            | K::ContinueStatement
            | K::DebuggerStatement
            | K::DoStatement
            | K::EmptyStatement
            | K::ForInStatement
            | K::ForOfStatement
            | K::ForStatement
            | K::IfStatement
            | K::LabeledStatement
            | K::ReturnStatement
            | K::SwitchStatement
            | K::ThrowStatement
            | K::TryStatement
            | K::WhileStatement
            | K::WithStatement
            | K::NotEmittedStatement
            | K::Block
            | K::MissingDeclaration
            | K::ExpressionStatement => {}
            _ => self.visit_declaration_subtree(id),
        }
    }

    /// `visitDeclarationStatements` (`transform.go:1144`). Export
    /// declarations and assignments write no entity name this walk checks
    /// (`export default <expr>` goes through the node builder: declined).
    fn visit_declaration_statements(&mut self, id: NodeId) {
        match self.kind(id) {
            K::ExportDeclaration => {}
            K::ExportAssignment => self.transform_export_assignment(id),
            _ => {
                if self.transformed.insert(id) {
                    self.transform_top_level_declaration(id);
                }
            }
        }
    }

    /// `transformExportAssignment` (`transform.go:1210`), for its `_default`
    /// arm: an expression that is neither an identifier, a class expression,
    /// a function-like expression nor a primitive literal gets a synthesized
    /// variable typed by `ensureType(assignment)`, under the
    /// `Default_export_of_the_module_has_or_is_using_private_name_0` context
    /// with the assignment pushed as the tracker's fallback node (`:1251`).
    /// The identifier arm writes `export default <name>` and checks nothing
    /// here; the class and function arms rebuild declarations (declined).
    fn transform_export_assignment(&mut self, id: NodeId) {
        let Some(Node::ExportAssignment(assignment)) = self.map.get(id) else { return };
        let Some(expression) = assignment.expression.and_then(|e| e.node_id()) else { return };
        if self.kind(expression) == K::Identifier {
            return;
        }
        // `SkipOuterExpressions(expression, OEKExpressionTypePassthrough)`.
        let mut unwrapped = expression;
        loop {
            let inner = match self.map.get(unwrapped) {
                Some(Node::ParenthesizedExpression(n)) => n.expression,
                Some(Node::AsExpression(n)) => n.expression,
                Some(Node::TypeAssertion(n)) => n.expression,
                Some(Node::SatisfiesExpression(n)) => n.expression,
                Some(Node::NonNullExpression(n)) => n.expression,
                _ => break,
            };
            match inner.and_then(|e| e.node_id()) {
                Some(inner) => unwrapped = inner,
                None => return,
            }
        }
        if matches!(
            self.kind(unwrapped),
            K::ClassExpression | K::FunctionExpression | K::ArrowFunction
        ) {
            return;
        }
        // `IsPrimitiveLiteralValue(unwrapParenthesizedExpression(e), true)`
        // takes the initializer arm, which serializes no type. Every arm the
        // tracker reports here is an object type, so declining any literal-
        // or prefix-shaped expression loses nothing.
        if matches!(
            self.kind(unwrapped),
            K::StringLiteral
                | K::NumericLiteral
                | K::BigIntLiteral
                | K::NoSubstitutionTemplateLiteral
                | K::TrueKeyword
                | K::FalseKeyword
                | K::PrefixUnaryExpression
        ) {
            return;
        }
        let previous_context = self.context;
        self.context = Some(Context::ForNode(id));
        self.fallback.push(id);
        // `ensureType(assignment)`: an export assignment has no name.
        let previous_name = self.error_name_node.take();
        self.track_inferred_type(id);
        self.error_name_node = previous_name;
        self.fallback.pop();
        self.context = previous_context;
    }

    /// The `SymbolTrackerImpl` side of a node-builder call
    /// (`tracker.go:44`–`:141`): each report becomes its diagnostic at
    /// `errorLocation()` — `errorNameNode`, else the fallback stack's top.
    fn track_inferred_type(&mut self, node: NodeId) {
        let reports = self.resolver.inferred_type_reports(node);
        if reports.is_empty() {
            return;
        }
        let Some(location) = self.error_name_node.or_else(|| self.fallback.last().copied()) else {
            return;
        };
        let span = self.nodes.span(location);
        for report in reports {
            match report {
                TrackerReport::PrivateInBaseOfClassExpression(name) => {
                    self.out.push(Diagnostic::with_args(
                        &m::PROPERTY_0_OF_EXPORTED_ANONYMOUS_CLASS_TYPE_MAY_NOT_BE_PRIVATE_OR_PROTECTED,
                        span,
                        vec![name],
                    ));
                }
            }
        }
    }

    /// `transformAndReplaceLatePaintedStatements` (`transform.go:386`): the
    /// queue, not the splice.
    fn transform_late_painted_statements(&mut self) {
        while !self.late_marked.is_empty() {
            let next = self.late_marked.remove(0);
            self.transformed.insert(next);
            self.transform_top_level_declaration(next);
        }
    }

    /// `isDeclarationAndNotVisible` (`declarations/util.go:61`).
    fn is_declaration_and_not_visible(&mut self, id: NodeId) -> bool {
        match self.kind(id) {
            K::FunctionDeclaration
            | K::ModuleDeclaration
            | K::InterfaceDeclaration
            | K::ClassDeclaration
            | K::TypeAliasDeclaration
            | K::EnumDeclaration => !self.resolver.is_declaration_visible(id),
            K::VariableDeclaration => !self.binding_name_visible(id),
            K::ClassStaticBlockDeclaration => true,
            _ => false,
        }
    }

    /// `getBindingNameVisible` (`declarations/util.go:87`).
    fn binding_name_visible(&mut self, id: NodeId) -> bool {
        let name = match self.map.get(id) {
            Some(Node::VariableDeclaration(declaration)) => declaration.name,
            Some(Node::BindingElement(element)) => element.name,
            _ => return false,
        };
        match name {
            None => false,
            Some(BindingName::BindingPattern(pattern)) => {
                let elements: Vec<NodeId> =
                    pattern.elements.iter().filter_map(|element| element.node_id).collect();
                elements.into_iter().any(|element| self.binding_name_visible(element))
            }
            Some(BindingName::Identifier(_)) => self.resolver.is_declaration_visible(id),
        }
    }

    /// `transformTopLevelDeclaration` (`transform.go:1703`).
    fn transform_top_level_declaration(&mut self, id: NodeId) {
        self.late_marked.retain(|&statement| statement != id);
        match self.kind(id) {
            K::ImportEqualsDeclaration => return self.transform_import_equals_declaration(id),
            K::ImportDeclaration => return self.transform_import_declaration(id),
            _ => {}
        }
        if self.is_declaration_and_not_visible(id) {
            return;
        }
        if self.resolver.is_implementation_of_overload(id) {
            return;
        }
        let previous_enclosing = self.enclosing;
        if is_enclosing_declaration(self.kind(id)) {
            self.enclosing = id;
        }
        let previous_context = self.context;
        if can_produce_diagnostics(self.kind(id)) {
            self.context = Some(Context::ForNode(id));
        }
        match self.map.get(id) {
            Some(Node::TypeAliasDeclaration(alias)) => {
                self.visit_type_parameters(alias.type_parameters);
                self.visit_opt(alias.r#type.and_then(|t| t.node_id()));
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                self.visit_type_parameters(interface.type_parameters);
                for clause in interface.heritage_clauses {
                    self.visit_opt(clause.node_id);
                }
                for member in interface.members {
                    self.visit_opt(type_element_id(member));
                }
            }
            Some(Node::FunctionDeclaration(function)) => {
                // `transformFunctionDeclaration` (`transform.go:1805`).
                self.ensure_type_params(id, function.type_parameters);
                self.update_param_list(id, function.parameters);
                self.ensure_type(id, false);
            }
            Some(Node::ModuleDeclaration(module)) => self.transform_module_declaration(module.body),
            Some(Node::ClassDeclaration(class)) => self.transform_class_declaration(id, class),
            Some(Node::VariableStatement(statement)) => {
                // `transformVariableStatement` (`transform.go:2207`).
                let declarations: Vec<NodeId> = statement
                    .declaration_list
                    .map(|list| list.declarations.iter().filter_map(|d| d.node_id).collect())
                    .unwrap_or_default();
                if declarations.iter().any(|&d| self.binding_name_visible(d)) {
                    for declaration in declarations {
                        self.visit(declaration);
                    }
                }
            }
            // `transformEnumDeclaration` writes member values, no names.
            _ => {}
        }
        self.enclosing = previous_enclosing;
        self.context = previous_context;
    }

    /// `transformModuleDeclaration` (`transform.go:1822`).
    fn transform_module_declaration(&mut self, body: Option<ModuleBody<'a>>) {
        match body {
            Some(ModuleBody::ModuleBlock(block)) => {
                for statement in block.statements {
                    self.visit_opt(statement.node_id());
                }
                self.transform_late_painted_statements();
            }
            Some(ModuleBody::ModuleDeclaration(inner)) => self.visit_opt(inner.node_id),
            None => {}
        }
    }

    /// `transformClassDeclaration` (`transform.go:1978`) with
    /// `buildClassMembers` (`transform.go:1918`).
    fn transform_class_declaration(&mut self, id: NodeId, class: &tsr_ast::ClassDeclaration<'a>) {
        let previous_enclosing = self.enclosing;
        self.enclosing = id;
        // `tx.state.errorNameNode = input.Name()` and the class pushed as the
        // fallback node (`transform.go:1983`). Native does not restore
        // `errorNameNode` afterwards; every later reader sets its own.
        self.error_name_node = class.name.and_then(|name| name.node_id);
        self.fallback.push(id);
        self.ensure_type_params(id, class.type_parameters);
        // Parameter properties first.
        let constructor = class.members.iter().find_map(|member| match member {
            ClassElement::ConstructorDeclaration(ctor) if ctor.body.is_some() => Some(*ctor),
            _ => None,
        });
        if let Some(ctor) = constructor {
            let previous_context = self.context;
            for parameter in ctor.parameters {
                let Some(param) = parameter.node_id else { continue };
                if !has_parameter_property_modifier(parameter.modifiers) {
                    continue;
                }
                self.context = Some(Context::ForNode(param));
                if matches!(parameter.name, Some(BindingName::Identifier(_))) {
                    self.ensure_type(param, false);
                }
                // A binding-pattern parameter property (`walkBindingPattern`):
                // declined.
            }
            self.context = previous_context;
        }
        for member in class.members {
            self.visit_opt(class_element_id(member));
        }
        // `getEffectiveBaseTypeNode` with a non-entity-name, non-`null`
        // expression is serialized by the node builder
        // (`CreateTypeOfExpression`, `transform.go:2018`) as the type of a
        // synthesized `<name>_base` variable. Its clause is filtered out of
        // the visited heritage clauses below either way
        // (`transformHeritageClause`).
        if let Some(base) = self.non_entity_base_type_node(class) {
            self.track_inferred_type(base);
        }
        for clause in class.heritage_clauses {
            self.visit_opt(clause.node_id);
        }
        self.fallback.pop();
        self.enclosing = previous_enclosing;
    }

    /// `getEffectiveBaseTypeNode` (`transform.go:1997`) when its expression is
    /// neither an entity-name expression nor `null`: the first type of the
    /// `extends` clause.
    fn non_entity_base_type_node(&self, class: &tsr_ast::ClassDeclaration<'a>) -> Option<NodeId> {
        let clause =
            class.heritage_clauses.iter().find(|clause| clause.token.kind == K::ExtendsKeyword)?;
        let base = clause.types.first()?;
        let expression = base.expression.and_then(|e| e.node_id());
        if self.is_entity_name_expression(expression)
            || expression.is_none_or(|e| self.kind(e) == K::NullKeyword)
        {
            return None;
        }
        base.node_id
    }

    /// `transformImportDeclaration` (`transform.go:2471`), for the one error
    /// it raises: a named-imports declaration none of whose bindings is
    /// visible is kept bare only when an augmentation needs it, which
    /// `isolatedDeclarations` cannot express. A side-effect import, a
    /// default-only import and a namespace import return before that arm
    /// (`:2472`, `:2491`, `:2509`), as upstream's do.
    ///
    /// Upstream raises this on the *first* transform of the statement; a
    /// later late-painted re-transform finds a visible binding and returns
    /// earlier, but the diagnostic already added stays. Visiting the
    /// statement once in order reproduces that.
    fn transform_import_declaration(&mut self, id: NodeId) {
        let Some(Node::ImportDeclaration(import)) = self.map.get(id) else { return };
        let Some(clause) = import.import_clause else { return };
        let Some(tsr_ast::NamedImportBindings::NamedImports(named)) = clause.named_bindings else {
            return;
        };
        if clause.name.is_some()
            && let Some(clause_id) = clause.node_id
            && self.resolver.is_declaration_visible(clause_id)
        {
            return;
        }
        let elements: Vec<NodeId> = named.elements.iter().filter_map(|e| e.node_id).collect();
        if elements.into_iter().any(|element| self.resolver.is_declaration_visible(element)) {
            return;
        }
        if self.resolver.is_import_required_by_augmentation(id)
            && self.options.isolated_declarations
        {
            self.out.push(Diagnostic::new(
                &m::DECLARATION_EMIT_FOR_THIS_FILE_REQUIRES_PRESERVING_THIS_IMPORT_FOR_AUGMENTATIONS_THIS_IS_NOT_SUPPORTED_WITH_ISOLATEDDECLARATIONS,
                self.nodes.span(id),
            ));
        }
    }

    /// `transformImportEqualsDeclaration` (`transform.go:2448`).
    fn transform_import_equals_declaration(&mut self, id: NodeId) {
        if !self.resolver.is_declaration_visible(id) {
            return;
        }
        let Some(Node::ImportEqualsDeclaration(import)) = self.map.get(id) else { return };
        let Some(reference) = import.module_reference else { return };
        if matches!(reference, tsr_ast::ModuleReference::ExternalModuleReference(_)) {
            return;
        }
        let previous = self.context.replace(Context::ForNode(id));
        if let Some(name) = reference.node_id() {
            self.check_entity_name_visibility(name);
        }
        self.context = previous;
    }

    /// `visitDeclarationSubtree` (`transform.go:573`).
    fn visit_declaration_subtree(&mut self, id: NodeId) {
        let kind = self.kind(id);
        if self.is_declaration_and_not_visible(id) {
            return;
        }
        // `ast.HasDynamicName`: upstream keeps a late-bound entity-name key
        // and then runs `checkName` on it. Declined as a whole member.
        if self.has_dynamic_name(id) {
            return;
        }
        if self.resolver.is_implementation_of_overload(id) {
            return;
        }
        if kind == K::SemicolonClassElement {
            return;
        }
        if let Some(Node::HeritageClause(clause)) = self.map.get(id)
            && (clause.types.is_empty()
                || (clause.types.len() == 1 && clause.types[0].node_id.is_none()))
        {
            return;
        }
        let previous_enclosing = self.enclosing;
        if is_enclosing_declaration(kind) {
            self.enclosing = id;
        }
        // `setupDiagnosticContext` (`transform.go:551`).
        let previous_context = self.context;
        let previous_suppress = self.suppress;
        if can_produce_diagnostics(kind) && !self.suppress {
            self.context = Some(Context::ForNode(id));
        }
        if matches!(kind, K::TypeLiteral | K::MappedType)
            && self.parent(id).is_none_or(|parent| self.kind(parent) != K::TypeAliasDeclaration)
        {
            self.suppress = true;
        }

        self.transform_subtree(id);

        self.enclosing = previous_enclosing;
        self.context = previous_context;
        self.suppress = previous_suppress;
    }

    /// The per-kind arms of `visitDeclarationSubtree`.
    fn transform_subtree(&mut self, id: NodeId) {
        let Some(node) = self.map.get(id) else { return };
        match node {
            Node::MappedTypeNode(mapped) => {
                self.visit_opt(mapped.type_parameter.and_then(|t| t.node_id));
                self.visit_opt(mapped.name_type.and_then(|t| t.node_id()));
                self.visit_opt(mapped.r#type.and_then(|t| t.node_id()));
            }
            Node::HeritageClause(clause) => {
                // `transformHeritageClause` (`transform.go:737`).
                let is_extends = clause.token.kind == K::ExtendsKeyword;
                let retained: Vec<NodeId> = clause
                    .types
                    .iter()
                    .filter(|t| {
                        t.expression.is_some_and(|e| {
                            self.is_entity_name_expression(e.node_id())
                                || (is_extends
                                    && e.node_id().is_some_and(|e| self.kind(e) == K::NullKeyword))
                        })
                    })
                    .filter_map(|t| t.node_id)
                    .collect();
                for t in retained {
                    self.visit(t);
                }
            }
            Node::MethodSignatureDeclaration(method) => {
                if has_modifier(method.modifiers, K::PrivateKeyword)
                    || matches!(method.name, PropertyName::PrivateIdentifier(_))
                {
                    return;
                }
                self.ensure_type_params(id, method.type_parameters);
                self.update_param_list(id, method.parameters);
                self.ensure_type(id, false);
            }
            Node::MethodDeclaration(method) => {
                if has_modifier(method.modifiers, K::PrivateKeyword)
                    || matches!(method.name, PropertyName::PrivateIdentifier(_))
                {
                    return;
                }
                self.ensure_type_params(id, method.type_parameters);
                self.update_param_list(id, method.parameters);
                self.ensure_type(id, false);
            }
            Node::ConstructSignatureDeclaration(signature) => {
                self.ensure_type_params(id, signature.type_parameters);
                self.update_param_list(id, signature.parameters);
                self.ensure_type(id, false);
            }
            Node::CallSignatureDeclaration(signature) => {
                self.ensure_type_params(id, signature.type_parameters);
                self.update_param_list(id, signature.parameters);
                self.ensure_type(id, false);
            }
            Node::ConstructorDeclaration(ctor) => self.update_param_list(id, ctor.parameters),
            Node::GetAccessorDeclaration(accessor) => {
                if matches!(accessor.name, PropertyName::PrivateIdentifier(_)) {
                    return;
                }
                let private = has_modifier(accessor.modifiers, K::PrivateKeyword);
                self.update_accessor_param_list(accessor.parameters, private, false);
                self.ensure_type(id, false);
            }
            Node::SetAccessorDeclaration(accessor) => {
                if matches!(accessor.name, PropertyName::PrivateIdentifier(_)) {
                    return;
                }
                let private = has_modifier(accessor.modifiers, K::PrivateKeyword);
                self.update_accessor_param_list(accessor.parameters, private, true);
            }
            Node::PropertyDeclaration(property) => {
                if matches!(property.name, PropertyName::PrivateIdentifier(_)) {
                    return;
                }
                self.ensure_type(id, false);
            }
            Node::PropertySignatureDeclaration(property) => {
                if matches!(property.name, PropertyName::PrivateIdentifier(_)) {
                    return;
                }
                self.ensure_type(id, false);
            }
            Node::IndexSignatureDeclaration(signature) => {
                // `transformIndexSignatureDeclaration` (`transform.go:941`).
                self.visit_opt(signature.r#type.and_then(|t| t.node_id()));
                self.update_param_list(id, signature.parameters);
            }
            Node::VariableDeclaration(declaration) => {
                // `transformVariableDeclaration` (`transform.go:835`). The
                // `recreateBindingPattern` arm (a pattern with initializers)
                // is declined.
                if let Some(BindingName::BindingPattern(_)) = declaration.name {
                    return;
                }
                self.suppress = true;
                self.ensure_type(id, false);
            }
            Node::TypeParameterDeclaration(parameter) => {
                // `transformTypeParameterDeclaration` (`transform.go:821`).
                let private_method = self.parent(id).is_some_and(|parent| {
                    matches!(self.map.get(parent), Some(Node::MethodDeclaration(method))
                        if has_modifier(method.modifiers, K::PrivateKeyword))
                });
                if private_method
                    && (parameter.default_type.is_some() || parameter.constraint.is_some())
                {
                    return;
                }
                self.visit_opt(parameter.constraint.and_then(|t| t.node_id()));
                self.visit_opt(parameter.default_type.and_then(|t| t.node_id()));
            }
            Node::ExpressionWithTypeArguments(expression) => {
                // `transformExpressionWithTypeArguments` (`transform.go:814`).
                if let Some(e) = expression.expression.and_then(|e| e.node_id())
                    && self.is_entity_name_expression(Some(e))
                {
                    self.check_entity_name_visibility(e);
                }
                for argument in expression.type_arguments {
                    self.visit_opt(argument.node_id());
                }
            }
            Node::TypeReferenceNode(reference) => {
                // `transformTypeReference` (`transform.go:809`).
                if let Some(name) = reference.type_name.and_then(|n| n.node_id()) {
                    self.check_entity_name_visibility(name);
                }
                for argument in reference.type_arguments {
                    self.visit_opt(argument.node_id());
                }
            }
            Node::ConditionalTypeNode(conditional) => {
                // `transformConditionalTypeNode` (`transform.go:791`).
                self.visit_opt(conditional.check_type.and_then(|t| t.node_id()));
                self.visit_opt(conditional.extends_type.and_then(|t| t.node_id()));
                let previous = self.enclosing;
                if let Some(true_type) = conditional.true_type.and_then(|t| t.node_id()) {
                    self.enclosing = true_type;
                    self.visit(true_type);
                }
                self.enclosing = previous;
                self.visit_opt(conditional.false_type.and_then(|t| t.node_id()));
            }
            Node::FunctionTypeNode(function) => {
                self.visit_type_parameters(function.type_parameters);
                self.update_param_list(id, function.parameters);
                self.visit_opt(function.r#type.and_then(|t| t.node_id()));
            }
            Node::ConstructorTypeNode(function) => {
                self.visit_type_parameters(function.type_parameters);
                self.update_param_list(id, function.parameters);
                self.visit_opt(function.r#type.and_then(|t| t.node_id()));
            }
            Node::TypeQueryNode(query) => {
                if let Some(name) = query.expr_name.and_then(|n| n.node_id()) {
                    self.check_entity_name_visibility(name);
                }
                for argument in query.type_arguments {
                    self.visit_opt(argument.node_id());
                }
            }
            // `transformImportTypeNode` checks no name; its type arguments
            // are visited.
            Node::ImportTypeNode(import) => {
                for argument in import.type_arguments {
                    self.visit_opt(argument.node_id());
                }
            }
            // JSDoc type nodes only occur in JavaScript, which is not walked.
            _ => {
                let mut children = Vec::new();
                tsr_ast::for_each_child_id(node, |child| children.push(child));
                for child in children {
                    self.visit(child);
                }
            }
        }
    }

    fn visit_opt(&mut self, id: Option<NodeId>) {
        if let Some(id) = id {
            self.visit(id);
        }
    }

    fn visit_type_parameters(&mut self, parameters: &[&tsr_ast::TypeParameterDeclaration<'a>]) {
        for parameter in parameters {
            self.visit_opt(parameter.node_id);
        }
    }

    /// `ensureTypeParams` (`transform.go:2350`): the written list, unless
    /// the owner is private. An absent list with a JSDoc full signature is
    /// JavaScript-only.
    fn ensure_type_params(
        &mut self,
        owner: NodeId,
        parameters: &[&tsr_ast::TypeParameterDeclaration<'a>],
    ) {
        if self.is_private(owner) {
            return;
        }
        self.visit_type_parameters(parameters);
    }

    /// `updateParamList` (`transform.go:2384`).
    fn update_param_list(
        &mut self,
        owner: NodeId,
        parameters: &[&tsr_ast::ParameterDeclaration<'a>],
    ) {
        if self.is_private(owner) {
            return;
        }
        for parameter in parameters {
            if let Some(id) = parameter.node_id {
                self.ensure_parameter(id);
            }
        }
    }

    /// `updateAccessorParamList` (`transform.go:1031`). A synthesized `value`
    /// parameter writes `any`, no name.
    fn update_accessor_param_list(
        &mut self,
        parameters: &[&tsr_ast::ParameterDeclaration<'a>],
        private: bool,
        setter: bool,
    ) {
        if private {
            return;
        }
        let this_parameter = parameters.first().filter(
            |p| matches!(p.name, Some(BindingName::Identifier(name)) if name.text == "this"),
        );
        if let Some(this) = this_parameter.and_then(|p| p.node_id) {
            self.ensure_parameter(this);
        }
        if setter {
            let value =
                if this_parameter.is_some() { parameters.get(1) } else { parameters.first() };
            if let Some(value) = value.and_then(|p| p.node_id) {
                self.ensure_parameter(value);
            }
        }
    }

    /// `ensureParameter` (`transform.go:2395`). `visitBindingName`'s
    /// computed binding-element keys are declined.
    fn ensure_parameter(&mut self, id: NodeId) {
        let previous = self.context;
        if !self.suppress {
            self.context = Some(Context::ForNode(id));
        }
        self.ensure_type(id, true);
        self.context = previous;
    }

    /// `ensureType` (`transform.go:1629`), the written-annotation arm only.
    fn ensure_type(&mut self, id: NodeId, ignore_private: bool) {
        if !ignore_private && self.is_private(id) {
            return;
        }
        let Some(node) = self.map.get(id) else { return };
        let (written, parameter_needs_undefined) = match node {
            Node::ParameterDeclaration(parameter) => (
                parameter.r#type,
                // `RequiresAddingImplicitUndefined` sends an initialized
                // parameter, or an optional uninitialized parameter
                // property, to the node builder: declined whenever it could.
                parameter.initializer.is_some()
                    || (parameter.question_token.is_some()
                        && has_parameter_property_modifier(parameter.modifiers)),
            ),
            Node::VariableDeclaration(declaration) => (declaration.r#type, false),
            Node::PropertyDeclaration(property) => (property.r#type, false),
            Node::PropertySignatureDeclaration(property) => (property.r#type, false),
            Node::FunctionDeclaration(function) => (function.r#type, false),
            Node::MethodDeclaration(method) => (method.r#type, false),
            Node::MethodSignatureDeclaration(method) => (method.r#type, false),
            Node::CallSignatureDeclaration(signature) => (signature.r#type, false),
            Node::ConstructSignatureDeclaration(signature) => (signature.r#type, false),
            Node::GetAccessorDeclaration(accessor) => (accessor.r#type, false),
            _ => (None, false),
        };
        // An annotated declaration's type is never a fresh literal, so
        // `shouldPrintWithInitializer` does not apply to it.
        if let Some(written) = written.and_then(|t| t.node_id())
            && !parameter_needs_undefined
        {
            self.visit(written);
        }
        // An absent annotation is `CreateTypeOfDeclaration` (`:1667`), with
        // `errorNameNode` the declaration's name. Only a variable
        // declaration's is asked (`inferred_type_reports`);
        // `CreateReturnTypeOfSignatureDeclaration` and the other declaration
        // kinds are declined.
        if written.is_none() && matches!(node, Node::VariableDeclaration(_)) {
            let previous_name = self.error_name_node;
            self.error_name_node = self.name_of_declaration(id);
            self.track_inferred_type(id);
            self.error_name_node = previous_name;
        }
    }

    /// `checkEntityNameVisibility` (`transform.go:1697`) with
    /// `handleSymbolAccessibilityError` (`tracker.go:181`).
    fn check_entity_name_visibility(&mut self, entity_name: NodeId) {
        match self.resolver.is_entity_name_visible(entity_name, self.enclosing) {
            EntityNameVisibility::Accessible(aliases) => {
                for alias in aliases {
                    if !self.late_marked.contains(&alias) {
                        self.late_marked.push(alias);
                    }
                }
            }
            EntityNameVisibility::NotResolved => {}
            EntityNameVisibility::NotAccessible { symbol_name, error_node } => {
                let Some(Context::ForNode(node)) = self.context else { return };
                let Some((message, type_name)) = self.diagnostic_for_node(node) else { return };
                let span = self.nodes.span(error_node);
                let mut args = Vec::with_capacity(3);
                if let Some(type_name) = type_name {
                    args.push(self.text_of(type_name));
                }
                args.push(symbol_name);
                // `ErrorModuleName`, empty from `IsEntityNameVisible`.
                args.push(String::new());
                self.out.push(Diagnostic::with_args(message, span, args));
            }
        }
    }

    fn text_of(&self, id: NodeId) -> String {
        let span = self.nodes.span(id);
        self.text.get(span.start as usize..span.end as usize).unwrap_or_default().to_string()
    }

    /// `ast.IsEntityNameExpression`.
    fn is_entity_name_expression(&self, id: Option<NodeId>) -> bool {
        let Some(id) = id else { return false };
        match self.map.get(id) {
            Some(Node::Identifier(_)) => true,
            Some(Node::PropertyAccessExpression(access)) => {
                matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                    && self.is_entity_name_expression(access.expression.and_then(|e| e.node_id()))
            }
            _ => false,
        }
    }

    /// `ast.HasDynamicName`: a computed name whose expression is not a
    /// string, numeric or no-substitution template literal.
    fn has_dynamic_name(&self, id: NodeId) -> bool {
        let name = match self.map.get(id) {
            Some(Node::PropertyDeclaration(n)) => Some(n.name),
            Some(Node::PropertySignatureDeclaration(n)) => Some(n.name),
            Some(Node::MethodDeclaration(n)) => Some(n.name),
            Some(Node::MethodSignatureDeclaration(n)) => Some(n.name),
            Some(Node::GetAccessorDeclaration(n)) => Some(n.name),
            Some(Node::SetAccessorDeclaration(n)) => Some(n.name),
            _ => None,
        };
        let Some(PropertyName::ComputedPropertyName(computed)) = name else { return false };
        !computed.expression.and_then(|e| e.node_id()).is_some_and(|e| {
            matches!(
                self.kind(e),
                K::StringLiteral | K::NumericLiteral | K::NoSubstitutionTemplateLiteral
            )
        })
    }

    /// `host.GetEffectiveDeclarationFlags(node, ModifierFlagsPrivate)`.
    fn is_private(&self, id: NodeId) -> bool {
        modifiers_of(self.map.get(id))
            .is_some_and(|modifiers| has_modifier(modifiers, K::PrivateKeyword))
    }

    fn is_static(&self, id: NodeId) -> bool {
        modifiers_of(self.map.get(id))
            .is_some_and(|modifiers| has_modifier(modifiers, K::StaticKeyword))
    }

    /// `ast.GetNameOfDeclaration` for the declarations a context is made
    /// for.
    fn name_of_declaration(&self, id: NodeId) -> Option<NodeId> {
        match self.map.get(id)? {
            Node::VariableDeclaration(n) => n.name.and_then(|n| n.node_id()),
            Node::ParameterDeclaration(n) => n.name.and_then(|n| n.node_id()),
            Node::PropertyDeclaration(n) => n.name.node_id(),
            Node::PropertySignatureDeclaration(n) => n.name.node_id(),
            Node::MethodDeclaration(n) => n.name.node_id(),
            Node::MethodSignatureDeclaration(n) => n.name.node_id(),
            Node::GetAccessorDeclaration(n) => n.name.node_id(),
            Node::SetAccessorDeclaration(n) => n.name.node_id(),
            Node::FunctionDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::ClassDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::InterfaceDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::TypeAliasDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::TypeParameterDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::ImportEqualsDeclaration(n) => n.name.and_then(|n| n.node_id),
            _ => None,
        }
    }

    /// `createGetSymbolAccessibilityDiagnosticForNode` (`diagnostics.go:136`)
    /// applied to a result without `ErrorModuleName` — the only kind
    /// `IsEntityNameVisible` produces — so each `selectDiagnosticBasedOn…`
    /// picks its private-name message. Returns the message and `typeName`;
    /// the error node is the result's own.
    fn diagnostic_for_node(&self, node: NodeId) -> Option<(&'static Message, Option<NodeId>)> {
        let kind = self.kind(node);
        let parent_kind = self.parent(node).map(|p| self.kind(p));
        let simple = |message: Option<&'static Message>| {
            message.map(|m| (m, self.name_of_declaration(node)))
        };
        match kind {
            K::VariableDeclaration
            | K::PropertyDeclaration
            | K::PropertySignature
            | K::BindingElement
            | K::Constructor => simple(self.variable_declaration_message(node)),
            K::SetAccessor | K::GetAccessor => {
                // `wrapNamedDiagnosticSelector`.
                let static_ = self.is_static(node);
                let message = match (kind, static_) {
                    (K::SetAccessor, true) => &m::PARAMETER_TYPE_OF_PUBLIC_STATIC_SETTER_0_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                    (K::SetAccessor, false) => &m::PARAMETER_TYPE_OF_PUBLIC_SETTER_0_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                    (_, true) => &m::RETURN_TYPE_OF_PUBLIC_STATIC_GETTER_0_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                    (_, false) => &m::RETURN_TYPE_OF_PUBLIC_GETTER_0_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                };
                Some((message, self.name_of_declaration(node)))
            }
            K::ConstructSignature
            | K::CallSignature
            | K::MethodDeclaration
            | K::MethodSignature
            | K::FunctionDeclaration
            | K::IndexSignature => {
                // `wrapFallbackErrorDiagnosticSelector`: no `typeName`.
                let message = match kind {
                    K::ConstructSignature => &m::RETURN_TYPE_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    K::CallSignature => &m::RETURN_TYPE_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    K::IndexSignature => &m::RETURN_TYPE_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    K::FunctionDeclaration => &m::RETURN_TYPE_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    _ if self.is_static(node) => &m::RETURN_TYPE_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    _ if parent_kind == Some(K::ClassDeclaration) => &m::RETURN_TYPE_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_0,
                    _ => &m::RETURN_TYPE_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_0,
                };
                Some((message, None))
            }
            K::Parameter => {
                let owner = self.parent(node)?;
                if has_parameter_property_modifier(modifiers_of(self.map.get(node)).unwrap_or(&[]))
                    && self.kind(owner) == K::Constructor
                    && self.is_private(owner)
                {
                    return simple(self.variable_declaration_message(node));
                }
                simple(self.parameter_message(node, owner))
            }
            K::TypeParameter => simple(self.type_parameter_message(node)),
            K::ExpressionWithTypeArguments => {
                let clause = self.parent(node)?;
                let owner = self.parent(clause)?;
                let message = if self.kind(owner) == K::ClassDeclaration {
                    if matches!(self.map.get(clause), Some(Node::HeritageClause(c)) if c.token.kind == K::ImplementsKeyword)
                    {
                        &m::IMPLEMENTS_CLAUSE_OF_EXPORTED_CLASS_0_HAS_OR_IS_USING_PRIVATE_NAME_1
                    } else if self.name_of_declaration(owner).is_some() {
                        &m::EXTENDS_CLAUSE_OF_EXPORTED_CLASS_0_HAS_OR_IS_USING_PRIVATE_NAME_1
                    } else {
                        &m::EXTENDS_CLAUSE_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_0
                    }
                } else {
                    &m::EXTENDS_CLAUSE_OF_EXPORTED_INTERFACE_0_HAS_OR_IS_USING_PRIVATE_NAME_1
                };
                Some((message, self.name_of_declaration(owner)))
            }
            K::ImportEqualsDeclaration => {
                simple(Some(&m::IMPORT_DECLARATION_0_IS_USING_PRIVATE_NAME_1))
            }
            K::TypeAliasDeclaration => Some((
                &m::EXPORTED_TYPE_ALIAS_0_HAS_OR_IS_USING_PRIVATE_NAME_1,
                self.name_of_declaration(node),
            )),
            _ => None,
        }
    }

    /// `getVariableDeclarationTypeVisibilityDiagnosticMessage`
    /// (`diagnostics.go:226`), private-name arm.
    fn variable_declaration_message(&self, node: NodeId) -> Option<&'static Message> {
        let kind = self.kind(node);
        if matches!(kind, K::VariableDeclaration | K::BindingElement) {
            return Some(&m::EXPORTED_VARIABLE_0_HAS_OR_IS_USING_PRIVATE_NAME_1);
        }
        let private_parameter =
            kind == K::Parameter && self.parent(node).is_some_and(|owner| self.is_private(owner));
        if matches!(kind, K::PropertyDeclaration | K::PropertySignature) || private_parameter {
            if self.is_static(node) {
                return Some(
                    &m::PUBLIC_STATIC_PROPERTY_0_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                );
            }
            if self.parent(node).is_some_and(|p| self.kind(p) == K::ClassDeclaration)
                || kind == K::Parameter
            {
                return Some(
                    &m::PUBLIC_PROPERTY_0_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
                );
            }
            return Some(&m::PROPERTY_0_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1);
        }
        None
    }

    /// `getParameterDeclarationTypeVisibilityDiagnosticMessage`
    /// (`diagnostics.go:356`), private-name arm.
    fn parameter_message(&self, node: NodeId, owner: NodeId) -> Option<&'static Message> {
        let _ = node;
        Some(match self.kind(owner) {
            K::Constructor => &m::PARAMETER_0_OF_CONSTRUCTOR_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::ConstructSignature | K::ConstructorType => &m::PARAMETER_0_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::CallSignature => &m::PARAMETER_0_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::IndexSignature => &m::PARAMETER_0_OF_INDEX_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::MethodDeclaration | K::MethodSignature => {
                if self.is_static(owner) {
                    &m::PARAMETER_0_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1
                } else if self.parent(owner).is_some_and(|p| self.kind(p) == K::ClassDeclaration) {
                    &m::PARAMETER_0_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1
                } else {
                    &m::PARAMETER_0_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1
                }
            }
            K::FunctionDeclaration | K::FunctionType | K::ArrowFunction | K::FunctionExpression => {
                &m::PARAMETER_0_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME_1
            }
            K::SetAccessor | K::GetAccessor => &m::PARAMETER_0_OF_ACCESSOR_HAS_OR_IS_USING_PRIVATE_NAME_1,
            _ => return None,
        })
    }

    /// `getTypeParameterConstraintVisibilityDiagnosticMessage`
    /// (`diagnostics.go:441`).
    fn type_parameter_message(&self, node: NodeId) -> Option<&'static Message> {
        let owner = self.parent(node)?;
        Some(match self.kind(owner) {
            K::ClassDeclaration => &m::TYPE_PARAMETER_0_OF_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::InterfaceDeclaration => &m::TYPE_PARAMETER_0_OF_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::MappedType => &m::TYPE_PARAMETER_0_OF_EXPORTED_MAPPED_OBJECT_TYPE_IS_USING_PRIVATE_NAME_1,
            K::ConstructorType | K::ConstructSignature => &m::TYPE_PARAMETER_0_OF_CONSTRUCTOR_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::CallSignature => &m::TYPE_PARAMETER_0_OF_CALL_SIGNATURE_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::MethodDeclaration | K::MethodSignature => {
                if self.is_static(owner) {
                    &m::TYPE_PARAMETER_0_OF_PUBLIC_STATIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1
                } else if self.parent(owner).is_some_and(|p| self.kind(p) == K::ClassDeclaration) {
                    &m::TYPE_PARAMETER_0_OF_PUBLIC_METHOD_FROM_EXPORTED_CLASS_HAS_OR_IS_USING_PRIVATE_NAME_1
                } else {
                    &m::TYPE_PARAMETER_0_OF_METHOD_FROM_EXPORTED_INTERFACE_HAS_OR_IS_USING_PRIVATE_NAME_1
                }
            }
            K::FunctionType | K::FunctionDeclaration => &m::TYPE_PARAMETER_0_OF_EXPORTED_FUNCTION_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::InferType => &m::EXTENDS_CLAUSE_FOR_INFERRED_TYPE_0_HAS_OR_IS_USING_PRIVATE_NAME_1,
            K::TypeAliasDeclaration => &m::TYPE_PARAMETER_0_OF_EXPORTED_TYPE_ALIAS_HAS_OR_IS_USING_PRIVATE_NAME_1,
            _ => return None,
        })
    }
}

/// `isEnclosingDeclaration` (`declarations/util.go:109`).
fn is_enclosing_declaration(kind: K) -> bool {
    matches!(
        kind,
        K::SourceFile
            | K::TypeAliasDeclaration
            | K::ModuleDeclaration
            | K::ClassDeclaration
            | K::InterfaceDeclaration
            | K::IndexSignature
            | K::MappedType
            | K::VariableDeclaration
            // `ast.IsFunctionLike`.
            | K::FunctionDeclaration
            | K::MethodDeclaration
            | K::Constructor
            | K::GetAccessor
            | K::SetAccessor
            | K::FunctionExpression
            | K::ArrowFunction
            | K::MethodSignature
            | K::CallSignature
            | K::ConstructSignature
            | K::FunctionType
            | K::ConstructorType
    )
}

/// `canProduceDiagnostics` (`declarations/util.go:25`).
fn can_produce_diagnostics(kind: K) -> bool {
    matches!(
        kind,
        K::VariableDeclaration
            | K::PropertyDeclaration
            | K::PropertySignature
            | K::BindingElement
            | K::SetAccessor
            | K::GetAccessor
            | K::ConstructSignature
            | K::CallSignature
            | K::MethodDeclaration
            | K::MethodSignature
            | K::FunctionDeclaration
            | K::Parameter
            | K::TypeParameter
            | K::ExpressionWithTypeArguments
            | K::ImportEqualsDeclaration
            | K::TypeAliasDeclaration
            | K::Constructor
            | K::IndexSignature
            | K::PropertyAccessExpression
            | K::ElementAccessExpression
            | K::BinaryExpression
            | K::CallExpression
    )
}

fn has_modifier(modifiers: &[ModifierLike<'_>], kind: K) -> bool {
    modifiers.iter().any(|m| matches!(m, ModifierLike::Token(token) if token.kind == kind))
}

/// `ModifierFlagsParameterPropertyModifier`: an accessibility modifier,
/// `readonly` or `override`.
fn has_parameter_property_modifier(modifiers: &[ModifierLike<'_>]) -> bool {
    modifiers.iter().any(|m| {
        matches!(m, ModifierLike::Token(token) if matches!(
            token.kind,
            K::PublicKeyword | K::PrivateKeyword | K::ProtectedKeyword | K::ReadonlyKeyword | K::OverrideKeyword
        ))
    })
}

fn modifiers_of(node: Option<Node<'_>>) -> Option<&[ModifierLike<'_>]> {
    Some(match node? {
        Node::ParameterDeclaration(n) => n.modifiers,
        Node::PropertyDeclaration(n) => n.modifiers,
        Node::PropertySignatureDeclaration(n) => n.modifiers,
        Node::MethodDeclaration(n) => n.modifiers,
        Node::MethodSignatureDeclaration(n) => n.modifiers,
        Node::ConstructorDeclaration(n) => n.modifiers,
        Node::GetAccessorDeclaration(n) => n.modifiers,
        Node::SetAccessorDeclaration(n) => n.modifiers,
        Node::IndexSignatureDeclaration(n) => n.modifiers,
        Node::FunctionDeclaration(n) => n.modifiers,
        Node::ClassDeclaration(n) => n.modifiers,
        _ => return None,
    })
}

fn type_element_id(member: &TypeElement<'_>) -> Option<NodeId> {
    member.node_id()
}

fn class_element_id(member: &ClassElement<'_>) -> Option<NodeId> {
    member.node_id()
}
