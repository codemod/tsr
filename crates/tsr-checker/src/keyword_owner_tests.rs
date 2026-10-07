use super::*;

fn snapshot(checker: &Checker<'_, '_>) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        checker.store,
        checker.declared_types,
        checker.instantiations,
        checker.alias_body_evaluations,
        checker.alias_evaluated_types,
        checker.type_reference_targets,
        checker.alias_evaluation_bindings,
        checker.type_parameter_symbols,
        checker.resolutions,
        checker.diagnostics,
        checker.mapped_template_depth
    )
}

#[test]
fn cold_keyword_references_complete_the_original_owner_with_shared_identity() {
    let source = "type A<F> = boolean; type B<F> = string; type C<F> = number; type D<F> = any; type E<F> = unknown; type F<T> = never; type G<F> = object; type H<F> = bigint; type I<F> = symbol; type J<F> = void; type K<F> = undefined;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "owner-reference.ts", text: source },
    );
    for reverse in [false, true] {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut controls = [
            ("A", checker.intrinsics.boolean),
            ("B", checker.intrinsics.string),
            ("C", checker.intrinsics.number),
            ("D", checker.intrinsics.any),
            ("E", checker.intrinsics.unknown),
            ("F", checker.intrinsics.never),
            ("G", checker.intrinsics.non_primitive),
            ("H", checker.intrinsics.bigint),
            ("I", checker.intrinsics.es_symbol),
            ("J", checker.intrinsics.void),
            ("K", checker.intrinsics.undefined),
        ];
        if reverse {
            controls.reverse();
        }
        let count = checker.store.len();
        for phase in 0..3 {
            if phase != 0 {
                controls.reverse();
            }
            for &(name, expected) in &controls {
                let owner = bound
                    .symbols()
                    .iter()
                    .find(|(_, entry)| {
                        entry.name == name && entry.flags.contains(SymbolFlags::TYPE_ALIAS)
                    })
                    .unwrap()
                    .0;
                let arguments = vec![checker.intrinsics.string];
                assert_eq!(checker.create_type_reference(owner, arguments.clone()), expected);
                assert_eq!(
                    checker.declared_types.get(&owner),
                    Some(&expected),
                    "{name}: original completion, reverse={reverse}, phase={phase}"
                );
                assert_eq!(checker.instantiations.get(&(owner, arguments)), Some(&expected));
                assert!(!checker.alias_evaluated_types.contains(&expected));
                assert!(!checker.type_reference_targets.contains_key(&expected));
                assert_eq!(checker.resolutions.depth(), 0);
            }
            assert_eq!(checker.store.len(), count, "no primitive wrapper or parameter allocation");
        }
        assert!(checker.diagnostics().is_empty());
    }
}

#[test]
fn cached_keyword_instance_still_completes_a_missing_original_owner() {
    let source = "type Inputs<First, Second = First> = string;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "cached-owner.ts", text: source },
    );
    let input = bound.symbols().iter().find(|(_, entry)| entry.name == "Inputs").unwrap().0;
    for reverse in [false, true] {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let parameter =
            bound.symbol_of(checker.local_type_parameters_of(input)[0].node_id.unwrap()).unwrap();
        let mut vectors = [
            vec![checker.intrinsics.string, checker.intrinsics.number],
            vec![checker.intrinsics.number, checker.intrinsics.string],
        ];
        if reverse {
            vectors.reverse();
        }
        checker
            .alias_evaluation_bindings
            .push([(parameter, checker.intrinsics.string)].into_iter().collect());
        for arguments in &vectors {
            assert_eq!(
                checker.create_type_reference(input, arguments.clone()),
                checker.intrinsics.string
            );
        }
        assert!(!checker.declared_types.contains_key(&input), "mapped route remains unexpanded");
        checker.alias_evaluation_bindings.pop();
        let count = checker.store.len();
        let keys = checker.instantiations.clone();
        for arguments in &vectors {
            assert_eq!(
                checker.create_type_reference(input, arguments.clone()),
                checker.intrinsics.string
            );
            assert_eq!(checker.declared_types.get(&input), Some(&checker.intrinsics.string));
        }
        assert_eq!(
            checker.instantiations, keys,
            "cache-hit publication preserves ordered keys/values"
        );
        assert_eq!(keys.len(), 2);
        assert_eq!(checker.store.len(), count);
        let completed = snapshot(&checker);
        for arguments in &vectors {
            checker.create_type_reference(input, arguments.clone());
        }
        assert_eq!(snapshot(&checker), completed, "completed hits perform no writes");
    }
}

