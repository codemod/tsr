//! Template literal construction from internal/checker/checker.go:29145.
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use std::fmt::Write as _;
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateLiteralParts {
    pub(crate) texts: Vec<String>,
    pub(crate) types: Vec<TypeId>,
}

impl Checker<'_, '_> {
    pub(crate) fn instantiate_template_alias(
        &mut self,
        symbol: tsr_binder::SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let selected = self.symbols.bound(symbol).expect("alias owner belongs to this Program");
        self.instantiate_template_alias_ref(&selected, arguments)
    }

    /// The alias owner is the actual checker-local symbol, as in native
    /// getTypeAliasInstantiation. Parameters still name shared syntax symbols;
    /// the active-owner set is not a completed instantiation cache.
    pub(crate) fn instantiate_template_alias_ref(
        &mut self,
        symbol: &crate::symbol_access::SymbolRef,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let owner = self.symbols.view(symbol).expect("alias owner belongs to this Checker");
        if !owner.flags().contains(tsr_binder::SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        // `core.Find(symbol.Declarations, IsEitherTypeAliasDeclaration)`
        // (checker.go:23845; r5-typeparams2) over main's selected owner.
        let declaration = owner.declarations().iter().copied().find(|&declaration| {
            matches!(self.node_map.get(declaration), Some(tsr_ast::Node::TypeAliasDeclaration(_)))
        })?;
        let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
        else {
            return None;
        };
        let body @ tsr_ast::TypeNode::TemplateLiteralTypeNode(_) = alias.r#type? else {
            return None;
        };
        if alias.type_parameters.len() != arguments.len()
            || !self.template_alias_in_progress.insert(symbol.clone())
        {
            return None;
        }
        let bindings = alias
            .type_parameters
            .iter()
            .filter_map(|p| p.node_id)
            .filter_map(|id| self.binder.symbol_of(id))
            .zip(arguments.iter().copied())
            .collect();
        self.alias_evaluation_bindings.push(bindings);
        let result = self.get_type_from_type_node(body);
        self.alias_evaluation_bindings.pop();
        self.template_alias_in_progress.remove(symbol);
        Some(result)
    }

