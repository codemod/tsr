//! Effective signature positions, ported from `internal/checker/relater.go`:
//! `getParameterCount`, `getMinArgumentCount`, `tryGetTypeAtPosition`, and
//! `getRestTypeAtPosition`. Tuple rests expand their fixed prefix; a variable
//! suffix counts as one effective parameter.

use crate::{Checker, flags::TypeFlags, signatures::Signature, types::TypeId};

impl Checker<'_, '_> {
    /// `isInstantiatedGenericParameter` and the instantiable/union/tuple
    /// cases of `isGenericType` (`internal/checker/relater.go`, `checker.go`).
    pub(crate) fn signature_has_instantiated_generic_parameter(
        &mut self,
        signature: &Signature,
        position: usize,
    ) -> bool {
        let Some(original) = &signature.target else { return false };
        let Some(t) = self.signature_type_at_position(original, position) else { return false };
        self.signature_parameter_type_is_generic(t)
    }

    pub(crate) fn signature_parameter_type_is_generic(&mut self, t: TypeId) -> bool {
        let flags = self.store.get(t).flags;
        if flags.intersects(
            TypeFlags::TYPE_PARAMETER
                | TypeFlags::INDEXED_ACCESS
                | TypeFlags::CONDITIONAL
                | TypeFlags::SUBSTITUTION
                | TypeFlags::INDEX,
        ) {
            return true;
        }
        if let crate::types::TypeData::Union { types, .. }
        | crate::types::TypeData::Intersection { types, .. } = &self.store.get(t).data
        {
            let types = types.clone();
            return types.into_iter().any(|part| self.signature_parameter_type_is_generic(part));
        }
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&t).cloned() {
            return elements.iter().any(|element| {
                element.spread && self.tuple_spread_array_element(element.r#type).is_none()
            });
        }
        false
    }

    /// `isTopSignature` (`internal/checker/relater.go`).
    pub(crate) fn signature_is_top(&mut self, signature: &Signature) -> bool {
        if !signature.type_parameters.is_empty()
            || signature.this_parameter.as_ref().is_some_and(|parameter| {
                !self.store.get(parameter.r#type).flags.contains(TypeFlags::ANY)
            })
            || !self.store.get(signature.r#type).flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return false;
        }
        let [parameter] = signature.parameters.as_slice() else { return false };
        if !parameter.rest {
            return false;
        }
        let rest = self.signature_array_element(parameter.r#type).unwrap_or(parameter.r#type);
        self.store.get(rest).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER)
    }

    /// Ported from `getParameterCount` (`internal/checker/relater.go`).
    pub(crate) fn signature_parameter_count(&self, signature: &Signature) -> usize {
        let count = signature.parameters.len();
        let Some(rest) = signature.parameters.last().filter(|parameter| parameter.rest) else {
            return count;
        };
        if let Some((elements, _)) = self.tuple_element_lists.get(&rest.r#type) {
            return count - 1 + elements.len();
        }
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&rest.r#type) {
            let fixed =
                elements.iter().position(|element| element.spread).unwrap_or(elements.len());
            return count + fixed;
        }
        count
    }

