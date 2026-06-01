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
        self.buffer[0] = 0;
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
        N - 1
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
        // SAFETY: All writes come from `&str` sources (valid UTF-8) — both
        // `FormatBuffer::write_str` and `FlushableFormatBuffer::write_str`
        // copy only from `&str::as_bytes()`. Fields are module-private.
        unsafe { core::str::from_utf8_unchecked(&self.buffer[..self.used]) }
    }

    /// Returns a C string view up to the first `NUL` byte.
    ///
    /// The buffer always contains a NUL terminator because `write_str`
    /// reserves the last byte.
    ///
    /// # Panics
    ///
    /// Panics if the buffer contains no NUL byte. This should never happen
    /// in practice — the NUL invariant is maintained by all mutation methods.
    #[must_use]
    pub const fn as_c_str(&self) -> &CStr {
        // Only scan up to `used + 1` — the NUL is guaranteed at `buffer[used]`.
        match CStr::from_bytes_until_nul(self.buffer.split_at(self.used + 1).0) {
            Ok(cstr) => cstr,
            // `unreachable!()` with a message uses `format_args!`, which is
            // not const-compatible. Use `panic!` with a string literal instead.
            Err(_) => {
                panic!("internal error: entered unreachable code: buffer must contain a NUL byte")
            }
        }
    }

    /// Appends `bytes` to the buffer and NUL-terminates.
    ///
    /// # Panics
    ///
    /// Panics if `bytes.len()` exceeds the remaining capacity (`N - 1 - used`).
    fn append_bytes(&mut self, bytes: &[u8]) {
        self.buffer[self.used..self.used + bytes.len()].copy_from_slice(bytes);
        self.used += bytes.len();
        self.buffer[self.used] = 0;
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
            .field("capacity", &(N - 1))
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
        let capacity = N - 1;
        let remaining = capacity - self.used;

        // Overflow: copy what fits at a char boundary and signal error.
        if s.len() > remaining {
            let fit = s.floor_char_boundary(remaining);
            self.append_bytes(&s.as_bytes()[..fit]);
            return Err(fmt::Error);
        }

        // Normal write: append the full string.
        self.append_bytes(s.as_bytes());
        Ok(())
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
        let capacity = N - 1;
        let mut remaining = s;

        // Fill what fits at a char boundary, flush, continue with the rest.
        while remaining.len() > capacity - self.format_buffer.used {
            let remaining_space = capacity - self.format_buffer.used;
            let split = remaining.floor_char_boundary(remaining_space);

            if split == 0 {
                if self.format_buffer.used == 0 {
                    // A single character doesn't fit in the entire buffer.
                    return Err(fmt::Error);
                }
                // Buffer has content but no room for the next char — flush and retry.
                self.flush();
                continue;
            }

            self.format_buffer
                .append_bytes(&remaining.as_bytes()[..split]);

            self.flush();

            remaining = &remaining[split..];
        }

        // Remaining bytes fit in the buffer.
        self.format_buffer.append_bytes(remaining.as_bytes());
        Ok(())
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

            // borrow fmt_buffer -- while this is in scope we cannot edit fmt_buffer
            let buf_str = fmt_buffer.as_str();
            // buf_str borrows fmt_buffer, so we cannot write to it here.
            assert_eq!(buf_str, "Hello world!");

            // buf_str cannot be used after this. The backing buffer stays in scope.
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
            // "0123456789" is 10 bytes — triggers 3 flushes (3+3+3), leaves "9" in buffer.
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
            assert!(flushed.is_empty());
        }

        #[test]
        fn flush_empty_buffer_is_noop() {
            let mut flushed: Vec<String> = Vec::new();
            let writer = FlushableFormatBuffer::<_, 8>::new(|buf| {
                flushed.push(buf.as_str().to_owned());
            });
            drop(writer);
            assert!(flushed.is_empty());
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
            // "abcd" (4 bytes) leaves 2 bytes of space — not enough for ❤️ (6 bytes).
            // Flushes "abcd", then chunks the hearts as in the previous test.
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
            assert!(flushed.is_empty());
        }
    }
}