    pub(crate) fn get_template_literal_type(
        &mut self,
        texts: &[String],
        types: &[TypeId],
    ) -> TypeId {
        if types.contains(&self.intrinsics.error) {
            return self.intrinsics.error;
        }
        if let Some(index) = types.iter().position(|&ty| {
            self.store.get(ty).flags.intersects(TypeFlags::UNION | TypeFlags::NEVER)
        }) {
            // checkCrossProductUnion rejects a product of 100,000 or more.
            let size = types.iter().try_fold(1usize, |size, &ty| {
                let count = match &self.store.get(ty).data {
                    _ if self.store.get(ty).flags.contains(TypeFlags::NEVER) => 0,
                    TypeData::Union { types, .. } => types.len(),
                    _ => 1,
                };
                size.checked_mul(count)
            });
            if size.is_none_or(|size| size >= 100_000) {
                return self.intrinsics.error;
            }
            if size == Some(0) {
                return self.intrinsics.never;
            }
            let TypeData::Union { types: parts, .. } = &self.store.get(types[index]).data else {
                return self.intrinsics.error;
            };
            let parts = parts.clone();
            let mapped: Vec<_> = parts
                .into_iter()
                .map(|part| {
                    let mut types = types.to_vec();
                    types[index] = part;
                    self.get_template_literal_type(texts, &types)
                })
                .collect();
            return self.get_union_type(&mapped);
        }
        let mut spans = TemplateLiteralParts { texts: Vec::new(), types: Vec::new() };
        let mut text = texts[0].clone();
        if !self.add_template_spans(texts, types, &mut text, &mut spans) {
            return self.intrinsics.string;
        }
        if spans.types.is_empty() {
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(text),
                false,
            );
        }
        spans.texts.push(text);
        if spans.texts.iter().all(String::is_empty) {
            if spans.types.iter().all(|&ty| self.store.get(ty).flags.contains(TypeFlags::STRING)) {
                return self.intrinsics.string;
            }
            if let [ty] = spans.types.as_slice()
                && self.is_pattern_template(*ty)
            {
                return *ty;
            }
        }
        if let Some(&id) = self.template_literal_cache.get(&spans) {
            return id;
        }
        let mut printed = String::from("`");
        printed.push_str(&Self::escape_template_text(&spans.texts[0]));
        for (i, &ty) in spans.types.iter().enumerate() {
            printed.push_str("${");
            printed.push_str(&self.type_to_string(ty));
            printed.push('}');
            printed.push_str(&Self::escape_template_text(&spans.texts[i + 1]));
        }
        printed.push('`');
        let id = self.store.new_named(TypeFlags::TEMPLATE_LITERAL, printed, None);
        self.template_literal_cache.insert(spans.clone(), id);
        self.template_literal_parts.insert(id, spans);
        id
    }
    /// A template part's text as the printer writes it: `escapeStringWorker`
    /// (`printer/utilities.go:77`) with `QuoteCharBacktick` and
    /// `getLiteralTextFlagsNeverAsciiEscape` — the node builder marks every
    /// part `EFNoAsciiEscaping` (`nodebuilderimpl.go:3482`). Escaped: `\`,
    /// the backtick, `$` before `{`, CR (a CRLF pair as one `\r\n`), the
    /// C0 controls other than LF (which a template keeps), and U+2028,
    /// U+2029, U+0085. `docs/parity/notes/r4-templates.md` §6.
    pub(crate) fn escape_template_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            let next = chars.peek().copied();
            match ch {
                '\\' => out.push_str("\\\\"),
                '`' => out.push_str("\\`"),
                '$' if next == Some('{') => out.push_str("\\$"),
                '\r' if next == Some('\n') => {
                    chars.next();
                    out.push_str("\\r\\n");
                }
                '\r' => out.push_str("\\r"),
                '\n' => out.push('\n'),
                '\t' => out.push_str("\\t"),
                '\u{000B}' => out.push_str("\\v"),
                '\u{000C}' => out.push_str("\\f"),
                '\u{0008}' => out.push_str("\\b"),
                '\0' => {
                    out.push_str(if next.is_some_and(|c| c.is_ascii_digit()) {
                        "\\x00"
                    } else {
                        "\\0"
                    });
                }
                '\u{2028}' | '\u{2029}' | '\u{0085}' => {
                    let _ = write!(out, "\\u{:04X}", u32::from(ch));
                }
                c if u32::from(c) <= 0x1f => {
                    let _ = write!(out, "\\u{:04X}", u32::from(c));
                }
                c => out.push(c),
            }
        }
        out
    }
    fn add_template_spans(
        &mut self,
        texts: &[String],
        types: &[TypeId],
        text: &mut String,
        out: &mut TemplateLiteralParts,
    ) -> bool {
        for (i, &ty) in types.iter().enumerate() {
            if let Some((_, value)) = self.enum_member_value(ty)
                && let Some(value) = value.strip_prefix("s:").or_else(|| value.strip_prefix("n:"))
            {
                text.push_str(value);
                text.push_str(&texts[i + 1]);
                continue;
            }
            match &self.store.get(ty).data {
                TypeData::StringLiteral(value) | TypeData::NumberLiteral(value) => {
                    text.push_str(value);
                }
                TypeData::BigIntLiteral(value) => text.push_str(value.trim_end_matches('n')),
                TypeData::BooleanLiteral(value) => {
                    text.push_str(if *value { "true" } else { "false" });
                }
                _ if self.store.get(ty).flags.intersects(TypeFlags::NULLABLE) => {
                    text.push_str(&self.type_to_string(ty));
                }
                _ => {
                    if let Some(nested) = self.template_literal_parts.get(&ty).cloned() {
                        text.push_str(&nested.texts[0]);
                        if !self.add_template_spans(&nested.texts, &nested.types, text, out) {
                            return false;
                        }
                    } else if self.is_generic_index_type(ty)
                        || self.is_pattern_template_placeholder(ty)
                    {
                        out.types.push(ty);
                        out.texts.push(std::mem::take(text));
                    } else {
                        return false;
                    }
                }
            }
            text.push_str(&texts[i + 1]);
        }
        true
    }
    /// `isGenericIndexType` (`checker.go:24876`): the `IsGenericIndexType`
    /// bit of `getGenericObjectFlags` (`:24880`) — a union or intersection
    /// has it when any constituent does; otherwise an instantiable
    /// non-primitive, an index type (a deferred `keyof` here is
    /// `deferred_keyof_operands`), or a generic string-like type (a template
    /// literal or string mapping that is not a pattern literal).
    ///
    /// Port record: upstream memoises the union/intersection answer on the
    /// type (`ObjectFlagsIsGenericTypeComputed`); this walks the
    /// constituents per call. Its only caller is `addSpans`, which sees a
    /// span after unions were distributed, so the walk is over one
    /// intersection's members. `docs/parity/notes/r4-templates.md` §7.
    pub(crate) fn is_generic_index_type(&self, id: TypeId) -> bool {
        let ty = self.store.get(id);
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } = &ty.data {
            return types.iter().any(|&member| self.is_generic_index_type(member));
        }
        ty.flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE | TypeFlags::INDEX)
            || self.deferred_keyof_operands.contains_key(&id)
            || (ty.flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
                && !self.is_pattern_template(id))
    }
    pub(crate) fn is_pattern_template(&self, id: TypeId) -> bool {
        if let Some((_, target)) = self.string_mapping_types.get(&id) {
            return self.is_pattern_template_placeholder(*target);
        }
        self.template_literal_parts.get(&id).is_some_and(|parts| {
            parts.types.iter().all(|&ty| self.is_pattern_template_placeholder(ty))
        })
    }
    pub(crate) fn is_pattern_template_placeholder(&self, id: TypeId) -> bool {
        let ty = self.store.get(id);
        if let Some((_, target)) = self.string_mapping_types.get(&id) {
            return self.is_pattern_template_placeholder(*target);
        }
        if let TypeData::Intersection { types, .. } = &ty.data {
            let mut seen = false;
            for &id in types {
                let flags = self.store.get(id).flags;
                if flags.intersects(TypeFlags::LITERAL | TypeFlags::NULLABLE)
                    || self.is_pattern_template_placeholder(id)
                {
                    seen = true;
                } else if !flags.contains(TypeFlags::OBJECT) {
                    return false;
                }
            }
            return seen;
        }
        ty.flags
            .intersects(TypeFlags::ANY | TypeFlags::STRING | TypeFlags::NUMBER | TypeFlags::BIG_INT)
            || self.is_pattern_template(id)
    }
}

