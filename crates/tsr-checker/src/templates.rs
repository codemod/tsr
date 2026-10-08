//! Template literal construction from internal/checker/checker.go:29145.
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
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
        if !self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
        else {
            return None;
        };
        let body @ tsr_ast::TypeNode::TemplateLiteralTypeNode(_) = alias.r#type? else {
            return None;
        };
        if alias.type_parameters.len() != arguments.len()
            || !self.template_alias_in_progress.insert(symbol)
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
        self.template_alias_in_progress.remove(&symbol);
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
                    out.push_str(&format!("\\u{:04X}", u32::from(ch)));
                }
                c if u32::from(c) <= 0x1f => out.push_str(&format!("\\u{:04X}", u32::from(c))),
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
