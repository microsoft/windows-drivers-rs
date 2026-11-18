use core::{ffi::CStr, fmt};

const DEFAULT_WDK_FORMAT_BUFFER_SIZE: usize = 512;

/// A fixed-size formatting buffer implementing [`fmt::Write`].
///
/// Allocates `N` bytes on the stack (default 512). The last byte is reserved
/// for a NUL terminator, so the usable content capacity is `N-1` bytes.
/// `N` must be at least 2; smaller values will not compile.
/// Intended for constrained driver environments where heap allocation is
/// undesirable.
///
/// Append with `write!`/`format_args!`; read via [`FormatBuffer::as_str`]
/// or [`FormatBuffer::as_c_str`].
///
/// # No runtime panics
///
/// No method on this type can panic at runtime. That is a deliberate
/// guarantee rather than an accident of the current implementation: in
/// kernel mode the `wdk-panic` handler calls `KeBugCheckEx`, so a panic
/// reached while formatting a diagnostic message would bugcheck the machine
/// over the message describing the problem. Overlong writes truncate and
/// report [`fmt::Error`]; nothing indexes a slice with a value the compiler
/// cannot bound.
///
/// The only `assert!` here is [`FormatBuffer::new`]'s `N >= 2`, which is
/// evaluated in a `const` block and so is a compile error, not a panic.
///
/// # Examples
/// ```
/// use core::fmt::Write;
///
/// use wdk::fmt::FormatBuffer;
///
/// let mut buf = FormatBuffer::<16>::new();
/// write!(&mut buf, "hello {}", 42).unwrap();
///
/// let s = buf.as_str();
/// assert_eq!(s, "hello 42");
///
/// let c = buf.as_c_str();
/// assert_eq!(c.to_bytes(), b"hello 42");
/// ```
#[derive(Clone)]
pub struct FormatBuffer<const N: usize = DEFAULT_WDK_FORMAT_BUFFER_SIZE> {
    buffer: [u8; N],
    used: usize,
}

