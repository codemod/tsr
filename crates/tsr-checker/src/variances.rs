//! Type-parameter variance measurement from `getVariancesWorker` in tsgo.

use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    types::TypeId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variance {
    Invariant,
    Covariant,
    Contravariant,
    Bivariant,
    Independent,
}

impl Checker<'_, '_> {
    /// Compare instantiations with related markers, then an unrelated marker
    /// to distinguish bivariance from an unwitnessed type parameter. An
    /// unsupported comparison leaves variance unmeasured.
    pub(crate) fn inference_variances(&mut self, symbol: SymbolId) -> Option<Vec<Variance>> {
        let symbol = self.binder.merged_symbol(symbol);
        if ["Array", "ReadonlyArray"]
            .iter()
            .any(|name| self.global_type_symbol(name) == Some(symbol))
        {
            return Some(vec![Variance::Covariant]);
        }
        if let Some(variance) = self.variance_cache.get(&symbol) {
            return variance.clone();
        }
        if self.variance_in_progress.contains(&symbol) {
            return Some(Vec::new());
        }
        let declarations = self.local_type_parameters_of(symbol).to_vec();
        let parameters = self.local_type_parameter_types_of(symbol)?;
        let alias = self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS);
        // A named reference to an unsupported alias body has no reliable
        // structural representation to compare.
        if alias && !self.binder.symbols().get(symbol).declarations.iter().any(|id| {
            matches!(self.node_map.get(*id), Some(Node::TypeAliasDeclaration(node)) if matches!(node.r#type,
                Some(TypeNode::FunctionTypeNode(_) | TypeNode::ConstructorTypeNode(_) | TypeNode::TypeLiteralNode(_))))
        }) {
            self.variance_cache.insert(symbol, None);
            return None;
        }
        let markers = if let Some(markers) = self.variance_markers {
            markers
        } else {
            let markers = ["__varianceSuper", "__varianceSub", "__varianceOther"]
                .map(|name| self.store.new_named(TypeFlags::OBJECT, name.to_owned(), None));
            self.variance_markers = Some(markers);
            markers
        };
        self.variance_in_progress.insert(symbol);
        let mut result = Some(Vec::with_capacity(parameters.len()));
        for (index, declaration) in declarations.iter().enumerate() {
            let input = declaration.modifiers.iter().any(|modifier| matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::InKeyword));
            let output = declaration.modifiers.iter().any(|modifier| matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == SyntaxKind::OutKeyword));
            let variance = match (input, output) {
                (true, true) => Some(Variance::Invariant),
                (true, false) => Some(Variance::Contravariant),
                (false, true) => Some(Variance::Covariant),
                (false, false) => {
                    let sup = self.create_variance_marker_type(
                        symbol,
                        &parameters,
                        index,
                        markers[0],
                        alias,
                    );
                    let sub = self.create_variance_marker_type(
                        symbol,
                        &parameters,
                        index,
                        markers[1],
                        alias,
                    );
                    match (sup, sub) {
                        (Some(sup), Some(sub)) => {
                            let covariant = self.relate_ternary(sub, sup, Relation::Assignable);
                            let contravariant = self.relate_ternary(sup, sub, Relation::Assignable);
                            match (covariant, contravariant) {
                                (Ternary::Unknown, _) | (_, Ternary::Unknown) => None,
                                (Ternary::Related, Ternary::NotRelated) => {
                                    Some(Variance::Covariant)
                                }
                                (Ternary::NotRelated, Ternary::Related) => {
                                    Some(Variance::Contravariant)
                                }
                                (Ternary::NotRelated, Ternary::NotRelated) => {
                                    Some(Variance::Invariant)
                                }
                                (Ternary::Related, Ternary::Related) => self
                                    .create_variance_marker_type(
                                        symbol,
                                        &parameters,
                                        index,
                                        markers[2],
                                        alias,
                                    )
                                    .and_then(|other| {
                                        match self.relate_ternary(other, sup, Relation::Assignable)
                                        {
                                            Ternary::Related => Some(Variance::Independent),
                                            Ternary::NotRelated => Some(Variance::Bivariant),
                                            Ternary::Unknown => None,
                                        }
                                    }),
                            }
                        }
                        _ => None,
                    }
                }
            };
            let Some(variance) = variance else {
                result = None;
                break;
            };
            result.as_mut().unwrap().push(variance);
        }
        self.variance_in_progress.remove(&symbol);
        self.variance_cache.insert(symbol, result.clone());
        result
    }

    fn create_variance_marker_type(
        &mut self,
        symbol: SymbolId,
        parameters: &[(TypeId, String)],
        index: usize,
        marker: TypeId,
        alias: bool,
    ) -> Option<TypeId> {
        let arguments: Vec<_> = parameters
            .iter()
            .enumerate()
            .map(|(i, (parameter, _))| if i == index { marker } else { *parameter })
            .collect();
        let result =
            if alias {
                let body = self.binder.symbols().get(symbol).declarations.iter().find_map(
                    |id| match self.node_map.get(*id) {
                        Some(Node::TypeAliasDeclaration(alias)) => alias.r#type,
                        _ => None,
                    },
                )?;
                if matches!(body, TypeNode::TypeLiteralNode(_)) {
                    self.create_type_reference(symbol, arguments)
                } else {
                    let owns_resolution = self.variadic_alias_in_progress.insert(symbol);
                    let declared = self.get_type_from_type_node(body);
                    if owns_resolution {
                        self.variadic_alias_in_progress.remove(&symbol);
                    }
                    let own: Vec<_> = parameters.iter().map(|(parameter, _)| *parameter).collect();
                    let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
                    let map: Vec<_> = own.iter().copied().zip(arguments).collect();
                    self.instantiate_type(declared, &map, &own, &names)
                }
            } else {
                self.create_type_reference(symbol, arguments)
            };
        if self.is_error(result) {
            return None;
        }
        self.variance_marker_types.insert(result);
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::Arena;

    fn measured(source: &str, name: &str) -> Option<Vec<Variance>> {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "variance.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let symbol = parsed
            .source_file
            .statements
            .iter()
            .find_map(|statement| match statement {
                tsr_ast::Statement::TypeAliasDeclaration(alias)
                    if alias.name.is_some_and(|id| id.text == name) =>
                {
                    alias.node_id.and_then(|id| bound.symbol_of(id))
                }
                _ => None,
            })
            .expect("declared type alias");
        checker.inference_variances(symbol)
    }

    #[test]
    fn a_function_alias_measures_its_input_and_output_separately() {
        assert_eq!(
            measured("type Op<I, O> = (input: I) => O;", "Op"),
            Some(vec![Variance::Contravariant, Variance::Covariant])
        );
    }

    #[test]
    fn a_function_alias_with_both_occurrences_is_invariant() {
        assert_eq!(
            measured("type Op<T> = (input: T) => T;", "Op"),
            Some(vec![Variance::Invariant])
        );
    }

    #[test]
    fn an_unused_type_parameter_is_independent() {
        assert_eq!(measured("type Op<T> = () => number;", "Op"), Some(vec![Variance::Independent]));
    }

    #[test]
    fn nested_references_preserve_a_function_alias_variance() {
        assert_eq!(
            measured(
                "type Box<T> = { value: T }; type Op<I, O> = (input: Box<I>) => Box<O>;",
                "Op"
            ),
            Some(vec![Variance::Contravariant, Variance::Covariant])
        );
    }

    #[test]
    fn a_method_input_is_bivariant() {
        assert_eq!(
            measured("type Sink<T> = { accept(input: T): void };", "Sink"),
            Some(vec![Variance::Bivariant])
        );
    }

    #[test]
    fn declared_variance_modifiers_take_precedence() {
        assert_eq!(
            measured(
                "type Declared<in I, out O, in out T> = { input: I; output: O; value: T };",
                "Declared"
            ),
            Some(vec![Variance::Contravariant, Variance::Covariant, Variance::Invariant])
        );
    }

    #[test]
    fn a_recursive_alias_with_generic_methods_can_be_measured() {
        let source = "type Op<I, O> = (thing: Thing<I>) => Thing<O>;
            type Thing<T> = { value: T; pipe<A, B>(opA: Op<T, A>, opB: Op<A, B>): Thing<B> };";
        assert_eq!(
            measured(source, "Op"),
            Some(vec![Variance::Contravariant, Variance::Covariant])
        );
    }
}