#[test]
fn cold_string_slots_reuse_the_unchanged_reader_and_boolean_slots_still_decline() {
    let source = "function text(value: TextInputs<string>): TextInputs<string> { return value; } function truth(value: Inputs<string>): Inputs<string> { return value; } type TextInputs<F> = string; type Inputs<F> = boolean;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "owner-slots.ts", text: source },
    );
    let owner = |name| bound.symbols().iter().find(|(_, entry)| entry.name == name).unwrap().0;
    for reverse in [false, true] {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut names = ["text", "truth"];
        if reverse {
            names.reverse();
        }
        let mut identities = Vec::new();
        for phase in 0..3 {
            if phase != 0 {
                names.reverse();
            }
            for name in names {
                let function = owner(name);
                let site = bound.symbols().get(function).declarations[0];
                let ty = checker.get_type_of_symbol(function);
                let signature = checker.signature_types[&ty][0].clone();
                let expected = if name == "text" {
                    checker.intrinsics.string
                } else {
                    checker.intrinsics.boolean
                };
                let alias = owner(if name == "text" { "TextInputs" } else { "Inputs" });
                assert_eq!(checker.declared_types.get(&alias), Some(&expected));
                assert_eq!(checker.completed_callable_symbol(ty), Some(function));
                assert!(signature.target.is_none());
                assert_eq!(signature.declaration, site);
                assert_eq!(checker.parameter_type(&signature.parameters[0]), expected);
                assert_eq!(signature.r#type, expected);
                let key = checker.type_literal_key(site);
                assert!(key.is_unmapped());
                assert_eq!(checker.signature_returns.get(&key), Some(&Some(expected)));
                assert!(!checker.pending_signature_returns.contains_key(&key));
                if name == "truth" {
                    assert!(matches!(
                        checker.type_of(expected).data,
                        crate::types::TypeData::Union { symbol: None, .. }
                    ));
                    assert_eq!(
                        checker.signature_parameter_alias_text_at(
                            &signature,
                            &signature.parameters[0],
                            site
                        ),
                        None
                    );
                    assert_eq!(checker.signature_return_alias_text_at(&signature, site), None);
                }
                let printed = checker.type_to_string_at(ty, site).unwrap();
                assert_eq!(
                    printed,
                    // `serializeTypeForDeclaration` reuses the written
                    // annotation: it denotes the very type printed.
                    if name == "text" {
                        "(value: TextInputs<string>) => TextInputs<string>"
                    } else {
                        "(value: Inputs<string>) => Inputs<string>"
                    }
                );
                if phase == 0 {
                    identities.push((function, ty));
                } else {
                    assert_eq!(identities.iter().find(|(id, _)| *id == function).unwrap().1, ty);
                }
                println!(
                    "SLOT reverse={reverse} phase={phase} originalPublished=true aliasOwnerPublished=true type={printed}"
                );
            }
        }
        assert!(checker.diagnostics().is_empty());
    }
}

