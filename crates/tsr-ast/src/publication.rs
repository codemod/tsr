//! Copy a private file AST into its immutable program identity space.
//!
//! The caller alone allocates in the canonical arena. Workers keep their own
//! arenas alive until copying finishes; no worker reference or lifetime escapes.

use crate::{Node, NodeId, NodeMap};
use tsr_core::{Arena, Idx as _};

/// A file's ordered finalization, with one memo row per registered private node.
pub struct Publication<'a> {
    /// The caller-owned arena; never sent to a worker.
    pub(crate) arena: &'a Arena,
    old_source_start: usize,
    old_source_len: usize,
    source: &'a str,
    base: u32,
    copied: Vec<Option<Node<'a>>>,
}

impl<'a> Publication<'a> {
    /// Begin publication at the base already reserved in the program's table.
    #[must_use]
    pub fn new(
        arena: &'a Arena,
        old_source: &str,
        source: &'a str,
        base: u32,
        nodes: usize,
    ) -> Self {
        Self {
            arena,
            old_source_start: old_source.as_ptr() as usize,
            old_source_len: old_source.len(),
            source,
            base,
            copied: vec![None; nodes],
        }
    }

    /// Relocate a registered node identity. File-relative offsets stay unchanged.
    #[must_use]
    pub fn id(&self, id: NodeId) -> NodeId {
        assert!(id.index() < self.copied.len(), "foreign worker node identity");
        NodeId::new(self.base.checked_add(id.as_u32()).expect("node count exceeds u32"))
    }

    /// Copy a node once, including children referenced before their table row.
    pub fn node(&mut self, node: Node<'_>) -> Node<'a> {
        if let Some(id) = node.node_id()
            && let Some(copied) = self.copied[id.index()]
        {
            return copied;
        }
        tsr_core::stack::ensure_sufficient(|| {
            if let Some(id) = node.node_id() {
                let copied = node.copy_to(self);
                self.copied[id.index()] = Some(copied);
                copied
            } else {
                node.copy_to(self)
            }
        })
    }

    /// Finish all registration rows, including JSDoc and recovery nodes that
    /// are not reached by the ordinary source-file syntax walk.
    pub fn finish(&mut self, local: &NodeMap<'_>, published: &mut NodeMap<'a>) {
        assert_eq!(
            published.len(),
            self.base as usize,
            "publication must follow reservation order"
        );
        assert_eq!(local.len(), self.copied.len());
        published.reserve(local.len());
        for index in 0..local.len() {
            let id = NodeId::new(u32::try_from(index).expect("node count exceeds u32"));
            let node = local.get(id).expect("every private row has a node");
            assert_eq!(node.node_id(), Some(id), "private node-map identity mismatch");
            published.push(self.node(node));
        }
    }

    fn text(&self, text: &str) -> &'a str {
        let start = (text.as_ptr() as usize).checked_sub(self.old_source_start);
        if let Some(start) = start
            && let Some(end) = start.checked_add(text.len())
            && end <= self.old_source_len
        {
            return &self.source[start..end];
        }
        self.arena.alloc_str(text)
    }
}

/// A syntax field whose borrows must be rehomed during publication.
pub trait Publish<'a>: Copy {
    /// The equivalent field in the caller-owned arena.
    type Output: Copy;
    /// Copy or recover this field without retaining worker-owned references.
    fn publish(self, publication: &mut Publication<'a>) -> Self::Output;
}

impl<'a> Publish<'a> for Node<'_> {
    type Output = Node<'a>;
    fn publish(self, p: &mut Publication<'a>) -> Self::Output {
        p.node(self)
    }
}

impl<'a> Publish<'a> for &str {
    type Output = &'a str;
    fn publish(self, p: &mut Publication<'a>) -> Self::Output {
        p.text(self)
    }
}

impl<'a, T: Publish<'a>> Publish<'a> for Option<T> {
    type Output = Option<T::Output>;
    fn publish(self, p: &mut Publication<'a>) -> Self::Output {
        self.map(|value| value.publish(p))
    }
}

impl<'a, T: Publish<'a>> Publish<'a> for &[T]
where
    T::Output: 'a,
{
    type Output = &'a [T::Output];
    fn publish(self, p: &mut Publication<'a>) -> Self::Output {
        let values: Vec<_> = self.iter().map(|value| value.publish(p)).collect();
        p.arena.alloc_slice(&values)
    }
}
