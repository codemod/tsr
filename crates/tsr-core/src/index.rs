//! Typed indices and index-keyed vectors.
//!
//! Every cross-referencing entity in the compiler — nodes, symbols, types, scopes,
//! flow nodes — is addressed by a newtype index rather than a pointer. This is what
//! lets the object graph be cyclic (a symbol refers to a node that refers back to
//! the symbol) without `Rc<RefCell<_>>`, and it is what makes the arenas `Send`.
//!
//! Indices are backed by [`nonmax::NonMaxU32`], so `Option<Id>` occupies 4 bytes via
//! niche optimization rather than 8. See PLAN.md §3.3.

use std::{fmt, marker::PhantomData};

pub use nonmax::NonMaxU32;

/// Implemented by all newtype indices produced by [`define_index!`].
pub trait Idx: Copy + Eq + std::hash::Hash + fmt::Debug + 'static {
    /// Construct from a `usize`.
    ///
    /// # Panics
    /// Panics if `value` is `>= u32::MAX`, which would exhaust the niche.
    fn from_usize(value: usize) -> Self;

    /// The underlying integer.
    fn index(self) -> usize;
}

/// Define a newtype index backed by `NonMaxU32`.
///
/// ```
/// # use tsr_core::define_index;
/// define_index! {
///     /// Identifies a node within a single source file.
///     pub struct NodeId;
/// }
/// ```
#[macro_export]
macro_rules! define_index {
    ($(#[$meta:meta])* $vis:vis struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        $vis struct $name($crate::index::NonMaxU32);

        // As with `side_tables!`: a complete accessor set, partially used.
        #[allow(dead_code)]
        impl $name {
            /// The first index. Useful as a root sentinel.
            $vis const ZERO: Self = Self(match $crate::index::NonMaxU32::new(0) {
                Some(v) => v,
                None => unreachable!(),
            });

            /// Construct from a `u32`.
            ///
            /// # Panics
            /// Panics if `value == u32::MAX`.
            #[inline]
            #[must_use]
            $vis const fn new(value: u32) -> Self {
                match $crate::index::NonMaxU32::new(value) {
                    Some(v) => Self(v),
                    None => panic!(concat!(stringify!($name), " overflow: u32::MAX is reserved as the niche")),
                }
            }

            /// The underlying integer.
            #[inline]
            #[must_use]
            $vis const fn as_u32(self) -> u32 {
                self.0.get()
            }
        }

        impl $crate::index::Idx for $name {
            #[inline]
            fn from_usize(value: usize) -> Self {
                debug_assert!(value < u32::MAX as usize, concat!(stringify!($name), " overflow"));
                #[allow(clippy::cast_possible_truncation)]
                Self::new(value as u32)
            }

            #[inline]
            fn index(self) -> usize {
                self.0.get() as usize
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0.get())
            }
        }
    };
}

/// A `Vec` indexed by a typed [`Idx`] rather than by `usize`.
///
/// Using this instead of `Vec<T>` means a `SymbolId` cannot be used to index the
/// node table — a class of bug that is easy to write and hard to find in the Go
/// original, where everything is a pointer.
pub struct IndexVec<I: Idx, T> {
    raw: Vec<T>,
    _marker: PhantomData<fn(I) -> I>,
}

impl<I: Idx, T> IndexVec<I, T> {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self { raw: Vec::new(), _marker: PhantomData }
    }

    /// An empty table with room for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self { raw: Vec::with_capacity(capacity), _marker: PhantomData }
    }

    /// Append `value`, returning its index.
    #[inline]
    pub fn push(&mut self, value: T) -> I {
        let idx = I::from_usize(self.raw.len());
        self.raw.push(value);
        idx
    }

    /// Number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Whether the table is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Get an entry, or `None` if out of bounds.
    #[inline]
    #[must_use]
    pub fn get(&self, index: I) -> Option<&T> {
        self.raw.get(index.index())
    }

    /// Get an entry mutably, or `None` if out of bounds.
    #[inline]
    pub fn get_mut(&mut self, index: I) -> Option<&mut T> {
        self.raw.get_mut(index.index())
    }

    /// Iterate over entries.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.raw.iter()
    }

    /// Iterate over `(index, entry)` pairs.
    #[must_use]
    pub fn iter_enumerated(&self) -> impl ExactSizeIterator<Item = (I, &T)> {
        self.raw.iter().enumerate().map(|(i, v)| (I::from_usize(i), v))
    }

    /// The underlying storage.
    #[must_use]
    pub fn as_raw(&self) -> &[T] {
        &self.raw
    }
}

impl<I: Idx, T: Clone> IndexVec<I, T> {
    /// A table of `len` copies of `value`.
    #[must_use]
    pub fn from_elem(value: T, len: usize) -> Self {
        Self { raw: vec![value; len], _marker: PhantomData }
    }

    /// Grow the table to at least `len`, filling with `value`.
    pub fn resize(&mut self, len: usize, value: T) {
        self.raw.resize(len, value);
    }
}

impl<I: Idx, T> Default for IndexVec<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Idx, T> std::ops::Index<I> for IndexVec<I, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: I) -> &T {
        &self.raw[index.index()]
    }
}

impl<I: Idx, T> std::ops::IndexMut<I> for IndexVec<I, T> {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut T {
        &mut self.raw[index.index()]
    }
}

impl<I: Idx, T: fmt::Debug> fmt::Debug for IndexVec<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.raw.iter()).finish()
    }
}

impl<I: Idx, T> FromIterator<T> for IndexVec<I, T> {
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self { raw: iter.into_iter().collect(), _marker: PhantomData }
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IndexVec<I, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    define_index! {
        /// Test index.
        pub struct TestId;
    }

    #[test]
    fn option_id_is_niche_optimized() {
        assert_eq!(size_of::<TestId>(), 4);
        assert_eq!(size_of::<Option<TestId>>(), 4);
    }

    #[test]
    fn push_returns_the_index_that_reads_back() {
        let mut v = IndexVec::<TestId, &str>::new();
        let a = v.push("a");
        let b = v.push("b");
        assert_eq!(v[a], "a");
        assert_eq!(v[b], "b");
        assert_eq!(v.len(), 2);
    }
}
