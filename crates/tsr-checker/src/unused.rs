//! `checkUnusedIdentifiers` (`checker.go:7046`) — the `noUnusedLocals` /
//! `noUnusedParameters` family.
//!
//! Seven codes: TS6133, TS6138, TS6192, TS6196, TS6198, TS6199, TS6205. See
//! `docs/architecture/checker-notes-diag2.md` §15 for the bar this was built
//! against and for why the reference-marking pass is deliberately an
//! over-approximation.
//!
//! # The one prerequisite, and why it is a pass of its own
//!
//! Upstream's `isReferenced` (`checker.go:7073`) reads
//! `symbolReferenceLinks[symbol].referenceKinds`, and that field is written from
//! exactly one place: the `SymbolReferenced` callback the checker hands its
//! `binder.NameResolver` (`checker.go:1499`). Every `resolveName` performed
//! during a type check marks the symbol it found with the meaning it was asked
//! for. Reference marking is therefore free upstream — a side effect of checking.
//!
//! This port's [`crate::check`] traversal computes no types, so there is no
//! resolution stream to hang the callback on.
//! [`Checker::mark_identifier_reference`] is that stream, rebuilt: every
//! identifier the walk passes that is not provably a *declaration* name or a
//! *member-access* name is resolved under each of the three meanings, and each
//! meaning that hits is recorded.
//!
//! **The approximation is one-sided on purpose.** A symbol marked that upstream
//! would not mark costs a missing diagnostic. A symbol left unmarked costs a
//! *wrong* one — which fails its own case and, under the `diagnostics` suite's
//! exact-multiset rule, can break a case that passes today. Everything ambiguous
//! is therefore resolved towards *marked*.

use rustc_hash::FxHashSet;
use tsr_ast::{ClassElement, ModifierLike, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `AccessKind` (`ast.go:1499`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    /// Only reads the value.
    Read,
    /// Only writes it.
    Write,
    /// Both — a compound assignment or `++`.
    ReadWrite,
}

impl AccessKind {
    /// `reverseAccessKind` (`ast.go:1487`).
    fn reversed(self) -> Self {
        match self {
            Self::Read => Self::Write,
            Self::Write => Self::Read,
            Self::ReadWrite => Self::ReadWrite,
        }
    }
}

/// `IsAssignmentOperator` — `=` through `??=`.
fn is_assignment_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EqualsToken
            | SyntaxKind::PlusEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::AsteriskEqualsToken
            | SyntaxKind::AsteriskAsteriskEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::LessThanLessThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanEqualsToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::BarEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::BarBarEqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken
    )
}

/// `UnusedKind` (`checker.go:7077`) — which option decides whether the
/// diagnostic is an error or a suggestion.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UnusedKind {
    /// `noUnusedLocals`.
    Local,
    /// `noUnusedParameters`.
    Parameter,
}

impl Checker<'_, '_> {
    /// Is the unused check switched on at all?
    ///
    /// `getDiagnostics`'s `checkUnused` (`checker.go:13961`). This port has no
    /// suggestion channel, so a case that sets neither option produces nothing
    /// from this module — which is what confines the whole family's blast radius
    /// to the 302 corpus cases that write one of the two directives.
    pub(crate) fn unused_check_enabled(&self) -> bool {
        self.no_unused_locals || self.no_unused_parameters
    }

    /// `registerForUnusedIdentifiersCheck` (`checker.go:7040`), decided by kind
    /// rather than by which `checkXxx` was reached.
    ///
    /// Upstream registers from inside eleven different `checkXxx` functions, and
    /// each carries a guard — `checkBlock` registers only a block with locals,
    /// `checkModuleDeclaration` skips a global augmentation,
    /// `checkSignatureDeclaration` skips an index signature. The guards are
    /// reproduced here; the *call sites* are not, because this port's walk is
    /// one generic child recursion (see [`crate::check::Checker::check_node`]'s
    /// own note) and has no per-kind `checkXxx` to hang them off.
    pub(crate) fn register_for_unused_check(&mut self, node: NodeId) {
        if !self.unused_check_enabled() {
            return;
        }
        let Some(typed) = self.node_map.get(node) else { return };
        let register = match typed {
            // `checkSourceFile` (`checker.go:2210`) registers the file **only
            // for an external or CommonJS module**, inside
            // `if ast.IsExternalOrCommonJSModule(sourceFile)`. A script's top
            // level is the global scope and its declarations are not locals, so
            // nothing there is ever unused.
            //
            // This was the first build's single largest wrong family: without
            // the guard the port reported every top-level `class`, `function`
            // and `namespace` of every script in the unused corpus — 108 of the
            // 158 wrong lines and all three losses.
            // `bindSourceFileAsExternalModule` gives a module file a symbol on
            // its `SourceFile` node and gives a script none, so the symbol is
            // the question.
            Node::SourceFile(_) => self.binder.symbol_of(node).is_some(),
            // `checkModuleDeclaration` (`checker.go:5134`): a body, and not
            // `declare global`.
            Node::ModuleDeclaration(declaration) => {
                declaration.body.is_some() && declaration.keyword.kind != SyntaxKind::GlobalKeyword
            }
            // `checkBlock` (`checker.go:3798`) — `len(node.Locals()) != 0`.
            Node::Block(_) | Node::ModuleBlock(_) | Node::CaseBlock(_) => {
                self.binder.locals(node).is_some_and(|l| !l.is_empty())
            }
            Node::ForStatement(_) | Node::ForInOrOfStatement(_) => {
                self.binder.locals(node).is_some()
            }
            Node::ClassDeclaration(_)
            | Node::ClassExpression(_)
            | Node::InterfaceDeclaration(_)
            | Node::TypeAliasDeclaration(_)
            | Node::ConstructorDeclaration(_)
            | Node::FunctionDeclaration(_)
            | Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::MethodDeclaration(_)
            | Node::MethodSignatureDeclaration(_)
            | Node::GetAccessorDeclaration(_)
            | Node::SetAccessorDeclaration(_)
            | Node::CallSignatureDeclaration(_)
            | Node::ConstructSignatureDeclaration(_)
            | Node::FunctionTypeNode(_)
            | Node::ConstructorTypeNode(_)
            | Node::InferTypeNode(_) => true,
            _ => false,
        };
        if register {
            self.unused_check_nodes.push(node);
        }
    }

