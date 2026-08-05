//! The control-flow graph: nodes, labels, and antecedent lists.
//!
//! Ported from `internal/ast/flow.go` and the flow-node constructors in
//! `internal/binder/binder.go` (`newFlowNode` … `finishFlowLabel`, lines
//! 449–570) at the pinned commit.
//!
//! # Why this is not a graph of pointers
//!
//! Upstream's [`FlowNode`] is a four-word struct linked by `*FlowNode`, with
//! label antecedents held in a heap-allocated singly linked list of `FlowList`
//! cells. That shape is not available here: the checker reads flow nodes from
//! several threads ([ADR-0012](../../../docs/adr/0012-ast-is-sync.md)) and
//! `addAntecedent` mutates a label *after* other nodes already point at it, so a
//! `&`-graph would need interior mutability and an `Arc` per node.
//!
//! Instead every flow node is a [`FlowId`] into one contiguous [`FlowStore`],
//! which is exactly the handles-not-references choice
//! [ADR-0013](../../../docs/adr/0013-checker-memoisation.md) settled for
//! symbols. Mutation during binding is `&mut FlowStore`; once binding returns
//! the store is shared read-only.
//!
//! # Layout
//!
//! One record is 16 bytes, against upstream's 32 (`Flags` plus three pointers,
//! padded), and it holds every variant:
//!
//! | field | branch/loop label | condition, assignment, call, … | switch clause | reduce label |
//! |---|---|---|---|---|
//! | `flags` | `BRANCH_LABEL`/`LOOP_LABEL` | the kind | `SWITCH_CLAUSE` | `REDUCE_LABEL` |
//! | `node` | — | the AST node | the `switch` statement | — |
//! | `antecedent` | — | the single antecedent | the single antecedent | the single antecedent |
//! | `aux` | head of the antecedent list | — | index into `clauses` | index into `reductions` |
//!
//! `aux` is private and every reader goes through an accessor that checks
//! `flags` first, so the overloading cannot leak into callers.
//!
//! Antecedent lists are a flat arena of 8-byte cells rather than one `Vec` per
//! label. Both alternatives were rejected for the same reason: a label's list is
//! appended to while *other* labels are also being appended to, so there is no
//! contiguous range per label, and a `Vec` per label costs 24 bytes of header
//! plus an allocation for a list whose median length is one. Upstream reached
//! for a linked list here for the same reason; this is that list with the cells
//! pooled.

use tsr_ast::NodeId;
use tsr_core::Idx as _;
use tsr_core::define_index;

bitflags::bitflags! {
    /// What a flow node is.
    ///
    /// A direct port of `ast.FlowFlags` (`internal/ast/flow.go:8`). `u16` rather
    /// than upstream's `uint32`: thirteen bits are defined and the narrower type
    /// is what lets a record fit in 16 bytes.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FlowFlags: u16 {
        /// Unreachable code.
        const UNREACHABLE = 1 << 0;
        /// Start of a flow graph.
        const START = 1 << 1;
        /// Non-looping junction.
        const BRANCH_LABEL = 1 << 2;
        /// Looping junction.
        const LOOP_LABEL = 1 << 3;
        /// Assignment.
        const ASSIGNMENT = 1 << 4;
        /// Condition known to be true.
        const TRUE_CONDITION = 1 << 5;
        /// Condition known to be false.
        const FALSE_CONDITION = 1 << 6;
        /// `switch` statement clause.
        const SWITCH_CLAUSE = 1 << 7;
        /// Potential array mutation.
        const ARRAY_MUTATION = 1 << 8;
        /// Potential assertion call.
        const CALL = 1 << 9;
        /// Temporarily reduce the antecedents of a label.
        const REDUCE_LABEL = 1 << 10;
        /// Referenced as an antecedent once.
        const REFERENCED = 1 << 11;
        /// Referenced as an antecedent more than once.
        const SHARED = 1 << 12;

        /// Either kind of junction.
        const LABEL = Self::BRANCH_LABEL.bits() | Self::LOOP_LABEL.bits();
        /// Either kind of condition.
        const CONDITION = Self::TRUE_CONDITION.bits() | Self::FALSE_CONDITION.bits();
    }
}

define_index! {
    /// Identifies a flow node within one file's [`FlowStore`].
    pub struct FlowId;
}

define_index! {
    /// Identifies a cell in the antecedent-list arena.
    ///
    /// Crate-private: a cell is an implementation detail of how a label's
    /// antecedents are stored, and callers outside the binder iterate them
    /// through [`Antecedents`] instead.
    pub(crate) struct CellId;
}