#[test]
fn default_filling_and_raw_arity_refusal_preserve_original_parameter_metadata() {
    let source = "function defaulted(value: Defaults): void {} function ordered(value: Ordered<number>): void {} type Defaults<F = string> = string; type Ordered<First, Second = First> = number;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "default-owner.ts", text: source },
    );
    let owner = |name| bound.symbols().iter().find(|(_, entry)| entry.name == name).unwrap().0;
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for name in ["defaulted", "ordered"] {
        let declaration = bound.symbols().get(owner(name)).declarations[0];
        let Node::FunctionDeclaration(function) = parsed.node_map.get(declaration).unwrap() else {
            panic!("function");
        };
        let result = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
        let (alias, arguments, expected) = if name == "defaulted" {
            (owner("Defaults"), vec![checker.intrinsics.string], checker.intrinsics.string)
        } else {
            (
                owner("Ordered"),
                vec![checker.intrinsics.number, checker.intrinsics.number],
                checker.intrinsics.number,
            )
        };
        assert_eq!(result, expected);
        assert_eq!(checker.instantiations.get(&(alias, arguments)), Some(&expected));
        assert_eq!(checker.declared_types.get(&alias), Some(&expected));
    }
    let parameters = checker.local_type_parameters_of(owner("Ordered"));
    assert_eq!(parameters[0].name.unwrap().text, "First");
    assert_eq!(parameters[1].name.unwrap().text, "Second");
    assert!(parameters[1].default_type.is_some());
    for arguments in [vec![], vec![checker.intrinsics.string], vec![checker.intrinsics.string; 3]] {
        let mut raw = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        assert_eq!(
            raw.create_type_reference(owner("Ordered"), arguments.clone()),
            raw.intrinsics.number
        );
        assert!(!raw.declared_types.contains_key(&owner("Ordered")));
        assert!(raw.instantiations.contains_key(&(owner("Ordered"), arguments)));
    }
}

#[test]
fn active_mapped_and_unsupported_contexts_keep_their_existing_routes() {
    let source = "type Good<F> = string; type Parenthesized<F> = ((number)); type BooleanUnion<F> = true | false; type Literal<F> = true; type Ref<F> = Good<F>; type ObjectBody<F> = { value: F }; type Conditional<F> = F extends string ? true : false; type Recursive<F> = Recursive<F>; type Marker<F> = intrinsic; function enclosing<Outer>() { type Captured<Inner> = Outer; }";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "refused-owner.ts", text: source },
    );
    let owner = |name| bound.symbols().iter().find(|(_, entry)| entry.name == name).unwrap().0;
    for context in 0..3 {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let good = owner("Good");
        match context {
            0 => {
                assert!(checker.resolutions.push(good, PropertyName::DeclaredType));
            }
            1 => {
                checker.mapped_template_depth = 1;
            }
            _ => {
                checker
                    .alias_evaluation_bindings
                    .push([(owner("Outer"), checker.intrinsics.string)].into_iter().collect());
            }
        }
        let before = snapshot(&checker);
        assert!(checker.original_generic_keyword_alias_body(good).is_none());
        assert_eq!(snapshot(&checker), before);
        assert_eq!(
            checker.create_type_reference(good, vec![checker.intrinsics.string]),
            checker.intrinsics.string
        );
        assert!(!checker.declared_types.contains_key(&good));
        match context {
            0 => {
                assert!(checker.resolutions.pop(), "no cycle marking by refusal");
            }
            1 => {
                checker.mapped_template_depth = 0;
            }
            _ => {
                checker.alias_evaluation_bindings.pop();
            }
        }
        assert!(checker.diagnostics().is_empty());
    }
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let before = snapshot(&checker);
    for name in [
        "BooleanUnion",
        "Literal",
        "Ref",
        "ObjectBody",
        "Conditional",
        "Recursive",
        "Marker",
        "Captured",
        "Outer",
        "enclosing",
    ] {
        assert!(checker.original_generic_keyword_alias_body(owner(name)).is_none(), "{name}");
    }
    assert_eq!(snapshot(&checker), before);
    let parenthesized = owner("Parenthesized");
    assert!(checker.original_generic_keyword_alias_body(parenthesized).is_some());
    let reference = checker.create_type_reference(parenthesized, vec![checker.intrinsics.string]);
    // getTypeFromTypeAliasReference: an intrinsic body carries no alias.
    assert_eq!(reference, checker.intrinsics.number);
    assert!(!checker.declared_types.contains_key(&parenthesized));
    let union =
        checker.create_type_reference(owner("BooleanUnion"), vec![checker.intrinsics.string]);
    assert_ne!(union, checker.intrinsics.boolean);
    assert_eq!(checker.type_to_string(union), "BooleanUnion<string>");
    assert!(!checker.declared_types.contains_key(&owner("BooleanUnion")));
    let input = owner("Good");
    checker.declared_types.insert(input, checker.intrinsics.error);
    let old_entry = checker.declared_types[&input];
    checker.create_type_reference(input, vec![checker.intrinsics.string]);
    assert_eq!(checker.declared_types[&input], old_entry, "existing failed entry is not rewritten");

    // A Program can expose a binder symbol whose declaration is not in this
    // private Checker's node map. A completed keyword in the owning file does
    // not certify that foreign declaration here.
    let foreign_map = tsr_ast::NodeMap::new();
    let mut foreign = Checker::new(&bound, &parsed.nodes, &foreign_map);
    let before = snapshot(&foreign);
    assert!(foreign.original_generic_keyword_alias_body(input).is_none());
    assert_eq!(snapshot(&foreign), before, "foreign preflight performs no work");
    let reference = foreign.create_type_reference(input, vec![foreign.intrinsics.string]);
    assert_ne!(reference, foreign.intrinsics.string);
    assert_eq!(foreign.type_to_string(reference), "Good<string>");
    assert!(!foreign.declared_types.contains_key(&input));
}