    /// The reference-marking stream this port has to rebuild — see the module
    /// header.
    ///
    /// `meaning` is recorded rather than the *symbol's* flags, which is what
    /// `symbolReferenced` (`checker.go:1499`) does: `referenceKinds |= meaning`.
    /// The two differ where a symbol carries more meanings than the reference
    /// asked for, and the consumers test the requested one —
    /// `isUnreferencedVariableDeclaration` asks for `Variable`,
    /// `isUnreferencedTypeParameter` for `TypeParameter`.
    pub(crate) fn mark_identifier_reference(&mut self, node: NodeId, text: &str) {
        if !self.unused_check_enabled() || text.is_empty() {
            return;
        }
        if self.is_declaration_or_member_name(node) {
            return;
        }
        // `getResolvedSymbol` (`checker.go:13896`) passes
        // `isUse: !ast.IsWriteOnlyAccess(node)`, and `resolveNameHelper`
        // (`nameresolver.go:314`) marks **only when `isUse`**. So `y = 1` is not
        // a reference to `y`, which is why `unusedLocalsInMethod3` reads
        // *"All variables are unused"* upstream for `var x, y; y = 1;`.
        //
        // This is the one place the module marks *less* than the naive reading,
        // and it is here because upstream says so rather than because it pays.
        let mut meanings: Vec<SymbolFlags> = Vec::with_capacity(3);
        if !self.is_write_only_access(node) {
            meanings.push(SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE | SymbolFlags::ALIAS);
        }
        // `SymbolFlagsAlias` rides along with every meaning: an `import x from
        // "y"` symbol carries *only* `Alias`, and leaving it out would leave
        // every used import unmarked — the one direction this module may not
        // fail in.
        meanings.push(SymbolFlags::TYPE | SymbolFlags::ALIAS);
        meanings.push(SymbolFlags::NAMESPACE | SymbolFlags::ALIAS);
        for meaning in meanings {
            if let Some(symbol) =
                self.binder.resolve_name(self.nodes, self.node_map, node, text, meaning)
            {
                let symbol = self.binder.merged_symbol(symbol);
                // **A self-reference is not a use.** `resolveNameHelper` marks
                // only when the result is not the enclosing self-reference
                // location's own symbol (`nameresolver.go:314`), and
                // `isSelfReferenceLocation` (`:489`) is a node-kind list. So a
                // `function f() { f; }` leaves `f` unread. §329.
                if self.reference_is_inside_own_declaration(node, symbol) {
                    continue;
                }
                *self.symbol_reference_kinds.entry(symbol).or_default() |= meaning;
            }
        }
    }

    /// Is this reference inside one of the symbol's own declarations?
    ///
    /// `isSelfReferenceLocation`'s kind list (`nameresolver.go:489`), minus its
    /// `KindParameter` arm — that one needs `lastLocation == node.Name()`,
    /// which is a walk state this port does not keep. §329.
    fn reference_is_inside_own_declaration(&self, node: NodeId, symbol: SymbolId) -> bool {
        let declarations: Vec<NodeId> =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect();
        if declarations.is_empty() {
            return false;
        }
        self.nodes.ancestors(node).any(|ancestor| {
            declarations.contains(&ancestor)
                && matches!(
                    self.nodes.kind(ancestor),
                    tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::ClassDeclaration
                        | tsr_ast::SyntaxKind::InterfaceDeclaration
                        | tsr_ast::SyntaxKind::EnumDeclaration
                        | tsr_ast::SyntaxKind::TypeAliasDeclaration
                        | tsr_ast::SyntaxKind::ModuleDeclaration
                )
        })
    }

    /// Record a member name for the by-name marking private class members need.
    ///
    /// `markPropertyAsReferenced` (`checker.go:27706`) marks the *symbol* a
    /// property access resolved to, which needs the type of the receiver. This
    /// port's check traversal computes none, so the question asked instead is
    /// "does this text occur anywhere in the file as a member name" — coarser in
    /// the safe direction, exactly as the identifier pass is.
    pub(crate) fn note_member_name(&mut self, text: &str) {
        if self.unused_check_enabled() && !text.is_empty() {
            self.referenced_member_names.insert(text.to_string());
        }
    }