impl<const N: usize> FormatBuffer<N> {
    /// Creates a zeroed formatting buffer with capacity `N`.
    ///
    /// The buffer starts empty (`used == 0`) and is ready for `fmt::Write`.
    ///
    /// `N` must be at least 2 (one byte of content plus the NUL terminator).
    /// Smaller values will not compile:
    /// ```compile_fail
    /// use wdk::fmt::FormatBuffer;
    /// let _ = FormatBuffer::<1>::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        const {
            assert!(
                N >= 2,
                "N must be at least 2 (one byte of content plus the NUL terminator)"
            );
        }
        Self {
            buffer: [0; N],
            used: 0,
        }
    }

    /// Clears the buffer, resetting it to its initial empty state.
    pub const fn clear(&mut self) {
        self.used = 0;
        // `first_mut` rather than `buffer[0]`, which panics for `N == 0`.
        if let Some(terminator) = self.buffer.first_mut() {
            *terminator = 0;
        }
    }

    /// Returns the number of bytes currently written.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.used
    }

    /// Returns the usable capacity in bytes (`N - 1`, excluding the reserved
    /// NUL terminator).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        Self::content_capacity()
    }

    /// The usable capacity, without needing a buffer to ask.
    ///
    /// `saturating_sub` rather than `N - 1` so that this is total for every
    /// `N`. `FormatBuffer::<0>` cannot be constructed — `new`'s `const`
    /// assertion rejects it — but an underflow guarded by a constructor is
    /// still an unchecked invariant, and this file does not keep any (see the
    /// type-level "No runtime panics" note).
    const fn content_capacity() -> usize {
        N.saturating_sub(1)
    }

    /// Returns `true` if no bytes have been written.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// Returns a UTF-8 view over the written bytes.
    ///
    /// Only the bytes successfully written are included in the returned
    /// slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // `get` rather than `&self.buffer[..self.used]`: `used` never exceeds
        // `N - 1`, but the index form would still emit a panicking branch, and
        // an empty view is a better failure than a bugcheck if it ever did.
        let written = match self.buffer.get(..self.used) {
            Some(written) => written,
            None => &[],
        };

        // SAFETY: All writes come from `&str` sources (valid UTF-8) — both
        // `FormatBuffer::write_str` and `FlushableFormatBuffer::write_str` copy
        // only from `&str::as_bytes()`, and `append_bytes` appends the whole
        // slice or none of it, so `buffer[..used]` never ends mid-sequence.
        // Fields are module-private.
        unsafe { core::str::from_utf8_unchecked(written) }
    }

    /// Returns a C string view up to the first `NUL` byte.
    ///
    /// The buffer always contains a NUL terminator because every mutation
    /// method reserves the last byte for one, so this is total: an empty
    /// [`CStr`] is the worst case, never a panic.
    #[must_use]
    pub const fn as_c_str(&self) -> &CStr {
        // Only scan up to `used + 1` — the NUL is guaranteed at `buffer[used]`.
        // `split_at_checked` rather than `split_at` because the latter panics
        // on an out-of-range index; the fallback keeps this total even if the
        // NUL invariant were ever broken by a future edit.
        let scanned = match self.buffer.split_at_checked(self.used + 1) {
            Some((scanned, _)) => scanned,
            None => &self.buffer,
        };

        match CStr::from_bytes_until_nul(scanned) {
            Ok(cstr) => cstr,
            // Unreachable while the NUL invariant holds. Reported as the empty
            // string rather than as a panic: this type is used to format
            // diagnostics in kernel mode, where a panic is a bugcheck, and an
            // empty trace message is a far better outcome than taking the
            // machine down while describing another problem.
            Err(_) => c"",
        }
    }

    /// Appends `bytes` to the buffer and NUL-terminates, if all of it fits.
    ///
    /// Returns `false` and leaves the buffer untouched if `bytes` is longer
    /// than the remaining capacity (`N - 1 - used`), where the previous
    /// implementation indexed past the end and panicked.
    ///
    /// All-or-nothing rather than "append what fits" on purpose. Every caller
    /// has already cut `bytes` at a `char` boundary, so it knows where a safe
    /// cut is and this does not; truncating here to whatever happened to fit
    /// could leave a partial UTF-8 sequence in the buffer, and
    /// [`as_str`](Self::as_str) reads it with `from_utf8_unchecked` — which
    /// would trade a panic for undefined behavior rather than fix it.
    fn append_bytes(&mut self, bytes: &[u8]) -> bool {
        let Some(end) = self
            .used
            .checked_add(bytes.len())
            .filter(|&end| end <= Self::content_capacity())
        else {
            return false;
        };

        // `get_mut` rather than indexing: the bound is established above, and
        // an index would still emit a panicking branch for the compiler's
        // benefit that the kernel panic handler would service as a bugcheck.
        let Some(destination) = self.buffer.get_mut(self.used..end) else {
            return false;
        };
        destination.copy_from_slice(bytes);
        self.used = end;

        // `end <= N - 1`, so the terminator is always in range; written through
        // `get_mut` for the same reason as above.
        if let Some(terminator) = self.buffer.get_mut(self.used) {
            *terminator = 0;
        }

        true
    }
}

impl<const N: usize> Default for FormatBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Debug for FormatBuffer<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormatBuffer")
            .field("used", &self.used)
            .field("capacity", &Self::content_capacity())
            .field("content", &self.as_str())
            .finish_non_exhaustive()
    }
}

impl<const N: usize> fmt::Write for FormatBuffer<N> {
    /// # Errors
    ///
    /// Returns [`fmt::Error`] if `s` exceeds the remaining capacity. UTF-8
    /// chars that fit are written to the buffer before the error is returned.
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // The last byte (buffer[N-1]) is reserved for the NUL terminator
        // so that the buffer always contains a valid `CStr`.
        let remaining = Self::content_capacity().saturating_sub(self.used);

        // Overflow: copy what fits at a char boundary and signal error.
        if s.len() > remaining {
            let fit = s.floor_char_boundary(remaining);
            // `get` rather than `s.as_bytes()[..fit]`: `floor_char_boundary`
            // already bounds `fit` by `remaining <= s.len()`, and the index
            // form would still emit a panicking branch.
            if let Some(fitting) = s.as_bytes().get(..fit) {
                self.append_bytes(fitting);
            }
            return Err(fmt::Error);
        }