/// One record. Private; readers go through [`FlowStore`]'s accessors.
#[derive(Debug, Clone, Copy)]
struct Record {
    flags: FlowFlags,
    /// The associated AST node, for the kinds that have one.
    node: Option<NodeId>,
    /// The single antecedent, for everything but a label.
    antecedent: Option<FlowId>,
    /// Overloaded by `flags`; see the module docs.
    aux: u32,
}

/// A cell in a label's antecedent list.
#[derive(Debug, Clone, Copy)]
struct Cell {
    flow: FlowId,
    next: Option<CellId>,
}

/// The `switch` clause range a [`FlowFlags::SWITCH_CLAUSE`] node narrows on.
///
/// Upstream models this as a synthetic AST node (`ast.FlowSwitchClauseData`)
/// because its `FlowNode.Node` field is typed `*ast.Node` and there is nowhere
/// else to put the two integers. Nothing forces that here, so the data is what
/// it is: a statement and a half-open range of clause indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchClause {
    /// The `switch` statement.
    pub switch_statement: NodeId,
    /// First clause in the range.
    pub clause_start: u32,
    /// One past the last clause in the range.
    pub clause_end: u32,
}

impl SwitchClause {
    /// Whether the range covers no clauses, which is how the binder records
    /// "the `switch` fell through every clause without matching".
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.clause_start == self.clause_end
    }
}

/// The target and reduced antecedent set of a [`FlowFlags::REDUCE_LABEL`] node.
///
/// Upstream: `ast.FlowReduceLabelData`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReduceLabel {
    /// The label whose antecedents are temporarily replaced.
    pub target: FlowId,
    /// Head of the replacement list; iterate with [`FlowStore::list`].
    antecedents: Option<CellId>,
}

/// Every flow node in one file.
///
/// [`FlowId::ZERO`] is always the file's single unreachable node — see
/// [`FlowStore::unreachable`].
#[derive(Debug)]
pub struct FlowStore {
    records: Vec<Record>,
    cells: Vec<Cell>,
    clauses: Vec<SwitchClause>,
    reductions: Vec<ReduceLabel>,
    /// Scratch for [`FlowStore::combine_lists`], reused rather than reallocated.
    scratch: Vec<FlowId>,
}

impl Default for FlowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowStore {
    /// A store holding only the unreachable node.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// A store sized for roughly `flow_nodes` records.
    ///
    /// The binder estimates from the node count. Being wrong costs a little
    /// memory; being right removes the growth reallocations, which for
    /// `checker.ts` is a dozen doublings of a multi-megabyte vector.
    #[must_use]
    pub fn with_capacity(flow_nodes: usize) -> Self {
        let mut store = Self {
            records: Vec::with_capacity(flow_nodes.max(1)),
            cells: Vec::new(),
            clauses: Vec::new(),
            reductions: Vec::new(),
            scratch: Vec::new(),
        };
        // The unreachable node is created first so that its id is a constant,
        // which is what lets the binder compare against it as cheaply as
        // upstream compares pointers.
        let id = store.new_node(FlowFlags::UNREACHABLE);
        debug_assert_eq!(id, FlowId::ZERO);
        store
    }

    /// Reserve room for `additional` more records.
    ///
    /// For a store several files of one program bind into, which cannot be
    /// sized once at construction. See [`FlowStore::with_capacity`] for why the
    /// sizing matters at all.
    pub fn reserve(&mut self, additional: usize) {
        self.records.reserve(additional);
    }

    /// The single unreachable node.
    ///
    /// Upstream has one per file. This has one per *store*, which is one per
    /// program once several files bind into it — a deliberate difference, and a
    /// safe one: the binder tests `current == unreachable()` by identity in
    /// several places (`bindChildren`, `bindCaseBlock`), and that test asks
    /// "is control unreachable here", which a single canonical sentinel answers
    /// for every file at once. The node carries no file-specific data. What
    /// would break the test is *more* than one such node, not fewer.
    #[must_use]
    pub const fn unreachable(&self) -> FlowId {
        FlowId::ZERO
    }

