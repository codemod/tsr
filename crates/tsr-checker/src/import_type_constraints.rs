//! TS2344 on the type arguments of an import type: `import("./m").Foo<T>`
//! where `Foo`'s type parameter has a constraint `T` does not satisfy.
//!
//! `checkImportType` (`checker.go:3324`) ends in `checkTypeReferenceOrImport`
//! (`:2998`), the same tail a type reference runs: when the node has type
//! arguments and its resolved symbol has type parameters,
//! `checkTypeArgumentConstraints` (`:3016`) relates each argument to its
//! instantiated constraint. The symbol is the one `getTypeFromImportTypeNode`
//! (`:24575`) resolves:
//!
//! 1. the argument's module (`resolveExternalModuleName`), with its
//!    `export =` followed (`resolveExternalModuleSymbol`);
//! 2. the qualifier's identifiers, leftmost first, each looked up in the
//!    previous symbol's exports (`getSymbol(getExportsOfSymbol(…))`, with
//!    `SymbolFlagsNamespace` for every segment but the last and
//!    `SymbolFlagsType` for the last);
//! 3. `resolveImportSymbolType`: `resolveSymbol` on the result.
//!
//! The constraint check itself is the type-reference arm's
//! (`check_type_argument_constraints_of`, `constraints.rs`), with its gates.
//!
//! Declined, each where this port does not answer what upstream does:
//!
//! - `typeof import(…)`: its symbol is a value, and its arguments are an
//!   instantiation expression's (TS2635), not a reference's;
//! - a qualifier-less import, whose symbol is the module;
//! - `checkTypeReferenceOrImport`'s `!isErrorType(t)` test: TSR's
//!   `get_type_from_import_type_node` answers the gap for every import type
//!   with type arguments (`declared.rs`), so the test is replaced by the
//!   resolution above, which fails exactly where upstream's type is an error
//!   from a missing module or member (TS2307, TS2694).
//!
//! No cache or side table: the walk reads export tables, and the constraint
//! relation goes through the relation cache.
//!
//! `docs/parity/notes/r6-smallcodes5.md` §2.4.

use tsr_ast::{EntityName, Node, NodeId, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// `checkTypeReferenceOrImport`'s constraint arm for one import type:
    /// the resolved class, interface or alias and the written arguments
    /// `checkTypeArgumentConstraints` relates, or `None` where it does not
    /// run.
    pub(crate) fn import_type_constraint_target(
        &mut self,
        node: NodeId,
    ) -> Option<(SymbolId, &'a [TypeNode<'a>])> {
        let Some(Node::ImportTypeNode(import)) = self.node_map.get(node) else { return None };
        if import.is_type_of || import.type_arguments.is_empty() {
            return None;
        }
        let symbol = self.import_type_target_symbol(node)?;
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::TYPE_ALIAS | SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            .then_some((symbol, import.type_arguments))
    }

    /// The symbol `getTypeFromImportTypeNode` resolves for a qualified,
    /// non-`typeof` import type, or `None` where it resolves none.
    fn import_type_target_symbol(&mut self, node: NodeId) -> Option<SymbolId> {
        let Some(Node::ImportTypeNode(import)) = self.node_map.get(node) else { return None };
        let Some(TypeNode::LiteralTypeNode(literal)) = import.argument else { return None };
        let specifier = literal.literal.and_then(|l| l.node_id())?;
        let module = self.resolve_external_module_name(node, specifier)?;
        let module = self.resolve_external_module_symbol(module);
        // `getIdentifierChain(qualifier)`, leftmost first.
        let mut chain: Vec<&str> = Vec::new();
        let mut current = import.qualifier?;
        loop {
            match current {
                EntityName::Identifier(name) => {
                    chain.push(name.text);
                    break;
                }
                EntityName::QualifiedName(qualified) => {
                    chain.push(qualified.right?.text);
                    current = qualified.left?;
                }
            }
        }
        chain.reverse();
        let mut symbol = module;
        for (index, segment) in chain.iter().enumerate() {
            let meaning =
                if index + 1 == chain.len() { SymbolFlags::TYPE } else { SymbolFlags::NAMESPACE };
            // `getMergedSymbol(resolveSymbol(currentNamespace))`.
            let namespace = self.resolve_symbol_alias_chain(symbol)?;
            let &found = self.binder.symbols().get(namespace).exports.get(*segment)?;
            let found = self.binder.merged_symbol(found);
            let resolved = self.resolve_symbol_alias_chain(found)?;
            if !self.binder.symbols().get(resolved).flags.intersects(meaning) {
                return None;
            }
            symbol = found;
        }
        self.resolve_symbol_alias_chain(symbol)
    }

    /// `getMergedSymbol(resolveSymbol(symbol))`: an alias followed to its
    /// target, `None` when the alias does not resolve.
    fn resolve_symbol_alias_chain(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let mut symbol = self.binder.merged_symbol(symbol);
        for _ in 0..8u8 {
            if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
                return Some(symbol);
            }
            symbol = self.binder.merged_symbol(self.resolve_alias(symbol)?);
        }
        None
    }
}