    /// Ported from `hasEffectiveRestParameter` (`internal/checker/relater.go`).
    pub(crate) fn signature_has_effective_rest(&self, signature: &Signature) -> bool {
        signature.parameters.last().is_some_and(|parameter| {
            parameter.rest && !self.tuple_element_lists.contains_key(&parameter.r#type)
        })
    }

    /// Ported from `getEffectiveRestType` (`internal/checker/relater.go`).
    pub(crate) fn signature_effective_rest_type(
        &mut self,
        signature: &Signature,
    ) -> Option<TypeId> {
        let rest = signature.parameters.last().filter(|parameter| parameter.rest)?.r#type;
        if self.tuple_element_lists.contains_key(&rest) {
            return None;
        }
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&rest).cloned() {
            let start = elements.iter().position(|element| element.spread)?;
            if elements.len() == start + 1 {
                return Some(elements[start].r#type);
            }
            return Some(self.normalize_variadic_tuple(elements[start..].to_vec(), false));
        }
        Some(rest)
    }

    /// Ported from `getNonArrayRestType` (`internal/checker/relater.go`).
    pub(crate) fn signature_non_array_rest_type(
        &mut self,
        signature: &Signature,
    ) -> Option<TypeId> {
        let rest = self.signature_effective_rest_type(signature)?;
        (rest != self.intrinsics.any && self.signature_array_element(rest).is_none())
            .then_some(rest)
    }

    /// Ported from `tryGetTypeAtPosition` (`internal/checker/relater.go`).
    /// A missing position is None; an unsupported lookup is errorType.
    pub(crate) fn signature_type_at_position(
        &mut self,
        signature: &Signature,
        position: usize,
    ) -> Option<TypeId> {
        let rest = signature.parameters.last().filter(|parameter| parameter.rest);
        let leading = signature.parameters.len() - usize::from(rest.is_some());
        if position < leading {
            return Some(signature.parameters[position].r#type);
        }
        let rest = rest?;
        let index = position - leading;
        if let Some((elements, _)) = self.tuple_element_lists.get(&rest.r#type)
            && index >= elements.len()
        {
            return None;
        }
        let index = self.store.intern_literal(
            TypeFlags::NUMBER_LITERAL,
            crate::types::TypeData::NumberLiteral(index.to_string()),
            false,
        );
        Some(
            self.resolved_indexed_access_type(rest.r#type, index, false)
                .unwrap_or(self.intrinsics.error),
        )
    }

    /// Ported from `getMinArgumentCountEx` (`internal/checker/relater.go`).
    pub(crate) fn signature_min_argument_count(&mut self, signature: &Signature) -> usize {
        let mut minimum = signature
            .parameters
            .iter()
            .rposition(|parameter| !parameter.optional && !parameter.rest)
            .map_or(0, |position| position + 1);
        if let Some(rest) = signature.parameters.last().filter(|parameter| parameter.rest)
            && (self.tuple_element_lists.contains_key(&rest.r#type)
                || self.variadic_tuple_elements.contains_key(&rest.r#type))
        {
            let leading = signature.parameters.len() - 1;
            let required = self.signature_tuple_arguments(signature)[leading..]
                .iter()
                .take_while(|element| !element.optional && !element.spread)
                .count();
            if required > 0 {
                minimum = leading + required;
            }
        }
        while minimum > 0 {
            let Some(t) = self.signature_type_at_position(signature, minimum - 1) else { break };
            let contains_void = match &self.store.get(t).data {
                crate::types::TypeData::Union { types, .. } => {
                    types.iter().any(|&part| self.store.get(part).flags.contains(TypeFlags::VOID))
                }
                _ => self.store.get(t).flags.contains(TypeFlags::VOID),
            };
            if !contains_void {
                break;
            }
            minimum -= 1;
        }
        minimum
    }

    /// Ported from `getRestTypeAtPosition` (`internal/checker/relater.go`).
    pub(crate) fn signature_rest_type_at_position(
        &mut self,
        signature: &Signature,
        position: usize,
    ) -> TypeId {
        let count = self.signature_parameter_count(signature);
        if let Some(rest) = self.signature_effective_rest_type(signature)
            && position >= count.saturating_sub(1)
        {
            if position == count - 1 {
                return rest;
            }
            let Some(element) =
                self.resolved_indexed_access_type(rest, self.intrinsics.number, false)
            else {
                return self.intrinsics.error;
            };
            let Some(array) = self.global_type_symbol_with_arity("Array", 1) else {
                return self.intrinsics.error;
            };
            return self.create_type_reference(array, vec![element]);
        }
        let elements = self.signature_tuple_arguments(signature);
        let minimum = self.signature_min_argument_count(signature);
        let mut remaining = elements.get(position..).unwrap_or_default().to_vec();
        for (offset, element) in remaining.iter_mut().enumerate() {
            if !element.spread {
                element.optional = position + offset >= minimum;
            }
        }
        self.normalize_variadic_tuple(remaining, false)
    }

    /// Ported from `getRestOrAnyTypeAtPosition` (`internal/checker/relater.go`).
    pub(crate) fn signature_rest_or_any_at_position(
        &mut self,
        signature: &Signature,
        position: usize,
    ) -> TypeId {
        let rest = self.signature_rest_type_at_position(signature, position);
        if self.signature_array_element(rest) == Some(self.intrinsics.any) {
            self.intrinsics.any
        } else {
            rest
        }
    }

    /// Signature rest comparison uses `getElementTypeOfArrayType`, which
    /// recognizes the global mutable Array target (`checker.go`).
    fn signature_array_element(&mut self, t: TypeId) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&t).cloned()?;
        let [element] = arguments.as_slice() else { return None };
        let array = self.global_type_symbol("Array")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(array)).then_some(*element)
    }
}
