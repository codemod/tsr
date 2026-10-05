// Append to inference.rs only in a frozen private archive. No production patch.
#[cfg(test)]
mod mapper_lifetime_audit {
    use super::{InferenceFlags, InferencePriority, add_directional_candidate};
    use crate::{Checker, flags::TypeFlags};

    macro_rules! checker {
        ($checker:ident, $parsed:ident, $bound:ident, $source:expr) => {
            let arena = tsr_core::Arena::new();
            let source = $source;
            let $parsed = tsr_parser::parse(&arena, source);
            assert!($parsed.diagnostics.is_empty());
            let $bound = tsr_binder::bind(
                &arena,
                $parsed.source_file,
                &$parsed.nodes,
                tsr_binder::FileInfo { name: "mapper.ts", text: source },
            );
            let mut $checker = Checker::new(&$bound, &$parsed.nodes, &$parsed.node_map);
            $checker.set_strict_null_checks(true);
        };
    }

    macro_rules! signature {
        ($checker:ident, $parsed:ident, $bound:ident, $index:expr) => {{
            let tsr_ast::Statement::FunctionDeclaration(node) =
                $parsed.source_file.statements[$index]
            else {
                panic!("function");
            };
            $checker
                .get_signatures_of_symbol($bound.symbol_of(node.node_id.unwrap()).unwrap())
                .unwrap()
                .remove(0)
        }};
    }

    #[test]
    fn ordered_pairs_and_first_duplicate_source() {
        checker!(c, parsed, bound, "");
        let t = c.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
        let u = c.store.new_named(TypeFlags::TYPE_PARAMETER, "U".into(), None);
        let s = c.intrinsics.string;
        let n = c.intrinsics.number;
        let first = [(t, s), (u, n)];
        let reordered = [(u, n), (t, s)];
        assert_ne!(first, reordered);
        assert_eq!(c.instantiate_type(t, &first, &[t, u], &["T", "U"]), s);
        assert_eq!(c.instantiate_type(t, &reordered, &[t, u], &["T", "U"]), s);
        assert_eq!(c.instantiate_type(t, &[(t, s), (t, n)], &[t], &["T"]), s);
        assert_eq!(c.instantiate_type(t, &[(t, n), (t, s)], &[t], &["T"]), n);
    }

    #[test]
    fn source_identity_and_changed_snapshot_answers() {
        checker!(c, parsed, bound, "");
        let left = c.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
        let right = c.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
        assert_ne!(left, right);
        let s = c.intrinsics.string;
        let n = c.intrinsics.number;
        let mut map = [(left, s), (right, n)];
        assert_eq!(c.instantiate_type(left, &map, &[left, right], &["T", "T"]), s);
        assert_eq!(c.instantiate_type(right, &map, &[left, right], &["T", "T"]), n);
        map[0].1 = n;
        assert_eq!(c.instantiate_type(left, &map, &[left, right], &["T", "T"]), n);
        // The direct lookup runs before the recursive worker/count guard.
        assert_eq!(c.instantiation_count, 0);
    }

    #[test]
    fn direct_merge_is_not_recursive_image_composition() {
        checker!(
            c,
            parsed,
            bound,
            "interface Box<X> { value: X } declare function f<T,U>(x: Box<U>): T;"
        );
        let sig = signature!(c, parsed, bound, 1);
        let p = c.type_parameter_types(&sig).unwrap();
        let box_u = sig.parameters[0].r#type;
        let s = c.intrinsics.string;
        let merged = c.instantiate_type(p[0], &[(p[0], box_u), (p[1], s)], &p, &["T", "U"]);
        assert_eq!(merged, box_u);
        let composed = c.instantiate_type(merged, &[(p[1], s)], &p, &["T", "U"]);
        assert_ne!(composed, merged);
        assert_eq!(c.type_reference_targets[&composed].1, vec![s]);
        assert_eq!(c.type_reference_targets[&merged].1, vec![p[1]]);
    }

