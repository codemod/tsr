//! `getSuggestedSymbolForNonexistentSymbol` (`checker.go:1741`): the symbol a
//! "Cannot find name 'x'. Did you mean 'y'?" names.
//!
//! The scope walk is [`tsr_binder::BindResult::resolve_name_for_suggestion`]
//! (`resolveNameForSymbolSuggestion`); this module supplies its `Lookup`
//! (`getSuggestionForSymbolNameLookup`, `checker.go:1774`), the globals
//! lookup that ends the walk, and `requiresScopeChange`.
//!
//! No cache or side table: native caches only `declarationRequiresScopeChange`
//! per function, and this path runs once per unresolved name. Alias candidates
//! go through the existing memoised [`Checker::resolve_alias`]
//! (`tryResolveAlias`), with no receiver or mapper context.

use tsr_ast::{Node, NodeFlags, NodeId, SyntaxKind};
use tsr_binder::{SuggestionHost, SuggestionWalk, SymbolFlags, SymbolId, SymbolTable};
use tsr_core::ScriptTarget;

use crate::checker::Checker;

/// `getPrimitiveTypeAliasSuggestions` (`checker.go:1745`): a global lookup
/// also offers `string` for a program that declares `String`, and so on.
const PRIMITIVE_TYPE_ALIAS_SUGGESTIONS: [(&str, &str); 6] = [
    ("string", "String"),
    ("number", "Number"),
    ("boolean", "Boolean"),
    ("object", "Object"),
    ("bigint", "BigInt"),
    ("symbol", "Symbol"),
];

/// A spelling suggestion: a real symbol, or one of the transient
/// `TypeAlias` symbols of [`PRIMITIVE_TYPE_ALIAS_SUGGESTIONS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameSuggestion {
    Symbol(SymbolId),
    Primitive(&'static str),
}

/// TSR's spellings of native `InternalSymbolNamePrefix` (`"\xFE"`) names.
/// `default` and `export=` are not prefixed natively either.
fn is_internal_symbol_name(name: &str) -> bool {
    matches!(
        name,
        "__class"
            | "__computed"
            | "__export"
            | "__function"
            | "__index"
            | "__jsxAttributes"
            | "__missing"
            | "__object"
            | "__type"
    )
}

/// `ast.IsTypeNodeKind`.
fn is_type_node_kind(kind: SyntaxKind) -> bool {
    (kind >= SyntaxKind::FIRST_TYPE_NODE && kind <= SyntaxKind::LAST_TYPE_NODE)
        || matches!(
            kind,
            SyntaxKind::AnyKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::IntrinsicKeyword
                | SyntaxKind::ExpressionWithTypeArguments
        )
}

impl SuggestionHost<'_> for Checker<'_, '_> {
    fn lookup(
        &mut self,
        table: &SymbolTable<'_>,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        match self.suggestion_for_symbol_name_lookup(table, name, meaning, false)? {
            NameSuggestion::Symbol(symbol) => Some(symbol),
            NameSuggestion::Primitive(_) => None,
        }
    }

    fn declaration_requires_scope_change(&mut self, function: NodeId) -> bool {
        let mut parameters = Vec::new();
        if let Some(node) = self.node_map.get(function) {
            tsr_ast::for_each_child_id(node, |child| {
                if self.nodes.kind(child) == SyntaxKind::Parameter {
                    parameters.push(child);
                }
            });
        }
        parameters.into_iter().any(|parameter| {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                return false;
            };
            declaration
                .name
                .and_then(|name| Node::from(name).node_id())
                .is_some_and(|name| self.requires_scope_change_worker(name))
                || declaration
                    .initializer
                    .and_then(|initializer| Node::from(initializer).node_id())
                    .is_some_and(|initializer| self.requires_scope_change_worker(initializer))
        })
    }
}

