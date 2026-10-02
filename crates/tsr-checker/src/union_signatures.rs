//! Union call/construct signatures: getUnionSignatures and
//! combineUnionOrIntersectionMemberSignatures (internal/checker/checker.go),
//! with signature identity and composite predicates from relater.go.

use crate::{
    Checker,
    relater::{Relation, Ternary},
    signatures::{Parameter, Signature, SignatureKind, TypePredicate},
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// createUnionOfSignaturesForOverloadFailure (checker.go:9581).
    /// Parameter and this types are unions; the return is an intersection.
    pub(crate) fn union_signature_for_overload_failure(
        &mut self,
        candidates: &[Signature],
    ) -> Option<Signature> {
        let mut result = candidates.first()?.clone();
        let non_rest = |signature: &Signature| {
            signature.parameters.len()
                - usize::from(signature.parameters.last().is_some_and(|p| p.rest))
        };
        let minimum = candidates.iter().map(non_rest).min()?;
        let maximum = candidates.iter().map(non_rest).max()?;
        let mut parameters = Vec::new();
        for index in 0..maximum {
            let mut first = candidates.iter().find_map(|signature| {
                signature
                    .parameters
                    .get(index)
                    .or_else(|| signature.parameters.last().filter(|p| p.rest))
                    .cloned()
            })?;
            let types: Vec<_> = candidates
                .iter()
                .filter_map(|signature| self.signature_type_at_position(signature, index))
                .collect();
            if types.iter().any(|&ty| self.is_error(ty)) {
                return None;
            }
            first.r#type = self.union_with_subtype_reduction(&types)?;
            first.optional = index >= minimum;
            first.rest = false;
            first.written_text = None;
            parameters.push(first);
        }
        if let Some(mut rest) = candidates
            .iter()
            .find_map(|signature| signature.parameters.last().filter(|p| p.rest).cloned())
        {
            let mut types = Vec::new();
            for signature in candidates {
                if let Some(ty) = self.signature_effective_rest_type(signature) {
                    let element =
                        self.resolved_indexed_access_type(ty, self.intrinsics.number, false)?;
                    if self.is_error(element) {
                        return None;
                    }
                    types.push(element);
                }
            }
            let element = self.union_with_subtype_reduction(&types)?;
            let array = self.global_type_symbol("Array")?;
            rest.r#type = self.create_type_reference(array, vec![element]);
            rest.optional = false;
            rest.written_text = None;
            parameters.push(rest);
        }
        result.this_parameter =
            if let Some(mut first) = candidates.iter().find_map(|s| s.this_parameter.clone()) {
                let types: Vec<_> = candidates
                    .iter()
                    .filter_map(|s| s.this_parameter.as_ref().map(|p| p.r#type))
                    .collect();
                first.r#type = self.union_with_subtype_reduction(&types)?;
                first.written_text = None;
                Some(first)
            } else {
                None
            };
        result.parameters = parameters;
        result.r#type = self
            .get_intersection_type(&candidates.iter().map(|s| s.r#type).collect::<Vec<_>>(), None);
        result.predicate = None;
        result.target = None;
        result.written_return = None;
        result.union_contains_abstract = false;
        Some(result)
    }

    /// resolveIntersectionTypeMembers / findMixins / includeMixinType
    /// (checker.go:21302). Ordinary constructors remain overloads; only mixin
    /// constructors contribute intersections to their return types.
    pub(crate) fn intersection_signatures(
        &mut self,
        ty: TypeId,
        kind: SignatureKind,
    ) -> Option<Vec<Signature>> {
        let is_call = kind == SignatureKind::Call;
        let key = (ty, is_call);
        if let Some(cached) = self.composite_signature_types.get(&key) {
            return cached.clone();
        }
        let TypeData::Intersection { types, .. } = self.store.get(ty).data.clone() else {
            return None;
        };
        self.composite_signature_types.insert(key, None);
        let result = (|| {
            let lists = types
                .iter()
                .map(|&part| self.signatures_of_type_kind(part, kind))
                .collect::<Option<Vec<_>>>()?;
            let mut mixins: Vec<_> = lists
                .iter()
                .map(|signatures| !is_call && self.is_mixin_constructor_signatures(signatures))
                .collect();
            let constructor_count = lists.iter().filter(|list| !list.is_empty()).count();
            let mixin_count = mixins.iter().filter(|&&mixin| mixin).count();
            // An all-mixin intersection retains its first constructor as the
            // signature receiving the other mixins' instance types.
            if constructor_count > 0 && constructor_count == mixin_count {
                let first = mixins.iter().position(|&mixin| mixin)?;
                mixins[first] = false;
            }
            let has_mixins = mixins.iter().any(|&mixin| mixin);
            let mut result = Vec::new();
            for (index, signatures) in lists.iter().enumerate() {
                if mixins[index] {
                    continue;
                }
                for signature in signatures {
                    let mut signature = signature.clone();
                    if has_mixins {
                        let returns: Vec<_> = lists
                            .iter()
                            .enumerate()
                            .filter_map(|(i, list)| {
                                if i == index {
                                    Some(signature.r#type)
                                } else if mixins[i] {
                                    Some(list[0].r#type)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        signature.r#type = self.get_intersection_type(&returns, None);
                        signature.written_return = None;
                    }
                    if !result
                        .iter()
                        .any(|held| self.union_signature_matches(held, &signature, false, false, 0))
                    {
                        result.push(signature);
                    }
                }
            }
            Some(result)
        })();
        if result.is_some() {
            self.composite_signature_types.insert(key, result.clone());
        } else {
            self.composite_signature_types.remove(&key);
        }
        result
    }

    /// isMixinConstructorType (checker.go:17026): one nongeneric construct
    /// signature with one rest parameter of type any or (readonly) any[].
    fn is_mixin_constructor_signatures(&mut self, signatures: &[Signature]) -> bool {
        let [signature] = signatures else { return false };
        let [parameter] = signature.parameters.as_slice() else { return false };
        signature.type_parameters.is_empty()
            && parameter.rest
            && (self.store.get(parameter.r#type).flags.contains(crate::flags::TypeFlags::ANY)
                || self.signature_array_element(parameter.r#type) == Some(self.intrinsics.any))
    }

    /// resolveUnionTypeMembers caches both signature kinds and applies the
    /// array-member fallback only when ordinary call construction is empty.
    pub(crate) fn resolved_union_signatures(
        &mut self,
        ty: TypeId,
        kind: SignatureKind,
    ) -> Option<Vec<Signature>> {
        let key = (ty, kind == SignatureKind::Call);
        if let Some(cached) = self.composite_signature_types.get(&key) {
            return cached.clone();
        }
        let TypeData::Union { types, .. } = self.store.get(ty).data.clone() else { return None };
        self.composite_signature_types.insert(key, None);
        let mut result = self.union_signatures_of_types(&types, kind);
        if kind == SignatureKind::Call && result.as_ref().is_some_and(Vec::is_empty) {
            if let Some(fallback) = self.array_member_union_signatures(&types) {
                result = Some(fallback);
            }
        }
        if result.is_some() {
            self.composite_signature_types.insert(key, result.clone());
        } else {
            self.composite_signature_types.remove(&key);
        }
        result
    }

    /// getArrayMemberCallSignatures (checker.go:21070) transforms the same
    /// global Array/ReadonlyArray member on A[] | B[] into that member on
    /// (A | B)[]. A readonly constituent chooses `ReadonlyArray` for the result.
    fn array_member_union_signatures(&mut self, types: &[TypeId]) -> Option<Vec<Signature>> {
        let array = self.global_type_symbol("Array")?;
        let readonly_array = self.global_type_symbol("ReadonlyArray")?;
        let mut member_name = None;
        let mut readonly = false;
        let mut elements = Vec::new();
        for &ty in types {
            let TypeData::Anonymous { symbol, .. } = self.store.get(ty).data else { return None };
            let member = self.binder.symbols().get(symbol);
            let owner = self.binder.merged_symbol(member.parent?);
            let name = member.name.to_string();
            if owner == readonly_array {
                readonly = true;
            } else if owner != array {
                return None;
            }
            if member_name.as_ref().is_some_and(|old| old != &name) {
                return None;
            }
            member_name = Some(name);
            let parameters = self.local_type_parameter_types_of(owner)?;
            let [(parameter, _)] = parameters.as_slice() else { return None };
            let mapper = self.instantiated_signature_mappers.get(&ty)?;
            elements.push(mapper.iter().find(|(source, _)| source == parameter)?.1);
        }
        let element = self.get_union_type(&elements);
        let receiver = self
            .create_type_reference(if readonly { readonly_array } else { array }, vec![element]);
        let member = self.get_type_of_property_of_type(receiver, &member_name?)?;
        self.call_signatures_of_type(member)
    }

    pub(crate) fn union_signatures_of_types(
        &mut self,
        constituents: &[TypeId],
        kind: SignatureKind,
    ) -> Option<Vec<Signature>> {
        let mut lists = Vec::with_capacity(constituents.len());
        for &part in constituents {
            let list = self.signatures_of_type_kind(part, kind)?;
            if list.is_empty() {
                return Some(Vec::new());
            }
            lists.push(list);
        }
        let mut result = Vec::new();
        for (index, list) in lists.iter().enumerate() {
            for signature in list {
                if result
                    .iter()
                    .any(|held| self.union_signature_matches(held, signature, false, true, 0))
                {
                    continue;
                }
                let generic = !signature.type_parameters.is_empty();
                if generic && index != 0 {
                    continue;
                }
                let mut matches = Vec::new();
                for (other, candidates) in lists.iter().enumerate() {
                    let found = if other == index {
                        Some(signature.clone())
                    } else {
                        candidates
                            .iter()
                            .find(|held| {
                                self.union_signature_matches(held, signature, false, !generic, 0)
                            })
                            .or_else(|| {
                                (!generic)
                                    .then(|| {
                                        candidates.iter().find(|held| {
                                            self.union_signature_matches(
                                                held, signature, true, true, 0,
                                            )
                                        })
                                    })
                                    .flatten()
                            })
                            .cloned()
                    };
                    let Some(found) = found else { break };
                    matches.push(found);
                }
                if matches.len() == lists.len() {
                    if generic {
                        result.push(signature.clone());
                    } else {
                        let mut combined = signature.clone();
                        self.combine_union_signature_returns(&mut combined, &matches);
                        combined.this_parameter = self.union_this_parameter(&matches);
                        result.push(combined);
                    }
                }
            }
        }
        if !result.is_empty() {
            return Some(result);
        }
        // Native avoids a powerset of overloads: only one constituent may
        // contain multiple signatures on the parameter-intersection pass.
        let overloaded: Vec<_> =
            lists.iter().enumerate().filter(|(_, list)| list.len() > 1).map(|(i, _)| i).collect();
        if overloaded.len() > 1 || lists.is_empty() {
            return Some(Vec::new());
        }
        let master = overloaded.first().copied().unwrap_or(0);
        result.clone_from(&lists[master]);
        for (index, list) in lists.iter().enumerate() {
            if index == master {
                continue;
            }
            let right = &list[0];
            result = result
                .into_iter()
                .map(|left| self.combine_union_signature(left, right.clone()))
                .collect::<Option<Vec<_>>>()?;
        }
        Some(result)
    }

    /// compareSignaturesIdentical / isMatchingSignature (relater.go).
    fn union_signature_matches(
        &mut self,
        source: &Signature,
        target: &Signature,
        partial: bool,
        ignore_return: bool,
        depth: usize,
    ) -> bool {
        if depth >= 32
            || (source.kind == SignatureKind::Call) != (target.kind == SignatureKind::Call)
            || source.type_parameters.len() != target.type_parameters.len()
        {
            return false;
        }
        let source_min = self.signature_min_argument_count(source);
        let target_min = self.signature_min_argument_count(target);
        let exact_shape = self.signature_parameter_count(source)
            == self.signature_parameter_count(target)
            && source_min == target_min
            && self.signature_has_effective_rest(source)
                == self.signature_has_effective_rest(target);
        if !(exact_shape || partial && source_min <= target_min) {
            return false;
        }
        let Some(source) = self.align_union_signature(source.clone(), target, true, depth + 1)
        else {
            return false;
        };
        if let (Some(source_this), Some(target_this)) =
            (&source.this_parameter, &target.this_parameter)
            && !self.union_signature_types_match(
                source_this.r#type,
                target_this.r#type,
                partial,
                depth + 1,
            )
        {
            return false;
        }
        for index in 0..self.signature_parameter_count(target) {
            let source_type =
                self.signature_type_at_position(&source, index).unwrap_or(self.intrinsics.any);
            let target_type =
                self.signature_type_at_position(target, index).unwrap_or(self.intrinsics.any);
            if !self.union_signature_types_match(target_type, source_type, partial, depth + 1) {
                return false;
            }
        }
        if ignore_return {
            return true;
        }
        match (&source.predicate, &target.predicate) {
            (Some(left), Some(right)) => {
                source.predicate_kinds_match(target) == Some(true)
                    && match (left.r#type, right.r#type) {
                        (Some(a), Some(b)) => {
                            self.union_signature_types_match(a, b, partial, depth + 1)
                        }
                        (None, None) => true,
                        _ => false,
                    }
            }
            (None, None) => {
                self.union_signature_types_match(source.r#type, target.r#type, partial, depth + 1)
            }
            _ => false,
        }
    }

    fn union_signature_types_match(
        &mut self,
        source: TypeId,
        target: TypeId,
        partial: bool,
        depth: usize,
    ) -> bool {
        if partial {
            self.relate_ternary(source, target, Relation::Subtype) == Ternary::Related
        } else {
            self.union_signature_types_identical(source, target, depth)
        }
    }

    /// Exact identity for interned types, references and pure callback types.
    /// Unsupported structural identities decline; mutual assignability would
    /// incorrectly equate any and unknown with other types.
    fn union_signature_types_identical(
        &mut self,
        source: TypeId,
        target: TypeId,
        depth: usize,
    ) -> bool {
        if source == target {
            return true;
        }
        if depth >= 32 || self.is_error(source) || self.is_error(target) {
            return false;
        }
        let source = self.get_regular_type_of_literal_type(source);
        let target = self.get_regular_type_of_literal_type(target);
        if source == target {
            return true;
        }
        if let (Some((left, a)), Some((right, b))) = (
            self.type_reference_targets.get(&source).cloned(),
            self.type_reference_targets.get(&target).cloned(),
        ) {
            return left == right
                && a.len() == b.len()
                && a.into_iter()
                    .zip(b)
                    .all(|(a, b)| self.union_signature_types_identical(a, b, depth + 1));
        }
        for ty in [source, target] {
            if !matches!(self.store.get(ty).data, TypeData::Anonymous { signature: true, .. })
                || self.anonymous_properties.get(&ty).is_some_and(|(props, _)| !props.is_empty())
            {
                return false;
            }
        }
        let (Some(left), Some(right)) = (
            self.signature_types.get(&source).cloned(),
            self.signature_types.get(&target).cloned(),
        ) else {
            return false;
        };
        !left.is_empty()
            && left.len() == right.len()
            && left
                .iter()
                .zip(&right)
                .all(|(a, b)| self.union_signature_matches(a, b, false, false, depth + 1))
    }

    /// Alpha-map source parameters to target identities. The first pass checks
    /// constraints and defaults; compareTypeParametersIdentical's second-pass
    /// check intentionally ignores defaults (relater.go:2247).
    fn align_union_signature(
        &mut self,
        source: Signature,
        target: &Signature,
        defaults: bool,
        depth: usize,
    ) -> Option<Signature> {
        if source.type_parameters.len() != target.type_parameters.len() {
            return None;
        }
        if source.type_parameters.is_empty() {
            return Some(source);
        }
        let source_ids = self.type_parameter_types(&source)?;
        let target_ids = self.type_parameter_types(target)?;
        let identity = source_ids == target_ids;
        let map: Vec<_> = source_ids.iter().copied().zip(target_ids).collect();
        let names: Vec<_> = source.type_parameters.iter().map(|p| p.name.as_str()).collect();
        for (left, right) in source.type_parameters.iter().zip(&target.type_parameters) {
            for (a, b) in [(left.constraint, right.constraint)]
                .into_iter()
                .chain(defaults.then_some((left.default, right.default)))
            {
                let a = a.unwrap_or(self.intrinsics.unknown);
                let a =
                    if identity { a } else { self.instantiate_type(a, &map, &source_ids, &names) };
                let b = b.unwrap_or(self.intrinsics.unknown);
                if !self.union_signature_types_identical(a, b, depth + 1) {
                    return None;
                }
            }
        }
        if identity {
            return Some(source);
        }
        let names: Vec<_> = names.into_iter().map(str::to_owned).collect();
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        self.instantiate_signature(source, &map, &source_ids, &names)
    }

    fn union_this_parameter(&mut self, signatures: &[Signature]) -> Option<Parameter> {
        let mut first = signatures.iter().find_map(|s| s.this_parameter.clone())?;
        let types: Vec<_> =
            signatures.iter().filter_map(|s| s.this_parameter.as_ref().map(|p| p.r#type)).collect();
        first.r#type = self.get_intersection_type(&types, None);
        first.written_text = None;
        Some(first)
    }

    /// getUnionOrIntersectionTypePredicate (relater.go:2049) ignores false-only
    /// members, requires matching predicate kinds/positions and excludes asserts.
    fn combine_union_signature_returns(
        &mut self,
        result: &mut Signature,
        signatures: &[Signature],
    ) {
        result.r#type =
            self.get_union_type(&signatures.iter().map(|s| s.r#type).collect::<Vec<_>>());
        result.written_return = None;
        result.target = None;
        result.union_contains_abstract = signatures.iter().any(|signature| {
            signature.kind == SignatureKind::AbstractConstruct || signature.union_contains_abstract
        });
        let mut first: Option<&Signature> = None;
        let mut types = Vec::new();
        let mut valid = true;
        for signature in signatures {
            if let Some(predicate) = &signature.predicate {
                if predicate.asserts
                    || first
                        .is_some_and(|first| first.predicate_kinds_match(signature) != Some(true))
                {
                    valid = false;
                    break;
                }
                let Some(ty) = predicate.r#type else {
                    valid = false;
                    break;
                };
                first.get_or_insert(signature);
                types.push(ty);
            } else if !matches!(
                self.store.get(signature.r#type).data,
                TypeData::BooleanLiteral(false)
            ) {
                valid = false;
                break;
            }
        }
        result.predicate = if valid {
            first.and_then(|first| {
                let pred = first.predicate.as_ref()?;
                let parameter_name = if let Some(name) = &pred.parameter_name {
                    let index = first.parameters.iter().position(|p| &p.name == name)?;
                    Some(result.parameters.get(index)?.name.clone())
                } else {
                    None
                };
                Some(TypePredicate {
                    asserts: false,
                    parameter_name,
                    r#type: Some(self.get_union_type(&types)),
                    written_text: None,
                })
            })
        } else {
            None
        };
    }

    /// combineUnionOrIntersectionParameters (checker.go:21227), with union
    /// callers intersecting positional types and retaining the maximum minimum.
    fn combine_union_signature(
        &mut self,
        mut left: Signature,
        right: Signature,
    ) -> Option<Signature> {
        let right = if !left.type_parameters.is_empty() && !right.type_parameters.is_empty() {
            self.align_union_signature(right, &left, false, 0)?
        } else {
            right
        };
        if left.type_parameters.is_empty() && !right.type_parameters.is_empty() {
            let ids = self.type_parameter_types(&right)?;
            left.type_parameters.clone_from(&right.type_parameters);
            for (parameter, id) in left.type_parameters.iter_mut().zip(ids) {
                parameter.resolved_type = Some(id);
            }
        }
        let left_count = self.signature_parameter_count(&left);
        let right_count = self.signature_parameter_count(&right);
        let (longest, shorter, count) = if left_count >= right_count {
            (&left, &right, left_count)
        } else {
            (&right, &left, right_count)
        };
        let rest =
            self.signature_has_effective_rest(&left) || self.signature_has_effective_rest(&right);
        let extra_rest = rest && !self.signature_has_effective_rest(longest);
        let minimum =
            self.signature_min_argument_count(&left).max(self.signature_min_argument_count(&right));
        let mut parameters = Vec::new();
        for index in 0..count {
            let a = self.signature_type_at_position(longest, index)?;
            let b =
                self.signature_type_at_position(shorter, index).unwrap_or(self.intrinsics.unknown);
            if self.is_error(a) || self.is_error(b) {
                return None;
            }
            let mut ty = self.get_intersection_type(&[a, b], None);
            let is_rest = rest && !extra_rest && index + 1 == count;
            if is_rest {
                let array = self.global_type_symbol("Array")?;
                ty = self.create_type_reference(array, vec![ty]);
            }
            let left_name = left.parameters.get(index).map(|p| p.name.as_str());
            let right_name = right.parameters.get(index).map(|p| p.name.as_str());
            let name = match (left_name, right_name) {
                (Some(a), Some(b)) if a == b => a.to_owned(),
                (Some(a), None) | (None, Some(a)) => a.to_owned(),
                _ => format!("arg{index}"),
            };
            parameters.push(Parameter {
                name,
                optional: index >= minimum && !is_rest,
                rest: is_rest,
                r#type: ty,
                written_text: None,
            });
        }
        if extra_rest {
            let ty = self.signature_type_at_position(shorter, count)?;
            if self.is_error(ty) {
                return None;
            }
            let array = self.global_type_symbol("Array")?;
            parameters.push(Parameter {
                name: "args".to_owned(),
                optional: false,
                rest: true,
                r#type: self.create_type_reference(array, vec![ty]),
                written_text: None,
            });
        }
        let mut result = left.clone();
        result.parameters = parameters;
        if right.kind == SignatureKind::AbstractConstruct {
            result.kind = SignatureKind::AbstractConstruct;
        }
        result.this_parameter = self.union_this_parameter(&[left.clone(), right.clone()]);
        self.combine_union_signature_returns(&mut result, &[left, right]);
        Some(result)
    }
}
