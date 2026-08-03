//! A bump allocator for AST nodes.
//!
//! The AST is built once and freed all at once, which is the exact shape a bump
//! allocator serves: allocation is a pointer increment, and deallocation is
//! dropping the arena. Upstream does the same thing with `core.Arena[T]`
//! (`internal/core/arena.go`), and oxc with `oxc_allocator`.
//!
//! # No destructors
//!
//! Arena-allocated values are **never dropped**. Everything in the AST is plain
//! data — `&'a str`, `&'a [T]`, ids, flags — so there is nothing to run, and
//! avoiding drop bookkeeping is most of the speed win. Allocating a type that
//! needs dropping is a leak, so [`Arena::alloc`] rejects it at compile time.
//!
//! # Interior mutability
//!
//! Allocation takes `&self`, not `&mut self`. The parser holds `&'a Arena` while
//! simultaneously holding `&'a` references to everything it has already
//! allocated; requiring `&mut` would make that borrow-check impossible.

use std::{
    alloc::{Layout, alloc, dealloc},
    cell::{Cell, RefCell},
    marker::PhantomData,
    mem, ptr,
};

/// Compile-time proof that `T` needs no destructor.
///
/// The arena never drops what it allocates, so allocating a type that owns
/// anything would leak. A marker trait cannot express this — a blanket impl for
/// `Copy` would exclude AST nodes, which hold a `Cell` and so are not `Copy`,
/// while `needs_drop` is exactly the property that matters. An inline `const`
/// block turns it into a compile error at the call site.
macro_rules! assert_no_drop {
    ($t:ty) => {
        const {
            assert!(
                !::std::mem::needs_drop::<$t>(),
                "arena-allocated types must not need dropping; the arena never runs a destructor",
            );
        }
    };
}

/// Default chunk size: 4 KiB, doubling up to [`MAX_CHUNK`].
const FIRST_CHUNK: usize = 4 * 1024;
/// Chunks stop growing here so one enormous file cannot reserve a huge block.
const MAX_CHUNK: usize = 1024 * 1024;

struct Chunk {
    ptr: ptr::NonNull<u8>,
    layout: Layout,
}

/// A bump allocator.
///
/// Values allocated here live as long as the borrow of the arena, and are freed
/// together when it is dropped.
pub struct Arena {
    /// Filled chunks, kept only so they can be freed.
    chunks: RefCell<Vec<Chunk>>,
    /// Bump pointer into the newest chunk.
    current: Cell<*mut u8>,
    /// One past the end of the newest chunk.
    end: Cell<*mut u8>,
    /// Bytes handed out, for diagnostics and tests.
    allocated: Cell<usize>,
    /// `Arena` is not `Sync`: allocation mutates the bump pointer through `&self`.
    _not_sync: PhantomData<Cell<()>>,
}