#[cfg(feature = "work-trace")]
#[test]
fn work_counts_distinguish_cold_cached_missing_and_completed_owner_hits() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    #[derive(Default)]
    struct Counter {
        queries: AtomicU64,
        other: AtomicU64,
    }
    impl crate::work_trace::WorkObserver for Counter {
        fn begin(&self, op: crate::work_trace::Operation, _: &[NodeId]) -> u64 {
            if op == crate::work_trace::Operation::DeclaredTypeQuery {
                self.queries.fetch_add(1, Ordering::Relaxed)
            } else {
                self.other.fetch_add(1, Ordering::Relaxed)
            }
        }
        fn end(&self, _: u64, _: bool) {}
    }
    let source = "type Inputs<F> = string;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "counted-owner.ts", text: source },
    );
    let input = bound.symbols().iter().find(|(_, entry)| entry.name == "Inputs").unwrap().0;
    for cached in [false, true] {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let observer = Arc::new(Counter::default());
        checker.set_work_observer(observer.clone());
        let arguments = vec![checker.intrinsics.string];
        checker.mapped_template_depth = 1;
        if cached {
            checker.create_type_reference(input, arguments.clone());
        }
        checker.mapped_template_depth = 0;
        assert_eq!(observer.queries.load(Ordering::Relaxed), 0);
        assert!(!checker.declared_types.contains_key(&input));
        let count = checker.store.len();
        checker.create_type_reference(input, arguments.clone());
        assert_eq!(observer.queries.load(Ordering::Relaxed), 1);
        assert_eq!(checker.declared_types.get(&input), Some(&checker.intrinsics.string));
        assert_eq!(checker.store.len(), count);
        let complete = snapshot(&checker);
        for _ in 0..3 {
            checker.create_type_reference(input, arguments.clone());
        }
        assert_eq!(snapshot(&checker), complete);
        assert_eq!(observer.queries.load(Ordering::Relaxed), 1);
        assert_eq!(observer.other.load(Ordering::Relaxed), 0);
        println!(
            "WORK cachedMissing={cached} refusedQueries=0 completionQueries=1 completedHitAdditionalQueries=0 primitiveAllocations=0"
        );
    }
}
