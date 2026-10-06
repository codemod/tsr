// Archive-only observation of alias_body_evaluations, a separate semantic table.
#![allow(missing_docs)]
use crate::TypeId;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

pub const ORIGINS: [&str; 3] = ["nonnullable", "ordinary", "other"];
pub const FIELDS: [&str; 25] = [
    "requests",
    "key_items",
    "cache_missing",
    "cache_error",
    "cache_value",
    "conditional_calls",
    "conditional_none",
    "conditional_error",
    "conditional_value",
    "body_calls",
    "body_error",
    "body_value",
    "published_error",
    "published_value",
    "returned_none",
    "returned_error",
    "returned_value",
    "reject_not_alias",
    "reject_no_declaration",
    "reject_not_alias_node",
    "reject_no_body",
    "reject_conditional",
    "reject_depth",
    "reject_arity",
    "reject_parameter",
];
const N: usize = ORIGINS.len() * FIELDS.len();
static COUNTS: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
std::thread_local! {
    static LOCAL: Cell<[u64; N]> = const { Cell::new([0; N]) };
    static CALLSITE: Cell<Option<usize>> = const { Cell::new(None) };
}
fn enabled() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| std::env::var_os("TSR_MAPPER_PROFILE").is_some())
}
fn add(origin: usize, field: usize, value: u64) {
    if !enabled() {
        return;
    }
    let index = origin * FIELDS.len() + field;
    COUNTS[index].fetch_add(value, Ordering::Relaxed);
    LOCAL.with(|local| {
        let mut counts = local.get();
        counts[index] += value;
        local.set(counts);
    });
}
// Only the first evaluate_alias_body admission consumes this direct delegate
// tag. Nested evaluations do not inherit it unless they have their own delegate.
pub struct Callsite {
    prior: Option<usize>,
    _thread: PhantomData<Rc<()>>,
}
impl Callsite {
    #[must_use]
    pub fn enter(worker_kind: usize) -> Self {
        let origin = match worker_kind {
            7 => Some(0),
            3 => Some(1),
            _ => None,
        };
        let prior = enabled().then(|| CALLSITE.with(|site| site.replace(origin))).flatten();
        Self { prior, _thread: PhantomData }
    }
}
impl Drop for Callsite {
    fn drop(&mut self) {
        if enabled() {
            CALLSITE.with(|site| site.set(self.prior));
        }
    }
}
#[derive(Clone, Copy)]
pub enum Rejection {
    NotAlias = 17,
    NoDeclaration,
    NotAliasNode,
    NoBody,
    Conditional,
    Depth,
    Arity,
    Parameter,
}
pub struct Request {
    origin: usize,
    error: TypeId,
    _thread: PhantomData<Rc<()>>,
}
impl Request {
    #[must_use]
    pub fn enter(arguments: usize, error: TypeId) -> Self {
        let origin =
            if enabled() { CALLSITE.with(|site| site.replace(None)).unwrap_or(2) } else { 2 };
        add(origin, 0, 1);
        add(origin, 1, arguments as u64);
        Self { origin, error, _thread: PhantomData }
    }
    pub fn lookup(&self, answer: Option<TypeId>) {
        add(
            self.origin,
            match answer {
                None => 2,
                Some(v) if v == self.error => 3,
                Some(_) => 4,
            },
            1,
        );
    }
    #[must_use]
    pub fn conditional(&self, original: impl FnOnce() -> Option<TypeId>) -> Option<TypeId> {
        add(self.origin, 5, 1);
        let result = original();
        add(
            self.origin,
            match result {
                None => 6,
                Some(v) if v == self.error => 7,
                Some(_) => 8,
            },
            1,
        );
        result
    }
    #[must_use]
    pub fn test(&self, reason: Rejection, original: bool) -> bool {
        if !original {
            add(self.origin, reason as usize, 1);
        }
        original
    }
    #[must_use]
    pub fn option<T>(&self, reason: Rejection, original: Option<T>) -> Option<T> {
        if original.is_none() {
            add(self.origin, reason as usize, 1);
        }
        original
    }
    #[must_use]
    pub fn alias_node<'a>(&self, original: Option<tsr_ast::Node<'a>>) -> Option<tsr_ast::Node<'a>> {
        if !matches!(original, Some(tsr_ast::Node::TypeAliasDeclaration(_))) {
            add(self.origin, Rejection::NotAliasNode as usize, 1);
        }
        original
    }
    pub fn body_begin(&self) {
        add(self.origin, 9, 1);
    }
    pub fn body_returned(&self, value: TypeId) {
        add(self.origin, if value == self.error { 10 } else { 11 }, 1);
    }
    pub fn published(&self, value: TypeId) {
        add(self.origin, if value == self.error { 12 } else { 13 }, 1);
    }
    pub fn returned(&self, result: Option<TypeId>) {
        add(
            self.origin,
            match result {
                None => 14,
                Some(v) if v == self.error => 15,
                Some(_) => 16,
            },
            1,
        );
    }
}
fn names() -> Vec<String> {
    ORIGINS
        .iter()
        .flat_map(|origin| FIELDS.iter().map(move |field| format!("alias_body_{origin}_{field}")))
        .collect()
}
pub fn global_counts() -> Vec<(String, u64)> {
    names().into_iter().zip(COUNTS.iter().map(|c| c.load(Ordering::Relaxed))).collect()
}
pub fn local_counts() -> Vec<(String, u64)> {
    names().into_iter().zip(LOCAL.with(Cell::get)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn with_checker(source: &str, test: impl FnOnce(&mut crate::Checker<'_, '_>, tsr_ast::NodeId)) {
        assert!(enabled(), "run archive tests with TSR_MAPPER_PROFILE=1");
        LOCAL.with(|counts| counts.set([0; N]));
        CALLSITE.with(|site| assert!(site.get().is_none()));
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "entry.ts", text: source },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        test(&mut checker, parsed.source_file.node_id.unwrap());
        CALLSITE.with(|site| assert!(site.get().is_none()));
    }
    fn count(name: &str) -> u64 {
        local_counts().into_iter().find(|(key, _)| key == name).unwrap().1
    }
    #[test]
    fn real_first_repeat_evaluates_body_once() {
        with_checker("type Identity<T> = T;", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Identity").unwrap();
            let number = checker.intrinsics.number;
            assert_eq!(checker.evaluate_alias_body(symbol, &[number]), Some(number));
            assert_eq!(checker.evaluate_alias_body(symbol, &[number]), Some(number));
            assert_eq!(count("alias_body_other_requests"), 2);
            assert_eq!(count("alias_body_other_cache_missing"), 1);
            assert_eq!(count("alias_body_other_cache_value"), 1);
            assert_eq!(count("alias_body_other_body_calls"), 1);
            assert_eq!(count("alias_body_other_published_value"), 1);
        });
    }
    #[test]
    fn real_evaluated_error_is_reused_as_none_without_body_work() {
        with_checker("type Identity<T> = T;", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Identity").unwrap();
            let error = checker.intrinsics.error;
            assert_eq!(checker.evaluate_alias_body(symbol, &[error]), None);
            assert_eq!(checker.alias_body_evaluations[&(symbol, vec![error])], error);
            assert_eq!(checker.evaluate_alias_body(symbol, &[error]), None);
            assert_eq!(count("alias_body_other_body_calls"), 1);
            assert_eq!(count("alias_body_other_body_error"), 1);
            assert_eq!(count("alias_body_other_cache_error"), 1);
            assert_eq!(count("alias_body_other_returned_none"), 2);
            let number = checker.intrinsics.number;
            assert_eq!(checker.evaluate_alias_body(symbol, &[number]), Some(number));
            assert_eq!(count("alias_body_other_body_calls"), 2);
        });
    }
    #[test]
    fn real_depth_refusal_is_published_and_later_same_tuple_stays_cached() {
        with_checker("type Identity<T> = T;", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Identity").unwrap();
            let number = checker.intrinsics.number;
            checker.instantiation_depth = 100;
            assert_eq!(checker.evaluate_alias_body(symbol, &[number]), None);
            assert_eq!(count("alias_body_other_reject_depth"), 1);
            checker.instantiation_depth = 0;
            assert_eq!(checker.evaluate_alias_body(symbol, &[number]), None);
            assert_eq!(count("alias_body_other_cache_error"), 1);
            assert_eq!(count("alias_body_other_body_calls"), 0);
            // A different tuple exercises a later actual evaluation.
            let string = checker.intrinsics.string;
            assert_eq!(checker.evaluate_alias_body(symbol, &[string]), Some(string));
            assert_eq!(count("alias_body_other_body_calls"), 1);
        });
    }
    #[test]
    fn real_arity_and_nonalias_rejection_precede_body_work() {
        with_checker("type Identity<T> = T; interface Box<T> {}", |checker, root| {
            let alias = checker.binder.lookup_local(root, "Identity").unwrap();
            let interface = checker.binder.lookup_local(root, "Box").unwrap();
            let number = checker.intrinsics.number;
            assert_eq!(checker.evaluate_alias_body(alias, &[]), None);
            assert_eq!(checker.evaluate_alias_body(interface, &[number]), None);
            assert_eq!(count("alias_body_other_reject_arity"), 1);
            assert_eq!(count("alias_body_other_reject_not_alias"), 1);
            assert_eq!(count("alias_body_other_body_calls"), 0);
            assert_eq!(count("alias_body_other_published_error"), 2);
        });
    }
    #[test]
    fn real_same_spelled_aliases_and_ordered_tuples_stay_distinct() {
        with_checker(
            "function a() { type Same<T,U> = T; } function b() { type Same<T,U> = U; }",
            |checker, _| {
                let aliases: Vec<_> = checker
                    .binder
                    .symbols()
                    .iter()
                    .filter(|(_, symbol)| {
                        symbol.name == "Same"
                            && symbol.flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
                    })
                    .map(|(id, _)| id)
                    .collect();
                assert_eq!(aliases.len(), 2);
                let number = checker.intrinsics.number;
                let string = checker.intrinsics.string;
                assert_eq!(
                    checker.evaluate_alias_body(aliases[0], &[number, string]),
                    Some(number)
                );
                assert_eq!(
                    checker.evaluate_alias_body(aliases[1], &[number, string]),
                    Some(string)
                );
                assert_eq!(
                    checker.evaluate_alias_body(aliases[0], &[string, number]),
                    Some(string)
                );
                assert_eq!(
                    checker.evaluate_alias_body(aliases[0], &[number, string]),
                    Some(number)
                );
                assert_eq!(count("alias_body_other_body_calls"), 3);
                assert_eq!(count("alias_body_other_cache_value"), 1);
            },
        );
    }
    #[test]
    fn real_nonnullable_direct_admissions_match_outer_delegate_calls() {
        with_checker("type NonNullable<T> = T & {};", |checker, _| {
            let number = checker.intrinsics.number;
            let first = checker.get_global_non_nullable_type_instantiation(number);
            assert_ne!(first, checker.intrinsics.error);
            assert_eq!(checker.get_global_non_nullable_type_instantiation(number), first);
            assert_eq!(count("alias_body_nonnullable_requests"), 1);
            assert_eq!(count("alias_body_nonnullable_body_calls"), 1);
            assert_eq!(count("alias_body_nonnullable_returned_value"), 1);
        });
    }
    #[test]
    fn direct_callsite_is_consumed_once_and_nested_evaluation_is_other() {
        with_checker("type Identity<T> = T;", |checker, _| {
            let error = checker.intrinsics.error;
            {
                let _scope = Callsite::enter(7);
                let direct = Request::enter(1, error);
                direct.lookup(Some(error));
                direct.returned(None);
                let nested = Request::enter(1, error);
                nested.lookup(Some(error));
                nested.returned(None);
            }
            assert_eq!(count("alias_body_nonnullable_requests"), 1);
            assert_eq!(count("alias_body_other_requests"), 1);
            assert_eq!(count("alias_body_nonnullable_cache_error"), 1);
            assert_eq!(count("alias_body_other_cache_error"), 1);
        });
    }
}