impl Arena {
    /// Create an empty arena. No memory is reserved until the first allocation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chunks: RefCell::new(Vec::new()),
            current: Cell::new(ptr::null_mut()),
            end: Cell::new(ptr::null_mut()),
            allocated: Cell::new(0),
            _not_sync: PhantomData,
        }
    }

    /// Total bytes handed out so far.
    ///
    /// Excludes chunk padding, so it measures what the AST asked for rather than
    /// what the allocator reserved.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.allocated.get()
    }

    /// Move `value` into the arena.
    ///
    /// Rejects types needing a destructor at compile time.
    ///
    /// Returning `&mut` from `&self` is sound and deliberate: the block is freshly
    /// allocated, so no other reference to it exists. This is the same contract
    /// `bumpalo` and `oxc_allocator` expose, and it is what lets the parser hold
    /// the arena immutably while building a tree.
    #[allow(clippy::mut_from_ref)]
    pub fn alloc<T>(&self, value: T) -> &mut T {
        assert_no_drop!(T);
        let layout = Layout::new::<T>();
        let ptr = self.alloc_layout(layout).cast::<T>();
        // SAFETY: `alloc_layout` returned a block of at least `size_of::<T>()`
        // bytes aligned to `align_of::<T>()`, and it is uninitialised and
        // exclusively ours.
        unsafe {
            ptr.write(value);
            &mut *ptr
        }
    }

    /// Copy a slice into the arena.
    ///
    /// See [`Arena::alloc`] on why `&mut` from `&self` is sound here.
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_slice<T: Copy>(&self, values: &[T]) -> &mut [T] {
        assert_no_drop!(T);
        if values.is_empty() {
            return &mut [];
        }
        let layout = Layout::array::<T>(values.len()).expect("slice layout overflows");
        let ptr = self.alloc_layout(layout).cast::<T>();
        // SAFETY: the destination is uninitialised, exclusively ours, correctly
        // aligned, and exactly `values.len()` elements long; source and
        // destination cannot overlap because the destination is freshly allocated.
        unsafe {
            ptr::copy_nonoverlapping(values.as_ptr(), ptr, values.len());
            std::slice::from_raw_parts_mut(ptr, values.len())
        }
    }

    /// Copy a string into the arena.
    ///
    /// See [`Arena::alloc`] on why `&mut` from `&self` is sound here.
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_str(&self, value: &str) -> &mut str {
        let bytes = self.alloc_slice(value.as_bytes());
        // SAFETY: the bytes were copied from a `&str`, so they are valid UTF-8.
        unsafe { std::str::from_utf8_unchecked_mut(bytes) }
    }

    /// Reserve `layout` bytes, growing the arena if needed.
    fn alloc_layout(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        let current = self.current.get();
        // Round the bump pointer up to the required alignment.
        let aligned =
            (current as usize).checked_add(align - 1).expect("address overflow") & !(align - 1);

        if aligned.checked_add(size).is_none_or(|end| end > self.end.get() as usize) {
            return self.grow_and_alloc(layout);
        }

        let ptr = aligned as *mut u8;
        // SAFETY: `aligned + size` was just checked to be within the chunk.
        self.current.set(unsafe { ptr.add(size) });
        self.allocated.set(self.allocated.get() + size);
        ptr
    }

    /// Allocate a fresh chunk large enough for `layout`, then satisfy from it.
    #[cold]
    fn grow_and_alloc(&self, layout: Layout) -> *mut u8 {
        let mut chunks = self.chunks.borrow_mut();

        // Grow geometrically, but always at least large enough for this request.
        let previous = chunks.last().map_or(FIRST_CHUNK / 2, |c| c.layout.size());
        let size = (previous * 2).clamp(FIRST_CHUNK, MAX_CHUNK).max(layout.size() + layout.align());

        let chunk_layout =
            Layout::from_size_align(size, layout.align().max(mem::align_of::<usize>()))
                .expect("chunk layout is valid");

        // SAFETY: `chunk_layout` has a non-zero size, since `size >= FIRST_CHUNK`.
        let ptr = unsafe { alloc(chunk_layout) };
        let ptr =
            ptr::NonNull::new(ptr).unwrap_or_else(|| std::alloc::handle_alloc_error(chunk_layout));

        chunks.push(Chunk { ptr, layout: chunk_layout });
        self.current.set(ptr.as_ptr());
        // SAFETY: the chunk is `size` bytes long, so one-past-the-end is valid to
        // compute.
        self.end.set(unsafe { ptr.as_ptr().add(size) });
        drop(chunks);

        // The fresh chunk is aligned at least as strictly as `layout` requires and
        // is large enough, so this cannot recurse.
        self.alloc_layout(layout)
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        for chunk in self.chunks.borrow().iter() {
            // SAFETY: each chunk was allocated by `alloc` with exactly this
            // layout, and nothing else frees it.
            unsafe { dealloc(chunk.ptr.as_ptr(), chunk.layout) }
        }
    }
}

impl std::fmt::Debug for Arena {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Arena")
            .field("chunks", &self.chunks.borrow().len())
            .field("allocated_bytes", &self.allocated.get())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_and_reads_back() {
        let arena = Arena::new();
        let a = arena.alloc(42u32);
        let b = arena.alloc(7u64);
        assert_eq!(*a, 42);
        assert_eq!(*b, 7);
    }

    #[test]
    fn allocations_are_correctly_aligned() {
        let arena = Arena::new();
        // Interleave sizes so the bump pointer lands off-alignment repeatedly.
        for _ in 0..64 {
            let a = arena.alloc(1u8);
            let b = arena.alloc(2u64);
            let c = arena.alloc(3u16);
            assert_eq!(std::ptr::from_ref(a) as usize % align_of::<u8>(), 0);
            assert_eq!(std::ptr::from_ref(b) as usize % align_of::<u64>(), 0);
            assert_eq!(std::ptr::from_ref(c) as usize % align_of::<u16>(), 0);
        }
    }

    #[test]
    fn earlier_allocations_survive_growth() {
        // The bug this guards: a bump allocator that reallocates instead of
        // chaining chunks would invalidate every reference handed out so far.
        let arena = Arena::new();
        let first: &u64 = arena.alloc(0xDEAD_BEEF);
        let mut refs = Vec::new();
        for i in 0..10_000u64 {
            refs.push(arena.alloc(i));
        }
        assert_eq!(*first, 0xDEAD_BEEF, "growth must not move earlier allocations");
        for (i, r) in refs.iter().enumerate() {
            assert_eq!(**r, i as u64);
        }
    }

    #[test]
    fn slices_and_strings_round_trip() {
        let arena = Arena::new();
        let s = arena.alloc_slice(&[1u32, 2, 3]);
        assert_eq!(s, &[1, 2, 3]);
        let text = arena.alloc_str("hello");
        assert_eq!(text, "hello");
        let empty: &[u32] = arena.alloc_slice(&[]);
        assert!(empty.is_empty());
    }

    #[test]
    fn allocation_larger_than_a_chunk_still_works() {
        let arena = Arena::new();
        let big = vec![7u8; MAX_CHUNK * 2];
        let slice = arena.alloc_slice(&big);
        assert_eq!(slice.len(), MAX_CHUNK * 2);
        assert!(slice.iter().all(|&b| b == 7));
    }

    #[test]
    fn allocated_bytes_tracks_requests() {
        let arena = Arena::new();
        assert_eq!(arena.allocated_bytes(), 0);
        arena.alloc(0u64);
        assert_eq!(arena.allocated_bytes(), 8);
        arena.alloc_slice(&[0u32; 4]);
        assert_eq!(arena.allocated_bytes(), 24);
    }
}
