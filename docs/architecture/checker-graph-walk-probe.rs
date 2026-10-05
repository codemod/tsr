// Archive-only graph observation. No semantic answers are cached.
#![allow(missing_docs)]
use crate::TypeId;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
static GLOBAL_COUNTS: [AtomicU64; 36] = [const { AtomicU64::new(0) }; 36];

pub const NAMES: [&str; 36] = [
    "roots",
    "explicit_roots",
    "registry_roots",
    "true_roots",
    "false_roots",
    "visits",
    "edges",
    "unique_visits",
    "duplicate_visits",
    "explicit_parameter_tests",
    "registry_parameter_tests",
    "explicit_parameter_comparisons",
    "parameter_hits",
    "visited_tests",
    "visited_comparisons",
    "visited_hits",
    "markers",
    "terminal_leaves",
    "fallbacks",
    "fallback_payload_bytes",
    "empty_names_fallbacks",
    "scratch_allocations",
    "scratch_requested_bytes",
    "scratch_usable_bytes",
    "printing_allocations",
    "printing_requested_bytes",
    "printing_usable_bytes",
    "root_capacity_sum",
    "max_visited",
    "max_capacity",
    "roots_visited_0",
    "roots_visited_1_4",
    "roots_visited_5_16",
    "roots_visited_17_64",
    "roots_visited_65_256",
    "roots_visited_over_256",
];
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("TSR_GRAPH_PROFILE").is_some())
}
#[derive(Hash, Eq, PartialEq)]
struct Query {
    root: TypeId,
    registry: bool,
    parameters: Vec<TypeId>,
    names: Vec<String>,
    registry_len: usize,
}
struct State {
    counts: [u64; 36],
    queries: HashSet<Query>,
    roots: HashSet<TypeId>,
    checks: Vec<(usize, String)>,
}
impl Default for State {
    fn default() -> Self {
        Self { counts: [0; 36], queries: HashSet::new(), roots: HashSet::new(), checks: Vec::new() }
    }
}
#[derive(Default)]
pub struct Owner(RefCell<State>);
pub struct Root {
    counts: [u64; 36],
    seen: HashSet<TypeId>,
    query: Option<Query>,
    allocation_before: [u64; 6],
}
impl Root {
    #[must_use]
    pub fn new(
        id: TypeId,
        parameters: &[TypeId],
        names: &[&str],
        registry: bool,
        len: usize,
    ) -> Self {
        let mut root = Self {
            counts: [0; 36],
            seen: HashSet::new(),
            query: None,
            allocation_before: allocation::snapshot(),
        };
        if enabled() {
            root.counts[0] = 1;
            root.counts[if registry { 2 } else { 1 }] = 1;
            root.query = Some(Query {
                root: id,
                registry,
                parameters: parameters.to_vec(),
                names: names.iter().map(|name| (*name).to_owned()).collect(),
                registry_len: len,
            });
        }
        root
    }
    pub fn visit(&mut self, id: TypeId) {
        if enabled() {
            self.counts[5] += 1;
            if self.seen.insert(id) {
                self.counts[7] += 1;
            } else {
                self.counts[8] += 1;
            }
        }
    }
    pub fn explicit(&mut self, parameters: &[TypeId], id: TypeId) -> bool {
        if !enabled() {
            return parameters.contains(&id);
        }
        self.counts[9] += 1;
        for &parameter in parameters {
            self.counts[11] += 1;
            if parameter == id {
                self.counts[12] += 1;
                return true;
            }
        }
        false
    }
    pub fn registered(&mut self, answer: bool) -> bool {
        if enabled() {
            self.counts[10] += 1;
            self.counts[12] += u64::from(answer);
        }
        answer
    }
    pub fn contains(&mut self, visited: &[TypeId], id: TypeId) -> bool {
        if !enabled() {
            return visited.contains(&id);
        }
        self.counts[13] += 1;
        for &previous in visited {
            self.counts[14] += 1;
            if previous == id {
                self.counts[15] += 1;
                return true;
            }
        }
        false
    }
    pub fn push(&mut self, visited: &mut Vec<TypeId>, id: TypeId) {
        {
            let _allocation = allocation::enter(1);
            visited.push(id);
        }
        if enabled() {
            self.counts[16] += 1;
            self.counts[28] = self.counts[28].max(visited.len() as u64);
            self.counts[29] = self.counts[29].max(visited.capacity() as u64);
        }
    }
    pub fn leaf(&mut self) {
        if enabled() {
            self.counts[17] += 1;
        }
    }
    pub fn fallback(&mut self, text: &str, names: &[&str]) {
        if enabled() {
            self.counts[18] += 1;
            self.counts[19] += text.len() as u64;
            self.counts[20] += u64::from(names.is_empty());
        }
    }
    #[must_use]
    pub fn printing() -> allocation::Scope {
        allocation::enter(2)
    }
}
impl Owner {
    pub fn finish(&self, mut root: Root, answer: bool, visited: &[TypeId], capacity: usize) {
        let Some(query) = root.query.take() else {
            return;
        };
        root.counts[if answer { 3 } else { 4 }] = 1;
        root.counts[6] = root.counts[5] - 1;
        root.counts[27] = capacity as u64;
        root.counts[30
            + match visited.len() {
                0 => 0,
                1..=4 => 1,
                5..=16 => 2,
                17..=64 => 3,
                65..=256 => 4,
                _ => 5,
            }] = 1;
        let after = allocation::snapshot();
        for (i, value) in after.into_iter().enumerate() {
            root.counts[21 + i] = value - root.allocation_before[i];
        }
        let mut state = self.0.borrow_mut();
        for (i, value) in root.counts.into_iter().enumerate() {
            if matches!(i, 28 | 29) {
                GLOBAL_COUNTS[i].fetch_max(value, Ordering::Relaxed);
            } else {
                GLOBAL_COUNTS[i].fetch_add(value, Ordering::Relaxed);
            }
            if matches!(i, 28 | 29) {
                state.counts[i] = state.counts[i].max(value);
            } else {
                state.counts[i] += value;
            }
        }
        state.roots.insert(query.root);
        state.queries.insert(query);
    }
    pub fn checked(&self, index: usize, name: &str) {
        if std::env::var_os("TSR_GRAPH_TRACE").is_some() {
            self.0.borrow_mut().checks.push((index, name.to_owned()));
        }
    }
    pub fn complete(&self, owner: usize, count: usize) {
        let Some(path) = std::env::var_os("TSR_GRAPH_TRACE") else {
            return;
        };
        let path = std::path::PathBuf::from(path).with_extension(format!("owner-{owner}.tsv"));
        let mut file = std::fs::File::create(path).expect("graph observer destination");
        let state = self.0.borrow();
        writeln!(file, "owner\t{}\t{owner}\t{count}\t{}", std::process::id(), u8::from(enabled()))
            .unwrap();
        for (index, name) in &state.checks {
            writeln!(file, "check\t{index}\t{name}").unwrap();
        }
        if enabled() {
            for (name, value) in NAMES.into_iter().zip(state.counts) {
                writeln!(file, "count\t{name}\t{value}").unwrap();
            }
            writeln!(
                file,
                "query_shapes\t{}\t{}\t{}",
                state.queries.len(),
                state.queries.capacity(),
                state.roots.len()
            )
            .unwrap();
        }
        writeln!(file, "complete").unwrap();
    }
}
mod allocation {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    std::thread_local! {
        static TAG: Cell<usize> = const { Cell::new(0) };
        static COUNTS: Cell<[u64; 6]> = const { Cell::new([0; 6]) };
    }
    pub struct Scope(usize);
    impl Drop for Scope {
        fn drop(&mut self) {
            TAG.with(|tag| tag.set(self.0));
        }
    }
    pub fn enter(tag: usize) -> Scope {
        Scope(TAG.with(|current| current.replace(if super::enabled() { tag } else { 0 })))
    }
    pub fn snapshot() -> [u64; 6] {
        COUNTS.with(Cell::get)
    }
    struct Probe;
    #[global_allocator]
    static GLOBAL: Probe = Probe;
    unsafe extern "C" {
        fn malloc_size(ptr: *const std::ffi::c_void) -> usize;
    }
    fn observe(pointer: *mut u8, size: usize) {
        if pointer.is_null() {
            return;
        }
        let tag = TAG.try_with(Cell::get).unwrap_or(0);
        if tag == 0 {
            return;
        }
        // SAFETY: successful live System allocation, observed without retaining it.
        let usable = unsafe { malloc_size(pointer.cast()) };
        COUNTS.with(|cell| {
            let mut counts = cell.get();
            for (i, value) in [1, size as u64, usable as u64].into_iter().enumerate() {
                counts[(tag - 1) * 3 + i] += value;
            }
            cell.set(counts);
        });
    }
    // SAFETY: delegate unchanged inputs to System. TLS counters never allocate or retain pointers.
    unsafe impl GlobalAlloc for Probe {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            observe(pointer, layout.size());
            pointer
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            observe(pointer, layout.size());
            pointer
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let pointer = unsafe { System.realloc(pointer, layout, size) };
            observe(pointer, size);
            pointer
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
        }
    }
}

