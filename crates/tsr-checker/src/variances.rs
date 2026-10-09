//! Type-parameter variance measurement from `getVariancesWorker` in tsgo.

use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    relation_cache::Reliability,
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
    /// The `Unmeasurable`/`Unreliable` flags of `symbol`'s measured
    /// variances (`VarianceFlags` beyond `VarianceFlagsVarianceMask`), one
    /// per type parameter; empty flags where nothing was reported or the
    /// variances were declared or not measured.
    pub(crate) fn variance_reliability(&self, symbol: SymbolId, count: usize) -> Vec<Reliability> {
        let symbol = self.binder.merged_symbol(symbol);
        let mut flags =
            self.relation_results.variance_reliability.get(&symbol).cloned().unwrap_or_default();
        flags.resize(count, Reliability::empty());
        flags
    }

    /// `reportUnreliableWorker` / `reportUnmeasurableWorker`
    /// (`checker.go:1136`, `:1143`): native instantiates `ty` with a mapper
    /// that adds `report` to `reliabilityFlags` when it meets a variance
    /// marker, so the report fires exactly when `ty` mentions one. Markers
    /// exist only inside a variance measurement, so outside one nothing can
    /// fire and the walk is skipped.
    pub(crate) fn report_variance_markers(&mut self, ty: TypeId, report: Reliability) {
        if self.variance_in_progress.is_empty() {
            return;
        }
        let Some(markers) = self.variance_markers else {
            return;
        };
        if self.mentions_type_parameter(ty, &markers, &[]) {
            self.relation_results.reliability |= report;
        }
    }

    /// The alias bodies whose marker instantiations this port can measure:
    /// function, constructor, object-literal, mapped and union bodies, and a
    /// body written as a reference to a class, an interface, or another
    /// alias with such a body (`type T<X> = Pick<X, 'x'>`), whose
    /// instantiation getTypeAliasInstantiation builds from the referenced
    /// type. A reference that ends at any other alias (a conditional body,
    /// say) is not measured.
    fn measurable_alias_body(&self, body: Option<TypeNode<'_>>, depth: usize) -> bool {
        match body {
            Some(
                TypeNode::FunctionTypeNode(_)
                | TypeNode::ConstructorTypeNode(_)
                | TypeNode::TypeLiteralNode(_)
                | TypeNode::MappedTypeNode(_)
                | TypeNode::UnionTypeNode(_),
            ) => true,
            Some(TypeNode::TypeReferenceNode(reference)) if depth < crate::relater::MAX_DEPTH => {
                let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
                    return false;
                };
                let Some(referenced) = name.node_id.and_then(|id| {
                    self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        name.text,
                        SymbolFlags::TYPE,
                    )
                }) else {
                    return false;
                };
                let referenced = self.binder.merged_symbol(referenced);
                let flags = self.binder.symbols().get(referenced).flags;
                if flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
                    return true;
                }
                flags.contains(SymbolFlags::TYPE_ALIAS)
                    && self.measurable_alias_body(self.type_alias_body(referenced), depth + 1)
            }
            _ => false,
        }
    }

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
        // Keep the written-alias body boundary: invalid in/out on a reference
        // alias must not override its ordinary relation. JSDoc declarations can
        // carry valid in/out even when their body cannot be measured here.
        if alias
            && self.binder.symbols().get(symbol).declarations.iter().any(|id| {
                matches!(self.node_map.get(*id), Some(Node::TypeAliasDeclaration(_)))
            })
            && !self.binder.symbols().get(symbol).declarations.iter().any(|&id| {
                matches!(self.node_map.get(id), Some(Node::TypeAliasDeclaration(node)) if self.measurable_alias_body(node.r#type, 0))
            })
        {
            self.variance_cache.insert(symbol, None);
            return None;
        }
        let markers = if let Some(markers) = self.variance_markers {
            markers
        } else {
            let markers = ["__varianceSuper", "__varianceSub", "__varianceOther"]
                .map(|name| self.store.new_named(TypeFlags::TYPE_PARAMETER, name.to_owned(), None));
            self.variance_markers = Some(markers);
            markers
        };
        // getVariancesWorker (`relater.go:1358`): the outermost variance
        // computation searches resolution cycles only from its own depth.
        let saved_resolution_start =
            self.variance_in_progress.is_empty().then(|| self.resolutions.reset_start());
        self.variance_in_progress.insert(symbol);
        let mut result = Some(Vec::with_capacity(parameters.len()));
        let mut reliability = Vec::with_capacity(parameters.len());
        for (index, declaration) in declarations.iter().enumerate() {
            // getVariancesWorker (relater.go:1378): each measured parameter
            // collects its own reports, and the enclosing comparison's are
            // restored after it.
            let saved_reliability = std::mem::take(&mut self.relation_results.reliability);
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
            let reported =
                std::mem::replace(&mut self.relation_results.reliability, saved_reliability);
            let Some(variance) = variance else {
                result = None;
                break;
            };
            // Declared `in`/`out` modifiers are not measured, so they report
            // nothing (the `default` arm only).
            reliability.push(if input || output { Reliability::empty() } else { reported });
            result.as_mut().unwrap().push(variance);
        }
        if result.is_some() && reliability.iter().any(|flags| !flags.is_empty()) {
            self.relation_results.variance_reliability.insert(symbol, reliability);
        }
        self.variance_in_progress.remove(&symbol);
        if let Some(saved) = saved_resolution_start {
            self.resolutions.restore_start(saved);
        }
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
        let result = if alias {
            let body = self.type_alias_body(symbol)?;
            // A mapped body is minted as the same alias image a written
            // `Required<T>` reference gets, which the relater resolves as a
            // mapped type (getTypeAliasInstantiation, `relater.go:1421`).
            if matches!(body, TypeNode::TypeLiteralNode(_) | TypeNode::MappedTypeNode(_)) {
                self.create_type_reference(symbol, arguments)
            } else if matches!(
                body,
                TypeNode::FunctionTypeNode(_) | TypeNode::ConstructorTypeNode(_)
            ) {
                let owns_resolution = self.variadic_alias_in_progress.insert(symbol);
                let declared = self.get_type_from_type_node(body);
                if owns_resolution {
                    self.variadic_alias_in_progress.remove(&symbol);
                }
                let own: Vec<_> = parameters.iter().map(|(parameter, _)| *parameter).collect();
                let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
                let map: Vec<_> = own.iter().copied().zip(arguments).collect();
                self.instantiate_type(declared, &map, &own, &names)
            } else if matches!(body, TypeNode::UnionTypeNode(_) | TypeNode::TypeReferenceNode(_)) {
                // createMarkerType's alias arm (`relater.go:1420`) is
                // getTypeAliasInstantiation: the instantiated union body. The
                // relater relates two instantiations of a union alias body to
                // body (its alias-variance gate, `relater.go:3392`), so the
                // markers measure the members' directions (tsr-2zk.927). A
                // body written as a type reference (`type T<X> = Pick<X,
                // 'x'>`) is instantiated the same way, so the referenced
                // type's own variances measure it, as natively; left
                // unmeasured, its failures would fall back structurally
                // (`docs/parity/notes/r6-relater.md` §2).
                self.evaluate_alias_body(symbol, &arguments)?
            } else {
                return None;
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
        measured_in_file(source, name, "variance.ts")
    }

    fn measured_in_file(source: &str, name: &str, file: &str) -> Option<Vec<Variance>> {
        let arena = Arena::new();
        let mut parsed = tsr_parser::parse_with_options(
            &arena,
            source,
            tsr_parser::ParseOptions::for_file(file),
        );
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.expect("source file");
        if std::path::Path::new(file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
        {
            parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        }
        let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
        let bound = tsr_binder::bind_into_with_jsdoc(
            tsr_binder::BindResult::empty(),
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: file, text: source },
            &jsdoc,
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_jsdoc(parsed.jsdoc.iter());
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
                tsr_ast::Statement::InterfaceDeclaration(interface)
                    if interface.name.is_some_and(|id| id.text == name) =>
                {
                    interface.node_id.and_then(|id| bound.symbol_of(id))
                }
                _ => None,
            })
            .or_else(|| bound.lookup_local(root, name))
            .expect("declared generic type");
        let result = checker.inference_variances(symbol);
        assert!(checker.variance_in_progress.is_empty());
        assert_eq!(checker.inference_variances(symbol), result, "cached variance");
        result
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

    #[test]
    fn circular_occurrences_do_not_obscure_witnessed_variance() {
        for source in [
            "type Foo<T> = { x: T; y: Foo<(arg: T) => void> };",
            "interface Foo<T> { x: T; y: Foo<(arg: T) => void> }",
        ] {
            assert_eq!(measured(source, "Foo"), Some(vec![Variance::Covariant]), "{source}");
        }
        for source in [
            "type Foo<T> = { x: T; y: { x: (arg: T) => void; y: Foo<(arg: T) => void> } };",
            "interface Foo<T> { x: T; y: { x: (arg: T) => void; y: Foo<(arg: T) => void> } }",
        ] {
            assert_eq!(measured(source, "Foo"), Some(vec![Variance::Invariant]), "{source}");
        }
    }

    #[test]
    fn exclusively_circular_occurrences_are_independent() {
        assert_eq!(
            measured("interface Foo<T> { next: Foo<T[]> }", "Foo"),
            Some(vec![Variance::Independent])
        );
    }

    #[test]
    fn circular_callable_members_preserve_input_and_output_directions() {
        assert_eq!(
            measured("interface Fn<A, B> { (a: A): B; then<C>(next: Fn<B, C>): Fn<A, C> }", "Fn"),
            Some(vec![Variance::Contravariant, Variance::Covariant])
        );
    }

    #[test]
    fn jsdoc_declared_variance_does_not_require_body_measurement() {
        assert_eq!(
            measured_in_file(
                "/** @template in I, out O, in out T @typedef {(value: I) => O} Op */ ;",
                "Op",
                "variance.js"
            ),
            Some(vec![Variance::Contravariant, Variance::Covariant, Variance::Invariant])
        );
    }

    #[test]
    fn jsdoc_supported_objects_measure_each_direction_and_independence() {
        for (source, expected) in [
            (
                "/** @template I, O @typedef {{ accept: (value: I) => O }} Op */ ;",
                vec![Variance::Contravariant, Variance::Covariant],
            ),
            (
                "/** @template T @typedef {Object} Op @property {T} value */ ;",
                vec![Variance::Covariant],
            ),
            (
                "/** @template T @typedef {Object} Op @property {(value: T) => void} accept */ ;",
                vec![Variance::Contravariant],
            ),
            ("/** @template T @typedef {{ tag: string }} Op */ ;", vec![Variance::Independent]),
        ] {
            assert_eq!(measured_in_file(source, "Op", "variance.js"), Some(expected), "{source}");
        }
    }

    #[test]
    fn jsdoc_function_alias_measures_through_its_reparsed_body() {
        // reparseUnhosted gives the alias its written function type, so the
        // marker instantiation measures it like `type Op<T> = (value: T) => void`.
        assert_eq!(
            measured_in_file(
                "/** @template T @typedef {(value: T) => void} Op */ ;",
                "Op",
                "variance.js"
            ),
            Some(vec![Variance::Contravariant])
        );
    }

    #[test]
    fn a_union_alias_measures_its_members() {
        assert_eq!(
            measured("type Func2<T> = ((x: T) => void) | undefined;", "Func2"),
            Some(vec![Variance::Contravariant])
        );
        assert_eq!(
            measured("type R<T> = { value: T | undefined } | undefined;", "R"),
            Some(vec![Variance::Covariant])
        );
    }

    /// `Op<X>` instantiates `Box`'s object literal with the alias `Op`
    /// (getTypeAliasInstantiation), so its written `in out` is valid and
    /// declares it invariant: tsgo reports TS2322 both ways between `Op<1>`
    /// and `Op<1 | 2>`, and no TS2637 (`docs/parity/notes/r6-relater.md` §2).
    #[test]
    fn written_variance_on_a_reference_to_an_object_alias_is_declared() {
        assert_eq!(
            measured("type Box<T> = { value: T }; type Op<in out T> = Box<T>;", "Op"),
            Some(vec![Variance::Invariant])
        );
    }
}
