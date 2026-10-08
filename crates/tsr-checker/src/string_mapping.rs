//! getStringMappingType/applyTemplateStringMapping (checker.go:29223).
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

impl Checker<'_, '_> {
    pub(crate) fn is_string_mapping_alias(&self, symbol: SymbolId) -> bool {
        let selected = self.symbols.bound(symbol).expect("mapping owner belongs to this Program");
        self.is_string_mapping_alias_ref(&selected)
    }

    pub(crate) fn is_string_mapping_alias_ref(
        &self,
        symbol: &crate::symbol_access::SymbolRef,
    ) -> bool {
        if !self
            .symbols
            .view(symbol)
            .expect("mapping owner belongs to this Checker")
            .flags()
            .contains(SymbolFlags::TYPE_ALIAS)
        {
            return false;
        }
        let Some(declaration) = self
            .symbols
            .view(symbol)
            .expect("mapping owner belongs to this Checker")
            .declarations()
            .first()
            .copied()
        else {
            return false;
        };
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return false;
        };
        matches!(alias.r#type, Some(TypeNode::KeywordTypeNode(keyword))
            if keyword.kind == SyntaxKind::IntrinsicKeyword)
            && matches!(
                self.symbols.view(symbol).expect("mapping owner belongs to this Checker").name(),
                "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize"
            )
    }

    pub(crate) fn instantiate_string_mapping_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let selected = self.symbols.bound(symbol).expect("mapping owner belongs to this Program");
        self.instantiate_string_mapping_alias_ref(&selected, arguments)
    }

    pub(crate) fn instantiate_string_mapping_alias_ref(
        &mut self,
        symbol: &crate::symbol_access::SymbolRef,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        if !self.is_string_mapping_alias_ref(symbol) {
            return None;
        }
        let [argument] = arguments else {
            return None;
        };
        Some(self.get_string_mapping_type_ref(symbol, *argument))
    }

    pub(crate) fn get_string_mapping_type_ref(
        &mut self,
        symbol: &crate::symbol_access::SymbolRef,
        id: TypeId,
    ) -> TypeId {
        if id == self.intrinsics.error {
            return id;
        }
        if self.store.get(id).flags.contains(TypeFlags::NEVER) {
            return id;
        }
        if let Some((_, value)) = self.enum_member_value(id)
            && let Some(value) = value.strip_prefix("s:")
        {
            let value = self.apply_string_mapping(symbol, value);
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(value),
                false,
            );
        }
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            let mapped: Vec<_> =
                types.into_iter().map(|ty| self.get_string_mapping_type_ref(symbol, ty)).collect();
            return self.get_union_type(&mapped);
        }
        if let TypeData::StringLiteral(value) = &self.store.get(id).data {
            let value = self.apply_string_mapping(symbol, value);
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(value),
                false,
            );
        }
        if let Some(mut parts) = self.template_literal_parts.get(&id).cloned() {
            match self.symbols.view(symbol).expect("mapping owner belongs to this Checker").name() {
                "Uppercase" | "Lowercase" => {
                    for text in &mut parts.texts {
                        *text = self.apply_string_mapping(symbol, text);
                    }
                    for ty in &mut parts.types {
                        *ty = self.get_string_mapping_type_ref(symbol, *ty);
                    }
                }
                "Capitalize" | "Uncapitalize" => {
                    if parts.texts[0].is_empty() {
                        parts.types[0] = self.get_string_mapping_type_ref(symbol, parts.types[0]);
                    } else {
                        parts.texts[0] = self.apply_string_mapping(symbol, &parts.texts[0]);
                    }
                }
                _ => {}
            }
            return self.get_template_literal_type(&parts.texts, &parts.types);
        }
        if self.string_mapping_types.get(&id).is_some_and(|(owner, _)| owner == symbol) {
            return id;
        }
        let flags = self.store.get(id).flags;
        if flags.intersects(
            TypeFlags::ANY
                | TypeFlags::STRING
                | TypeFlags::STRING_MAPPING
                | TypeFlags::TYPE_PARAMETER
                | TypeFlags::INDEX
                | TypeFlags::INDEXED_ACCESS
                | TypeFlags::CONDITIONAL
                | TypeFlags::SUBSTITUTION,
        ) || self.deferred_keyof_operands.contains_key(&id)
            || self.deferred_indexed_access_types.contains_key(&id)
        {
            return self.generic_string_mapping_type(symbol, id);
        }
        if self.is_pattern_template_placeholder(id) {
            let template = self.get_template_literal_type(&[String::new(), String::new()], &[id]);
            return self.generic_string_mapping_type(symbol, template);
        }
        id
    }

    fn generic_string_mapping_type(
        &mut self,
        symbol: &crate::symbol_access::SymbolRef,
        target: TypeId,
    ) -> TypeId {
        if let Some(&cached) = self.string_mapping_cache.get(&(symbol.clone(), target)) {
            return cached;
        }
        let text = format!(
            "{}<{}>",
            self.symbols.view(symbol).expect("mapping owner belongs to this Checker").name(),
            self.type_to_string(target)
        );
        let id = self.store.new_named(TypeFlags::STRING_MAPPING, text, None);
        self.string_mapping_types.insert(id, (symbol.clone(), target));
        self.string_mapping_cache.insert((symbol.clone(), target), id);
        id
    }

    pub(crate) fn apply_string_mapping(
        &self,
        symbol: &crate::symbol_access::SymbolRef,
        value: &str,
    ) -> String {
        match self.symbols.view(symbol).expect("mapping owner belongs to this Checker").name() {
            "Uppercase" => js_case(value, true),
            "Lowercase" => js_case(value, false),
            "Capitalize" | "Uncapitalize" => {
                let end = value.chars().next().map_or(0, char::len_utf8);
                js_case(
                    &value[..end],
                    self.symbols
                        .view(symbol)
                        .expect("mapping owner belongs to this Checker")
                        .name()
                        == "Capitalize",
                ) + &value[end..]
            }
            _ => value.to_owned(),
        }
    }

    /// isMemberOfStringMapping/applyTargetStringMappingToSource (relater.go:2498).
    pub(crate) fn is_member_of_string_mapping(&mut self, source: TypeId, target: TypeId) -> bool {
        if self.store.get(target).flags.contains(TypeFlags::ANY) {
            return true;
        }
        if self.store.get(target).flags.intersects(TypeFlags::STRING | TypeFlags::TEMPLATE_LITERAL)
        {
            return self.is_type_assignable_to(source, target);
        }
        let Some((symbol, inner)) = self.string_mapping_types.get(&target).cloned() else {
            return false;
        };
        let (mapped, inner) = self.apply_target_mapping(source, &symbol, inner);
        self.get_regular_type_of_literal_type(mapped)
            == self.get_regular_type_of_literal_type(source)
            && self.is_member_of_string_mapping(source, inner)
    }

    fn apply_target_mapping(
        &mut self,
        source: TypeId,
        symbol: &crate::symbol_access::SymbolRef,
        mut inner: TypeId,
    ) -> (TypeId, TypeId) {
        let mut source = source;
        if let Some((symbol, target)) = self.string_mapping_types.get(&inner).cloned() {
            (source, inner) = self.apply_target_mapping(source, &symbol, target);
        }
        (self.get_string_mapping_type_ref(symbol, source), inner)
    }
}