pub fn pool_joined(count: usize) {
    let Some(path) = std::env::var_os("TSR_GRAPH_TRACE") else {
        return;
    };
    let mut file = std::fs::File::create(path).expect("graph pool destination");
    writeln!(file, "schema\tgraph-walk-v1").unwrap();
    writeln!(file, "process\t{}\t{count}\t{}", std::process::id(), u8::from(enabled())).unwrap();
    if enabled() {
        for (name, value) in NAMES.into_iter().zip(GLOBAL_COUNTS.iter()) {
            writeln!(file, "count\t{name}\t{}", value.load(Ordering::Relaxed)).unwrap();
        }
    }
    writeln!(file, "joined").unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Checker;
    use crate::flags::TypeFlags;
    use tsr_ast::Statement;
    use tsr_core::Arena;

    fn fixture(f: impl FnOnce(&mut Checker<'_, '_>, tsr_binder::SymbolId, TypeId)) {
        assert!(enabled(), "run archive controls with TSR_GRAPH_PROFILE=1");
        let arena = Arena::new();
        let source = "declare function owner<T>(value: T): T;";
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "graph.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(declaration) = parsed.source_file.statements[0] else {
            panic!("owner declaration");
        };
        let owner = bound.symbol_of(declaration.node_id.unwrap()).unwrap();
        let parameter = checker.get_signatures_of_symbol(owner).unwrap()[0].parameters[0].r#type;
        *checker.graph_walk_probe.0.borrow_mut() = State::default();
        f(&mut checker, owner, parameter);
    }
    fn count(checker: &Checker<'_, '_>, name: &str) -> u64 {
        checker.graph_walk_probe.0.borrow().counts[NAMES.iter().position(|n| *n == name).unwrap()]
    }
    fn object(checker: &mut Checker<'_, '_>, name: &str) -> TypeId {
        checker.store.new_named(TypeFlags::OBJECT, name.into(), None)
    }

    #[test]
    fn actual_diamond_walk_counts_comparisons_and_duplicate_edges() {
        fixture(|checker, owner, _| {
            let common = object(checker, "Common");
            let left = object(checker, "Left");
            let right = object(checker, "Right");
            let root = object(checker, "Root");
            checker.type_reference_targets.insert(common, (owner, vec![checker.intrinsics.number]));
            checker.type_reference_targets.insert(left, (owner, vec![common]));
            checker.type_reference_targets.insert(right, (owner, vec![common]));
            checker.type_reference_targets.insert(root, (owner, vec![left, right]));
            assert!(!checker.mentions_type_parameter(root, &[], &[]));
            for (name, expected) in [
                ("roots", 1),
                ("visits", 6),
                ("edges", 5),
                ("unique_visits", 5),
                ("duplicate_visits", 1),
                ("visited_comparisons", 12),
                ("visited_hits", 1),
                ("markers", 4),
                ("terminal_leaves", 1),
                ("explicit_parameter_comparisons", 0),
                ("max_visited", 4),
            ] {
                assert_eq!(count(checker, name), expected, "{name}");
            }
            assert_eq!(count(checker, "scratch_allocations"), 1);
            assert_eq!(
                count(checker, "scratch_requested_bytes"),
                count(checker, "max_capacity") * std::mem::size_of::<TypeId>() as u64
            );
        });
    }

    #[test]
    fn deep_and_wide_graphs_count_actual_linear_membership_and_capacity_growth() {
        fixture(|checker, owner, parameter| {
            let mut next = checker.intrinsics.number;
            for index in 0..64 {
                let node = object(checker, &format!("Depth{index}"));
                checker.type_reference_targets.insert(node, (owner, vec![next]));
                next = node;
            }
            assert!(!checker.mentions_type_parameter(next, &[parameter], &[]));
            assert_eq!(count(checker, "visited_comparisons"), 64 * 65 / 2);
            assert_eq!(count(checker, "explicit_parameter_comparisons"), 65);
            assert_eq!(count(checker, "max_visited"), 64);
            assert!(count(checker, "scratch_allocations") > 1);
            assert_eq!(count(checker, "roots_visited_17_64"), 1);
            let wide = object(checker, "Wide");
            let children: Vec<_> = (0..32)
                .map(|i| {
                    let child = object(checker, &format!("Child{i}"));
                    checker
                        .type_reference_targets
                        .insert(child, (owner, vec![checker.intrinsics.number]));
                    child
                })
                .collect();
            checker.type_reference_targets.insert(wide, (owner, children));
            assert!(!checker.mentions_type_parameter(wide, &[parameter], &[]));
            assert_eq!(count(checker, "roots"), 2);
            assert_eq!(count(checker, "visits"), 130);
            assert_eq!(count(checker, "duplicate_visits"), 31);
        });
    }

    #[test]
    fn identity_precedes_markers_and_registry_updates_are_visible() {
        fixture(|checker, owner, parameter| {
            let shadow = checker.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
            assert!(!checker.mentions_type_parameter(shadow, &[parameter], &["T"]));
            assert!(checker.mentions_type_parameter(parameter, &[shadow, parameter], &["T"]));
            assert_eq!(count(checker, "explicit_parameter_comparisons"), 3);
            assert_eq!(count(checker, "visited_tests"), 1);
            assert_eq!(count(checker, "markers"), 0);
            assert!(checker.mentions_registered_type_parameter(parameter));
            checker.type_parameter_symbols.remove(&parameter);
            assert!(!checker.mentions_registered_type_parameter(parameter));
            checker.type_parameter_symbols.insert(shadow, owner);
            assert!(checker.mentions_registered_type_parameter(shadow));
            assert_eq!(count(checker, "registry_roots"), 3);
            assert_eq!(count(checker, "registry_parameter_tests"), 3);
        });
    }

    #[test]
    fn mapping_and_template_cycles_are_marked_before_terminal_flags() {
        fixture(|checker, owner, parameter| {
            let mapping = checker.store.new_named(TypeFlags::STRING, "Mapping".into(), None);
            let template = checker.store.new_named(TypeFlags::STRING, "Template".into(), None);
            checker.string_mapping_types.insert(mapping, (owner, template));
            checker.template_literal_parts.insert(
                template,
                crate::templates::TemplateLiteralParts {
                    texts: vec![String::new(), String::new(), String::new()],
                    types: vec![mapping, parameter],
                },
            );
            assert!(checker.mentions_type_parameter(mapping, &[parameter], &[]));
            assert_eq!(count(checker, "visited_comparisons"), 2);
            assert_eq!(count(checker, "visited_hits"), 1);
            assert_eq!(count(checker, "markers"), 2);
            assert_eq!(count(checker, "parameter_hits"), 1);
            assert_eq!(count(checker, "true_roots"), 1);
        });
    }

    #[test]
    fn fallback_and_later_structural_metadata_are_not_cached_semantic_answers() {
        fixture(|checker, owner, parameter| {
            let node = object(checker, "Fallback<T>");
            assert!(checker.mentions_type_parameter(node, &[], &["T"]));
            assert!(!checker.mentions_type_parameter(node, &[], &[]));
            assert_eq!(count(checker, "fallbacks"), 2);
            assert_eq!(count(checker, "empty_names_fallbacks"), 1);
            assert!(count(checker, "printing_allocations") > 0);
            checker.type_reference_targets.insert(node, (owner, vec![checker.intrinsics.number]));
            assert!(!checker.mentions_type_parameter(node, &[parameter], &["T"]));
            checker.type_reference_targets.insert(node, (owner, vec![parameter]));
            assert!(checker.mentions_type_parameter(node, &[parameter], &["T"]));
            assert_eq!(count(checker, "fallbacks"), 2);
            // The same request shape can observe different evolving graph metadata.
            assert_eq!(checker.graph_walk_probe.0.borrow().queries.len(), 3);
            assert_eq!(count(checker, "roots"), 4);
        });
    }

    #[test]
    fn allocation_tags_restore_parent_and_do_not_count_other_threads() {
        let baseline = allocation::snapshot();
        {
            let _outer = allocation::enter(1);
            std::hint::black_box(vec![0_u8; 73]);
            {
                let _inner = allocation::enter(2);
                std::hint::black_box(vec![0_u8; 89]);
            }
            std::hint::black_box(vec![0_u8; 107]);
        }
        let after = allocation::snapshot();
        assert_eq!(after[0] - baseline[0], 2);
        assert_eq!(after[1] - baseline[1], 180);
        assert_eq!(after[3] - baseline[3], 1);
        assert_eq!(after[4] - baseline[4], 89);
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let before = allocation::snapshot();
                    let _scope = allocation::enter(1);
                    std::hint::black_box(vec![0_u8; 127]);
                    let after = allocation::snapshot();
                    assert_eq!(after[0] - before[0], 1);
                    assert_eq!(after[1] - before[1], 127);
                })
                .join()
                .unwrap();
        });
        assert_eq!(allocation::snapshot(), after);
    }
    #[test]
    fn stored_deferred_mapped_tuple_object_and_union_edges_remain_visible() {
        fixture(|checker, owner, parameter| {
            let signature = checker.get_signatures_of_symbol(owner).unwrap()[0].clone();
            for kind in 0..8 {
                let mut node = object(checker, &format!("Edge{kind}"));
                match kind {
                    0 => {
                        checker.deferred_keyof_operands.insert(node, parameter);
                    }
                    1 => {
                        checker
                            .deferred_indexed_access_types
                            .insert(node, (checker.intrinsics.number, parameter, false));
                    }
                    2 => {
                        checker.mapped_types.insert(
                            node,
                            crate::mapped::MappedTypeInfo {
                                declaration: signature.declaration,
                                parameter,
                                constraint: checker.intrinsics.number,
                                constraint_intersection: None,
                                template: parameter,
                                name_type: None,
                                optionality: None,
                                readonly: None,
                                modifiers_source: None,
                                keyof_constraint: false,
                                homomorphic_symbol: None,
                            },
                        );
                    }
                    3 => {
                        checker.mapped_conditionals.insert(
                            node,
                            crate::mapped::MappedConditionalInfo {
                                declaration: signature.declaration,
                                bindings: rustc_hash::FxHashMap::default(),
                                operands: [
                                    checker.intrinsics.number,
                                    checker.intrinsics.string,
                                    parameter,
                                    checker.intrinsics.never,
                                ],
                            },
                        );
                    }
                    4 => {
                        checker.object_literal_index_infos.insert(
                            node,
                            vec![crate::index_signatures::IndexInfo {
                                components: None,
                                declaration: None,
                                key: checker.intrinsics.string,
                                value: parameter,
                                readonly: false,
                            }],
                        );
                    }
                    5 => {
                        checker
                            .tuple_element_lists
                            .insert(node, (vec![checker.intrinsics.number, parameter], false));
                    }
                    6 => {
                        checker.anonymous_properties.insert(
                            node,
                            (
                                vec![crate::objects::AnonymousProperty {
                                    accessor_write: None,
                                    method: false,
                                    origin: None,
                                    checked_declaration: None,
                                    name: "value".into(),
                                    printed_name: "value".into(),
                                    printed_type: "T".into(),
                                    optional: false,
                                    readonly: false,
                                    r#type: parameter,
                                }],
                                false,
                            ),
                        );
                    }
                    _ => {
                        node = checker.store.intern_union(
                            TypeFlags::UNION,
                            crate::types::TypeData::Union {
                                text: "number | T".into(),
                                types: vec![checker.intrinsics.number, parameter],
                                symbol: None,
                            },
                        );
                    }
                }
                assert!(checker.mentions_type_parameter(node, &[parameter], &[]), "edge {kind}");
            }
            assert_eq!(count(checker, "fallbacks"), 0);
        });
    }

    #[test]
    fn signature_predicate_this_constraint_and_default_edges_are_walked_by_identity() {
        fixture(|checker, owner, parameter| {
            let signature = checker.get_signatures_of_symbol(owner).unwrap()[0].clone();
            let shadow = checker.store.new_named(TypeFlags::TYPE_PARAMETER, "T".into(), None);
            for kind in 0..5 {
                let node = object(checker, &format!("Signature{kind}"));
                let mut nested = signature.clone();
                nested.parameters[0].r#type = shadow;
                nested.r#type = checker.intrinsics.number;
                nested.type_parameters[0].constraint = None;
                nested.type_parameters[0].default = None;
                match kind {
                    0 => {
                        nested.predicate = Some(crate::signatures::TypePredicate {
                            asserts: false,
                            parameter_name: Some("value".into()),
                            r#type: Some(parameter),
                            written_text: None,
                        });
                    }
                    1 => {
                        let mut this = nested.parameters[0].clone();
                        this.r#type = parameter;
                        nested.this_parameter = Some(this);
                    }
                    2 => {
                        nested.type_parameters[0].constraint = Some(parameter);
                    }
                    3 => {
                        nested.type_parameters[0].default = Some(parameter);
                    }
                    _ => {}
                }
                checker.signature_types.insert(node, vec![nested]);
                assert_eq!(
                    checker.mentions_type_parameter(node, &[parameter], &["T"]),
                    kind < 4,
                    "signature {kind}"
                );
            }
            assert_eq!(count(checker, "fallbacks"), 0);
        });
    }
}
