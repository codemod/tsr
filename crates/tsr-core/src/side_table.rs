//! Struct-of-arrays side tables.
//!
//! The architectural claim in PLAN.md §3.2 is that *the tree is a tree, and
//! everything cyclic lives in id-keyed side tables*. This module provides the side
//! tables.
//!
//! Two shapes, matching the two shapes typescript-go uses in
//! `internal/core/linkstore.go`:
//!
//! - [`side_tables!`] builds a **dense** struct-of-arrays for data every node has
//!   (parent, flags). One allocation per column, one shared length. This is the
//!   analogue of oxc's `multi_index_vec!`, itself modeled on Zig's `MultiArrayList`.
//! - [`PagedTable`] is **sparse**, for data only some nodes have — the shape of
//!   typescript-go's `PagedLinkStore`, which is what the checker's 23 link stores
//!   are built on. Entries are materialized lazily in 256-entry pages.

use std::fmt;

use crate::index::Idx;

/// Define a dense struct-of-arrays side table keyed by a typed index.
///
/// Each field becomes its own column, so iterating one column touches no cache
/// lines belonging to the others — the reason to prefer this over
/// `IndexVec<I, Struct>` when columns are read independently. typescript-go's
/// checker reads `.Parent` 2,092 times without touching flags.
///
/// Accessor names are spelled out per column (`get / get_mut / column`) because
/// `macro_rules!` cannot concatenate identifiers. oxc's `multi_index_vec!` makes
/// the same trade for the same reason.
///
/// ```
/// # use tsr_core::{define_index, side_tables};
/// define_index! { pub struct NodeId; }
///
/// side_tables! {
///     /// Per-node data assigned by the binder.
///     pub struct NodeTable<NodeId> {
///         parent, parent_mut, parents: Option<NodeId>,
///         depth, depth_mut, depths: u32,
///     }
/// }
///
/// let mut t = NodeTable::new();
/// let root = t.push(None, 0);
/// let child = t.push(Some(root), 1);
/// assert_eq!(t.parent(child), Some(root));
/// assert_eq!(t.depths(), &[0, 1]);
/// ```
#[macro_export]
macro_rules! side_tables {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident<$idx:ty> {
            $(
                $(#[$fmeta:meta])*
                $get:ident, $get_mut:ident, $column:ident : $ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Default)]
        $vis struct $name {
            $($(#[$fmeta])* $get: ::std::vec::Vec<$ty>,)*
        }

        // The macro emits a complete accessor set; any given consumer uses a
        // subset, and that is not a defect worth a warning at every call site.
        #[allow(dead_code)]
        impl $name {
            /// An empty table.
            #[must_use]
            $vis fn new() -> Self {
                Self { $($get: ::std::vec::Vec::new(),)* }
            }

            /// An empty table with room for `capacity` rows.
            #[must_use]
            $vis fn with_capacity(capacity: usize) -> Self {
                Self { $($get: ::std::vec::Vec::with_capacity(capacity),)* }
            }

            /// Number of rows.
            ///
            /// All columns are pushed together, so any one of them is the length.
            #[must_use]
            $vis fn len(&self) -> usize {
                $crate::side_tables!(@first self, $($get),*)
            }

            /// Whether the table has no rows.
            #[must_use]
            $vis fn is_empty(&self) -> bool {
                self.len() == 0
            }

            /// Append a row, returning its index.
            $vis fn push(&mut self, $($get: $ty),*) -> $idx {
                let idx = <$idx as $crate::index::Idx>::from_usize(self.len());
                $(self.$get.push($get);)*
                idx
            }

            $(
                #[doc = concat!("Read the `", stringify!($get), "` column at `index`.")]
                ///
                /// # Panics
                /// Panics if `index` is out of bounds.
                #[inline]
                #[must_use]
                $vis fn $get(&self, index: $idx) -> $ty
                where
                    $ty: ::std::marker::Copy,
                {
                    self.$get[<$idx as $crate::index::Idx>::index(index)]
                }

                #[doc = concat!("Mutably access the `", stringify!($get), "` column at `index`.")]
                ///
                /// # Panics
                /// Panics if `index` is out of bounds.
                #[inline]
                $vis fn $get_mut(&mut self, index: $idx) -> &mut $ty {
                    &mut self.$get[<$idx as $crate::index::Idx>::index(index)]
                }

                #[doc = concat!("The whole `", stringify!($get), "` column, for bulk iteration.")]
                #[inline]
                #[must_use]
                $vis fn $column(&self) -> &[$ty] {
                    &self.$get
                }
            )*
        }
    };

    (@first $self:ident, $first:ident $(, $rest:ident)*) => { $self.$first.len() };
}

const PAGE_SHIFT: usize = 8;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
const PAGE_MASK: usize = PAGE_SIZE - 1;

/// A sparse table keyed by a typed index, materialized in 256-entry pages.
///
/// Ported from typescript-go's `core.PagedLinkStore`
/// (`internal/core/linkstore.go`). Use this for data attached to *some* nodes —
/// the checker's per-node links, where allocating a row for every node in each of
/// 23 stores would be wasteful.
///
/// Values are created on demand via [`Default`], matching the Go original's `Get`.
pub struct PagedTable<I: Idx, T> {
    pages: Vec<Option<Box<[T]>>>,
    _marker: std::marker::PhantomData<fn(I) -> I>,
}

impl<I: Idx, T> PagedTable<I, T> {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self { pages: Vec::new(), _marker: std::marker::PhantomData }
    }

    /// Whether the page backing `index` has been materialized.
    ///
    /// Page-granular, exactly as in the Go original: it answers "has anything in
    /// this page been touched", not "was this exact entry set".
    #[must_use]
    pub fn is_materialized(&self, index: I) -> bool {
        self.pages.get(index.index() >> PAGE_SHIFT).is_some_and(Option::is_some)
    }

    /// Read an entry without materializing its page.
    #[must_use]
    pub fn try_get(&self, index: I) -> Option<&T> {
        let raw = index.index();
        self.pages.get(raw >> PAGE_SHIFT)?.as_ref().map(|page| &page[raw & PAGE_MASK])
    }

    /// Number of materialized pages. Exposed for memory accounting and tests.
    #[must_use]
    pub fn materialized_pages(&self) -> usize {
        self.pages.iter().filter(|p| p.is_some()).count()
    }
}

impl<I: Idx, T: Default> PagedTable<I, T> {
    /// Read an entry mutably, materializing its page if needed.
    pub fn get_mut(&mut self, index: I) -> &mut T {
        let raw = index.index();
        let page_index = raw >> PAGE_SHIFT;
        if page_index >= self.pages.len() {
            self.pages.resize_with(page_index + 1, || None);
        }
        let page = self.pages[page_index].get_or_insert_with(|| {
            (0..PAGE_SIZE).map(|_| T::default()).collect::<Vec<T>>().into_boxed_slice()
        });
        &mut page[raw & PAGE_MASK]
    }
}

impl<I: Idx, T> Default for PagedTable<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Idx, T: fmt::Debug> fmt::Debug for PagedTable<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PagedTable")
            .field("materialized_pages", &self.materialized_pages())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::define_index;

    define_index! { pub struct TestId; }

    side_tables! {
        pub struct Table<TestId> {
            parent, parent_mut, parents: Option<TestId>,
            flags, flags_mut, all_flags: u32,
        }
    }

    #[test]
    fn columns_stay_in_step() {
        let mut t = Table::new();
        let a = t.push(None, 1);
        let b = t.push(Some(a), 2);
        assert_eq!(t.len(), 2);
        assert_eq!(t.parent(b), Some(a));
        assert_eq!(t.flags(a), 1);
        assert_eq!(t.all_flags(), &[1, 2]);
        *t.flags_mut(a) = 9;
        assert_eq!(t.flags(a), 9);
    }

    #[test]
    fn paged_table_is_sparse_but_reads_back() {
        let mut t = PagedTable::<TestId, u32>::new();
        let far = TestId::new(100_000);
        assert!(!t.is_materialized(far));
        assert_eq!(t.try_get(far), None);

        *t.get_mut(far) = 7;
        assert_eq!(t.try_get(far), Some(&7));
        assert!(t.is_materialized(far));

        // Only the one page exists: the table did not densely allocate 100k entries.
        assert_eq!(t.materialized_pages(), 1);
        assert!(!t.is_materialized(TestId::new(0)));
    }
}