impl Checker<'_, '_> {
    /// `isTypeMatchedByTemplateLiteralType` (`relater.go:2332`) with
    /// `compareTypesAssignable`, the comparer
    /// `isTypeMatchedByTemplateLiteralOrStringMapping` (`checker.go:25874`)
    /// passes. The relater's own copy (`relater.rs`
    /// `valid_template_placeholder`) compares inside an open relation; this
    /// one is for callers outside one, such as union reduction.
    ///
    /// The source is matched against the template's literal parts directly
    /// (`inferTypesFromTemplateLiteralType`, [`Checker::template_literal_inferences`]),
    /// and each inferred placeholder is checked on its own. A string literal
    /// source therefore never opens a relation against the template itself;
    /// a placeholder relation opens only when its target is not `string`.
    pub(crate) fn is_type_matched_by_template_literal_type(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(parts) = self.template_literal_parts.get(&target).cloned() else {
            return false;
        };
        self.is_type_matched_by_template_literal_parts(source, &parts)
    }

    /// [`Checker::is_type_matched_by_template_literal_type`] against a
    /// template's parts the caller already holds. Native passes the template
    /// by pointer; a caller matching many sources against one template
    /// (union reduction) reads the parts once rather than copying them out of
    /// `template_literal_parts` per source.
    pub(crate) fn is_type_matched_by_template_literal_parts(
        &mut self,
        source: TypeId,
        parts: &TemplateLiteralParts,
    ) -> bool {
        let Some(inferences) = self.template_literal_inferences(source, parts) else {
            return false;
        };
        inferences.into_iter().zip(parts.types.iter().copied()).all(|(inference, placeholder)| {
            self.is_valid_type_for_template_literal_placeholder(inference, placeholder)
        })
    }

