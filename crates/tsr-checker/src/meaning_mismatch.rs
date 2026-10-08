//! TS2661 / TS2693 / TS2708 / TS2709 / TS2749 — a name that resolves, but not
//! under the meaning the position wanted.
//!
//! `onFailedToResolveSymbol`'s cascade (`checker.go:1564`): four of the seven
//! `checkAndReportErrorForXxx` arms that run **before** the missing-lib /
//! spelling-suggestion / `Cannot find name` sequence
//! [`Checker::check_value_identifier`] already ports.
//!
//! # This is a debt with an address, not a discovery
//!
//! Both TS2304 rules already located the gap and left a comment on it —
//! `check_value_identifier`'s *"a name that resolves under another meaning gets
//! a different code, so silence is the only sound answer until those arms are
//! ported"*, and `check_type_reference_name`'s *"a name that resolves as a
//! value is TS2749, and as a namespace TS2709"*. Each rule's meaning ladder is
//! a loop that returns on the first hit; this module is what that hit means.
//!
//! `docs/architecture/checker-notes-diag2.md` §163.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `isPrimitiveTypeName` (`checker.go:1637`).
///
/// Six names, and **not** `undefined`, `null`, `void`, `object` or `symbol` —
/// upstream's list exactly.
fn is_primitive_type_name(name: &str) -> bool {
    matches!(name, "any" | "string" | "number" | "boolean" | "never" | "unknown")
}

/// `isES2015OrLaterConstructorName` (`checker.go:1703`).
fn is_es2015_or_later_constructor_name(name: &str) -> bool {
    matches!(name, "Promise" | "Symbol" | "Map" | "WeakMap" | "Set" | "WeakSet")
}