    /// How many flow nodes exist, including the unreachable one.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store holds nothing but the unreachable node.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.len() <= 1
    }

    /// What kind of flow node this is.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn flags(&self, id: FlowId) -> FlowFlags {
        self.records[id.index()].flags
    }

    /// The AST node this flow node is about, for the kinds that have one.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn node(&self, id: FlowId) -> Option<NodeId> {
        self.records[id.index()].node
    }

    /// The single antecedent, for every kind but a label.
    ///
    /// A label has a list instead; see [`FlowStore::antecedents`].
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn antecedent(&self, id: FlowId) -> Option<FlowId> {
        self.records[id.index()].antecedent
    }

    /// A label's antecedents, in the order they were added.
    ///
    /// Empty for a node that is not a label.
    #[must_use]
    pub fn antecedents(&self, id: FlowId) -> Antecedents<'_> {
        let record = self.records[id.index()];
        let head =
            if record.flags.intersects(FlowFlags::LABEL) { unpack(record.aux) } else { None };
        Antecedents { store: self, next: head }
    }

    /// The clause range of a [`FlowFlags::SWITCH_CLAUSE`] node.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn switch_clause(&self, id: FlowId) -> Option<SwitchClause> {
        let record = self.records[id.index()];
        record.flags.contains(FlowFlags::SWITCH_CLAUSE).then(|| self.clauses[record.aux as usize])
    }

    /// The target and reduced antecedents of a [`FlowFlags::REDUCE_LABEL`] node.
    ///
    /// # Panics
    /// Panics if `id` is out of bounds.
    #[must_use]
    pub fn reduce_label(&self, id: FlowId) -> Option<ReduceLabel> {
        let record = self.records[id.index()];
        record.flags.contains(FlowFlags::REDUCE_LABEL).then(|| self.reductions[record.aux as usize])
    }

    /// The replacement antecedents carried by a reduce label.
    #[must_use]
    pub fn reduced_antecedents(&self, reduce: ReduceLabel) -> Antecedents<'_> {
        self.list(reduce.antecedents)
    }

    /// Walk a raw list.
    fn list(&self, head: Option<CellId>) -> Antecedents<'_> {
        Antecedents { store: self, next: head }
    }

    // ---- construction, used only while binding ----

    /// A node with no AST node and no antecedent.
    pub(crate) fn new_node(&mut self, flags: FlowFlags) -> FlowId {
        self.new_node_ex(flags, None, None)
    }

    /// A node with an AST node and an antecedent.
    ///
    /// # Panics
    /// Panics if the file needs more than `u32::MAX - 1` flow nodes.
    pub(crate) fn new_node_ex(
        &mut self,
        flags: FlowFlags,
        node: Option<NodeId>,
        antecedent: Option<FlowId>,
    ) -> FlowId {
        let id = FlowId::from_usize(self.records.len());
        self.records.push(Record { flags, node, antecedent, aux: u32::MAX });
        id
    }

    /// A looping junction (`createLoopLabel`).
    pub(crate) fn new_loop_label(&mut self) -> FlowId {
        self.new_node(FlowFlags::LOOP_LABEL)
    }

    /// A non-looping junction (`createBranchLabel`).
    pub(crate) fn new_branch_label(&mut self) -> FlowId {
        self.new_node(FlowFlags::BRANCH_LABEL)
    }

    /// A switch-clause node (`createFlowSwitchClause`).
    ///
    /// # Panics
    /// Panics if the file needs more than `u32::MAX` clause records.
    pub(crate) fn new_switch_clause(
        &mut self,
        antecedent: FlowId,
        switch_statement: NodeId,
        clause_start: usize,
        clause_end: usize,
    ) -> FlowId {
        self.set_referenced(antecedent);
        let aux = u32::try_from(self.clauses.len()).expect("clause count exceeds u32");
        self.clauses.push(SwitchClause {
            switch_statement,
            clause_start: clause_start.try_into().expect("clause index exceeds u32"),
            clause_end: clause_end.try_into().expect("clause index exceeds u32"),
        });
        let id = self.new_node_ex(FlowFlags::SWITCH_CLAUSE, None, Some(antecedent));
        self.records[id.index()].aux = aux;
        id
    }

    /// A reduce label (`createReduceLabel`).
    ///
    /// # Panics
    /// Panics if the file needs more than `u32::MAX` reduce records.
    pub(crate) fn new_reduce_label(
        &mut self,
        target: FlowId,
        antecedents: Option<CellId>,
        antecedent: FlowId,
    ) -> FlowId {
        let aux = u32::try_from(self.reductions.len()).expect("reduce count exceeds u32");
        self.reductions.push(ReduceLabel { target, antecedents });
        let id = self.new_node_ex(FlowFlags::REDUCE_LABEL, None, Some(antecedent));
        self.records[id.index()].aux = aux;
        id
    }

    /// Mark a node as used as an antecedent (`setFlowNodeReferenced`).
    ///
    /// First reference sets `REFERENCED`; any later one sets `SHARED`. The
    /// checker uses `SHARED` to decide what is worth caching, so the
    /// once-versus-more-than-once distinction is the point, not a count.
    pub(crate) fn set_referenced(&mut self, id: FlowId) {
        let flags = &mut self.records[id.index()].flags;
        if flags.contains(FlowFlags::REFERENCED) {
            *flags |= FlowFlags::SHARED;
        } else {
            *flags |= FlowFlags::REFERENCED;
        }
    }

    /// Add `antecedent` to `label`'s list (`addAntecedent`).
    ///
    /// Unreachable antecedents are dropped, and an antecedent already on the
    /// list is not added twice — that de-duplication is what keeps a loop label
    /// from growing one entry per `continue` to the same place. The scan is
    /// linear, as upstream's is; lists are short.
    ///
    /// # Panics
    /// Panics if the file needs more than `u32::MAX - 1` list cells.
    pub(crate) fn add_antecedent(&mut self, label: FlowId, antecedent: FlowId) {
        if self.flags(antecedent).contains(FlowFlags::UNREACHABLE) {
            return;
        }
        debug_assert!(
            self.flags(label).intersects(FlowFlags::LABEL),
            "antecedent lists belong to labels"
        );

        let mut last = None;
        let mut cursor = unpack(self.records[label.index()].aux);
        while let Some(cell) = cursor {
            if self.cells[cell.index()].flow == antecedent {
                return;
            }
            last = Some(cell);
            cursor = self.cells[cell.index()].next;
        }

        let cell = CellId::from_usize(self.cells.len());
        self.cells.push(Cell { flow: antecedent, next: None });
        match last {
            Some(previous) => self.cells[previous.index()].next = Some(cell),
            None => self.records[label.index()].aux = cell.as_u32(),
        }
        self.set_referenced(antecedent);
    }

    /// Collapse a label to what control actually reaches (`finishFlowLabel`).
    ///
    /// No antecedents means the label is unreachable; exactly one means the
    /// junction is not a junction and the antecedent stands in for it. Only a
    /// genuine merge survives as a label, which is what keeps the graph the
    /// checker walks proportional to the branching in the source rather than to
    /// the number of labels the binder happened to create.
    pub(crate) fn finish_label(&mut self, label: FlowId) -> FlowId {
        let Some(head) = unpack(self.records[label.index()].aux) else {
            return self.unreachable();
        };
        let cell = self.cells[head.index()];
        if cell.next.is_none() { cell.flow } else { label }
    }

    /// A label's antecedent list head, for the reduce-label plumbing.
    pub(crate) fn antecedent_list(&self, label: FlowId) -> Option<CellId> {
        unpack(self.records[label.index()].aux)
    }

    /// Whether a label has any antecedents.
    pub(crate) fn has_antecedents(&self, label: FlowId) -> bool {
        self.antecedent_list(label).is_some()
    }

    /// Replace a label's antecedents wholesale.
    ///
    /// Used once, for the `finally` label, which upstream also assigns rather
    /// than appends to because the three source lists have already been built.
    pub(crate) fn set_antecedent_list(&mut self, label: FlowId, head: Option<CellId>) {
        self.records[label.index()].aux = pack(head);
    }

    /// `head` followed by `tail`, as a fresh chain (`combineFlowLists`).
    ///
    /// Upstream recurses to the end of `head` and rebuilds on the way out; the
    /// depth of that recursion is the number of mutations in a `try` block,
    /// which is unbounded in generated code. This collects and walks backwards
    /// instead, which is the same list for a bounded amount of stack.
    ///
    /// # Panics
    /// Panics if the file needs more than `u32::MAX - 1` list cells.
    pub(crate) fn combine_lists(
        &mut self,
        head: Option<CellId>,
        tail: Option<CellId>,
    ) -> Option<CellId> {
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let mut cursor = head;
        while let Some(cell) = cursor {
            scratch.push(self.cells[cell.index()].flow);
            cursor = self.cells[cell.index()].next;
        }
        let mut result = tail;
        for flow in scratch.iter().rev() {
            let cell = CellId::from_usize(self.cells.len());
            self.cells.push(Cell { flow: *flow, next: result });
            result = Some(cell);
        }
        self.scratch = scratch;
        result
    }

    /// Bytes of heap this store holds, for the memory reporting in
    /// `docs/architecture/performance.md`.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.records.capacity() * size_of::<Record>()
            + self.cells.capacity() * size_of::<Cell>()
            + self.clauses.capacity() * size_of::<SwitchClause>()
            + self.reductions.capacity() * size_of::<ReduceLabel>()
    }
}