    #[test]
    fn fresh_signature_parameters_precede_outer_sources() {
        checker!(c, parsed, bound, "declare function f<T>(x: T): T;");
        let sig = signature!(c, parsed, bound, 0);
        let own = c.type_parameter_types(&sig).unwrap();
        let s = c.intrinsics.string;
        let a = c
            .instantiate_signature_with_fresh_parameters(sig.clone(), &[(own[0], s)], &own, &["T"])
            .unwrap();
        let b = c
            .instantiate_signature_with_fresh_parameters(sig, &[(own[0], s)], &own, &["T"])
            .unwrap();
        let fresh_a = a.type_parameters[0].resolved_type.unwrap();
        let fresh_b = b.type_parameters[0].resolved_type.unwrap();
        assert_ne!(fresh_a, fresh_b);
        assert_ne!(fresh_a, own[0]);
        assert_ne!(fresh_a, s);
        assert_eq!(a.parameters[0].r#type, fresh_a);
        assert_eq!(a.r#type, fresh_a);
        let carried = &c.instantiated_type_parameters[&fresh_a];
        assert_eq!(carried.target, own[0]);
        assert_eq!(carried.map, vec![(own[0], fresh_a), (own[0], s)]);
    }

    #[test]
    fn nonfixing_candidates_change_but_fixed_answers_survive() {
        checker!(c, parsed, bound, "declare function f<T>(x: T): T;");
        let sig = signature!(c, parsed, bound, 0);
        let p = c.type_parameter_types(&sig).unwrap();
        let s = c.intrinsics.string;
        let n = c.intrinsics.number;
        let mut infos = Vec::new();
        add_directional_candidate(&mut infos, p[0], s, false, InferencePriority::NONE);
        assert_eq!(
            c.resolved_inference_map(&infos, &sig, &p, InferenceFlags::NONE),
            Some(vec![(p[0], s)])
        );
        infos[0].candidates = vec![n];
        assert_eq!(
            c.resolved_inference_map(&infos, &sig, &p, InferenceFlags::NONE),
            Some(vec![(p[0], n)])
        );
        infos[0].is_fixed = true;
        infos[0].fixed_type = Some(s);
        let before = infos[0].candidates.clone();
        add_directional_candidate(&mut infos, p[0], s, false, InferencePriority::NONE);
        assert_eq!(infos[0].candidates, before);
        assert_eq!(
            c.resolved_inference_map(&infos, &sig, &p, InferenceFlags::NONE),
            Some(vec![(p[0], s)])
        );
    }

    #[test]
    fn persistent_object_keys_keep_ordered_snapshot_context() {
        checker!(c, parsed, bound, "declare function f<T,U>(x: {value: T}): U;");
        let sig = signature!(c, parsed, bound, 0);
        let parameters = c.type_parameter_types(&sig).unwrap();
        let id = sig.parameters[0].r#type;
        let string = c.intrinsics.string;
        let number = c.intrinsics.number;
        let map = [(parameters[0], string), (parameters[1], number)];
        let reversed = [(parameters[1], number), (parameters[0], string)];
        let first = c.instantiate_type(id, &map, &parameters, &["T", "U"]);
        assert_ne!(first, c.intrinsics.error);
        let entries = c.instantiated_objects.len();
        assert_eq!(c.instantiate_type(id, &map, &parameters, &["T", "U"]), first);
        assert_eq!(c.instantiated_objects.len(), entries);
        let reordered_image = c.instantiate_type(id, &reversed, &parameters, &["T", "U"]);
        assert_ne!(reordered_image, c.intrinsics.error);
        assert_eq!(c.type_to_string(first), c.type_to_string(reordered_image));
        assert_ne!(first, reordered_image);
        assert!(c.instantiated_objects.contains_key(&(id, map.to_vec())));
        assert!(c.instantiated_objects.contains_key(&(id, reversed.to_vec())));
    }

    #[test]
    fn print_mode_result_can_outlive_its_operation() {
        checker!(c, parsed, bound, "declare function f<T,U>(x: {value: T; other: U}): U;");
        let sig = signature!(c, parsed, bound, 0);
        let p = c.type_parameter_types(&sig).unwrap();
        let id = sig.parameters[0].r#type;
        let map = [(p[0], c.intrinsics.string)];
        // This characterizes a current key gap; it is not a desired-behavior test.
        let cold = c.instantiate_type(id, &map, &p, &["T", "U"]);
        assert_eq!(cold, c.intrinsics.error);
        c.instantiated_objects.remove(&(id, map.to_vec()));
        c.identity_unmapped_type_parameters = true;
        let printed = c.instantiate_type(id, &map, &p, &["T", "U"]);
        assert_ne!(printed, c.intrinsics.error);
        c.identity_unmapped_type_parameters = false;
        let warm = c.instantiate_type(id, &map, &p, &["T", "U"]);
        assert_eq!(warm, printed);
        assert_ne!(warm, cold);
        c.instantiated_objects.remove(&(id, map.to_vec()));
        assert_eq!(c.instantiate_type(id, &map, &p, &["T", "U"]), cold);
    }

    #[test]
    fn stored_signature_composition_maps_previous_images() {
        checker!(c, parsed, bound, "declare function f<T,U>(x: T): T;");
        let mut sig = signature!(c, parsed, bound, 0);
        let p = c.type_parameter_types(&sig).unwrap();
        // Model an anonymous callable capturing outer parameters, not owning them.
        sig.type_parameters.clear();
        let owner = bound.symbol_of(sig.declaration).unwrap();
        let id = c.store.new_anonymous(TypeFlags::OBJECT, "(x: T) => T".into(), owner, true);
        c.signature_types.insert(id, vec![sig]);
        let first = c.instantiate_type(id, &[(p[0], p[1])], &p, &["T", "U"]);
        assert_ne!(first, c.intrinsics.error);
        assert_eq!(c.instantiated_signature_mappers[&first], vec![(p[0], p[1])]);
        let s = c.intrinsics.string;
        let second = c.instantiate_type(first, &[(p[1], s)], &p, &["T", "U"]);
        assert_ne!(second, c.intrinsics.error);
        assert_eq!(c.signature_types[&second][0].parameters[0].r#type, s);
        assert_eq!(c.instantiated_signature_mappers[&second], vec![(p[0], s)]);
        assert!(c.instantiated_signatures.contains_key(&(first, vec![(p[1], s)])));
    }
}