        // Normal write: append the full string. The append cannot be refused
        // here — `s.len() <= remaining` — but a refusal is reported rather than
        // asserted, since `Err` is already this method's way of saying "not all
        // of it landed" and an assertion would be a bugcheck in kernel mode.
        if self.append_bytes(s.as_bytes()) {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

/// A [`FormatBuffer`] wrapper that auto-flushes on overflow.
///
/// When a `write_str` call would exceed the buffer capacity, the current
/// contents are flushed via the provided closure, the buffer is cleared, and
/// writing continues with the remainder. This allows arbitrarily long
/// formatted output to be processed in fixed-size chunks.
/// `N` must be at least 2 (enforced by [`FormatBuffer::new`]).
///
/// After all writes are complete, any remaining buffered content is
/// automatically flushed when the writer is dropped. The caller may also
/// call [`flush`](Self::flush) explicitly to drain the buffer early.
///
/// # Panics
///
/// If `flush_fn` panics, the panic propagates from [`flush`](Self::flush).
/// If `flush_fn` panics during [`drop`](Drop::drop), the drop will also
/// panic.
pub struct FlushableFormatBuffer<
    F: FnMut(&FormatBuffer<N>),
    const N: usize = DEFAULT_WDK_FORMAT_BUFFER_SIZE,
> {
    format_buffer: FormatBuffer<N>,
    flush_fn: F,
}

impl<F: FnMut(&FormatBuffer<N>), const N: usize> FlushableFormatBuffer<F, N> {
    /// Creates a new flushable writer with the given flush closure.
    #[must_use]
    pub const fn new(flush_fn: F) -> Self {
        Self {
            format_buffer: FormatBuffer::new(),
            flush_fn,
        }
    }

    /// Flushes any remaining buffered content via the closure.
    ///
    /// This is a no-op if the buffer is empty.
    pub fn flush(&mut self) {
        if self.format_buffer.used == 0 {
            return;
        }
        (self.flush_fn)(&self.format_buffer);
        self.format_buffer.clear();
    }
}

impl<F: FnMut(&FormatBuffer<N>), const N: usize> Drop for FlushableFormatBuffer<F, N> {
    fn drop(&mut self) {
        self.flush();
    }
}

impl<F: FnMut(&FormatBuffer<N>), const N: usize> fmt::Write for FlushableFormatBuffer<F, N> {
    /// Appends `s` to the buffer, flushing via the closure whenever the
    /// buffer fills. Returns [`fmt::Error`] only when a single UTF-8 code
    /// point is larger than the usable buffer capacity (`N - 1` bytes).
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let capacity = FormatBuffer::<N>::content_capacity();
        let mut remaining = s;

        // Fill what fits at a char boundary, flush, continue with the rest.
        // `saturating_sub` throughout: `used <= capacity` is an invariant of
        // `append_bytes`, but this file does not index or subtract on an
        // invariant it has not just established (see `FormatBuffer`'s
        // "No runtime panics" note).
        while remaining.len() > capacity.saturating_sub(self.format_buffer.used) {
            let remaining_space = capacity.saturating_sub(self.format_buffer.used);
            let split = remaining.floor_char_boundary(remaining_space);

            if split == 0 {
                if self.format_buffer.used == 0 {
                    // A single character doesn't fit in the entire buffer.
                    return Err(fmt::Error);
                }
                // Buffer has content but no room for the next char — flush and
                // retry.
                self.flush();
                continue;
            }

            // `get` rather than indexing, on both the source and the remainder:
            // `split` is a `char` boundary no greater than `remaining.len()`,
            // so neither can fail, and neither should cost a panicking branch.
            // A `None` would mean that reasoning is wrong, which is reported as
            // `Err` rather than as a bugcheck.
            let (Some(fitting), Some(rest)) =
                (remaining.as_bytes().get(..split), remaining.get(split..))
            else {
                return Err(fmt::Error);
            };

            if !self.format_buffer.append_bytes(fitting) {
                return Err(fmt::Error);
            }

            self.flush();

            remaining = rest;
        }

        // Remaining bytes fit in the buffer.
        if self.format_buffer.append_bytes(remaining.as_bytes()) {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_WDK_FORMAT_BUFFER_SIZE, FlushableFormatBuffer, FormatBuffer};

    mod format_buffer {
        use core::fmt::Write;

        use super::*;

        #[test]
        fn initialize() {
            let fmt_buffer: FormatBuffer = FormatBuffer::new();
            assert_eq!(fmt_buffer.used, 0);
            assert_eq!(fmt_buffer.buffer.len(), DEFAULT_WDK_FORMAT_BUFFER_SIZE);
            assert!(fmt_buffer.buffer.iter().all(|&b| b == 0));
        }

        #[test]
        fn change_len() {
            let fmt_buffer: FormatBuffer<2> = FormatBuffer::new();
            assert_eq!(fmt_buffer.buffer.len(), 2);
        }

        #[test]
        fn minimum_buffer_write() {
            let mut fmt_buffer = FormatBuffer::<2>::new();
            assert!(write!(&mut fmt_buffer, "a").is_ok());
            assert_eq!(fmt_buffer.as_str(), "a");
            assert!(write!(&mut fmt_buffer, "b").is_err());
        }

        #[test]
        fn write() {
            let mut fmt_buffer: FormatBuffer = FormatBuffer::new();
            let world: &str = "world";
            assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());

            let mut cmp_buffer: [u8; 512] = [0; 512];
            let cmp_str: &str = "Hello world!";
            cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

            assert_eq!(fmt_buffer.buffer, cmp_buffer);
        }

        #[test]
        fn as_str() {
            let mut fmt_buffer: FormatBuffer = FormatBuffer::new();
            let world: &str = "world";
            assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());
            assert_eq!(fmt_buffer.as_str(), "Hello world!");
        }

        #[test]
        fn ref_sanity_check() {
            let mut fmt_buffer: FormatBuffer = FormatBuffer::new();
            let world: &str = "world";
            assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());

            // borrow fmt_buffer -- while this is in scope we cannot edit
            // fmt_buffer
            let buf_str = fmt_buffer.as_str();
            // buf_str borrows fmt_buffer, so we cannot write to it here.
            assert_eq!(buf_str, "Hello world!");

            // buf_str cannot be used after this. The backing buffer stays in
            // scope.
            assert!(write!(&mut fmt_buffer, " Second sentence!").is_ok());
            assert_eq!(fmt_buffer.as_str(), "Hello world! Second sentence!");

            // as_c_str now borrows immutably
            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"Hello world! Second sentence!\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);

            // mutable borrow ends here so we can edit the backing buffer.
            assert!(write!(&mut fmt_buffer, " A third sentence!").is_ok());
            assert_eq!(
                fmt_buffer.as_str(),
                "Hello world! Second sentence! A third sentence!"
            );
        }

