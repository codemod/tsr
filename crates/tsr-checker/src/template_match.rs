//! Template matching helpers from internal/checker/relater.go:2345.
use crate::{
    Checker,
    flags::TypeFlags,
    templates::TemplateLiteralParts,
    types::{TypeData, TypeId},
};
impl Checker<'_, '_> {
    pub(crate) fn template_base_constraint(&mut self, id: TypeId) -> TypeId {
        let mut current = id;
        let mut visited = Vec::new();
        while self.store.get(current).flags.contains(TypeFlags::TYPE_PARAMETER) {
            if visited.contains(&current) {
                return id;
            }
            visited.push(current);
            let Some(constraint) = self.type_parameter_constraint(current) else {
                break;
            };
            current = constraint;
        }
        current
    }
    pub(crate) fn template_literal_inferences(
        &mut self,
        source: TypeId,
        target: &TemplateLiteralParts,
    ) -> Option<Vec<TypeId>> {
        if let TypeData::StringLiteral(value) = &self.store.get(source).data {
            let parts = TemplateLiteralParts { texts: vec![value.clone()], types: Vec::new() };
            return self.match_template_parts(parts, target);
        }
        let parts = self.template_literal_parts.get(&source)?.clone();
        if parts.texts == target.texts {
            return Some(
                parts
                    .types
                    .into_iter()
                    .zip(target.types.iter())
                    .map(|(source, &target)| {
                        let source_base = self.template_base_constraint(source);
                        let target_base = self.template_base_constraint(target);
                        if self.is_type_assignable_to(source_base, target_base)
                            || self
                                .store
                                .get(source)
                                .flags
                                .intersects(TypeFlags::ANY | TypeFlags::STRING_LIKE)
                        {
                            source
                        } else {
                            self.get_template_literal_type(
                                &[String::new(), String::new()],
                                &[source],
                            )
                        }
                    })
                    .collect(),
            );
        }
        self.match_template_parts(parts, target)
    }
    fn match_template_parts(
        &mut self,
        mut source: TemplateLiteralParts,
        target: &TemplateLiteralParts,
    ) -> Option<Vec<TypeId>> {
        let last = source.texts.len() - 1;
        let end = &target.texts[target.texts.len() - 1];
        if (last == 0 && source.texts[0].len() < target.texts[0].len() + end.len())
            || !source.texts[0].starts_with(&target.texts[0])
            || !source.texts[last].ends_with(end)
        {
            return None;
        }
        let remaining = source.texts[last].len() - end.len();
        source.texts[last].truncate(remaining);
        let mut segment = 0;
        let mut position = target.texts[0].len();
        let mut matches = Vec::new();
        for delimiter in &target.texts[1..target.texts.len() - 1] {
            let (next_segment, next_position) = if delimiter.is_empty() {
                if let Some(character) = source.texts[segment][position..].chars().next() {
                    (segment, position + character.len_utf8())
                } else if segment < last {
                    (segment + 1, 0)
                } else {
                    return None;
                }
            } else {
                let mut next_segment = segment;
                let mut next_position = position;
                loop {
                    if let Some(offset) =
                        source.texts[next_segment][next_position..].find(delimiter)
                    {
                        break (next_segment, next_position + offset);
                    }
                    next_segment += 1;
                    if next_segment > last {
                        return None;
                    }
                    next_position = 0;
                }
            };
            matches.push(self.template_segment_type(
                &source,
                segment,
                position,
                next_segment,
                next_position,
            ));
            segment = next_segment;
            position = next_position + delimiter.len();
        }
        matches.push(self.template_segment_type(
            &source,
            segment,
            position,
            last,
            source.texts[last].len(),
        ));
        Some(matches)
    }
    fn template_segment_type(
        &mut self,
        source: &TemplateLiteralParts,
        start: usize,
        start_position: usize,
        end: usize,
        end_position: usize,
    ) -> TypeId {
        if start == end {
            return self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(
                    source.texts[start][start_position..end_position].to_owned(),
                ),
                false,
            );
        }
        let mut texts = vec![source.texts[start][start_position..].to_owned()];
        texts.extend_from_slice(&source.texts[start + 1..end]);
        texts.push(source.texts[end][..end_position].to_owned());
        self.get_template_literal_type(&texts, &source.types[start..end])
    }
}

/// isValidNumberString: numeric coercion differs from source literal parsing
/// (leading zero is decimal, separators and signed radix prefixes are invalid).
pub(crate) fn template_number(value: &str, round_trip: bool) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let text = value.trim_matches(|c| {
        tsr_scanner::is_whitespace_single_line(c) || tsr_scanner::is_line_break(c)
    });
    let number = if text.is_empty() {
        0.0
    } else {
        let radix = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))
            .map(|s| (s, 16))
            .or_else(|| text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")).map(|s| (s, 8)))
            .or_else(|| {
                text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")).map(|s| (s, 2))
            });
        if let Some((digits, radix)) = radix {
            if digits.is_empty() {
                return None;
            }
            digits.chars().try_fold(0.0, |value, digit| {
                digit.to_digit(radix).map(|digit| value * f64::from(radix) + f64::from(digit))
            })?
        } else {
            text.parse::<f64>().ok()?
        }
    };
    if !number.is_finite() {
        return None;
    }
    let canonical = tsr_core::jsnum::format_number(number);
    (!round_trip || canonical == value).then_some(canonical)
}

/// isValidBigIntString plus parsePseudoBigInt. The scanner validates the full
/// token; decimal digits retain arbitrary precision during radix conversion.
pub(crate) fn template_bigint(value: &str, round_trip: bool) -> Option<String> {
    use tsr_ast::SyntaxKind;
    if value.is_empty() {
        return None;
    }
    let text = format!("{value}n");
    let mut scanner = tsr_scanner::Scanner::new(&text);
    let mut token = scanner.scan();
    if token.span.start != 0 {
        return None;
    }
    let negative = token.kind == SyntaxKind::MinusToken;
    if negative {
        token = scanner.scan();
        if token.span.start != 1 {
            return None;
        }
    }
    if token.kind != SyntaxKind::BigIntLiteral
        || token.span.end as usize != text.len()
        || token.flags.contains(tsr_scanner::TokenFlags::CONTAINS_SEPARATOR)
        || !scanner.diagnostics().is_empty()
    {
        return None;
    }
    let digits = value.strip_prefix('-').unwrap_or(value);
    let (digits, radix) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .map(|s| (s, 16))
        .or_else(|| digits.strip_prefix("0o").or_else(|| digits.strip_prefix("0O")).map(|s| (s, 8)))
        .or_else(|| digits.strip_prefix("0b").or_else(|| digits.strip_prefix("0B")).map(|s| (s, 2)))
        .unwrap_or((digits, 10));
    let mut decimal = vec![0u32];
    for digit in digits.chars() {
        let mut carry = digit.to_digit(radix)?;
        for slot in &mut decimal {
            let value = *slot * radix + carry;
            *slot = value % 10;
            carry = value / 10;
        }
        while carry != 0 {
            decimal.push(carry % 10);
            carry /= 10;
        }
    }
    let mut canonical: String = decimal
        .into_iter()
        .rev()
        .map(|digit| char::from_digit(digit, 10).expect("decimal digit"))
        .collect();
    if negative && canonical != "0" {
        canonical.insert(0, '-');
    }
    (!round_trip || canonical == value).then_some(canonical)
}