/// `u32::MAX` is the empty list, matching the niche `Option<CellId>` uses.
fn unpack(aux: u32) -> Option<CellId> {
    (aux != u32::MAX).then(|| CellId::new(aux))
}

fn pack(cell: Option<CellId>) -> u32 {
    cell.map_or(u32::MAX, CellId::as_u32)
}

/// A label's antecedents, in insertion order.
#[derive(Debug, Clone)]
pub struct Antecedents<'a> {
    store: &'a FlowStore,
    next: Option<CellId>,
}

impl Iterator for Antecedents<'_> {
    type Item = FlowId;

    fn next(&mut self) -> Option<FlowId> {
        let cell = self.store.cells[self.next?.index()];
        self.next = cell.next;
        Some(cell.flow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unreachable_node_is_id_zero_and_unique() {
        let store = FlowStore::new();
        assert_eq!(store.unreachable(), FlowId::ZERO);
        assert!(store.flags(store.unreachable()).contains(FlowFlags::UNREACHABLE));
        assert!(store.is_empty());
    }

    #[test]
    fn a_label_with_one_antecedent_collapses_to_it() {
        let mut store = FlowStore::new();
        let start = store.new_node(FlowFlags::START);
        let label = store.new_branch_label();
        assert_eq!(store.finish_label(label), store.unreachable(), "no antecedents");
        store.add_antecedent(label, start);
        assert_eq!(store.finish_label(label), start, "one antecedent is not a junction");
    }

    #[test]
    fn a_label_with_two_antecedents_stays_a_label() {
        let mut store = FlowStore::new();
        let a = store.new_node(FlowFlags::START);
        let b = store.new_node(FlowFlags::START);
        let label = store.new_branch_label();
        store.add_antecedent(label, a);
        store.add_antecedent(label, b);
        assert_eq!(store.finish_label(label), label);
        assert_eq!(store.antecedents(label).collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn antecedents_are_deduplicated_and_unreachable_ones_dropped() {
        let mut store = FlowStore::new();
        let a = store.new_node(FlowFlags::START);
        let label = store.new_branch_label();
        store.add_antecedent(label, a);
        store.add_antecedent(label, a);
        let unreachable = store.unreachable();
        store.add_antecedent(label, unreachable);
        assert_eq!(store.antecedents(label).collect::<Vec<_>>(), vec![a]);
    }

    #[test]
    fn referencing_twice_marks_shared() {
        let mut store = FlowStore::new();
        let a = store.new_node(FlowFlags::START);
        let first = store.new_branch_label();
        let second = store.new_branch_label();
        store.add_antecedent(first, a);
        assert!(store.flags(a).contains(FlowFlags::REFERENCED));
        assert!(!store.flags(a).contains(FlowFlags::SHARED));
        store.add_antecedent(second, a);
        assert!(store.flags(a).contains(FlowFlags::SHARED));
    }

    #[test]
    fn combining_lists_concatenates_without_disturbing_either() {
        let mut store = FlowStore::new();
        let a = store.new_node(FlowFlags::START);
        let b = store.new_node(FlowFlags::START);
        let c = store.new_node(FlowFlags::START);
        let first = store.new_branch_label();
        store.add_antecedent(first, a);
        store.add_antecedent(first, b);
        let second = store.new_branch_label();
        store.add_antecedent(second, c);

        let head = store.antecedent_list(first);
        let tail = store.antecedent_list(second);
        let combined = store.combine_lists(head, tail);
        assert_eq!(store.list(combined).collect::<Vec<_>>(), vec![a, b, c]);
        assert_eq!(store.antecedents(first).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(store.antecedents(second).collect::<Vec<_>>(), vec![c]);
    }

    #[test]
    fn a_record_is_sixteen_bytes() {
        // Not a style preference: the binder allocates one record per branch in
        // the program, so this is the constant that decides whether the flow
        // graph is a rounding error against the AST or a peer of it.
        assert_eq!(size_of::<Record>(), 16);
        assert_eq!(size_of::<Cell>(), 8);
    }
}