        #[test]
        fn overflow_buffer() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            assert!(write!(&mut fmt_buffer, "0123456789").is_err());

            // Usable capacity is N-1 = 7; last byte reserved for NUL
            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn exact_buffer_size() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            // Writing exactly N bytes overflows (capacity is N-1)
            assert!(write!(&mut fmt_buffer, "01234567").is_err());

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn exact_capacity_fit() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            // Writing exactly N-1 bytes succeeds
            assert!(write!(&mut fmt_buffer, "0123456").is_ok());

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn overflow_buffer_after_multiple_writes() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            assert!(write!(&mut fmt_buffer, "01234").is_ok());
            assert!(write!(&mut fmt_buffer, "56789").is_err());

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn overflow_buffer_then_multiple_writes() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            assert!(write!(&mut fmt_buffer, "01234").is_ok());
            assert!(write!(&mut fmt_buffer, "56789").is_err());
            assert!(write!(&mut fmt_buffer, "overflow!").is_err());
            assert!(write!(&mut fmt_buffer, "overflow!").is_err());

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn exact_buffer_size_multiple_writes() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            assert!(write!(&mut fmt_buffer, "01234").is_ok());
            // "56" fits in remaining capacity (2 bytes), but "567" overflows
            assert!(write!(&mut fmt_buffer, "567").is_err());

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "0123456");

            let cmp_c_str: &core::ffi::CStr =
                core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn empty_buffer_strs() {
            let fmt_buffer: FormatBuffer<8> = FormatBuffer::new();

            let buf_str = fmt_buffer.as_str();
            assert_eq!(buf_str, "");

            let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn write_empty_strings() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            assert!(write!(&mut fmt_buffer, "").is_ok());
            assert!(write!(&mut fmt_buffer, "").is_ok());

            assert_eq!(fmt_buffer.used, 0);
            assert!(fmt_buffer.buffer.iter().all(|&b| b == 0));

            assert_eq!(fmt_buffer.as_str(), "");

            let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
            let buf_c_str = fmt_buffer.as_c_str();
            assert_eq!(buf_c_str, cmp_c_str);
        }

        #[test]
        fn overflow_truncates_at_char_boundary() {
            let mut fmt_buffer: FormatBuffer<8> = FormatBuffer::new();
            // Capacity is 7. "❤️🧡💛💚💙💜" is 26 bytes.
            // ❤️ is 6 bytes, 🧡 starts at byte 6 but needs 4 bytes (total 10).
            // floor_char_boundary(7) = 6, so only ❤️ fits.
            assert!(write!(&mut fmt_buffer, "❤️🧡💛💚💙💜").is_err());
            assert_eq!(fmt_buffer.as_str(), "❤️");
        }

        #[test]
        fn interior_nul_truncates_cstr() {
            let mut fmt_buffer = FormatBuffer::<16>::new();
            assert!(write!(&mut fmt_buffer, "hello\0world").is_ok());
            assert_eq!(fmt_buffer.as_str(), "hello\0world");
            assert_eq!(fmt_buffer.as_c_str(), c"hello");
        }

        #[test]
        fn clear_empties_buffer() {
            let mut fmt_buffer = FormatBuffer::<8>::new();
            assert!(write!(&mut fmt_buffer, "hello").is_ok());
            fmt_buffer.clear();
            assert_eq!(fmt_buffer.used, 0);
            assert_eq!(fmt_buffer.as_str(), "");
            assert_eq!(fmt_buffer.as_c_str(), c"");
        }

        #[test]
        fn clear_then_shorter_write_produces_correct_cstr() {
            let mut fmt_buffer = FormatBuffer::<8>::new();
            assert!(write!(&mut fmt_buffer, "hello").is_ok());
            fmt_buffer.clear();
            assert!(write!(&mut fmt_buffer, "hi").is_ok());
            assert_eq!(fmt_buffer.as_str(), "hi");
            assert_eq!(fmt_buffer.as_c_str(), c"hi");
        }

        /// `append_bytes` used to index `buffer[used..used + bytes.len()]`,
        /// which panics when that exceeds the capacity. No caller can reach
        /// that — both `write_str` impls bound the slice first — but the branch
        /// existed in the emitted code regardless, and in kernel mode the
        /// `wdk-panic` handler makes any such branch a `KeBugCheckEx` call
        /// sitting in a shipped driver. Called directly here because that is
        /// the only way to reach the condition at all.
        #[test]
        fn append_bytes_refuses_an_oversized_slice_rather_than_panicking() {
            let mut fmt_buffer = FormatBuffer::<8>::new();

            // Capacity is 7. Eight bytes cannot fit even into an empty buffer.
            assert!(!fmt_buffer.append_bytes(b"01234567"));
            assert_eq!(fmt_buffer.used, 0, "a refused append must not advance");
            assert_eq!(fmt_buffer.as_str(), "");

            assert!(fmt_buffer.append_bytes(b"01234"));
            assert_eq!(fmt_buffer.as_str(), "01234");

            // Three more would reach 8, one past the capacity.
            assert!(!fmt_buffer.append_bytes(b"567"));
            assert_eq!(
                fmt_buffer.as_str(),
                "01234",
                "a refused append must leave the buffer exactly as it was"
            );

            // Two more land exactly on the capacity.
            assert!(fmt_buffer.append_bytes(b"56"));
            assert_eq!(fmt_buffer.as_str(), "0123456");
            assert_eq!(fmt_buffer.as_c_str(), c"0123456");

            // A full buffer refuses anything further, including at the
            // boundary.
            assert!(!fmt_buffer.append_bytes(b"7"));
            assert!(fmt_buffer.append_bytes(b""), "an empty append always fits");
            assert_eq!(fmt_buffer.as_str(), "0123456");
        }

        /// The refusal is all-or-nothing rather than "append what fits" because
        /// `as_str` reads the buffer with `from_utf8_unchecked`: appending a
        /// prefix of a multi-byte sequence would trade a panic for undefined
        /// behavior. The callers cut at a `char` boundary; this does not cut.
        #[test]
        fn append_bytes_never_leaves_a_partial_utf8_sequence() {
            let mut fmt_buffer = FormatBuffer::<8>::new();
            assert!(fmt_buffer.append_bytes(b"01234"));

            // 💜 is 4 bytes and only 2 remain. Truncating to fit would leave
            // two bytes of a 4-byte sequence behind.
            assert!(!fmt_buffer.append_bytes("💜".as_bytes()));
            assert_eq!(fmt_buffer.as_str(), "01234");
            assert!(core::str::from_utf8(&fmt_buffer.buffer[..fmt_buffer.used]).is_ok());
        }

        /// `as_c_str` used to `panic!` on a missing NUL and `capacity` used to
        /// evaluate `N - 1`. Both are total now, so the smallest constructible
        /// buffer exercises the edges without a panic.
        #[test]
        fn the_smallest_buffer_has_no_edge_case() {
            let mut fmt_buffer = FormatBuffer::<2>::new();
            assert_eq!(fmt_buffer.capacity(), 1);
            assert_eq!(fmt_buffer.as_c_str(), c"");

            assert!(fmt_buffer.append_bytes(b"a"));
            assert_eq!(fmt_buffer.as_c_str(), c"a");
            assert!(!fmt_buffer.append_bytes(b"b"));

            fmt_buffer.clear();
            assert_eq!(fmt_buffer.as_c_str(), c"");
        }
    }

    mod flushable_format_buffer {
        extern crate alloc;

        use alloc::{borrow::ToOwned, string::String, vec, vec::Vec};
        use core::fmt::Write;

        use super::*;

        #[test]
        fn write_fits_in_buffer() {
            let mut flushed: Vec<String> = Vec::new();
            let mut writer = FlushableFormatBuffer::<_, 16>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "hello").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["hello"]);
        }

        #[test]
        fn explicit_flush_then_continue() {
            let mut flushed: Vec<String> = Vec::new();
            let mut writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "abc").is_ok());
            writer.flush();
            assert!(write!(&mut writer, "def").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["abc", "def"]);
        }

        #[test]
        fn overflow_triggers_flush() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 7 usable bytes
            let mut writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            // "0123456789" is 10 bytes — exceeds 7-byte capacity.
            // First 7 bytes fill the buffer, triggering a flush.
            // Remaining "789" goes into the cleared buffer.
            assert!(write!(&mut writer, "0123456789").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["0123456", "789"]);
        }

        #[test]
        fn multi_flush() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 3 usable bytes
            let mut writer = FlushableFormatBuffer::<_, 4>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            // "0123456789" is 10 bytes — triggers 3 flushes (3+3+3), leaves "9"
            // in buffer.
            assert!(write!(&mut writer, "0123456789").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["012", "345", "678", "9"]);
        }

        #[test]
        fn empty_write_does_not_flush() {
            let mut flushed: Vec<String> = Vec::new();
            let mut writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "").is_ok());
            assert!(write!(&mut writer, "").is_ok());
            drop(writer);
            assert_eq!(flushed, Vec::<String>::new());
        }

        #[test]
        fn flush_empty_buffer_is_noop() {
            let mut flushed: Vec<String> = Vec::new();
            let writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            drop(writer);
            assert_eq!(flushed, Vec::<String>::new());
        }

        #[test]
        fn exact_capacity_fit() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 7 usable bytes
            let mut writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            // Exactly 7 bytes — fits perfectly, no flush triggered.
            assert!(write!(&mut writer, "0123456").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["0123456"]);
        }

        #[test]
        fn multiple_writes_with_intermittent_overflow() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 7 usable bytes
            let mut writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "abc").is_ok());
            assert!(write!(&mut writer, "def").is_ok());
            assert!(write!(&mut writer, "ghi").is_ok());
            assert!(write!(&mut writer, "jkl").is_ok());
            assert!(write!(&mut writer, "mno").is_ok());
            drop(writer);
            // Flush order proves overflow happened at the right boundaries:
            // "abcdefg" (7), "hijklmn" (7), "o" (remainder)
            assert_eq!(flushed, vec!["abcdefg", "hijklmn", "o"]);
        }

        #[test]
        fn multi_byte_chars_split_at_char_boundary() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 6 usable bytes.
            // ❤️ is 6 bytes (U+2764 + U+FE0F), each other heart is 4 bytes.
            // "❤️🧡💛💚💙💜" is 26 bytes total — each heart gets its own chunk.
            let mut writer = FlushableFormatBuffer::<_, 7>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["❤️", "🧡", "💛", "💚", "💙", "💜"]);
        }

        #[test]
        fn multi_byte_char_triggers_early_flush() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 6 usable bytes.
            // "abcd" (4 bytes) leaves 2 bytes of space — not enough for ❤️ (6
            // bytes). Flushes "abcd", then chunks the hearts as in
            // the previous test.
            let mut writer = FlushableFormatBuffer::<_, 7>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "abcd").is_ok());
            assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_ok());
            drop(writer);
            assert_eq!(flushed, vec!["abcd", "❤️", "🧡", "💛", "💚", "💙", "💜"]);
        }

        #[test]
        fn multi_byte_char_too_big_for_buffer() {
            let mut flushed: Vec<String> = Vec::new();
            // Capacity is N-1 = 2 usable bytes.
            // ❤️🧡💛💚💙💜 starts with ❤ (3 bytes) — can never fit.
            let mut writer = FlushableFormatBuffer::<_, 3>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_err());
            drop(writer);
            assert_eq!(flushed, Vec::<String>::new());
        }
    }
}