    /// `isValidTypeForTemplateLiteralPlaceholder` (`relater.go:2476`) with
    /// `compareTypesAssignable`.
    fn is_valid_type_for_template_literal_placeholder(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if let TypeData::Intersection { types, .. } = &self.store.get(target).data {
            let types = types.clone();
            return types.into_iter().all(|target| {
                self.is_empty_anonymous_object_type(target)
                    || self.is_valid_type_for_template_literal_placeholder(source, target)
            });
        }
        let flags = self.store.get(target).flags;
        if flags.contains(TypeFlags::STRING) || self.is_type_assignable_to(source, target) {
            return true;
        }
        if let TypeData::StringLiteral(value) = &self.store.get(source).data {
            let value = value.clone();
            return (flags.contains(TypeFlags::NUMBER)
                && crate::template_match::template_number(&value, false).is_some())
                || (flags.contains(TypeFlags::BIG_INT)
                    && crate::template_match::template_bigint(&value, false).is_some())
                || (flags.intersects(TypeFlags::BOOLEAN_LITERAL | TypeFlags::NULLABLE)
                    && value == self.type_to_string(target))
                || (flags.contains(TypeFlags::STRING_MAPPING)
                    && self.is_member_of_string_mapping(source, target))
                || (flags.contains(TypeFlags::TEMPLATE_LITERAL)
                    && self.is_type_matched_by_template_literal_type(source, target));
        }
        if let Some(parts) = self.template_literal_parts.get(&source)
            && parts.types.len() == 1
            && parts.texts.iter().all(String::is_empty)
        {
            let inner = parts.types[0];
            return self.is_type_assignable_to(inner, target);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_alias_selected_owners_keep_independent_active_state() {
        let arena = tsr_core::Arena::new();
        let source = "type Owner<T extends string> = `owner-${T}`; \
                      type Extra<U extends string> = `extra-${U}`; \
                      type Plain<V> = V;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "template.ts", text: source },
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
        for reverse in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let original = checker.symbols.bound(owners[0]).unwrap();
            let head = checker.symbols.clone_symbol(&original).unwrap();
            let twin = checker.symbols.clone_symbol(&original).unwrap();
            let extra = checker.symbols.bound(owners[1]).unwrap();
            let extra = checker.symbols.clone_symbol(&extra).unwrap();
            let plain = checker.symbols.bound(owners[2]).unwrap();
            let plain = checker.symbols.clone_symbol(&plain).unwrap();
            assert_ne!(head, twin);
            assert_ne!(head, original);
            let argument = checker.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral("value".to_owned()),
                false,
            );
            let mut order = [&original, &head, &twin];
            if reverse {
                order.reverse();
            }
            for active in order {
                assert!(checker.template_alias_in_progress.insert(active.clone()));
                let captured = checker.alias_evaluation_bindings.clone();
                assert_eq!(checker.instantiate_template_alias_ref(active, &[argument]), None);
                for other in [&original, &head, &twin].into_iter().filter(|other| *other != active)
                {
                    let result =
                        checker.instantiate_template_alias_ref(other, &[argument]).unwrap();
                    assert_eq!(checker.type_to_string(result), "\"owner-value\"");
                }
                assert_eq!(checker.template_alias_in_progress.len(), 1);
                assert!(checker.template_alias_in_progress.contains(active));
                assert_eq!(checker.alias_evaluation_bindings, captured);
                assert!(checker.template_alias_in_progress.remove(active));
                let result = checker.instantiate_template_alias_ref(active, &[argument]).unwrap();
                assert_eq!(checker.type_to_string(result), "\"owner-value\"");
                assert_eq!(
                    checker.instantiate_template_alias_ref(active, &[argument]),
                    Some(result)
                );
                assert!(checker.template_alias_in_progress.is_empty());
            }
            let result = checker.instantiate_template_alias_ref(&extra, &[argument]).unwrap();
            assert_eq!(checker.type_to_string(result), "\"extra-value\"");
            assert_eq!(checker.instantiate_template_alias_ref(&head, &[]), None);
            assert_eq!(checker.instantiate_template_alias_ref(&plain, &[argument]), None);
            assert!(checker.template_alias_in_progress.is_empty());
            assert!(checker.alias_evaluation_bindings.is_empty());
        }
    }

    #[test]
    fn template_alias_bound_evaluation_restores_captured_bindings() {
        let arena = tsr_core::Arena::new();
        let source = "type Owner<T extends string> = `owner-${T}`;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "template.ts", text: source },
        );
        let tsr_ast::Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[0]
        else {
            panic!("template alias fixture");
        };
        let owner = bound.symbol_of(alias.node_id.unwrap()).unwrap();
        let parameter = bound.symbol_of(alias.type_parameters[0].node_id.unwrap()).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let argument = checker.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral("value".to_owned()),
            false,
        );
        let captured: rustc_hash::FxHashMap<_, _> =
            [(parameter, checker.intrinsics.number)].into_iter().collect();
        checker.alias_evaluation_bindings.push(captured.clone());
        for _ in 0..2 {
            let result = checker.instantiate_template_alias(owner, &[argument]).unwrap();
            assert_eq!(checker.type_to_string(result), "\"owner-value\"");
            assert_eq!(checker.alias_evaluation_bindings, vec![captured.clone()]);
            assert!(checker.template_alias_in_progress.is_empty());
        }
    }
}