impl Checker<'_, '_> {
    /// The cascade for a name in a **type** position, in upstream's order.
    ///
    /// Returns `true` when it reported, which is the `||` chain's contract at
    /// `checker.go:1570`: the caller stops rather than falling through.
    ///
    /// **Only the two type-position arms are here.** §163 built all four and
    /// §164 measured the value-position arm at 238 wrong lines; these two
    /// measured zero wrong and zero converts, the signature of a correct rule
    /// that never fires. §166 found why — `resolve_name`'s globals fallback
    /// ignored `meaning`, so the ladder above always hit `TYPE` first — and
    /// this is that build's payoff.
    pub(crate) fn report_meaning_mismatch_in_type_position(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        // `checkAndReportErrorForUsingNamespaceAsTypeOrValue` (`checker.go:1641`),
        // type branch: the position wanted `Type &^ Value`, and the name is a
        // module.
        if self.resolve_symbol_under(node, text, SymbolFlags::MODULE).is_some() {
            self.report_at(node, &messages::CANNOT_USE_NAMESPACE_0_AS_A_TYPE, text);
            return true;
        }
        // `checkAndReportErrorForUsingValueAsType` (`checker.go:1722`):
        // `resolveName(…, ^SymbolFlagsType & SymbolFlagsValue)`, and the symbol
        // must **not** also be a namespace — a namespace-and-value is the
        // `Cannot use namespace as a type` case, already handled above.
        let Some(symbol) = self.resolve_under(node, text, SymbolFlags::VALUE - SymbolFlags::TYPE)
        else {
            return false;
        };
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::NAMESPACE) {
            return false;
        }
        self.report_at(
            node,
            &messages::_0_REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_0,
            text,
        );
        true
    }

    fn resolve_under(
        &self,
        node: NodeId,
        text: &str,
        meaning: SymbolFlags,
    ) -> Option<tsr_binder::SymbolId> {
        self.binder.resolve_name(self.nodes, self.node_map, node, text, meaning)
    }

    /// `c.resolveSymbol(c.resolveName(…, meaning))` — the lookup **with the
    /// alias followed**.
    ///
    /// `import modes = _modes` binds `modes` as an alias, whose own flags carry
    /// `ALIAS` and not `MODULE`, so asking `resolve_under` for `MODULE` finds
    /// nothing where upstream finds the alias and resolves it. §675.
    ///
    /// **An alias matches by its target's flags.** Upstream's `getSymbol`
    /// (`checker.go`, reached from `resolveName`) returns an alias for
    /// `meaning` only when `getSymbolFlags(alias)` — the flags along the whole
    /// alias chain — intersect it. This binder's lookup answers an alias for
    /// any meaning, so `import I = require("./m")` whose `export =` is an
    /// interface matched `NAMESPACE_MODULE` and reported TS2708 where upstream
    /// reports TS2693. r4-helpers notes §4.
    fn resolve_symbol_under(
        &mut self,
        node: NodeId,
        text: &str,
        meaning: SymbolFlags,
    ) -> Option<tsr_binder::SymbolId> {
        let symbol = self
            .resolve_under(node, text, meaning)
            .or_else(|| self.resolve_under(node, text, SymbolFlags::ALIAS))?;
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(meaning) {
            return Some(symbol);
        }
        if !flags.intersects(SymbolFlags::ALIAS)
            || !self.get_symbol_flags(symbol).intersects(meaning)
        {
            return None;
        }
        // `resolveSymbol`.
        Some(self.resolve_alias_fully(symbol))
    }

    /// `checkAndReportErrorForMissingPrefix` (`checker.go:1532`): the first arm
    /// of `onFailedToResolveSymbol`, TS2662 / TS2663 for a name that is a
    /// static member of an enclosing class, or an instance member of the class
    /// whose non-static member directly contains the reference.
    ///
    /// The walk starts at `getThisContainer(errorLocation, false, false)` and
    /// visits every ancestor whose parent is class-like, so a reference inside
    /// a nested class also finds an outer class's static member. The instance
    /// arm reads `getDeclaredTypeOfSymbol(class).thisType`; the `this` type's
    /// constraint is the class's declared instance type, so its properties are
    /// read from that declared type. No cache: this runs only on a name that
    /// failed to resolve, after resolution has already done its work.
    pub(crate) fn check_and_report_error_for_missing_prefix(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        if self.nodes.kind(node) != SyntaxKind::Identifier || self.is_in_type_query(node) {
            return false;
        }
        let Some(container) = self.get_this_container(node, false) else { return false };
        let mut location = container;
        while let Some(parent) = self.nodes.parent(location) {
            if matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            ) {
                let Some(class_symbol) = self.binder.symbol_of(parent) else { break };
                let constructor_type = self.get_type_of_symbol(class_symbol);
                if self.get_property_of_type(constructor_type, text).is_some() {
                    let class_name = self.class_symbol_to_string(class_symbol);
                    let Some(file) = self.source_file_of_for_diagnostics(node) else {
                        return true;
                    };
                    let span = self.error_span(node);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_THE_STATIC_MEMBER_1_0,
                            span,
                            [text.to_string(), class_name],
                        ),
                    );
                    return true;
                }
                if location == container && !self.is_static_class_element(location) {
                    let instance_type = self.get_declared_type_of_symbol(class_symbol);
                    if self.get_property_of_type(instance_type, text).is_some() {
                        self.report_at(
                            node,
                            &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_THE_INSTANCE_MEMBER_THIS_0,
                            text,
                        );
                        return true;
                    }
                }
            }
            location = parent;
        }
        false
    }

    /// `checkAndReportErrorForExtendingInterface` (`checker.go:11666`): the
    /// second arm of `onFailedToResolveSymbol`. A value name that failed to
    /// resolve inside `class C extends X` is TS2689 when the heritage
    /// expression resolves with `Interface` meaning.
    ///
    /// Reachable since the locals lookup applies `getSymbol`'s alias arm
    /// (`docs/parity/notes/names-modules.md` §1): `import type { I }` naming an
    /// interface no longer answers a value lookup, so `class C extends I`
    /// fails resolution and lands here, as it does upstream. The expression's
    /// text is its identifiers joined by `.`, which is `GetTextOfNode` for
    /// every entity name written without interior trivia.
    ///
    /// **Class `extends` only.** Upstream reaches this arm from a *value*
    /// resolution, and only a class's `extends` expression is resolved as a
    /// value. This port's `is_value_reference` also admits the names of
    /// `interface I extends A` and `class C implements I` so TS2304 fires
    /// there (`report_meaning_mismatch_in_value_position` explains why);
    /// those positions never reach upstream's cascade, so they decline here.
    pub(crate) fn check_and_report_error_for_extending_interface(&mut self, node: NodeId) -> bool {
        let Some(expression) = self.entity_name_for_extending_interface(node) else {
            return false;
        };
        let in_class_extends = self
            .nodes
            .parent(expression)
            .and_then(|with_arguments| self.nodes.parent(with_arguments))
            .is_some_and(|clause| {
                matches!(self.node_map.get(clause), Some(Node::HeritageClause(heritage))
                    if heritage.token.kind == SyntaxKind::ExtendsKeyword)
                    && self.nodes.parent(clause).is_some_and(|owner| {
                        matches!(
                            self.nodes.kind(owner),
                            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                        )
                    })
            });
        if !in_class_extends {
            return false;
        }
        if self.resolve_entity_name_expression(expression, SymbolFlags::INTERFACE).is_none() {
            return false;
        }
        let Some(text) = self.entity_name_expression_text(expression) else { return false };
        self.report_at(
            node,
            &messages::CANNOT_EXTEND_AN_INTERFACE_0_DID_YOU_MEAN_IMPLEMENTS,
            &text,
        );
        true
    }

    /// `getEntityNameForExtendingInterface` (`checker.go:11679`): climb
    /// identifiers and property accesses to an `ExpressionWithTypeArguments`
    /// and answer its expression when that is an entity name expression.
    fn entity_name_for_extending_interface(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            match self.node_map.get(current)? {
                Node::Identifier(_) | Node::PropertyAccessExpression(_) => {
                    current = self.nodes.parent(current)?;
                }
                Node::ExpressionWithTypeArguments(with_arguments) => {
                    let expression = with_arguments.expression?.node_id()?;
                    return self.is_entity_name_expression_node(expression).then_some(expression);
                }
                _ => return None,
            }
        }
    }

    /// `ast.IsEntityNameExpression`.
    fn is_entity_name_expression_node(&self, node: NodeId) -> bool {
        self.entity_name_expression_text(node).is_some()
    }

    /// The dotted text of an entity name expression, or `None` when `node` is
    /// not one.
    pub(crate) fn entity_name_expression_text(&self, node: NodeId) -> Option<String> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => Some(identifier.text.to_string()),
            Node::PropertyAccessExpression(access) => {
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else { return None };
                let left = self.entity_name_expression_text(access.expression?.node_id()?)?;
                Some(format!("{left}.{}", name.text))
            }
            _ => None,
        }
    }

    /// `resolveEntityName(expression, meaning, ignoreErrors=true)`
    /// (`checker.go:15772`) for an entity name expression: the identifier arm
    /// through the meaning-filtered resolver, and the property-access arm as
    /// `resolveQualifiedName` — the left at `Namespace` meaning with its alias
    /// followed, then the right in the namespace's exports, accepted when its
    /// own flags or its alias target's carry `meaning`.
    fn resolve_entity_name_expression(
        &mut self,
        node: NodeId,
        meaning: SymbolFlags,
    ) -> Option<tsr_binder::SymbolId> {
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => {
                let text = identifier.text;
                self.resolve_name_with_export_alias(node, text, meaning)
            }
            Node::PropertyAccessExpression(access) => {
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else { return None };
                let left = access.expression?.node_id()?;
                let mut namespace =
                    self.resolve_entity_name_expression(left, SymbolFlags::NAMESPACE)?;
                if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS) {
                    namespace = self.resolve_alias(namespace)?;
                }
                let namespace = self.binder.merged_symbol(namespace);
                let found = *self.binder.symbols().get(namespace).exports.get(name.text)?;
                let found = self.binder.merged_symbol(found);
                (self.binder.symbols().get(found).flags.intersects(meaning)
                    || self.get_symbol_flags(found).intersects(meaning))
                .then_some(found)
            }
            _ => None,
        }
    }

    /// `IsInTypeQuery` (`checker/utilities.go`): an ancestor walk through
    /// identifiers and qualified names that stops at a `TypeQuery`.
    fn is_in_type_query(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            match self.nodes.kind(id) {
                SyntaxKind::TypeQuery => return true,
                SyntaxKind::Identifier | SyntaxKind::QualifiedName => {}
                _ => return false,
            }
            current = self.nodes.parent(id);
        }
        false
    }

    /// `ast.IsStatic` (`ast/utilities.go`): a class element with the `static`
    /// modifier, or a class static block.
    fn is_static_class_element(&self, node: NodeId) -> bool {
        let modifiers = match self.node_map.get(node) {
            Some(Node::ClassStaticBlockDeclaration(_)) => return true,
            Some(Node::MethodDeclaration(member)) => member.modifiers,
            Some(Node::PropertyDeclaration(member)) => member.modifiers,
            Some(Node::GetAccessorDeclaration(member)) => member.modifiers,
            Some(Node::SetAccessorDeclaration(member)) => member.modifiers,
            _ => return false,
        };
        tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword)
    }

    /// `symbolToString(classSymbol)` with no enclosing declaration: the class's
    /// own name, or the written name of an anonymous class expression.
    fn class_symbol_to_string(&self, symbol: tsr_binder::SymbolId) -> String {
        let name = self.binder.symbols().get(symbol).name;
        if name.is_empty() || name == "__class" {
            return self
                .anonymous_class_written_name(symbol)
                .unwrap_or_else(|| "(Anonymous class)".to_string());
        }
        name.to_string()
    }

    /// `c.error(errorLocation, message, name)` — both arms report at the same
    /// place with the same single argument.
    fn report_at(&mut self, node: NodeId, message: &'static tsr_diagnostics::Message, text: &str) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// The cascade for a name in a **value** position, in upstream's order.
    ///
    /// §164 measured this arm at **238 wrong lines** — against the pre-§166
    /// resolver, whose `globals` fallback ignored `meaning` and made every
    /// ladder above it return early. That number is not evidence about this
    /// code any more, which is why §169 re-measures rather than quotes it.
    pub(crate) fn report_meaning_mismatch_in_value_position(
        &mut self,
        node: NodeId,
        text: &str,
    ) -> bool {
        // **A heritage clause is not this cascade's position in this port**,
        // and the bound goes FIRST rather than before the type-as-value arm.
        // Two reasons, both measured:
        //
        // - `is_value_reference` admits the names of `interface I extends A, B`
        //   and of `class C implements I` so that TS2304 still fires there — a
        //   position §165 verified this port is *right* to visit, but not to
        //   read a `TYPE` hit in as a meaning mismatch. §164 measured that at
        //   232 of 238 wrong lines.
        // - `class C1 extends M.I1` is upstream's **TS2689**,
        //   `checkAndReportErrorForExtendingInterface` — the cascade's *second*
        //   arm, which is not ported. It runs ahead of both namespace arms, so
        //   reporting TS2708 there is a wrong code at a right position.
        //   `classExtendsInterfaceInModule` is three of §169's wrong lines and
        //   every one of them is that. **Owner: the TS2689 arm.**
        if self.nodes.ancestors(node).any(|a| self.nodes.kind(a) == SyntaxKind::HeritageClause) {
            return false;
        }
        if self.report_exporting_primitive_type(node, text) {
            return true;
        }
        // `checkAndReportErrorForUsingNamespaceAsTypeOrValue`
        // (`checker.go:1641`), value branch.
        // `c.resolveSymbol(c.resolveName(…))` on **this** branch too
        // (`checker.go:1643`). `import a = A` carries `ALIAS` and none of the
        // other three meanings in this binder, so the bare lookup matches
        // nothing. §688.
        if self.resolve_symbol_under(node, text, SymbolFlags::NAMESPACE_MODULE).is_some() {
            // `export = ns` may legitimately name a namespace, and
            // `checkExportAssignment` decides whether that is an error.
            if !self.is_export_assignment_expression_name(node) {
                self.report_at(node, &messages::CANNOT_USE_NAMESPACE_0_AS_A_VALUE, text);
            }
            return true;
        }
        // `checkAndReportErrorForUsingTypeAsValue` (`checker.go:1662`).
        if is_primitive_type_name(text) {
            self.report_primitive_type_as_value(node, text);
            return true;
        }
        // `maybeMappedType`'s **syntactic half** (`checker.go:1710-1716`).
        // Upstream then asks a type question this port cannot ask here and
        // picks a different message when the answer is yes; declining the whole
        // shape suppresses rather than mis-codes. Owner: `checker_types`.
        if self.maybe_mapped_type_position(node) {
            return false;
        }
        // `resolveName(errorLocation, name, SymbolFlagsType &^ SymbolFlagsValue)`
        // — narrower than this port's ladder, and written upstream's way.
        // `c.resolveSymbol(…)` then `getSymbolFlags`: an alias answers by its
        // target (`resolve_symbol_under`), and the value test reads the
        // resolved symbol's flags.
        let Some(symbol) =
            self.resolve_symbol_under(node, text, SymbolFlags::TYPE - SymbolFlags::VALUE)
        else {
            return false;
        };
        if self.get_symbol_flags(symbol).intersects(SymbolFlags::VALUE) {
            return false;
        }
        if self.is_export_assignment_expression_name(node) {
            return true;
        }
        let message = if is_es2015_or_later_constructor_name(text) {
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_ES2015_OR_LATER
        } else {
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        };
        self.report_at(node, message, text);
        true
    }

    /// TS2661 from `checkExportSpecifier` (`checker.go:5565`) — the **second**
    /// site for this code, and the one the corpus wants.
    ///
    /// `export { X }` with no module specifier, where `X` resolves to a
    /// declaration whose container is a **global source file** (a script, not
    /// an external module), or to `undefined`/`globalThis`.
    ///
    /// `GetDeclarationContainer` walks past variable-declaration wrappers;
    /// `IsGlobalSourceFile` asks whether the container is a non-module source
    /// file. A declaration inside a namespace has that namespace as its
    /// container and upstream does not report it, so this declines whenever the
    /// walk to the `SourceFile` passes a `ModuleDeclaration` or a function.
    /// §375.
    ///
    /// Also the `default` specifier's `checkExternalEmitHelpers` request
    /// (`checker.go:5383`, `:5571`), which shares this function's call site
    /// for import and export specifiers alike.
    pub(crate) fn check_export_specifier_is_local(&mut self, node: NodeId) {
        self.check_specifier_default_emit_helper(node);
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ExportSpecifier(specifier)) = self.node_map.get(node) else { return };
        // `PropertyNameOrName`, and a string literal is skipped outright.
        let named = specifier
            .property_name
            .and_then(|name| name.node_id())
            .or_else(|| specifier.name.and_then(|name| name.node_id()));
        let Some(named) = named else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(named) else { return };
        // `hasModuleSpecifier := node.Parent.Parent.ModuleSpecifier() != nil`
        let declaration = self
            .nodes
            .parent(node)
            .and_then(|list| self.nodes.parent(list))
            .and_then(|declaration| self.node_map.get(declaration));
        let Some(Node::ExportDeclaration(export)) = declaration else { return };
        if export.module_specifier.is_some() {
            return;
        }
        let text = identifier.text;
        let global = if text == "undefined" || text == "globalThis" {
            true
        } else {
            // **Resolve from the parent scope**, which is what upstream's
            // `resolveEntityName` does here — the alias this specifier itself
            // created is not a candidate. §836 added the mode (`bd tsr-8esz`);
            // §765's attempt to drop the specifier from the *declaration list*
            // measured `+0` because the reality is two symbols, not one symbol
            // with two declarations.
            let Some(symbol) = self.binder.resolve_name_excluding(
                self.nodes,
                self.node_map,
                named,
                text,
                SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::MODULE | SymbolFlags::ALIAS,
                Some(node),
            ) else {
                // **Upstream's ladder entry condition is a name that did not
                // resolve**, and this early return is exactly that. The
                // primitive-export rung lives in the ladder, which is entered
                // from `check_value_identifier` — a path `is_value_reference`
                // never admits an export specifier's name to, and §841 measured
                // what widening that allow-list costs (−7 cases, +10 wrong
                // lines). The position is reached from the specifier instead.
                // §844.
                self.report_exporting_primitive_type(named, text);
                return;
            };
            let symbol = self.binder.merged_symbol(symbol);
            // **The specifier is its own answer.** The binder gives
            // `export { X }` a symbol named `X`, so `resolve_name` finds this
            // very node and `declarations.first()` is the `ExportSpecifier` —
            // whose container is never a script. §713 recorded the mechanism
            // for TS2552; upstream's `resolveEntityName` skips the specifier's
            // own symbol and this port's lookup does not, so the skip goes
            // here. §765.
            let Some(&first) = self.binder.symbols().get(symbol).declarations.first() else {
                return;
            };
            self.declaration_container_is_a_script(first)
        };
        if !global {
            return;
        }
        self.report_at(
            named,
            &messages::CANNOT_EXPORT_0_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
            text,
        );
    }

    /// `IsGlobalSourceFile(GetDeclarationContainer(declaration))` — the walk
    /// reaches a `SourceFile` that is not an external module, without passing a
    /// namespace or a function on the way. §375.
    fn declaration_container_is_a_script(&self, declaration: NodeId) -> bool {
        for ancestor in self.nodes.ancestors(declaration) {
            match self.node_map.get(ancestor) {
                Some(Node::SourceFile(source)) => {
                    return !tsr_binder::is_external_module(source);
                }
                Some(Node::ModuleDeclaration(_)) => return false,
                _ => {}
            }
            if self.is_function_like_or_static_block(ancestor) {
                return false;
            }
        }
        false
    }

    /// `checkAndReportErrorForExportingPrimitiveType` (`checker.go:1629`).
    pub(crate) fn report_exporting_primitive_type(&mut self, node: NodeId, text: &str) -> bool {
        if !is_primitive_type_name(text) {
            return false;
        }
        if self
            .nodes
            .parent(node)
            .is_none_or(|parent| self.nodes.kind(parent) != SyntaxKind::ExportSpecifier)
        {
            return false;
        }
        self.report_at(
            node,
            &messages::CANNOT_EXPORT_0_ONLY_LOCAL_DECLARATIONS_CAN_BE_EXPORTED_FROM_A_MODULE,
            text,
        );
        true
    }

    /// The primitive-name branch of `checkAndReportErrorForUsingTypeAsValue`
    /// (`checker.go:1664-1678`): three dedicated heritage messages, then
    /// TS2693.
    /// [`Self::report_primitive_type_as_value`] reached directly from the
    /// value path's primitive-keyword decline, which returns before the ladder.
    /// §880.
    pub(crate) fn report_primitive_type_as_value_at(&mut self, node: NodeId, text: &str) {
        self.report_primitive_type_as_value(node, text);
    }

    fn report_primitive_type_as_value(&mut self, node: NodeId, text: &str) {
        let grandparent = self.nodes.parent(node).and_then(|parent| self.nodes.parent(parent));
        let heritage = grandparent.filter(|g| self.nodes.kind(*g) == SyntaxKind::HeritageClause);
        if let Some(clause) = heritage
            && let Some(owner) = self.nodes.parent(clause)
        {
            let extends = matches!(
                self.node_map.get(clause),
                Some(Node::HeritageClause(c)) if c.token.kind == SyntaxKind::ExtendsKeyword
            );
            let owner_kind = self.nodes.kind(owner);
            let class_like =
                matches!(owner_kind, SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression);
            let message = if owner_kind == SyntaxKind::InterfaceDeclaration && extends {
                Some(&messages::AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_0_IT_CAN_ONLY_EXTEND_OTHER_NAMED_OBJECT_TYPES)
            } else if class_like && extends {
                Some(&messages::A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_0_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABLE_VALUES)
            } else if class_like {
                Some(&messages::A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_0_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED_OBJECT_TYPES)
            } else {
                None
            };
            if let Some(message) = message {
                self.report_at(node, message, text);
            }
            // Upstream's `else` inside the heritage branch reports nothing and
            // still returns `true`. Faithful, and why this is not a fallthrough.
            return;
        }
        self.report_at(
            node,
            &messages::_0_ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            text,
        );
    }

    /// The syntactic half of `maybeMappedType` (`checker.go:1707`).
    fn maybe_mapped_type_position(&self, node: NodeId) -> bool {
        let mut at = node;
        loop {
            let Some(parent) = self.nodes.parent(at) else { return false };
            if !matches!(
                self.nodes.kind(parent),
                SyntaxKind::ComputedPropertyName | SyntaxKind::PropertySignature
            ) {
                return self.nodes.kind(parent) == SyntaxKind::TypeLiteral;
            }
            at = parent;
        }
    }

    /// `isExportAssignmentExpressionName` (`checker/utilities.go:144`).
    fn is_export_assignment_expression_name(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if !matches!(
                self.nodes.kind(parent),
                SyntaxKind::PropertyAccessExpression | SyntaxKind::QualifiedName
            ) {
                break;
            }
            current = parent;
        }
        let Some(parent) = self.nodes.parent(current) else { return false };
        matches!(
            self.node_map.get(parent),
            Some(Node::ExportAssignment(assignment))
                if assignment.expression.and_then(|e| e.node_id()) == Some(current)
        )
    }
}
