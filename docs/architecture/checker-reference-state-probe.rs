// Archive-only observation; it never supplies a semantic cache answer.
#![allow(missing_docs)]
use crate::TypeId;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tsr_binder::SymbolId;

pub const SITES: [&str; 19] = [
    "empty_lookup",
    "empty_insert",
    "deferred_lookup",
    "deferred_insert",
    "string_mapping_insert",
    "identity_mapping_insert",
    "ordinary_lookup",
    "keyword_insert",
    "conditional_insert",
    "alias_body_insert",
    "template_insert",
    "normalized_mapping_insert",
    "named_insert",
    "mapped_sequence_insert",
    "nonnullable_lookup",
    "nonnullable_insert",
    "index_original_lookup",
    "index_body_lookup",
    "member_body_lookup",
];
pub const FIELDS: [&str; 10] = [
    "calls",
    "key_items",
    "misses",
    "errors",
    "values",
    "reserved_answers",
    "framed_answers",
    "new_entries",
    "replacement_entries",
    "replaced_errors",
];
pub const REQUESTS: [&str; 4] = ["ordinary", "nonnullable", "empty", "deferred"];
pub const REQUEST_FIELDS: [&str; 9] = [
    "requests",
    "reentries",
    "returned_error",
    "returned_value",
    "unreturned",
    "reservations",
    "reservation_closed",
    "cache_hit_returns",
    "other_returns",
];
pub const WORKERS: [&str; 8] = [
    "string_mapping",
    "identity_mapping",
    "conditional",
    "alias_body",
    "template",
    "normalized_mapping",
    "mapped_sequence",
    "nonnullable_body",
];
pub const WORKER_FIELDS: [&str; 4] = ["calls", "refused", "returned_error", "returned_value"];
const REQUEST_BASE: usize = SITES.len() * FIELDS.len();
const WORKER_BASE: usize = REQUEST_BASE + REQUESTS.len() * REQUEST_FIELDS.len();
const N: usize = WORKER_BASE + WORKERS.len() * WORKER_FIELDS.len();
static COUNTS: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
std::thread_local! {
    static LOCAL: Cell<[u64; N]> = const { Cell::new([0; N]) };
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}
fn enabled() -> bool {
    static VALUE: OnceLock<bool> = OnceLock::new();
    *VALUE.get_or_init(|| std::env::var_os("TSR_MAPPER_PROFILE").is_some())
}
fn add(index: usize, value: u64) {
    COUNTS[index].fetch_add(value, Ordering::Relaxed);
    LOCAL.with(|local| {
        let mut counts = local.get();
        counts[index] += value;
        local.set(counts);
    });
}
struct Frame {
    owner: usize,
    symbol: Option<SymbolId>,
    arguments: Vec<TypeId>,
    reserved: Option<TypeId>,
    kind: usize,
}
impl Frame {
    fn matches(&self, owner: usize, key: &(SymbolId, Vec<TypeId>)) -> bool {
        self.owner == owner && self.symbol == Some(key.0) && self.arguments == key.1
    }
}
pub fn lookup(
    owner: usize,
    site: usize,
    key: &(SymbolId, Vec<TypeId>),
    answer: Option<TypeId>,
    error: TypeId,
) {
    if !enabled() {
        return;
    }
    let base = site * FIELDS.len();
    add(base, 1);
    add(base + 1, key.1.len() as u64);
    add(
        base + match answer {
            None => 2,
            Some(v) if v == error => 3,
            Some(_) => 4,
        },
        1,
    );
    if let Some(value) = answer {
        let (framed, reserved) = FRAMES.with(|frames| {
            let frames = frames.borrow();
            (
                frames.iter().any(|f| f.matches(owner, key)),
                frames.iter().any(|f| f.matches(owner, key) && f.reserved == Some(value)),
            )
        });
        add(base + 5, u64::from(reserved));
        add(base + 6, u64::from(framed));
    }
}
// Called immediately before the original insert. No semantic work runs between
// this hook and that insert; successful pool receipts reject unwind/abort.
pub fn publication(
    owner: usize,
    site: usize,
    key: &(SymbolId, Vec<TypeId>),
    value: TypeId,
    error: TypeId,
) {
    if !enabled() {
        return;
    }
    let base = site * FIELDS.len();
    add(base, 1);
    add(base + 1, key.1.len() as u64);
    add(base + if value == error { 3 } else { 4 }, 1);
    if site == 12 {
        FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            let frame = frames
                .iter_mut()
                .rev()
                .find(|f| f.matches(owner, key))
                .expect("named reservation has an exact owning request");
            assert!(frame.reserved.replace(value).is_none());
            add(REQUEST_BASE + frame.kind * REQUEST_FIELDS.len() + 5, 1);
        });
    }
}
pub fn inserted(site: usize, previous: Option<TypeId>, error: TypeId) {
    if !enabled() {
        return;
    }
    let base = site * FIELDS.len();
    add(base + if previous.is_some() { 8 } else { 7 }, 1);
    add(base + 9, u64::from(previous == Some(error)));
}
pub struct Request {
    index: Option<usize>,
    kind: usize,
    returned: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Request {
    #[must_use]
    pub fn enter(
        owner: usize,
        kind: usize,
        symbol: Option<SymbolId>,
        arguments: &[TypeId],
    ) -> Self {
        let index = enabled().then(|| {
            FRAMES.with(|frames| {
                let mut frames = frames.borrow_mut();
                let reentry = symbol.is_some()
                    && frames.iter().any(|f| {
                        f.owner == owner && f.symbol == symbol && f.arguments == arguments
                    });
                let base = REQUEST_BASE + kind * REQUEST_FIELDS.len();
                add(base, 1);
                add(base + 1, u64::from(reentry));
                let index = frames.len();
                frames.push(Frame {
                    owner,
                    symbol,
                    arguments: arguments.to_vec(),
                    reserved: None,
                    kind,
                });
                index
            })
        });
        Self { index, kind, returned: false, _thread: PhantomData }
    }
    pub fn bind(&self, symbol: SymbolId) {
        if let Some(index) = self.index {
            FRAMES.with(|frames| {
                let mut frames = frames.borrow_mut();
                assert!(frames[index].symbol.replace(symbol).is_none());
                let frame = &frames[index];
                let reentry = frames[..index].iter().any(|other| {
                    other.owner == frame.owner
                        && other.symbol == frame.symbol
                        && other.arguments == frame.arguments
                });
                add(REQUEST_BASE + self.kind * REQUEST_FIELDS.len() + 1, u64::from(reentry));
            });
        }
    }
    pub fn returned(&mut self, value: TypeId, error: TypeId, cache_hit: bool) {
        if self.index.is_none() {
            return;
        }
        assert!(!self.returned);
        self.returned = true;
        let base = REQUEST_BASE + self.kind * REQUEST_FIELDS.len();
        add(base + if value == error { 2 } else { 3 }, 1);
        add(base + if cache_hit { 7 } else { 8 }, 1);
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Some(index) = self.index {
            FRAMES.with(|frames| {
                let mut frames = frames.borrow_mut();
                assert_eq!(frames.len(), index + 1, "request guards are nested");
                let frame = frames.pop().unwrap();
                let base = REQUEST_BASE + self.kind * REQUEST_FIELDS.len();
                add(base + 4, u64::from(!self.returned));
                add(base + 6, u64::from(frame.reserved.is_some()));
            });
        }
    }
}
pub fn worker(
    kind: usize,
    error: TypeId,
    original: impl FnOnce() -> Option<TypeId>,
) -> Option<TypeId> {
    if !enabled() {
        return original();
    }
    let base = WORKER_BASE + kind * WORKER_FIELDS.len();
    add(base, 1);
    let result = original();
    add(
        base + match result {
            None => 1,
            Some(v) if v == error => 2,
            Some(_) => 3,
        },
        1,
    );
    result
}
fn names() -> Vec<String> {
    let mut names = Vec::with_capacity(N);
    for (prefix, sites, fields) in [
        ("state_site", SITES.as_slice(), FIELDS.as_slice()),
        ("state_request", REQUESTS.as_slice(), REQUEST_FIELDS.as_slice()),
        ("state_worker", WORKERS.as_slice(), WORKER_FIELDS.as_slice()),
    ] {
        for site in sites {
            for field in fields {
                names.push(format!("{prefix}_{site}_{field}"));
            }
        }
    }
    names
}
pub fn global_counts() -> Vec<(String, u64)> {
    names().into_iter().zip(COUNTS.iter().map(|c| c.load(Ordering::Relaxed))).collect()
}
pub fn local_counts() -> Vec<(String, u64)> {
    names().into_iter().zip(LOCAL.with(Cell::get)).collect()
}

// These expand at each real table call. Original key/value expressions execute
// once, in receiver/key/value order; map methods supply every semantic answer.
macro_rules! get {
    ($checker:ident, $site:expr, $key:expr) => {{
        let owner = std::ptr::from_ref($checker) as usize;
        let error = $checker.intrinsics.error;
        let key = $key;
        let answer = $checker.instantiations.get(key);
        $crate::reference_state_probe::lookup(owner, $site, key, answer.copied(), error);
        answer
    }};
}
macro_rules! insert {
    ($checker:ident, $site:expr, $key:expr, $value:expr) => {{
        let owner = std::ptr::from_ref($checker) as usize;
        let error = $checker.intrinsics.error;
        let key = $key;
        let value = $value;
        $crate::reference_state_probe::publication(owner, $site, &key, value, error);
        let previous = $checker.instantiations.insert(key, value);
        $crate::reference_state_probe::inserted($site, previous, error);
        previous
    }};
}
pub(crate) use {get, insert};

#[cfg(test)]
mod tests {
    use super::*;
    fn with_checker(source: &str, test: impl FnOnce(&mut crate::Checker<'_, '_>, tsr_ast::NodeId)) {
        assert!(enabled(), "run archive tests with TSR_MAPPER_PROFILE=1");
        FRAMES.with(|frames| assert!(frames.borrow().is_empty()));
        LOCAL.with(|counts| counts.set([0; N]));
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
        FRAMES.with(|frames| assert!(frames.borrow().is_empty()));
    }
    fn count(name: &str) -> u64 {
        local_counts().into_iter().find(|(key, _)| key == name).unwrap().1
    }
    #[test]
    fn real_named_reference_first_repeat_is_reserved_then_nonreserved() {
        with_checker("interface Box<T> { value: T }", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Box").unwrap();
            let argument = checker.intrinsics.number;
            let first = checker.create_type_reference(symbol, vec![argument]);
            assert_eq!(checker.instantiations[&(symbol, vec![argument])], first);
            assert!(checker.type_reference_targets.contains_key(&first));
            assert_eq!(checker.create_type_reference(symbol, vec![argument]), first);
            assert_eq!(count("state_site_named_insert_calls"), 1);
            assert_eq!(count("state_request_ordinary_reservations"), 1);
            assert_eq!(count("state_request_ordinary_reservation_closed"), 1);
            assert_eq!(count("state_site_ordinary_lookup_misses"), 1);
            assert_eq!(count("state_site_ordinary_lookup_values"), 1);
            assert_eq!(count("state_site_ordinary_lookup_reserved_answers"), 0);
            assert_eq!(count("state_request_ordinary_cache_hit_returns"), 1);
        });
    }
    #[test]
    fn real_error_argument_is_a_value_handle_not_an_error_answer() {
        with_checker("interface Box<T> { value: T }", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Box").unwrap();
            let error = checker.intrinsics.error;
            let first = checker.create_type_reference(symbol, vec![error]);
            assert_ne!(first, error);
            assert_eq!(checker.create_type_reference(symbol, vec![error]), first);
            assert_eq!(count("state_site_ordinary_lookup_errors"), 0);
            assert_eq!(count("state_site_ordinary_lookup_values"), 1);
            assert_eq!(count("state_request_ordinary_returned_error"), 0);
        });
    }
    #[test]
    fn real_table_seeded_error_and_later_value_stay_distinct() {
        with_checker("interface Box<T> { value: T }", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Box").unwrap();
            let number = checker.intrinsics.number;
            let error = checker.intrinsics.error;
            checker.instantiations.insert((symbol, vec![number]), error);
            assert_eq!(checker.create_type_reference(symbol, vec![number]), error);
            checker.instantiations.insert((symbol, vec![number]), number);
            assert_eq!(checker.create_type_reference(symbol, vec![number]), number);
            assert_eq!(count("state_site_ordinary_lookup_errors"), 1);
            assert_eq!(count("state_site_ordinary_lookup_values"), 1);
            assert_eq!(count("state_request_ordinary_returned_error"), 1);
            assert_eq!(count("state_request_ordinary_cache_hit_returns"), 2);
        });
    }
    #[test]
    fn real_nonnullable_refusal_does_not_publish_an_error() {
        with_checker("interface NonNullable<T> {}", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "NonNullable").unwrap();
            let number = checker.intrinsics.number;
            let error = checker.intrinsics.error;
            for _ in 0..2 {
                assert_eq!(checker.get_global_non_nullable_type_instantiation(number), error);
                assert!(!checker.instantiations.contains_key(&(symbol, vec![number])));
            }
            assert_eq!(count("state_worker_nonnullable_body_refused"), 2);
            assert_eq!(count("state_site_nonnullable_lookup_misses"), 2);
            assert_eq!(count("state_site_nonnullable_insert_calls"), 0);
            assert_eq!(count("state_request_nonnullable_returned_error"), 2);
            // Explicit seed tests subsequent lookup, not natural error recovery.
            checker.instantiations.insert((symbol, vec![number]), number);
            assert_eq!(checker.get_global_non_nullable_type_instantiation(number), number);
            assert_eq!(count("state_request_nonnullable_cache_hit_returns"), 1);
        });
    }
    #[test]
    fn real_string_mapping_is_observed_before_ordinary_lookup() {
        with_checker("type Uppercase<S extends string> = intrinsic;", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Uppercase").unwrap();
            let string = checker.intrinsics.string;
            let first = checker.create_type_reference(symbol, vec![string]);
            let second = checker.create_type_reference(symbol, vec![string]);
            assert_ne!(first, string);
            assert_eq!(first, second);
            assert_eq!(checker.string_mapping_types[&first], (symbol, string));
            assert_eq!(checker.string_mapping_types[&second], (symbol, string));
            assert_eq!(checker.type_to_string(first), "Uppercase<string>");
            assert_eq!(checker.type_to_string(second), "Uppercase<string>");
            assert_eq!(count("state_site_ordinary_lookup_calls"), 0);
            assert_eq!(count("state_worker_string_mapping_calls"), 2);
            assert_eq!(count("state_worker_string_mapping_returned_value"), 2);
            assert_eq!(count("state_site_string_mapping_insert_new_entries"), 1);
            assert_eq!(count("state_site_string_mapping_insert_replacement_entries"), 1);
        });
    }
    #[test]
    fn exact_reservation_owner_key_value_and_nested_return_controls() {
        with_checker("interface Box<T,U> {} interface Other<T,U> {}", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Box").unwrap();
            let other = checker.binder.lookup_local(root, "Other").unwrap();
            let number = checker.intrinsics.number;
            let string = checker.intrinsics.string;
            let error = checker.intrinsics.error;
            let key = (symbol, vec![number, string]);
            let mut outer = Request::enter(1, 0, Some(symbol), &key.1);
            publication(1, 12, &key, number, error);
            inserted(12, None, error);
            lookup(1, 6, &key, Some(number), error);
            let mut inner = Request::enter(1, 0, Some(symbol), &key.1);
            lookup(1, 6, &key, Some(number), error);
            inner.returned(number, error, true);
            drop(inner);
            lookup(2, 6, &key, Some(number), error);
            lookup(1, 6, &(other, key.1.clone()), Some(number), error);
            lookup(1, 6, &(symbol, vec![string, number]), Some(number), error);
            lookup(1, 6, &key, Some(error), error);
            assert_eq!(count("state_site_ordinary_lookup_reserved_answers"), 2);
            assert_eq!(count("state_site_ordinary_lookup_errors"), 1);
            assert_eq!(count("state_request_ordinary_reentries"), 1);
            let mut late = Request::enter(1, 1, None, &key.1);
            late.bind(symbol);
            late.returned(number, error, true);
            drop(late);
            assert_eq!(count("state_request_nonnullable_reentries"), 1);
            outer.returned(number, error, false);
            drop(outer);
            lookup(1, 6, &key, Some(number), error);
            assert_eq!(count("state_site_ordinary_lookup_reserved_answers"), 2);
            assert_eq!(count("state_request_ordinary_reservation_closed"), 1);
        });
    }
    #[test]
    fn optional_worker_refusal_error_value_and_request_unwind_controls() {
        with_checker("interface Box<T> {}", |checker, root| {
            let symbol = checker.binder.lookup_local(root, "Box").unwrap();
            let error = checker.intrinsics.error;
            let number = checker.intrinsics.number;
            assert_eq!(worker(7, error, || None), None);
            assert_eq!(worker(7, error, || Some(error)), Some(error));
            assert_eq!(worker(7, error, || Some(number)), Some(number));
            drop(Request::enter(1, 0, Some(symbol), &[number]));
            assert_eq!(count("state_worker_nonnullable_body_refused"), 1);
            assert_eq!(count("state_worker_nonnullable_body_returned_error"), 1);
            assert_eq!(count("state_worker_nonnullable_body_returned_value"), 1);
            assert_eq!(count("state_request_ordinary_unreturned"), 1);
        });
    }
}