    /// Record any member name this node mentions.
    ///
    /// The three shapes that can reach a private class member without going
    /// through a scope: `a.x`, `a["x"]`, and `N.x`. Called for every node of the
    /// walk, so it is a filter rather than a dispatch.
    pub(crate) fn note_member_name_at(&mut self, node: NodeId) {
        if !self.unused_check_enabled() {
            return;
        }
        // **A write is not a read.** `isReferenced` (`checker.go:7123`) asks
        // whether a member is *read*, so `this.x = 1` leaves a private `x`
        // unused — the `noUnusedLocals_writeOnlyProperty` family. A compound
        // assignment reads as well as writes and keeps marking, which is
        // exactly what `is_write_only_access` already answers for the local
        // rule (§329). §383.
        if self.is_write_only_access(node) {
            return;
        }
        // **A private `static` member is reached by exactly one syntax**, so
        // its reference can be keyed by receiver without a type: `Class.member`
        // or, inside the class's own static members, `this.member`. Recorded
        // *alongside* the bare name, so the instance path is unchanged. §704.
        let qualified = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => access
                .name
                .and_then(|n| n.node_id())
                .and_then(|member| self.identifier_text_of(member).map(str::to_string))
                .zip(access.expression.and_then(|e| e.node_id())),
            // `Test5["m1"]()` reaches a static exactly as `Test5.m1()` does,
            // and dropping it made two wrong lines the first time this was
            // measured. §704.
            Some(Node::ElementAccessExpression(access)) => access
                .argument_expression
                .and_then(|argument| argument.node_id())
                .and_then(|argument| match self.node_map.get(argument) {
                    Some(Node::StringLiteral(literal)) => Some(literal.text.to_string()),
                    _ => None,
                })
                .zip(access.expression.and_then(|e| e.node_id())),
            _ => None,
        };
        if let Some((member_text, receiver)) = qualified {
            let receiver_text = match self.nodes.kind(receiver) {
                SyntaxKind::ThisKeyword => Some("this".to_string()),
                SyntaxKind::Identifier => self.identifier_text_of(receiver).map(str::to_string),
                _ => None,
            };
            // **A self-reference is not a use**, the same rule
            // [`Checker::reference_is_inside_own_declaration`] applies to
            // locals: `private static m1(n) { … Test4.m1(n - 1) … }` leaves
            // `m1` unread upstream. §704.
            if let Some(receiver_text) = receiver_text
                && !self.reference_is_inside_named_member(node, &member_text)
            {
                self.referenced_member_names.insert(format!("{receiver_text}.{member_text}"));
            }
        }
        let named = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => access.name.and_then(|n| n.node_id()),
            Some(Node::QualifiedName(name)) => name.right.and_then(|n| n.node_id),
            // An element access whose index is a **literal union** names every
            // property the union can reach, and upstream marks each of them
            // inside the access resolution (`checker.go:27033`). Asking the
            // argument for its type is the one type question this otherwise
            // syntactic pass needs; anything that is not a string literal or a
            // union of them keeps the old answer, which is the
            // missing-diagnostic direction (`checker-notes-diag2.md` §61).
            Some(Node::ElementAccessExpression(access)) => {
                if let Some(argument) = access.argument_expression {
                    let indexed = self.check_expression(argument);
                    for value in self.string_literal_values(indexed) {
                        self.note_member_name(&value);
                    }
                }
                access.argument_expression.and_then(|e| e.node_id())
            }
            // `({ x } = this)` and `({ x: y } = this)` both reach a member `x`
            // without a property access — `noUnusedLocals_destructuringAssignment`
            // is exactly that shape, and it was 3 of the first build's 13 wrong
            // lines.
            Some(Node::ShorthandPropertyAssignment(shorthand)) => shorthand.name.node_id(),
            Some(Node::PropertyAssignment(assignment)) => assignment.name.node_id(),
            Some(Node::BindingElement(element)) => element.property_name.and_then(|n| n.node_id()),
            _ => None,
        };
        let Some(named) = named else { return };
        let Some(text) = self.identifier_text_of(named) else { return };
        let text = text.to_string();
        self.note_member_name(&text);
    }

    /// Every string-literal value a type can be — the type itself, or each
    /// constituent of a union of them. Empty for anything else.
    fn string_literal_values(&self, id: crate::types::TypeId) -> Vec<String> {
        let value_of = |id: crate::types::TypeId| match &self.store.get(id).data {
            crate::types::TypeData::StringLiteral(value) => Some(value.clone()),
            _ => None,
        };
        if let Some(value) = value_of(id) {
            return vec![value];
        }
        match &self.store.get(id).data {
            crate::types::TypeData::Union { types, .. } => {
                types.iter().filter_map(|&constituent| value_of(constituent)).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Is this identifier a name being *declared*, or a member being named,
    /// rather than a reference?
    ///
    /// Deliberately a short list. Every entry left off costs a missed report;
    /// every entry wrongly present costs a wrong diagnostic, so the list holds
    /// only slots that are unambiguously one or the other. `{ a }` — a
    /// `ShorthandPropertyAssignment` — is absent on purpose: it is the one place
    /// a declaration name is also a reference.
    fn is_declaration_or_member_name(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(typed) = self.node_map.get(parent) else { return false };
        let is = |slot: Option<NodeId>| slot == Some(node);
        match typed {
            Node::PropertyAccessExpression(n) => is(n.name.and_then(|n| n.node_id())),
            Node::QualifiedName(n) => is(n.right.and_then(|n| n.node_id)),
            Node::PropertyAssignment(n) => is(n.name.node_id()),
            Node::PropertyDeclaration(n) => is(n.name.node_id()),
            Node::PropertySignatureDeclaration(n) => is(n.name.node_id()),
            Node::MethodDeclaration(n) => is(n.name.node_id()),
            Node::MethodSignatureDeclaration(n) => is(n.name.node_id()),
            Node::GetAccessorDeclaration(n) => is(n.name.node_id()),
            Node::SetAccessorDeclaration(n) => is(n.name.node_id()),
            Node::EnumMember(n) => is(n.name.node_id()),
            Node::VariableDeclaration(n) => is(n.name.and_then(|n| n.node_id())),
            Node::ParameterDeclaration(n) => is(n.name.and_then(|n| n.node_id())),
            Node::BindingElement(n) => {
                is(n.name.and_then(|n| n.node_id()))
                    || is(n.property_name.and_then(|n| n.node_id()))
            }
            Node::TypeParameterDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::FunctionDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::FunctionExpression(n) => is(n.name.and_then(|n| n.node_id)),
            Node::ClassDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::ClassExpression(n) => is(n.name.and_then(|n| n.node_id)),
            Node::InterfaceDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::TypeAliasDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::EnumDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::ModuleDeclaration(n) => is(n.name.and_then(|n| n.node_id())),
            Node::ImportClause(n) => is(n.name.and_then(|n| n.node_id)),
            Node::NamespaceImport(n) => is(n.name.and_then(|n| n.node_id)),
            // **A property name names an export of another module.** It is not
            // a reference to anything in this file, so marking it can only
            // resolve by accident — `import { Member } …; import { Member as M }
            // …` marks the first import used from the second's property name.
            // Upstream never asks: `checkUnusedIdentifiers` marks from
            // `getResolvedSymbol`, which an import specifier's property name
            // does not go through. §702.
            Node::ImportSpecifier(n) => {
                is(n.name.and_then(|n| n.node_id)) || is(n.property_name.and_then(|n| n.node_id()))
            }
            Node::ImportEqualsDeclaration(n) => is(n.name.and_then(|n| n.node_id)),
            Node::NamespaceExport(n) => is(n.name.and_then(|n| n.node_id())),
            // **§702 mirrored.** An import specifier's *property name* is the
            // foreign one; an export specifier's *name* is. `export { x as y }`
            // writes the local in `property_name` and the exported name in
            // `name`, and a bare `export { x }` writes the local in `name` — so
            // the name is skipped only when a property name is present. §726.
            Node::ExportSpecifier(n) => {
                n.property_name.is_some() && is(n.name.and_then(|n| n.node_id()))
            }
            // An attribute's name is never a variable.
            Node::JsxAttribute(n) => is(n.name.and_then(|n| n.node_id())),
            Node::LabeledStatement(n) => is(n.label.and_then(|n| n.node_id)),
            _ => false,
        }
    }

    /// `checkUnusedIdentifiers` (`checker.go:7046`) — the dispatch, run once the
    /// file's walk is complete so that every reference has been marked.
    pub(crate) fn check_unused_identifiers(&mut self) {
        if !self.unused_check_enabled() {
            return;
        }
        // `checkSourceFile` (`checker.go:2221`): declaration files are exempt.
        // A file the parser recovered stands in for upstream's
        // `NodeFlagsThisNodeOrAnySubNodesHasError`, which `reportUnused`
        // (`checker.go:7092`) tests on every location.
        if self.file_is_ambient || self.file_has_parse_errors {
            self.unused_check_nodes.clear();
            return;
        }
        let nodes = std::mem::take(&mut self.unused_check_nodes);
        for node in nodes {
            let Some(typed) = self.node_map.get(node) else { continue };
            match typed {
                Node::ClassDeclaration(_) | Node::ClassExpression(_) => {
                    self.check_unused_class_members(node);
                    self.check_unused_type_parameters(node);
                }
                Node::SourceFile(_)
                | Node::ModuleDeclaration(_)
                | Node::Block(_)
                | Node::ModuleBlock(_)
                | Node::CaseBlock(_)
                | Node::ForStatement(_)
                | Node::ForInOrOfStatement(_) => self.check_unused_locals_and_parameters(node),
                // "Only report unused parameters on the implementation, not
                // overloads" (`checker.go:7057`).
                Node::ConstructorDeclaration(_)
                | Node::FunctionDeclaration(_)
                | Node::FunctionExpression(_)
                | Node::ArrowFunction(_)
                | Node::MethodDeclaration(_)
                | Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_) => {
                    if self.declaration_body_of(node).is_some() {
                        self.check_unused_locals_and_parameters(node);
                    }
                    self.check_unused_type_parameters(node);
                }
                Node::MethodSignatureDeclaration(_)
                | Node::CallSignatureDeclaration(_)
                | Node::ConstructSignatureDeclaration(_)
                | Node::FunctionTypeNode(_)
                | Node::ConstructorTypeNode(_)
                | Node::TypeAliasDeclaration(_)
                | Node::InterfaceDeclaration(_) => self.check_unused_type_parameters(node),
                Node::InferTypeNode(_) => self.check_unused_infer_type_parameter(node),
                _ => {}
            }
        }
    }

    /// `checkUnusedLocalsAndParameters` (`checker.go:7140`).
    fn check_unused_locals_and_parameters(&mut self, node: NodeId) {
        let Some(locals) = self.binder.locals(node) else { return };
        let candidates: Vec<SymbolId> = locals.values().copied().collect();

        let mut variable_parents: Vec<NodeId> = Vec::new();
        let mut import_clauses: Vec<(NodeId, Vec<NodeId>)> = Vec::new();
        let mut plain: Vec<(NodeId, String)> = Vec::new();

        for local in candidates {
            let merged = self.binder.merged_symbol(local);
            let symbol = self.binder.symbols().get(merged);
            let flags = symbol.flags;
            let referenced = self
                .symbol_reference_kinds
                .get(&merged)
                .copied()
                .unwrap_or_else(SymbolFlags::empty);
            let is_type_parameter = flags.intersects(SymbolFlags::TYPE_PARAMETER);
            if is_type_parameter
                && (!flags.intersects(SymbolFlags::VARIABLE)
                    || referenced.intersects(SymbolFlags::VARIABLE))
            {
                continue;
            }
            if !is_type_parameter
                && (!referenced.is_empty()
                    || symbol.export_symbol.is_some()
                    || flags.intersects(SymbolFlags::MODULE_EXPORTS))
            {
                continue;
            }
            let name = symbol.name.to_string();
            let declarations: Vec<NodeId> = symbol.declarations.to_vec();
            for declaration in declarations {
                match self.nodes.kind(declaration) {
                    SyntaxKind::VariableDeclaration
                    | SyntaxKind::Parameter
                    | SyntaxKind::BindingElement => {
                        let root = self.root_declaration(declaration);
                        if let Some(parent) = self.nodes.parent(root)
                            && !variable_parents.contains(&parent)
                        {
                            variable_parents.push(parent);
                        }
                    }
                    SyntaxKind::ImportClause
                    | SyntaxKind::ImportSpecifier
                    | SyntaxKind::NamespaceImport => {
                        if !self.name_starts_with_underscore(declaration) {
                            let Some(clause) = self.import_clause_from_imported(declaration) else {
                                continue;
                            };
                            match import_clauses.iter_mut().find(|(owner, _)| *owner == clause) {
                                Some((_, list)) => list.push(declaration),
                                None => import_clauses.push((clause, vec![declaration])),
                            }
                        }
                    }
                    SyntaxKind::TypeParameter => {}
                    _ => {
                        if !self.is_ambient_module_declaration(declaration) {
                            plain.push((declaration, name.clone()));
                        }
                    }
                }
            }
        }

        for (declaration, name) in plain {
            self.report_unused_local(declaration, &name);
        }
        for declaration in variable_parents {
            if self.nodes.kind(declaration) == SyntaxKind::VariableDeclarationList {
                self.report_unused_variables(declaration);
            } else {
                self.report_unused_parameters(declaration);
            }
        }
        for (clause, unused) in import_clauses {
            self.report_unused_imports(clause, &unused);
        }
    }

    /// `reportUnusedLocal` (`checker.go:7181`).
    fn report_unused_local(&mut self, node: NodeId, name: &str) {
        let message = if is_type_declaration_kind(self.nodes.kind(node)) {
            &messages::_0_IS_DECLARED_BUT_NEVER_USED
        } else {
            &messages::_0_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ
        };
        let at = self.name_node_of(node).unwrap_or(node);
        let span = self.error_span(at);
        self.report_unused(
            node,
            UnusedKind::Local,
            Diagnostic::with_args(message, span, [name.to_string()]),
        );
    }

    /// `reportUnusedVariables` (`checker.go:7186`) — the TS6199 grouping.
    fn report_unused_variables(&mut self, list: NodeId) {
        let declarations = self.variable_declarations_of(list);
        if declarations.len() > 1
            && declarations.iter().all(|d| self.is_unreferenced_variable_declaration(*d))
        {
            let span = self.error_span(list);
            self.report_unused_variable(
                list,
                Diagnostic::new(&messages::ALL_VARIABLES_ARE_UNUSED, span),
            );
        } else {
            self.report_unused_variable_declarations(&declarations);
        }
    }

    /// `reportUnusedParameters` (`checker.go:7195`).
    fn report_unused_parameters(&mut self, node: NodeId) {
        let parameters = self.parameters_of(node);
        self.report_unused_variable_declarations(&parameters);
    }

    /// `reportUnusedBindingElements` (`checker.go:7199`) — the TS6198 grouping.
    fn report_unused_binding_elements(&mut self, pattern: NodeId) {
        let elements = self.binding_elements_of(pattern);
        if elements.len() > 1
            && elements.iter().all(|d| self.is_unreferenced_variable_declaration(*d))
        {
            let span = self.error_span(pattern);
            self.report_unused_variable(
                pattern,
                Diagnostic::new(&messages::ALL_DESTRUCTURED_ELEMENTS_ARE_UNUSED, span),
            );
        } else {
            self.report_unused_variable_declarations(&elements);
        }
    }

    /// `reportUnusedVariableDeclarations` (`checker.go:7208`).
    fn report_unused_variable_declarations(&mut self, declarations: &[NodeId]) {
        for declaration in declarations {
            let Some(name) = self.name_node_of(*declaration) else { continue };
            if self.is_parameter_property(*declaration) || self.is_this_parameter(*declaration) {
                continue;
            }
            if matches!(self.node_map.get(name), Some(Node::BindingPattern(_))) {
                self.report_unused_binding_elements(name);
            } else if self.is_unreferenced_variable_declaration(*declaration) {
                let span = self.error_span(name);
                let text = self.identifier_text_of(name).unwrap_or_default().to_string();
                self.report_unused_variable(
                    *declaration,
                    Diagnostic::with_args(
                        &messages::_0_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
                        span,
                        [text],
                    ),
                );
            }
        }
    }

    /// `reportUnusedVariable` (`checker.go:7087`) — the walk that decides
    /// whether this is a *local* or a *parameter* for option purposes.
    fn report_unused_variable(&mut self, location: NodeId, diagnostic: Diagnostic) {
        let mut at = location;
        loop {
            let kind = self.nodes.kind(at);
            if kind == SyntaxKind::BindingElement
                || kind == SyntaxKind::ObjectBindingPattern
                || kind == SyntaxKind::ArrayBindingPattern
            {
                let Some(parent) = self.nodes.parent(at) else { break };
                at = parent;
            } else {
                break;
            }
        }
        let kind = if self.nodes.kind(at) == SyntaxKind::Parameter {
            UnusedKind::Parameter
        } else {
            UnusedKind::Local
        };
        self.report_unused(at, kind, diagnostic);
    }

    /// `reportUnused` (`checker.go:7091`) — the ambient gate and the
    /// error-versus-suggestion choice.
    fn report_unused(&mut self, location: NodeId, kind: UnusedKind, diagnostic: Diagnostic) {
        if self.is_in_ambient_context(location) {
            return;
        }
        let is_error = match kind {
            UnusedKind::Local => self.no_unused_locals,
            UnusedKind::Parameter => self.no_unused_parameters,
        };
        if !is_error {
            // The suggestion channel is not ported: `.errors.txt` records
            // errors only, so a suggestion here could only be a false positive.
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(location) else { return };
        self.report(file, diagnostic);
    }

    /// `isUnreferencedVariableDeclaration` (`checker.go:7224`).
    fn is_unreferenced_variable_declaration(&self, node: NodeId) -> bool {
        let Some(name) = self.name_node_of(node) else { return true };
        if matches!(self.node_map.get(name), Some(Node::BindingPattern(_))) {
            return self
                .binding_elements_of(name)
                .iter()
                .all(|element| self.is_unreferenced_variable_declaration(*element));
        }
        if let Some(symbol) = self.binder.symbol_of(node) {
            let merged = self.binder.merged_symbol(symbol);
            if self
                .symbol_reference_kinds
                .get(&merged)
                .is_some_and(|kinds| kinds.intersects(SymbolFlags::VARIABLE))
            {
                return false;
            }
        }
        let kind = self.nodes.kind(node);
        let parent = self.nodes.parent(node);
        if kind == SyntaxKind::BindingElement
            && parent.is_some_and(|p| self.nodes.kind(p) == SyntaxKind::ObjectBindingPattern)
        {
            // `{ a, ...b }`: `a` removes a property from `b`, so it counts as
            // used (`checker.go:7234`).
            let siblings = self.binding_elements_of(parent.unwrap_or(node));
            if let Some(last) = siblings.last()
                && *last != node
                && self.has_dot_dot_dot(*last)
            {
                return false;
            }
        }
        let underscore_exempt = kind == SyntaxKind::Parameter
            || (kind == SyntaxKind::VariableDeclaration
                && parent.and_then(|p| self.nodes.parent(p)).is_some_and(|gp| {
                    self.nodes.kind(gp) == SyntaxKind::ForInStatement
                        || self.nodes.kind(gp) == SyntaxKind::ForOfStatement
                }))
            || (kind == SyntaxKind::BindingElement
                && !(parent
                    .is_some_and(|p| self.nodes.kind(p) == SyntaxKind::ObjectBindingPattern)
                    && self.property_name_of(node).is_none()));
        if underscore_exempt
            && self.identifier_text_of(name).is_some_and(|text| text.starts_with('_'))
        {
            return false;
        }
        true
    }

    /// `reportUnusedImports` (`checker.go:7245`) — the TS6192 grouping.
    fn report_unused_imports(&mut self, clause: NodeId, unused: &[NodeId]) {
        let Some(Node::ImportClause(import)) = self.node_map.get(clause) else { return };
        let mut declaration_count = usize::from(import.name.is_some());
        match import.named_bindings {
            Some(tsr_ast::NamedImportBindings::NamespaceImport(_)) => declaration_count += 1,
            Some(tsr_ast::NamedImportBindings::NamedImports(named)) => {
                declaration_count += named.elements.len();
            }
            None => {}
        }
        if declaration_count > 1 && declaration_count == unused.len() {
            let Some(parent) = self.nodes.parent(clause) else { return };
            let span = self.error_span(parent);
            self.report_unused(
                clause,
                UnusedKind::Local,
                Diagnostic::new(&messages::ALL_IMPORTS_IN_IMPORT_DECLARATION_ARE_UNUSED, span),
            );
        } else {
            for declaration in unused {
                let name = self
                    .name_node_of(*declaration)
                    .and_then(|n| self.identifier_text_of(n))
                    .unwrap_or_default()
                    .to_string();
                self.report_unused_local(*declaration, &name);
            }
        }
    }

    /// Is this node inside a class member whose own name is `text`? §704.
    fn reference_is_inside_named_member(&self, node: NodeId, text: &str) -> bool {
        self.nodes.ancestors(node).any(|ancestor| {
            self.name_node_of(ancestor)
                .and_then(|name| self.identifier_text_of(name))
                .is_some_and(|owner| owner == text)
        })
    }

    /// The written name of a class declaration or expression. §704.
    fn class_name_text_of(&self, node: NodeId) -> Option<String> {
        let name = match self.node_map.get(node)? {
            Node::ClassDeclaration(class) => class.name?,
            Node::ClassExpression(class) => class.name?,
            _ => return None,
        };
        Some(name.text.to_string())
    }

    /// `checkUnusedClassMembers` (`checker.go:7115`).
    ///
    /// Reference marking here is by *name* rather than by symbol — see
    /// [`Checker::note_member_name`].
    fn check_unused_class_members(&mut self, node: NodeId) {
        let members: Vec<ClassElement<'_>> = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.members.to_vec(),
            Some(Node::ClassExpression(class)) => class.members.to_vec(),
            _ => return,
        };
        for member in members {
            match member {
                ClassElement::MethodDeclaration(_)
                | ClassElement::PropertyDeclaration(_)
                | ClassElement::GetAccessorDeclaration(_)
                | ClassElement::SetAccessorDeclaration(_) => {
                    // A set accessor whose getter already reported is skipped
                    // (`checker.go:7119`).
                    if matches!(member, ClassElement::SetAccessorDeclaration(_)) {
                        continue;
                    }
                    let Some(id) = member.node_id() else { continue };
                    let Some(name) = self.name_node_of(id) else { continue };
                    let private = self
                        .member_modifiers(id)
                        .is_some_and(|m| has_keyword(m, SyntaxKind::PrivateKeyword))
                        || self.nodes.kind(name) == SyntaxKind::PrivateIdentifier;
                    if !private {
                        continue;
                    }
                    let Some(text) = self.identifier_text_of(name) else { continue };
                    // A private **static** member is not inherited, not visible
                    // through an instance, and not reachable through any
                    // expression whose type this port would have to compute —
                    // so the bare-name key is unnecessarily coarse for it, and
                    // `Test2.p1` was marking `Test3.p1` used. §704.
                    let is_static = self
                        .member_modifiers(id)
                        .is_some_and(|m| has_keyword(m, SyntaxKind::StaticKeyword));
                    let referenced = if is_static {
                        let owner = self.class_name_text_of(node);
                        self.referenced_member_names.contains(&format!("this.{text}"))
                            || owner.is_some_and(|owner| {
                                self.referenced_member_names.contains(&format!("{owner}.{text}"))
                            })
                    } else {
                        self.referenced_member_names.contains(text)
                    };
                    if referenced {
                        continue;
                    }
                    let text = text.to_string();
                    let span = self.error_span(name);
                    self.report_unused(
                        id,
                        UnusedKind::Local,
                        Diagnostic::with_args(
                            &messages::_0_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
                            span,
                            [text],
                        ),
                    );
                }
                ClassElement::ConstructorDeclaration(constructor) => {
                    for parameter in constructor.parameters {
                        if !has_keyword(parameter.modifiers, SyntaxKind::PrivateKeyword) {
                            continue;
                        }
                        let Some(id) = parameter.node_id else { continue };
                        let Some(name) = self.name_node_of(id) else { continue };
                        let Some(text) = self.identifier_text_of(name) else { continue };
                        if self.referenced_member_names.contains(text) {
                            continue;
                        }
                        let text = text.to_string();
                        let span = self.error_span(name);
                        self.report_unused(
                            id,
                            UnusedKind::Local,
                            Diagnostic::with_args(
                                &messages::PROPERTY_0_IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
                                span,
                                [text],
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    /// `checkUnusedTypeParameters` (`checker.go:7290`).
    fn check_unused_type_parameters(&mut self, node: NodeId) {
        let parameters = self.type_parameters_of(node);
        if parameters.is_empty() || !self.all_declarations_in_same_file(node) {
            return;
        }
        let all_unused = parameters.iter().all(|p| self.is_unreferenced_type_parameter(*p));
        if parameters.len() > 1 && all_unused {
            // `rangeOfTypeParameters` (`utilities.go:1554`) starts one character
            // before the list, which is the `<`.
            let first = self.nodes.span(parameters[0]);
            let span = tsr_core::Span::new(first.start.saturating_sub(1), first.end);
            self.report_unused(
                node,
                UnusedKind::Parameter,
                Diagnostic::new(&messages::ALL_TYPE_PARAMETERS_ARE_UNUSED, span),
            );
        } else {
            for parameter in parameters {
                if !self.is_unreferenced_type_parameter(parameter) {
                    continue;
                }
                let Some(name) = self.name_node_of(parameter) else { continue };
                let text = self.identifier_text_of(name).unwrap_or_default().to_string();
                let span = self.error_span(parameter);
                self.report_unused(
                    node,
                    UnusedKind::Parameter,
                    Diagnostic::with_args(&messages::_0_IS_DECLARED_BUT_NEVER_USED, span, [text]),
                );
            }
        }
    }

    /// `checkUnusedInferTypeParameter` (`checker.go:7283`).
    fn check_unused_infer_type_parameter(&mut self, node: NodeId) {
        let Some(Node::InferTypeNode(infer)) = self.node_map.get(node) else { return };
        let Some(parameter) = infer.type_parameter.and_then(|p| p.node_id) else { return };
        if !self.is_unreferenced_type_parameter(parameter) {
            return;
        }
        let Some(name) = self.name_node_of(parameter) else { return };
        let text = self.identifier_text_of(name).unwrap_or_default().to_string();
        let span = self.error_span(name);
        self.report_unused(
            node,
            UnusedKind::Parameter,
            Diagnostic::with_args(&messages::_0_IS_DECLARED_BUT_NEVER_USED, span, [text]),
        );
    }

    /// `isUnreferencedTypeParameter` (`checker.go:7310`).
    fn is_unreferenced_type_parameter(&self, parameter: NodeId) -> bool {
        if let Some(name) = self.name_node_of(parameter)
            && self.identifier_text_of(name).is_some_and(|text| text.starts_with('_'))
        {
            return false;
        }
        let Some(symbol) = self.binder.symbol_of(parameter) else { return false };
        let merged = self.binder.merged_symbol(symbol);
        !self
            .symbol_reference_kinds
            .get(&merged)
            .is_some_and(|kinds| kinds.intersects(SymbolFlags::TYPE_PARAMETER))
    }

    /// `IsWriteOnlyAccess` (`ast.go:1264`) — `accessKind(node) == Write`.
    pub(crate) fn is_write_only_access(&self, node: NodeId) -> bool {
        self.access_kind(node) == AccessKind::Write
    }

    /// `accessKind` (`ast.go:1426`), arm for arm.
    ///
    /// The recursion is bounded by the tree, but a corpus written to break
    /// compilers gets a budget anyway — the same reason
    /// [`crate::check::MAX_CHECK_DEPTH`] exists.
    fn access_kind(&self, node: NodeId) -> AccessKind {
        self.access_kind_at(node, 0)
    }

    fn access_kind_at(&self, node: NodeId, depth: u32) -> AccessKind {
        if depth > 64 {
            return AccessKind::Read;
        }
        let Some(parent) = self.nodes.parent(node) else { return AccessKind::Read };
        let Some(typed) = self.node_map.get(parent) else { return AccessKind::Read };
        // The two transparent arms are kept together rather than in upstream's
        // order: `clippy::match_same_arms` refuses identical bodies, and one
        // merged arm reads as the fact it is — a parenthesis and an array
        // literal both pass the access kind through unchanged.
        match typed {
            Node::ParenthesizedExpression(_) | Node::ArrayLiteralExpression(_) => {
                self.access_kind_at(parent, depth + 1)
            }
            Node::PrefixUnaryExpression(unary) => {
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) {
                    AccessKind::ReadWrite
                } else {
                    AccessKind::Read
                }
            }
            Node::PostfixUnaryExpression(unary) => {
                if matches!(
                    unary.operator.kind,
                    SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
                ) {
                    AccessKind::ReadWrite
                } else {
                    AccessKind::Read
                }
            }
            Node::BinaryExpression(binary) => {
                if binary.left.and_then(|left| left.node_id()) == Some(node)
                    && let Some(operator) = binary.operator_token
                    && is_assignment_operator(operator.kind)
                {
                    if operator.kind == SyntaxKind::EqualsToken {
                        AccessKind::Write
                    } else {
                        AccessKind::ReadWrite
                    }
                } else {
                    AccessKind::Read
                }
            }
            Node::PropertyAccessExpression(access) => {
                if access.name.and_then(|name| name.node_id()) == Some(node) {
                    self.access_kind_at(parent, depth + 1)
                } else {
                    AccessKind::Read
                }
            }
            Node::PropertyAssignment(assignment) => {
                let outer = self
                    .nodes
                    .parent(parent)
                    .map_or(AccessKind::Read, |owner| self.access_kind_at(owner, depth + 1));
                if assignment.name.node_id() == Some(node) { outer.reversed() } else { outer }
            }
            Node::ShorthandPropertyAssignment(shorthand) => {
                if shorthand.object_assignment_initializer.and_then(|e| e.node_id()) == Some(node) {
                    AccessKind::Read
                } else {
                    self.nodes
                        .parent(parent)
                        .map_or(AccessKind::Read, |owner| self.access_kind_at(owner, depth + 1))
                }
            }
            Node::ForInOrOfStatement(statement) => {
                if statement.initializer.and_then(|e| e.node_id()) == Some(node) {
                    AccessKind::Write
                } else {
                    AccessKind::Read
                }
            }
            _ => AccessKind::Read,
        }
    }

    // ---- small structural helpers -------------------------------------------

    /// `GetRootDeclaration` (`ast/utilities.go`): walk out of binding patterns.
    fn root_declaration(&self, node: NodeId) -> NodeId {
        let mut at = node;
        while self.nodes.kind(at) == SyntaxKind::BindingElement {
            let Some(pattern) = self.nodes.parent(at) else { break };
            let Some(owner) = self.nodes.parent(pattern) else { break };
            at = owner;
        }
        at
    }

    /// `importClauseFromImported` (`checker.go:7273`).
    fn import_clause_from_imported(&self, node: NodeId) -> Option<NodeId> {
        match self.nodes.kind(node) {
            SyntaxKind::ImportClause => Some(node),
            SyntaxKind::NamespaceImport => self.nodes.parent(node),
            _ => self.nodes.parent(node).and_then(|p| self.nodes.parent(p)),
        }
    }

    /// The `name` of any declaration this module reports on.
    pub(crate) fn name_node_of(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::VariableDeclaration(n) => n.name.and_then(|n| n.node_id()),
            Node::ParameterDeclaration(n) => n.name.and_then(|n| n.node_id()),
            Node::BindingElement(n) => n.name.and_then(|n| n.node_id()),
            Node::TypeParameterDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::FunctionDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::FunctionExpression(n) => n.name.and_then(|n| n.node_id),
            Node::ClassDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::ClassExpression(n) => n.name.and_then(|n| n.node_id),
            Node::InterfaceDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::TypeAliasDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::EnumDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::ModuleDeclaration(n) => n.name.and_then(|n| n.node_id()),
            Node::ImportClause(n) => n.name.and_then(|n| n.node_id),
            Node::NamespaceImport(n) => n.name.and_then(|n| n.node_id),
            Node::ImportSpecifier(n) => n.name.and_then(|n| n.node_id),
            Node::ImportEqualsDeclaration(n) => n.name.and_then(|n| n.node_id),
            Node::PropertyDeclaration(n) => n.name.node_id(),
            Node::MethodDeclaration(n) => n.name.node_id(),
            Node::GetAccessorDeclaration(n) => n.name.node_id(),
            Node::SetAccessorDeclaration(n) => n.name.node_id(),
            Node::EnumMember(n) => n.name.node_id(),
            _ => None,
        }
    }

    /// The text of an identifier-like node.
    fn identifier_text_of(&self, node: NodeId) -> Option<&str> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text),
            Node::PrivateIdentifier(identifier) => Some(identifier.text),
            Node::StringLiteral(literal) => Some(literal.text),
            _ => None,
        }
    }

    fn property_name_of(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::BindingElement(element) => element.property_name.and_then(|n| n.node_id()),
            _ => None,
        }
    }

    fn has_dot_dot_dot(&self, node: NodeId) -> bool {
        matches!(self.node_map.get(node), Some(Node::BindingElement(element)) if element.dot_dot_dot_token.is_some())
    }

    fn binding_elements_of(&self, pattern: NodeId) -> Vec<NodeId> {
        match self.node_map.get(pattern) {
            Some(Node::BindingPattern(bindings)) => {
                bindings.elements.iter().filter_map(|element| element.node_id).collect()
            }
            _ => Vec::new(),
        }
    }

    fn variable_declarations_of(&self, list: NodeId) -> Vec<NodeId> {
        match self.node_map.get(list) {
            Some(Node::VariableDeclarationList(declarations)) => {
                declarations.declarations.iter().filter_map(|d| d.node_id).collect()
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn parameters_of(&self, node: NodeId) -> Vec<NodeId> {
        let collect = |parameters: &[&tsr_ast::ParameterDeclaration<'_>]| -> Vec<NodeId> {
            parameters.iter().filter_map(|p| p.node_id).collect()
        };
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => collect(n.parameters),
            Some(Node::FunctionExpression(n)) => collect(n.parameters),
            Some(Node::ArrowFunction(n)) => collect(n.parameters),
            Some(Node::MethodDeclaration(n)) => collect(n.parameters),
            Some(Node::ConstructorDeclaration(n)) => collect(n.parameters),
            Some(Node::GetAccessorDeclaration(n)) => collect(n.parameters),
            Some(Node::SetAccessorDeclaration(n)) => collect(n.parameters),
            // **The signature kinds.** A rule reached through this accessor
            // sees only what it enumerates, and `check_grammar_parameter_list`
            // was dispatched for every node while this returned nothing for an
            // interface member — §601. `unused.rs`'s own caller is guarded by
            // the declaration having a body, so widening here does not reach
            // TS6133.
            Some(Node::MethodSignatureDeclaration(n)) => collect(n.parameters),
            Some(Node::CallSignatureDeclaration(n)) => collect(n.parameters),
            Some(Node::ConstructSignatureDeclaration(n)) => collect(n.parameters),
            Some(Node::FunctionTypeNode(n)) => collect(n.parameters),
            Some(Node::ConstructorTypeNode(n)) => collect(n.parameters),
            _ => Vec::new(),
        }
    }

    fn type_parameters_of(&self, node: NodeId) -> Vec<NodeId> {
        let collect = |parameters: &[&tsr_ast::TypeParameterDeclaration<'_>]| -> Vec<NodeId> {
            parameters.iter().filter_map(|p| p.node_id).collect()
        };
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => collect(n.type_parameters),
            Some(Node::FunctionExpression(n)) => collect(n.type_parameters),
            Some(Node::ArrowFunction(n)) => collect(n.type_parameters),
            Some(Node::MethodDeclaration(n)) => collect(n.type_parameters),
            Some(Node::MethodSignatureDeclaration(n)) => collect(n.type_parameters),
            Some(Node::ConstructorDeclaration(n)) => collect(n.type_parameters),
            Some(Node::GetAccessorDeclaration(n)) => collect(n.type_parameters),
            Some(Node::SetAccessorDeclaration(n)) => collect(n.type_parameters),
            Some(Node::CallSignatureDeclaration(n)) => collect(n.type_parameters),
            Some(Node::ConstructSignatureDeclaration(n)) => collect(n.type_parameters),
            Some(Node::FunctionTypeNode(n)) => collect(n.type_parameters),
            Some(Node::ConstructorTypeNode(n)) => collect(n.type_parameters),
            Some(Node::ClassDeclaration(n)) => collect(n.type_parameters),
            Some(Node::ClassExpression(n)) => collect(n.type_parameters),
            Some(Node::InterfaceDeclaration(n)) => collect(n.type_parameters),
            Some(Node::TypeAliasDeclaration(n)) => collect(n.type_parameters),
            _ => Vec::new(),
        }
    }

    fn declaration_body_of(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::FunctionDeclaration(n) => n.body.and_then(|b| b.node_id()),
            Node::FunctionExpression(n) => n.body.and_then(|b| b.node_id()),
            Node::ArrowFunction(n) => n.body.and_then(|b| b.node_id()),
            Node::MethodDeclaration(n) => n.body.and_then(|b| b.node_id()),
            Node::ConstructorDeclaration(n) => n.body.and_then(|b| b.node_id()),
            Node::GetAccessorDeclaration(n) => n.body.and_then(|b| b.node_id()),
            Node::SetAccessorDeclaration(n) => n.body.and_then(|b| b.node_id()),
            _ => None,
        }
    }

    fn member_modifiers(&self, node: NodeId) -> Option<&[ModifierLike<'_>]> {
        match self.node_map.get(node)? {
            Node::PropertyDeclaration(n) => Some(n.modifiers),
            Node::MethodDeclaration(n) => Some(n.modifiers),
            Node::GetAccessorDeclaration(n) => Some(n.modifiers),
            Node::SetAccessorDeclaration(n) => Some(n.modifiers),
            _ => None,
        }
    }

    /// `isParameterPropertyDeclaration` — a constructor parameter carrying an
    /// accessibility or `readonly` modifier declares a property, and upstream
    /// reports on the property rather than the parameter.
    fn is_parameter_property(&self, node: NodeId) -> bool {
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else {
            return false;
        };
        parameter.modifiers.iter().any(|modifier| {
            matches!(
                modifier,
                ModifierLike::Token(token)
                    if matches!(
                        token.kind,
                        SyntaxKind::PublicKeyword
                            | SyntaxKind::PrivateKeyword
                            | SyntaxKind::ProtectedKeyword
                            | SyntaxKind::ReadonlyKeyword
                    )
            )
        })
    }

    fn is_this_parameter(&self, node: NodeId) -> bool {
        self.name_node_of(node)
            .and_then(|name| self.identifier_text_of(name))
            .is_some_and(|text| text == "this")
    }

    /// Are every one of this node's symbol's declarations in one file?
    ///
    /// `allDeclarationsInSameSourceFile` (`utilities.go:1578`).
    fn all_declarations_in_same_file(&self, node: NodeId) -> bool {
        let Some(symbol) = self.binder.symbol_of(node) else { return true };
        let declarations =
            &self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations;
        let mut file = None;
        for declaration in declarations {
            let owner = self.source_file_of_for_diagnostics(*declaration);
            match file {
                None => file = Some(owner),
                Some(seen) if seen != owner => return false,
                Some(_) => {}
            }
        }
        true
    }

    pub(crate) fn is_ambient_module_declaration(&self, node: NodeId) -> bool {
        matches!(
            self.node_map.get(node),
            Some(Node::ModuleDeclaration(declaration))
                if matches!(declaration.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                    || declaration.keyword.kind == SyntaxKind::GlobalKeyword
        )
    }

    fn name_starts_with_underscore(&self, node: NodeId) -> bool {
        self.name_node_of(node)
            .and_then(|name| self.identifier_text_of(name))
            .is_some_and(|text| text.starts_with('_'))
    }

    /// `node.Flags & ast.NodeFlagsAmbient`, asked of the ancestors.
    ///
    /// The same stand-in [`crate::check`] documents: this port's parser never
    /// sets the flag (`bd tsr-o9tl`), so the question is answered by walking to
    /// the nearest `declare` or ambient module.
    fn is_in_ambient_context(&self, node: NodeId) -> bool {
        if self.file_is_ambient {
            return true;
        }
        let mut at = Some(node);
        while let Some(current) = at {
            if self.is_ambient_module_declaration(current) {
                return true;
            }
            if let Some(modifiers) = self.declaration_modifiers(current)
                && has_keyword(modifiers, SyntaxKind::DeclareKeyword)
            {
                return true;
            }
            at = self.nodes.parent(current);
        }
        false
    }

    fn declaration_modifiers(&self, node: NodeId) -> Option<&[ModifierLike<'_>]> {
        match self.node_map.get(node)? {
            Node::VariableStatement(n) => Some(n.modifiers),
            Node::FunctionDeclaration(n) => Some(n.modifiers),
            Node::ClassDeclaration(n) => Some(n.modifiers),
            Node::InterfaceDeclaration(n) => Some(n.modifiers),
            Node::TypeAliasDeclaration(n) => Some(n.modifiers),
            Node::EnumDeclaration(n) => Some(n.modifiers),
            Node::ModuleDeclaration(n) => Some(n.modifiers),
            Node::PropertyDeclaration(n) => Some(n.modifiers),
            Node::MethodDeclaration(n) => Some(n.modifiers),
            _ => None,
        }
    }

    /// Reset the per-file state this module keeps.
    pub(crate) fn reset_unused_state(&mut self) {
        self.unused_check_nodes.clear();
        self.symbol_reference_kinds.clear();
        self.referenced_member_names.clear();
    }
}

/// Which declaration kinds take TS6196 (*"declared but never used"*) rather than
/// TS6133 — `ast.IsTypeDeclaration`.
fn is_type_declaration_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::TypeParameter
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::EnumDeclaration
    )
}

fn has_keyword(modifiers: &[ModifierLike<'_>], keyword: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == keyword))
}

/// The set of member names seen in the file, for the by-name marking private
/// class members need. Named here so the field's type has one home.
pub(crate) type MemberNames = FxHashSet<String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_declaration_takes_the_never_used_message() {
        // `reportUnusedLocal` (`checker.go:7181`) picks TS6196 for a type
        // declaration and TS6133 for everything else, and the two codes are not
        // interchangeable under the suite's exact-multiset comparison.
        assert!(is_type_declaration_kind(SyntaxKind::InterfaceDeclaration));
        assert!(is_type_declaration_kind(SyntaxKind::TypeParameter));
        assert!(!is_type_declaration_kind(SyntaxKind::VariableDeclaration));
        assert!(!is_type_declaration_kind(SyntaxKind::Parameter));
    }

    #[test]
    fn every_compound_assignment_is_an_assignment_operator() {
        // `accessKind`'s `BinaryExpression` arm splits on `=` versus the
        // compound forms: `y = 1` is a pure write and does not mark `y`, while
        // `y += 1` reads it first and does. Missing one of the compound tokens
        // would silently turn a read into a non-reference and produce a wrong
        // TS6133 — the one direction `crate::unused` may not fail in.
        for kind in [
            SyntaxKind::EqualsToken,
            SyntaxKind::PlusEqualsToken,
            SyntaxKind::QuestionQuestionEqualsToken,
            SyntaxKind::BarBarEqualsToken,
            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken,
        ] {
            assert!(is_assignment_operator(kind), "{kind:?}");
        }
        assert!(!is_assignment_operator(SyntaxKind::EqualsEqualsToken));
        assert!(!is_assignment_operator(SyntaxKind::PlusToken));
    }

    #[test]
    fn reversing_an_access_kind_is_an_involution_except_for_read_write() {
        // `reverseAccessKind` (`ast.go:1487`) is what makes
        // `({ x: y } = obj)` read `x` and write `y`.
        assert!(AccessKind::Read.reversed() == AccessKind::Write);
        assert!(AccessKind::Write.reversed() == AccessKind::Read);
        assert!(AccessKind::ReadWrite.reversed() == AccessKind::ReadWrite);
    }
}
