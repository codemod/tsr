//! `getTypeParametersFromDeclaration` (`checker.go:19912`): a signature's
//! type parameters are appended with `core.AppendIfUnique` over
//! `getDeclaredTypeOfTypeParameter(node.Symbol())`.
//!
//! `SymbolFlagsTypeParameterExcludes` is `SymbolFlagsType &^
//! SymbolFlagsTypeParameter` (`ast/symbolflags.go:72`), so two type
//! parameters of one declaration with the same name do not conflict in the
//! binder: they merge into one symbol, the checker reports the duplicate,
//! and the list holds the parameter once. `function f<T, T>() { }` is
//! `<T>() => void` (`typesWithDuplicateTypeParameters`). Classes and
//! interfaces already dedupe through `local_type_parameters_of`; this is the
//! signature half. `docs/parity/notes/r6-typesroots.md` §5.

use tsr_ast::TypeParameterDeclaration;

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// `nodes` keeping the first declaration of each merged type-parameter
    /// symbol, in order. A node the binder did not bind is kept.
    pub(crate) fn unique_type_parameter_declarations(
        &self,
        nodes: Vec<&'a TypeParameterDeclaration<'a>>,
    ) -> Vec<&'a TypeParameterDeclaration<'a>> {
        if nodes.len() < 2 {
            return nodes;
        }
        let mut seen = Vec::with_capacity(nodes.len());
        let mut out = Vec::with_capacity(nodes.len());
        for node in nodes {
            let symbol = node
                .node_id
                .and_then(|id| self.binder.symbol_of(id))
                .map(|symbol| self.binder.merged_symbol(symbol));
            if let Some(symbol) = symbol {
                if seen.contains(&symbol) {
                    continue;
                }
                seen.push(symbol);
            }
            out.push(node);
        }
        out
    }
}