/// Unicode15.1 default casing and `Final_Sigma` from pinned `stringutil/js_case.go`.
fn js_case(value: &str, upper: bool) -> String {
    use crate::js_case_data::{CASE_IGNORABLE, CASE_MAPPINGS, CASED};
    fn contains(ranges: &[(u32, u32, u32)], c: char) -> bool {
        let code = c as u32;
        let index = ranges.partition_point(|&(start, _, _)| start <= code);
        index > 0 && {
            let (start, end, stride) = ranges[index - 1];
            code <= end && (code - start) % stride == 0
        }
    }
    let mut out = String::with_capacity(value.len());
    let mut cased_before = false;
    for (offset, c) in value.char_indices() {
        if !upper
            && c == 'Σ'
            && cased_before
            && !value[offset + c.len_utf8()..]
                .chars()
                .find(|&c| !contains(CASE_IGNORABLE, c))
                .is_some_and(|c| contains(CASED, c))
        {
            out.push('ς');
        } else if let Ok(index) =
            CASE_MAPPINGS.binary_search_by_key(&(c as u32), |&(code, _, _)| code)
        {
            out.push_str(if upper { CASE_MAPPINGS[index].2 } else { CASE_MAPPINGS[index].1 });
        } else {
            out.push(c);
        }
        if !contains(CASE_IGNORABLE, c) {
            cased_before = contains(CASED, c);
        }
    }
    out
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    #[test]
    fn selected_string_mapping_owners_keep_distinct_generic_images() {
        let arena = tsr_core::Arena::new();
        let source = "type Uppercase<S extends string> = intrinsic; \
                      type Lowercase<S extends string> = intrinsic; \
                      type Parameter<T extends string> = T;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapping.ts", text: source },
        );
        let owners: Vec<_> = parsed
            .source_file
            .statements
            .iter()
            .map(|statement| {
                let tsr_ast::Statement::TypeAliasDeclaration(alias) = statement else {
                    panic!("alias fixture");
                };
                bound.symbol_of(alias.node_id.unwrap()).unwrap()
            })
            .collect();
        let tsr_ast::Statement::TypeAliasDeclaration(parameter) = parsed.source_file.statements[2]
        else {
            panic!("parameter fixture")
        };
        let parameter = bound.symbol_of(parameter.type_parameters[0].node_id.unwrap()).unwrap();
        for reverse in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let raw = checker.symbols.bound(owners[0]).unwrap();
            let head = checker.symbols.clone_symbol(&raw).unwrap();
            let twin = checker.symbols.clone_symbol(&raw).unwrap();
            let lower = checker.symbols.bound(owners[1]).unwrap();
            let lower = checker.symbols.clone_symbol(&lower).unwrap();
            let plain = checker.symbols.bound(owners[2]).unwrap();
            let plain = checker.symbols.clone_symbol(&plain).unwrap();
            let argument = checker.get_declared_type_of_symbol(parameter);
            let literal = checker.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral("Mixed".to_owned()),
                false,
            );
            let mut order = [&raw, &head, &twin];
            if reverse {
                order.reverse();
            }
            let mut images = Vec::new();
            let mut shared_literal = None;
            for owner in order {
                let image =
                    checker.instantiate_string_mapping_alias_ref(owner, &[argument]).unwrap();
                assert_eq!(checker.type_to_string(image), "Uppercase<T>");
                assert_eq!(checker.string_mapping_types[&image], (owner.clone(), argument));
                assert_eq!(checker.get_string_mapping_type_ref(owner, argument), image);
                assert_eq!(checker.get_string_mapping_type_ref(owner, image), image);
                let string_image =
                    checker.get_string_mapping_type_ref(owner, checker.intrinsics.string);
                assert_ne!(image, string_image);
                let mut inferred = Vec::new();
                checker.infer_from_types(string_image, image, &[argument], &mut inferred, 0);
                assert_eq!(inferred.len(), 1);
                assert_eq!(inferred[0].candidates, vec![checker.intrinsics.string]);
                for previous in &images {
                    assert_ne!(*previous, image);
                    assert!(!checker.is_type_assignable_to(*previous, image));
                    let mut refused = Vec::new();
                    checker.infer_from_types(string_image, *previous, &[argument], &mut refused, 0);
                    assert!(refused.is_empty());
                }
                let concrete =
                    checker.instantiate_type(image, &[(argument, literal)], &[argument], &["T"]);
                assert_eq!(checker.type_to_string(concrete), "\"MIXED\"");
                assert_eq!(*shared_literal.get_or_insert(concrete), concrete);
                let rebound = checker.instantiate_type(
                    image,
                    &[(argument, checker.intrinsics.string)],
                    &[argument],
                    &["T"],
                );
                assert_eq!(rebound, string_image);
                assert!(checker.is_member_of_string_mapping(concrete, string_image));
                assert!(!checker.is_member_of_string_mapping(literal, string_image));
                let nested = checker.get_string_mapping_type_ref(&lower, image);
                let concrete =
                    checker.instantiate_type(nested, &[(argument, literal)], &[argument], &["T"]);
                assert_eq!(checker.type_to_string(concrete), "\"mixed\"");
                assert_eq!(checker.instantiate_string_mapping_alias_ref(owner, &[]), None);
                assert_eq!(
                    checker.instantiate_string_mapping_alias_ref(owner, &[argument, literal]),
                    None
                );
                images.push(image);
            }
            assert_eq!(checker.instantiate_string_mapping_alias_ref(&plain, &[argument]), None);
            assert!(checker.alias_evaluation_bindings.is_empty());
        }
    }

    #[test]
    fn bound_string_mapping_reuses_generic_identity_and_maps_literals() {
        let arena = tsr_core::Arena::new();
        let source = "type Uppercase<S extends string> = intrinsic; \
                      type Lowercase<S extends string> = intrinsic; \
                      type Parameter<T extends string> = T;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapping.ts", text: source },
        );
        let owners: Vec<_> = parsed
            .source_file
            .statements
            .iter()
            .map(|statement| {
                let tsr_ast::Statement::TypeAliasDeclaration(alias) = statement else {
                    panic!("alias fixture");
                };
                bound.symbol_of(alias.node_id.unwrap()).unwrap()
            })
            .collect();
        let tsr_ast::Statement::TypeAliasDeclaration(parameter) = parsed.source_file.statements[2]
        else {
            panic!("parameter fixture")
        };
        let parameter = bound.symbol_of(parameter.type_parameters[0].node_id.unwrap()).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let argument = checker.get_declared_type_of_symbol(parameter);
        let upper = checker.instantiate_string_mapping_alias(owners[0], &[argument]).unwrap();
        assert_eq!(checker.type_to_string(upper), "Uppercase<T>");
        assert_eq!(
            checker.instantiate_string_mapping_alias(owners[0], &[argument]).unwrap(),
            upper
        );
        assert_eq!(checker.instantiate_string_mapping_alias(owners[0], &[upper]).unwrap(), upper);
        let literal = checker.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral("Mixed".to_owned()),
            false,
        );
        let mapped = checker.instantiate_type(upper, &[(argument, literal)], &[argument], &["T"]);
        assert_eq!(checker.type_to_string(mapped), "\"MIXED\"");
        let nested = checker.instantiate_string_mapping_alias(owners[1], &[upper]).unwrap();
        let mapped = checker.instantiate_type(nested, &[(argument, literal)], &[argument], &["T"]);
        assert_eq!(checker.type_to_string(mapped), "\"mixed\"");
        let lower_string = checker
            .instantiate_string_mapping_alias(owners[1], &[checker.intrinsics.string])
            .unwrap();
        assert!(checker.is_member_of_string_mapping(mapped, lower_string));
        assert_eq!(checker.instantiate_string_mapping_alias(owners[0], &[]), None);
        assert_eq!(checker.instantiate_string_mapping_alias(owners[2], &[argument]), None);
        assert!(checker.alias_evaluation_bindings.is_empty());
    }
}
