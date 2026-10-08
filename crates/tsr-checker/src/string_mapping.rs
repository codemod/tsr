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
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return false;
        }
        let Some(declaration) = self.type_alias_declaration_of(symbol) else {
            return false;
        };
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return false;
        };
        matches!(alias.r#type, Some(TypeNode::KeywordTypeNode(keyword))
            if keyword.kind == SyntaxKind::IntrinsicKeyword)
            && matches!(
                self.binder.symbols().get(symbol).name,
                "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize"
            )
    }

    pub(crate) fn instantiate_string_mapping_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        if !self.is_string_mapping_alias(symbol) {
            return None;
        }
        let [argument] = arguments else {
            return None;
        };
        Some(self.get_string_mapping_type(symbol, *argument))
    }

    pub(crate) fn get_string_mapping_type(&mut self, symbol: SymbolId, id: TypeId) -> TypeId {
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
                types.into_iter().map(|ty| self.get_string_mapping_type(symbol, ty)).collect();
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
            match self.binder.symbols().get(symbol).name {
                "Uppercase" | "Lowercase" => {
                    for text in &mut parts.texts {
                        *text = self.apply_string_mapping(symbol, text);
                    }
                    for ty in &mut parts.types {
                        *ty = self.get_string_mapping_type(symbol, *ty);
                    }
                }
                "Capitalize" | "Uncapitalize" => {
                    if parts.texts[0].is_empty() {
                        parts.types[0] = self.get_string_mapping_type(symbol, parts.types[0]);
                    } else {
                        parts.texts[0] = self.apply_string_mapping(symbol, &parts.texts[0]);
                    }
                }
                _ => {}
            }
            return self.get_template_literal_type(&parts.texts, &parts.types);
        }
        if self.string_mapping_types.get(&id).is_some_and(|(owner, _)| *owner == symbol) {
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
            // getStringMappingType's `isGenericIndexType(t)` (checker.go:29233):
            // a generic intersection such as `K & string` stays deferred.
            || self.is_generic_index_type(id)
        {
            return self.generic_string_mapping_type(symbol, id);
        }
        if self.is_pattern_template_placeholder(id) {
            let template = self.get_template_literal_type(&[String::new(), String::new()], &[id]);
            return self.generic_string_mapping_type(symbol, template);
        }
        id
    }

    fn generic_string_mapping_type(&mut self, symbol: SymbolId, target: TypeId) -> TypeId {
        if let Some(&cached) = self.string_mapping_cache.get(&(symbol, target)) {
            return cached;
        }
        let text =
            format!("{}<{}>", self.binder.symbols().get(symbol).name, self.type_to_string(target));
        let id = self.store.new_named(TypeFlags::STRING_MAPPING, text, None);
        self.string_mapping_types.insert(id, (symbol, target));
        self.string_mapping_cache.insert((symbol, target), id);
        id
    }

    pub(crate) fn apply_string_mapping(&self, symbol: SymbolId, value: &str) -> String {
        match self.binder.symbols().get(symbol).name {
            "Uppercase" => js_case(value, true),
            "Lowercase" => js_case(value, false),
            "Capitalize" | "Uncapitalize" => {
                let end = value.chars().next().map_or(0, char::len_utf8);
                js_case(&value[..end], self.binder.symbols().get(symbol).name == "Capitalize")
                    + &value[end..]
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
        let Some((symbol, inner)) = self.string_mapping_types.get(&target).copied() else {
            return false;
        };
        let (mapped, inner) = self.apply_target_mapping(source, symbol, inner);
        self.get_regular_type_of_literal_type(mapped)
            == self.get_regular_type_of_literal_type(source)
            && self.is_member_of_string_mapping(source, inner)
    }

    fn apply_target_mapping(
        &mut self,
        source: TypeId,
        symbol: SymbolId,
        mut inner: TypeId,
    ) -> (TypeId, TypeId) {
        let mut source = source;
        if let Some((symbol, target)) = self.string_mapping_types.get(&inner).copied() {
            (source, inner) = self.apply_target_mapping(source, symbol, target);
        }
        (self.get_string_mapping_type(symbol, source), inner)
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