impl Checker<'_, '_> {
    /// `getSuggestedSymbolForNonexistentSymbol(location, name, meaning)`
    /// (`checker.go:1741`): the suggestion walk, then
    /// `r.lookup(r.Globals, name, meaning|SymbolFlagsGlobalLookup)`.
    pub(crate) fn suggested_symbol_for_nonexistent_symbol(
        &mut self,
        location: NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<NameSuggestion> {
        let binder = self.binder;
        match binder.resolve_name_for_suggestion(
            self.nodes,
            self.node_map,
            location,
            name,
            meaning,
            self,
        ) {
            SuggestionWalk::Found(symbol) => Some(NameSuggestion::Symbol(symbol)),
            SuggestionWalk::Declined => None,
            SuggestionWalk::Globals => {
                self.suggestion_for_symbol_name_lookup(binder.globals(), name, meaning, true)
            }
        }
    }

    /// `onFailedToResolveSymbol`'s suggestion arm (`checker.go:1590`): the
    /// suggestion's printed name, unless its value declaration is a
    /// `declare global` block. `symbolToString` without an enclosing
    /// declaration prints the symbol's own name for every symbol this walk can
    /// answer.
    pub(crate) fn spelling_suggestion_for(
        &mut self,
        location: NodeId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<String> {
        match self.suggested_symbol_for_nonexistent_symbol(location, name, meaning)? {
            NameSuggestion::Primitive(primitive) => Some(primitive.to_string()),
            NameSuggestion::Symbol(symbol) => {
                let entry = self.binder.symbols().get(symbol);
                if let Some(declaration) = entry.value_declaration
                    && matches!(self.node_map.get(declaration), Some(Node::ModuleDeclaration(module))
                        if module.keyword.kind == SyntaxKind::GlobalKeyword)
                {
                    return None;
                }
                Some(entry.name.to_string())
            }
        }
    }

    /// `getSuggestionForSymbolNameLookup` (`checker.go:1774`).
    fn suggestion_for_symbol_name_lookup(
        &mut self,
        table: &SymbolTable<'_>,
        name: &str,
        meaning: SymbolFlags,
        global: bool,
    ) -> Option<NameSuggestion> {
        // `getSymbol` (`checker.go:2176`).
        if !meaning.is_empty()
            && let Some(&found) = table.get(name)
        {
            let found = self.binder.merged_symbol(found);
            if self.is_global_augmentation_symbol(found) {
                return self.spelling_suggestion_over(table, name, meaning, global);
            }
            let flags = self.binder.symbols().get(found).flags;
            if flags.intersects(meaning)
                || flags.intersects(SymbolFlags::ALIAS)
                    && self.get_symbol_flags(found).intersects(meaning)
            {
                return Some(NameSuggestion::Symbol(found));
            }
        }
        self.spelling_suggestion_over(table, name, meaning, global)
    }

    /// `getSpellingSuggestionForName` (`checker.go:1800`): the candidate name
    /// test, then `core.GetSpellingSuggestion` with `compareSymbols`.
    fn spelling_suggestion_over(
        &mut self,
        table: &SymbolTable<'_>,
        name: &str,
        meaning: SymbolFlags,
        global: bool,
    ) -> Option<NameSuggestion> {
        let mut candidates: Vec<(NameSuggestion, &str)> = Vec::with_capacity(table.len());
        for &symbol in table.values() {
            let candidate_name = self.binder.symbols().get(symbol).name;
            if candidate_name.is_empty()
                || candidate_name.starts_with('"')
                || is_internal_symbol_name(candidate_name)
                || self.is_global_augmentation_symbol(symbol)
            {
                continue;
            }
            let flags = self.binder.symbols().get(symbol).flags;
            if flags.intersects(meaning)
                || flags.intersects(SymbolFlags::ALIAS)
                    && self.try_resolve_alias_target(symbol).is_some_and(|target| {
                        self.binder.symbols().get(target).flags.intersects(meaning)
                    })
            {
                candidates.push((NameSuggestion::Symbol(symbol), candidate_name));
            }
        }
        if global {
            for (primitive, builtin) in PRIMITIVE_TYPE_ALIAS_SUGGESTIONS {
                if table.contains_key(builtin) && SymbolFlags::TYPE_ALIAS.intersects(meaning) {
                    candidates.push((NameSuggestion::Primitive(primitive), primitive));
                }
            }
        }
        tsr_core::get_spelling_suggestion(
            name,
            candidates.iter(),
            |candidate| candidate.1,
            |left, right| self.compare_suggestions(left.0, right.0),
        )
        .map(|candidate| candidate.0)
    }

    /// A `declare global` block's symbol. Native names it
    /// `InternalSymbolNameGlobal` (`"\xFEglobal"`), so `global` never finds
    /// it and no candidate test sees a name; TSR files it under `global` (see
    /// [`tsr_binder::BindResult::resolve_name`]'s `global` refusal).
    fn is_global_augmentation_symbol(&self, symbol: SymbolId) -> bool {
        let declarations = &self.binder.symbols().get(symbol).declarations;
        !declarations.is_empty()
            && declarations.iter().all(|&declaration| {
                matches!(self.node_map.get(declaration), Some(Node::ModuleDeclaration(module))
                    if module.keyword.kind == SyntaxKind::GlobalKeyword)
            })
    }

    /// `compareSymbols` with the transient primitive suggestions, which have
    /// no declarations and so sort after every declared symbol, then by name.
    fn compare_suggestions(
        &self,
        left: NameSuggestion,
        right: NameSuggestion,
    ) -> std::cmp::Ordering {
        match (left, right) {
            (NameSuggestion::Symbol(left), NameSuggestion::Symbol(right)) => {
                self.compare_symbols(left, right)
            }
            (NameSuggestion::Symbol(symbol), NameSuggestion::Primitive(primitive)) => {
                let entry = self.binder.symbols().get(symbol);
                if entry.declarations.is_empty() {
                    entry.name.cmp(primitive)
                } else {
                    std::cmp::Ordering::Less
                }
            }
            (NameSuggestion::Primitive(primitive), NameSuggestion::Symbol(symbol)) => {
                let entry = self.binder.symbols().get(symbol);
                if entry.declarations.is_empty() {
                    primitive.cmp(entry.name)
                } else {
                    std::cmp::Ordering::Greater
                }
            }
            (NameSuggestion::Primitive(left), NameSuggestion::Primitive(right)) => left.cmp(right),
        }
    }

    /// `tryResolveAlias` (`checker.go`): the end of the alias chain, `None`
    /// when it cannot be resolved or loops.
    fn try_resolve_alias_target(&mut self, alias: SymbolId) -> Option<SymbolId> {
        let mut seen = vec![alias];
        let mut current = alias;
        while self.binder.symbols().get(current).flags.intersects(SymbolFlags::ALIAS) {
            let target = self.resolve_alias(current)?;
            if seen.contains(&target) {
                return None;
            }
            seen.push(target);
            current = target;
        }
        Some(current)
    }

    /// `requiresScopeChangeWorker` (`nameresolver.go:376`).
    fn requires_scope_change_worker(&self, node: NodeId) -> bool {
        let name_of = |node: NodeId| self.node_map.get(node).and_then(|typed| typed.name_id());
        match self.nodes.kind(node) {
            SyntaxKind::ArrowFunction
            | SyntaxKind::FunctionExpression
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::Constructor => false,
            SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::PropertyAssignment => {
                name_of(node).is_some_and(|name| self.requires_scope_change_worker(name))
            }
            SyntaxKind::PropertyDeclaration => {
                let is_static = matches!(self.node_map.get(node), Some(Node::PropertyDeclaration(property))
                    if property.modifiers.iter().any(|modifier| matches!(modifier,
                        tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::StaticKeyword)));
                if is_static {
                    // `GetEmitStandardClassFields`.
                    return !(self.standard_class_fields
                        && self.language_version >= ScriptTarget::ES2022);
                }
                name_of(node).is_some_and(|name| self.requires_scope_change_worker(name))
            }
            kind => {
                let nullish = matches!(self.node_map.get(node), Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::QuestionQuestionToken));
                let optional_chain = self.nodes.flags(node).contains(NodeFlags::OPTIONAL_CHAIN)
                    && matches!(
                        kind,
                        SyntaxKind::PropertyAccessExpression
                            | SyntaxKind::ElementAccessExpression
                            | SyntaxKind::CallExpression
                            | SyntaxKind::NonNullExpression
                    );
                if nullish || optional_chain {
                    return self.language_version < ScriptTarget::ES2020;
                }
                if let Some(Node::BindingElement(element)) = self.node_map.get(node)
                    && element.dot_dot_dot_token.is_some()
                    && self.nodes.parent(node).is_some_and(|parent| {
                        self.nodes.kind(parent) == SyntaxKind::ObjectBindingPattern
                    })
                {
                    return self.language_version < ScriptTarget::ES2017;
                }
                if is_type_node_kind(kind) {
                    return false;
                }
                let mut children = Vec::new();
                if let Some(typed) = self.node_map.get(node) {
                    tsr_ast::for_each_child_id(typed, |child| children.push(child));
                }
                children.into_iter().any(|child| self.requires_scope_change_worker(child))
            }
        }
    }
}
