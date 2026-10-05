// Archive-only child of mapper_key_probe. Never serves a semantic query.
#![allow(missing_docs)]
use super::{Checker, Key, TypeId, active, allocation, enabled};
use crate::objects::AnonymousProperty;
use crate::signatures::{Parameter, Signature};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

const FIELDS: [&str; 25] = [
    "calls",
    "property_images",
    "property_elements",
    "property_owned_strings",
    "property_string_len_bytes",
    "property_string_capacity_bytes",
    "property_vec_capacity_bytes",
    "property_allocations",
    "property_requested_bytes",
    "property_usable_bytes",
    "signature_images",
    "signature_elements",
    "signature_type_parameter_elements",
    "signature_value_parameter_elements",
    "signature_this_parameters",
    "signature_owned_strings",
    "signature_string_len_bytes",
    "signature_string_capacity_bytes",
    "signature_vec_capacity_bytes",
    "signature_arc_clones",
    "signature_allocations",
    "signature_requested_bytes",
    "signature_usable_bytes",
    "origin_refusals",
    "metadata_refusals",
];
const BUCKETS: [&str; 5] = ["hit", "active", "error", "miss", "refusal"];
const WIDTH: usize = FIELDS.len();
const TOTAL: usize = WIDTH * BUCKETS.len();
static COUNTS: [AtomicU64; TOTAL] = [const { AtomicU64::new(0) }; TOTAL];
std::thread_local! { static LOCAL: Cell<[u64; TOTAL]> = const { Cell::new([0; TOTAL]) }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum Outcome {
    Hit,
    Active,
    Error,
    Miss,
    Refusal,
}

fn classify(result: Option<TypeId>, error: TypeId, in_progress: bool) -> Outcome {
    match result {
        None => Outcome::Miss,
        Some(_) if in_progress => Outcome::Active,
        Some(value) if value == error => Outcome::Error,
        Some(_) => Outcome::Hit,
    }
}

// Only the already-tagged clone call is enclosed. Scanning the returned clone
// happens after the allocation delta, outside its allocation tag.
fn allocation_snapshot(tag: usize) -> [u64; 3] {
    super::LOCAL_COUNTS.with(|local| {
        let values = local.get();
        let offset = 64 + (tag - 1) * 3;
        [values[offset], values[offset + 1], values[offset + 2]]
    })
}
fn delta(before: [u64; 3], after: [u64; 3]) -> [u64; 3] {
    std::array::from_fn(|index| after[index].checked_sub(before[index]).unwrap())
}
fn string(values: &mut [u64; WIDTH], offset: usize, value: &String) {
    values[offset] += 1;
    values[offset + 1] += value.len() as u64;
    values[offset + 2] += value.capacity() as u64;
}
fn parameter(values: &mut [u64; WIDTH], offset: usize, value: &Parameter) {
    string(values, offset, &value.name);
    if let Some(text) = &value.written_text {
        string(values, offset, text);
    }
}

pub struct LiteralCopies {
    observed: bool,
    origin_found: bool,
    outcome: Outcome,
    values: [u64; WIDTH],
}
impl Default for LiteralCopies {
    fn default() -> Self {
        Self::new()
    }
}
impl LiteralCopies {
    #[must_use]
    pub fn new() -> Self {
        Self {
            observed: enabled(),
            origin_found: false,
            outcome: Outcome::Refusal,
            values: [0; WIDTH],
        }
    }
    pub fn origin_found(&mut self) {
        self.origin_found = true;
    }
    pub fn lookup(&mut self, owner: usize, key: &Key, result: Option<TypeId>, error: TypeId) {
        if self.observed {
            self.outcome = classify(result, error, result.is_some() && active(owner, 3, key));
        }
    }
}
impl Drop for LiteralCopies {
    fn drop(&mut self) {
        if !self.observed {
            return;
        }
        self.values[0] = 1;
        if self.outcome == Outcome::Refusal {
            self.values[if self.origin_found { 24 } else { 23 }] = 1;
        }
        let offset = self.outcome as usize * WIDTH;
        LOCAL.with(|local| {
            let mut values = local.get();
            for (index, value) in self.values.iter().copied().enumerate() {
                values[offset + index] += value;
                COUNTS[offset + index].fetch_add(value, Ordering::Relaxed);
            }
            local.set(values);
        });
    }
}

pub(crate) fn properties(
    checker: &Checker<'_, '_>,
    id: TypeId,
    copies: &mut LiteralCopies,
) -> Option<Vec<AnonymousProperty>> {
    if !copies.observed {
        return checker.anonymous_properties.get(&id).map(|(p, _)| p.clone());
    }
    let before = allocation_snapshot(8);
    let result = {
        let _scope = allocation::enter(8);
        checker.anonymous_properties.get(&id).map(|(p, _)| p.clone())
    };
    let allocated = delta(before, allocation_snapshot(8));
    if copies.observed {
        copies.values[7..10].copy_from_slice(&allocated);
        if let Some(properties) = &result {
            copies.values[1] = 1;
            copies.values[2] = properties.len() as u64;
            copies.values[6] =
                (properties.capacity() * std::mem::size_of::<AnonymousProperty>()) as u64;
            for property in properties {
                for value in [&property.name, &property.printed_name, &property.printed_type] {
                    string(&mut copies.values, 3, value);
                }
                if let Some(write) = &property.accessor_write {
                    parameter(&mut copies.values, 3, write);
                }
            }
        }
    }
    result
}
pub(crate) fn signatures(
    checker: &Checker<'_, '_>,
    id: TypeId,
    copies: &mut LiteralCopies,
) -> Option<Vec<Signature>> {
    if !copies.observed {
        return checker.signature_types.get(&id).cloned();
    }
    let before = allocation_snapshot(9);
    let result = {
        let _scope = allocation::enter(9);
        checker.signature_types.get(&id).cloned()
    };
    let allocated = delta(before, allocation_snapshot(9));
    if copies.observed {
        copies.values[20..23].copy_from_slice(&allocated);
        if let Some(signatures) = &result {
            copies.values[10] = 1;
            copies.values[11] = signatures.len() as u64;
            copies.values[18] = (signatures.capacity() * std::mem::size_of::<Signature>()) as u64;
            for signature in signatures {
                copies.values[12] += signature.type_parameters.len() as u64;
                copies.values[13] += signature.parameters.len() as u64;
                copies.values[14] += u64::from(signature.this_parameter.is_some());
                copies.values[19] += u64::from(signature.target.is_some());
                copies.values[18] += (signature.type_parameters.capacity()
                    * std::mem::size_of::<crate::signatures::TypeParameter>()
                    + signature.parameters.capacity() * std::mem::size_of::<Parameter>())
                    as u64;
                for t in &signature.type_parameters {
                    string(&mut copies.values, 15, &t.name);
                    if let Some(text) = &t.written_constraint {
                        string(&mut copies.values, 15, text);
                    }
                }
                for p in signature.parameters.iter().chain(signature.this_parameter.iter()) {
                    parameter(&mut copies.values, 15, p);
                }
                if let Some(text) = &signature.written_return {
                    string(&mut copies.values, 15, text);
                }
                if let Some(predicate) = &signature.predicate {
                    for text in
                        [&predicate.parameter_name, &predicate.written_text].into_iter().flatten()
                    {
                        string(&mut copies.values, 15, text);
                    }
                }
                // target is Arc<Signature>: its reference count is incremented,
                // but its nested payload is not cloned by Signature::clone.
            }
        }
    }
    result
}

#[must_use]
pub fn global_counts() -> Vec<(String, u64)> {
    BUCKETS
        .iter()
        .enumerate()
        .flat_map(|(bucket, name)| {
            FIELDS.iter().enumerate().map(move |(field, label)| {
                (
                    format!("literal_copy_{name}_{label}"),
                    COUNTS[bucket * WIDTH + field].load(Ordering::Relaxed),
                )
            })
        })
        .collect()
}
pub fn local_counts() -> [u64; TOTAL] {
    LOCAL.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_ast::Statement;
    use tsr_core::Arena;

    fn clear() {
        assert!(enabled(), "run archive controls with TSR_MAPPER_PROFILE=1");
        LOCAL.with(|local| local.set([0; TOTAL]));
        for counter in &COUNTS {
            counter.store(0, Ordering::Relaxed);
        }
    }
    fn value(bucket: Outcome, field: usize) -> u64 {
        local_counts()[bucket as usize * WIDTH + field]
    }
    fn with_literal(source: &str, action: impl FnOnce(&mut Checker<'_, '_>, TypeId, &[TypeId])) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "copy.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(node) = parsed.source_file.statements[0] else {
            panic!("function")
        };
        let signature = checker
            .get_signatures_of_symbol(bound.symbol_of(node.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let parameters = checker.type_parameter_types(&signature).unwrap();
        let original = signature.parameters[0].r#type;
        assert!(checker.type_literal_origins.contains_key(&original));
        clear();
        action(&mut checker, original, &parameters);
    }

    #[test]
    fn production_lookup_classifies_hits_active_errors_misses_and_refusals() {
        with_literal(
            "declare function f<T>(x: { value: T }): T;",
            |checker, original, parameters| {
                let map = [(parameters[0], checker.intrinsics.string)];
                let key = (original, map.to_vec());
                let owner = std::ptr::from_ref(checker) as usize;
                let image =
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]).unwrap();
                assert_ne!(image, checker.intrinsics.error);
                assert_eq!(value(Outcome::Miss, 0), 1);
                assert_eq!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]),
                    Some(image)
                );
                assert_eq!(value(Outcome::Hit, 0), 1);
                super::super::ACTIVE.with(|frames| {
                    frames.borrow_mut().push((owner, super::super::table_domain(3), key.clone()));
                });
                assert_eq!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]),
                    Some(image)
                );
                super::super::ACTIVE.with(|frames| {
                    frames.borrow_mut().pop().unwrap();
                });
                assert_eq!(value(Outcome::Active, 0), 1);
                checker.instantiated_objects.insert(key, checker.intrinsics.error);
                assert_eq!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]),
                    Some(checker.intrinsics.error)
                );
                assert_eq!(value(Outcome::Error, 0), 1);
                assert!(
                    checker
                        .instantiate_type_literal(
                            checker.intrinsics.number,
                            &map,
                            parameters,
                            &["T"]
                        )
                        .is_none()
                );
                assert_eq!(value(Outcome::Refusal, 23), 1);
                checker.anonymous_properties.remove(&original);
                checker.signature_types.remove(&original);
                assert!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]).is_none()
                );
                assert_eq!(value(Outcome::Refusal, 24), 1);
                for bucket in [Outcome::Hit, Outcome::Active, Outcome::Error, Outcome::Miss] {
                    assert_eq!(value(bucket, 2), 1);
                    assert_eq!(value(bucket, 3), 3);
                    assert!(value(bucket, 7) > 0);
                    assert!(value(bucket, 8) >= value(bucket, 4));
                }
            },
        );
    }

    #[test]
    fn empty_images_and_distinct_ordered_maps_remain_distinct() {
        with_literal(
            "declare function f<T,U>(x: { value: T; other: U }): U;",
            |checker, original, parameters| {
                let first = [
                    (parameters[0], checker.intrinsics.string),
                    (parameters[1], checker.intrinsics.number),
                ];
                let reverse = [first[1], first[0]];
                let distinct = [(parameters[0], checker.intrinsics.number), first[1]];
                for map in [&first, &reverse, &distinct] {
                    checker
                        .instantiated_objects
                        .insert((original, map.to_vec()), checker.intrinsics.boolean);
                }
                checker.anonymous_properties.insert(original, (Vec::new(), true));
                checker.signature_types.insert(original, Vec::new());
                for (printing, map) in
                    [(true, &first), (false, &reverse), (true, &distinct), (false, &first)]
                {
                    checker.identity_unmapped_type_parameters = printing;
                    assert_eq!(
                        checker.instantiate_type_literal(original, map, parameters, &["T", "U"]),
                        Some(checker.intrinsics.boolean)
                    );
                }
                checker.identity_unmapped_type_parameters = false;
                assert_eq!(value(Outcome::Hit, 0), 4);
                assert_eq!(value(Outcome::Hit, 1), 4);
                assert_eq!(value(Outcome::Hit, 10), 4);
                for field in [2, 7, 8, 11, 20, 21] {
                    assert_eq!(value(Outcome::Hit, field), 0);
                }
                assert_eq!(checker.instantiated_objects.len(), 3);
            },
        );
    }

    #[test]
    fn signature_payload_counts_owned_fields_without_deep_cloning_arc_target() {
        with_literal(
            "declare function f<T>(x: { <U extends T>(arg: U): T }): T;",
            |checker, original, parameters| {
                let mut signature = checker.signature_types.get(&original).unwrap()[0].clone();
                signature.target = Some(std::sync::Arc::new(signature.clone()));
                signature.this_parameter = Some(Parameter {
                    name: "self".into(),
                    optional: false,
                    rest: false,
                    r#type: checker.intrinsics.string,
                    written_text: Some("typeof self".into()),
                });
                signature.written_return = Some("return-spelling".into());
                signature.predicate = Some(crate::signatures::TypePredicate {
                    asserts: false,
                    parameter_name: Some("arg".into()),
                    r#type: Some(checker.intrinsics.string),
                    written_text: Some("Guard.Value".into()),
                });
                let expected_strings = 1
                    + usize::from(signature.type_parameters[0].written_constraint.is_some())
                    + 1
                    + usize::from(signature.parameters[0].written_text.is_some())
                    + 2
                    + 1
                    + 2;
                checker.signature_types.insert(original, vec![signature]);
                let map = [(parameters[0], checker.intrinsics.string)];
                checker
                    .instantiated_objects
                    .insert((original, map.to_vec()), checker.intrinsics.boolean);
                assert_eq!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]),
                    Some(checker.intrinsics.boolean)
                );
                assert_eq!(value(Outcome::Hit, 11), 1);
                assert_eq!(value(Outcome::Hit, 12), 1);
                assert_eq!(value(Outcome::Hit, 13), 1);
                assert_eq!(value(Outcome::Hit, 14), 1);
                assert_eq!(value(Outcome::Hit, 15), expected_strings as u64);
                assert_eq!(value(Outcome::Hit, 19), 1);
                assert!(value(Outcome::Hit, 20) > 0);
            },
        );
    }

    #[test]
    fn accessor_parameter_strings_are_part_of_the_property_clone() {
        with_literal(
            "declare function f<T>(x: { value: T }): T;",
            |checker, original, parameters| {
                let properties = &mut checker.anonymous_properties.get_mut(&original).unwrap().0;
                properties[0].accessor_write = Some(Parameter {
                    name: "setter".into(),
                    optional: false,
                    rest: false,
                    r#type: checker.intrinsics.string,
                    written_text: Some("typeof setter".into()),
                });
                let expected = properties[0].name.len()
                    + properties[0].printed_name.len()
                    + properties[0].printed_type.len()
                    + "setter".len()
                    + "typeof setter".len();
                let map = [(parameters[0], checker.intrinsics.string)];
                checker
                    .instantiated_objects
                    .insert((original, map.to_vec()), checker.intrinsics.boolean);
                assert_eq!(
                    checker.instantiate_type_literal(original, &map, parameters, &["T"]),
                    Some(checker.intrinsics.boolean)
                );
                assert_eq!(value(Outcome::Hit, 3), 5);
                assert_eq!(value(Outcome::Hit, 4), expected as u64);
                assert!(value(Outcome::Hit, 8) >= expected as u64);
            },
        );
    }

    #[test]
    fn allocation_deltas_are_local_and_nested_tags_restore() {
        clear();
        let before = allocation_snapshot(8);
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _tag = allocation::enter(8);
                    std::hint::black_box(vec![1u8; 1024]);
                    assert!(allocation_snapshot(8)[1] >= 1024);
                })
                .join()
                .unwrap();
        });
        assert_eq!(allocation_snapshot(8), before);
        let outer = allocation::enter(8);
        {
            let _inner = allocation::enter(9);
            std::hint::black_box(vec![2u8; 256]);
        }
        let after_inner = allocation_snapshot(8);
        std::hint::black_box(vec![3u8; 512]);
        drop(outer);
        assert_eq!(delta(after_inner, allocation_snapshot(8))[1], 512);
    }
}
