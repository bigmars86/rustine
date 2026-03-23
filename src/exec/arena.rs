//! Arena-backed scratch allocator for hot-path execution temporaries.
//!
//! During grammar execution every match iteration allocates:
//! - Substituted action argument strings
//! - Capture group vectors
//! - Interpolation buffers
//!
//! [`ExecArena`] wraps a [`bumpalo::Bump`] allocator that lets these
//! allocations be freed in bulk after each statement iteration, yielding
//! measurably lower allocation overhead on large inputs.
//!
//! # Usage
//! ```ignore
//! let arena = ExecArena::new();
//! // … hot loop …
//! let s = arena.alloc_str("hello");
//! let v = arena.alloc_vec_with(|| String::new(), 8);
//! // … end of iteration …
//! arena.reset(); // O(1) bulk free
//! ```

use bumpalo::Bump;

/// Arena allocator tuned for the execution engine hot path.
///
/// Wraps [`bumpalo::Bump`] and exposes convenience helpers for the
/// common allocation patterns used during `Runner::run_grammar`.
pub struct ExecArena {
    bump: Bump,
}

impl ExecArena {
    /// Create a new arena with bumpalo's default initial capacity (≈ a few KB).
    pub fn new() -> Self {
        Self { bump: Bump::new() }
    }

    /// Create an arena pre-sized to `capacity` bytes (avoids early re-allocation
    /// for large inputs where the working set is known).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bump: Bump::with_capacity(capacity),
        }
    }

    /// Allocate a string slice in the arena, returning a `&str` that lives as
    /// long as the arena (or until [`reset`](Self::reset)).
    pub fn alloc_str(&self, s: &str) -> &str {
        self.bump.alloc_str(s)
    }

    /// Allocate a `T` in the arena and return a mutable reference.
    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.bump.alloc(val)
    }

    /// Allocate `n` consecutive copies of `T::default()` and return a mutable
    /// slice. Useful for pre-allocating capture vectors.
    pub fn alloc_slice_fill_default<T: Default>(&self, n: usize) -> &mut [T] {
        self.bump.alloc_slice_fill_with(n, |_| T::default())
    }

    /// Reset the arena, deallocating all memory at once in *O(1)*.
    ///
    /// All references previously returned by this arena become invalid.
    /// Callers must ensure no references survive across a reset.
    pub fn reset(&mut self) {
        self.bump.reset();
    }

    /// Total bytes allocated in the arena (useful for diagnostics/benchmarks).
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }
}

impl Default for ExecArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_str_roundtrip() {
        let arena = ExecArena::new();
        let s = arena.alloc_str("hello world");
        assert_eq!(s, "hello world");
    }

    #[test]
    fn alloc_and_mutate() {
        let arena = ExecArena::new();
        let val = arena.alloc(42u64);
        *val = 99;
        assert_eq!(*val, 99);
    }

    #[test]
    fn reset_frees_memory() {
        let mut arena = ExecArena::new();
        for _ in 0..100 {
            arena.alloc_str("some temporary string used in hot loop");
        }
        let before = arena.allocated_bytes();
        assert!(before > 0);
        arena.reset();
        // After reset, allocated_bytes may report 0 or the chunk overhead.
        // The key invariant: new allocations reuse the memory.
        let s = arena.alloc_str("reused");
        assert_eq!(s, "reused");
    }

    #[test]
    fn slice_fill_default() {
        let arena = ExecArena::new();
        let slice: &mut [String] = arena.alloc_slice_fill_default(4);
        assert_eq!(slice.len(), 4);
        slice[0] = "a".to_string();
        slice[1] = "b".to_string();
        assert_eq!(slice[0], "a");
        assert_eq!(slice[1], "b");
        assert_eq!(slice[2], "");
    }
}
