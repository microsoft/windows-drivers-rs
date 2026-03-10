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
/// Append with `write!`/`format_args!`; read via [`WdkFormatBuffer::as_str`]
/// or [`WdkFormatBuffer::as_cstr`].
///
/// # Examples
/// ```
/// use core::fmt::Write;
///
/// use wdk::WdkFormatBuffer;
///
/// let mut buf = WdkFormatBuffer::<16>::new();
/// write!(&mut buf, "hello {}", 42).unwrap();
///
/// let s = buf.as_str();
/// assert_eq!(s, "hello 42");
///
/// let c = buf.as_cstr();
/// assert_eq!(c.to_bytes(), b"hello 42");
/// ```
#[derive(Debug)]
pub struct WdkFormatBuffer<const N: usize = DEFAULT_WDK_FORMAT_BUFFER_SIZE> {
    buffer: [u8; N],
    used: usize,
}

impl<const N: usize> WdkFormatBuffer<N> {
    /// Creates a zeroed formatting buffer with capacity `N`.
    ///
    /// The buffer starts empty (`used == 0`) and is ready for `fmt::Write`.
    ///
    /// `N` must be at least 2 (one byte of content plus the NUL terminator).
    /// Smaller values will not compile:
    /// ```compile_fail
    /// use wdk::WdkFormatBuffer;
    /// let _ = WdkFormatBuffer::<1>::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        const {
            assert!(
                N >= 2,
                "N must be at least 2 (one byte of content plus the NUL terminator)"
            )
        }
        Self {
            buffer: [0; N],
            used: 0,
        }
    }

    /// Resets the buffer to its initial empty state.
    pub fn reset(&mut self) {
        self.buffer = [0; N];
        self.used = 0;
    }

    /// Returns a UTF-8 view over the written bytes.
    ///
    /// Only the bytes successfully written are included in the returned
    /// slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `fmt::Write::write_str` only accepts `&str` (valid UTF-8),
        // and `buffer`/`used` are private — no code path writes invalid UTF-8
        // into the buffer.
        unsafe { core::str::from_utf8_unchecked(&self.buffer[..self.used]) }
    }

    /// Returns a C string view up to the first `NUL` byte.
    ///
    /// The buffer always contains a NUL terminator because `write_str`
    /// reserves the last byte.
    #[must_use]
    pub const fn as_cstr(&self) -> &CStr {
        match CStr::from_bytes_until_nul(&self.buffer) {
            Ok(cstr) => cstr,
            // `unreachable!()` with a message uses `format_args!`, which is
            // not const-compatible. Use `panic!` with a string literal instead.
            Err(_) => {
                panic!("internal error: entered unreachable code: buffer must contain a NUL byte")
            }
        }
    }
}

impl<const N: usize> Default for WdkFormatBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> fmt::Write for WdkFormatBuffer<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // The last byte (buffer[N-1]) is reserved for the NUL terminator
        // so that the buffer always contains a valid `CStr`.
        let capacity = N - 1;
        let remaining = capacity - self.used;

        // Overflow: copy what fits at a char boundary and signal error.
        if s.len() > remaining {
            let fit = s.floor_char_boundary(remaining);
            self.buffer[self.used..self.used + fit].copy_from_slice(s[..fit].as_bytes());
            self.used += fit;
            return Err(fmt::Error);
        }

        // Normal write: append the full string.
        self.buffer[self.used..self.used + s.len()].copy_from_slice(s.as_bytes());
        self.used += s.len();
        Ok(())
    }
}

/// A [`WdkFormatBuffer`] wrapper that auto-flushes on overflow.
///
/// When a `write_str` call would exceed the buffer capacity, the current
/// contents are flushed via the provided closure, the buffer is reset, and
/// writing continues with the remainder. This allows arbitrarily long
/// formatted output to be processed in fixed-size chunks.
/// `N` must be at least 2 (enforced by [`WdkFormatBuffer::new`]).
///
/// After all writes are complete, the caller must call [`flush`](Self::flush)
/// to drain any remaining buffered content.
pub struct WdkFlushableFormatBuffer<
    F: FnMut(&WdkFormatBuffer<N>),
    const N: usize = DEFAULT_WDK_FORMAT_BUFFER_SIZE,
> {
    format_buffer: WdkFormatBuffer<N>,
    flush_fn: F,
}

impl<F: FnMut(&WdkFormatBuffer<N>), const N: usize> WdkFlushableFormatBuffer<F, N> {
    /// Creates a new flushable writer with the given flush closure.
    #[must_use]
    pub fn new(flush_fn: F) -> Self {
        Self {
            format_buffer: WdkFormatBuffer::new(),
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
        self.format_buffer.reset();
    }
}

impl<F: FnMut(&WdkFormatBuffer<N>), const N: usize> fmt::Write for WdkFlushableFormatBuffer<F, N> {
    /// Appends `s` to the buffer, flushing via the closure whenever the
    /// buffer fills. Returns [`fmt::Error`] only when a single UTF-8 code
    /// point is larger than the usable buffer capacity (`N - 1` bytes).
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let capacity = N - 1;
        let mut remaining = s;

        // Fill what fits at a char boundary, flush, continue with the rest.
        while remaining.len() > capacity - self.format_buffer.used {
            let space = capacity - self.format_buffer.used;
            let split = remaining.floor_char_boundary(space);

            if split == 0 {
                if self.format_buffer.used == 0 {
                    // A single character doesn't fit in the entire buffer.
                    return Err(fmt::Error);
                }
                // Buffer has content but no room for the next char — flush and retry.
                (self.flush_fn)(&self.format_buffer);
                self.format_buffer.reset();
                continue;
            }

            self.format_buffer.buffer[self.format_buffer.used..self.format_buffer.used + split]
                .copy_from_slice(remaining[..split].as_bytes());
            self.format_buffer.used += split;

            (self.flush_fn)(&self.format_buffer);
            self.format_buffer.reset();

            remaining = &remaining[split..];
        }

        // Remaining bytes fit in the buffer.
        self.format_buffer.buffer
            [self.format_buffer.used..self.format_buffer.used + remaining.len()]
            .copy_from_slice(remaining.as_bytes());
        self.format_buffer.used += remaining.len();
        Ok(())
    }
}

#[cfg(test)]
mod wdk_format_buffer_tests {
    use core::fmt::Write;

    use super::WdkFormatBuffer;
    #[test]
    fn initialize() {
        let fmt_buffer: WdkFormatBuffer = WdkFormatBuffer::new();
        assert_eq!(fmt_buffer.used, 0);
        assert_eq!(fmt_buffer.buffer.len(), 512);
        assert!(fmt_buffer.buffer.iter().all(|&b| b == 0));
    }

    #[test]
    fn change_len() {
        let fmt_buffer: WdkFormatBuffer<2> = WdkFormatBuffer::new();
        assert_eq!(fmt_buffer.buffer.len(), 2);
    }

    #[test]
    fn minimum_buffer_write() {
        let mut fmt_buffer = WdkFormatBuffer::<2>::new();
        assert!(write!(&mut fmt_buffer, "a").is_ok());
        assert_eq!(fmt_buffer.as_str(), "a");
        assert!(write!(&mut fmt_buffer, "b").is_err());
    }

    #[test]
    fn write() {
        let mut fmt_buffer: WdkFormatBuffer = WdkFormatBuffer::new();
        let world: &str = "world";
        assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());

        let mut cmp_buffer: [u8; 512] = [0; 512];
        let cmp_str: &str = "Hello world!";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);
    }

    #[test]
    fn as_str() {
        let mut fmt_buffer: WdkFormatBuffer = WdkFormatBuffer::new();
        let world: &str = "world";
        assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());
        assert_eq!(fmt_buffer.as_str(), "Hello world!");
    }

    #[test]
    fn ref_sanity_check() {
        let mut fmt_buffer: WdkFormatBuffer = WdkFormatBuffer::new();
        let world: &str = "world";
        assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());

        // borrow fmt_buffer -- while this is in scope we cannot edit fmt_buffer
        let buf_str = fmt_buffer.as_str();
        // buf_str borrows fmt_buffer, so we cannot write to it here.
        assert_eq!(buf_str, "Hello world!");

        // buf_str cannot be used after this. The backing buffer stays in scope.
        assert!(write!(&mut fmt_buffer, " Second sentence!").is_ok());
        assert_eq!(fmt_buffer.as_str(), "Hello world! Second sentence!");

        // as_cstr now borrows immutably
        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"Hello world! Second sentence!\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
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
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "0123456789").is_err());

        // Usable capacity is N-1 = 7; last byte reserved for NUL
        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn exact_buffer_size() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        // Writing exactly N bytes overflows (capacity is N-1)
        assert!(write!(&mut fmt_buffer, "01234567").is_err());

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn exact_capacity_fit() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        // Writing exactly N-1 bytes succeeds
        assert!(write!(&mut fmt_buffer, "0123456").is_ok());

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn overflow_buffer_after_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        assert!(write!(&mut fmt_buffer, "56789").is_err());

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn overflow_buffer_then_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        assert!(write!(&mut fmt_buffer, "56789").is_err());
        assert!(write!(&mut fmt_buffer, "overflow!").is_err());
        assert!(write!(&mut fmt_buffer, "overflow!").is_err());

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn exact_buffer_size_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        // "56" fits in remaining capacity (2 bytes), but "567" overflows
        assert!(write!(&mut fmt_buffer, "567").is_err());

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn empty_buffer_strs() {
        let fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();

        let buf_str = fmt_buffer.as_str();
        assert_eq!(buf_str, "");

        let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn write_empty_strings() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "").is_ok());
        assert!(write!(&mut fmt_buffer, "").is_ok());

        assert_eq!(fmt_buffer.used, 0);
        assert!(fmt_buffer.buffer.iter().all(|&b| b == 0));

        assert_eq!(fmt_buffer.as_str(), "");

        let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn overflow_truncates_at_char_boundary() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        // Capacity is 7. "❤️🧡💛💚💙💜" is 26 bytes.
        // ❤️ is 6 bytes, 🧡 starts at byte 6 but needs 4 bytes (total 10).
        // floor_char_boundary(7) = 6, so only ❤️ fits.
        assert!(write!(&mut fmt_buffer, "❤️🧡💛💚💙💜").is_err());
        assert_eq!(fmt_buffer.as_str(), "❤️");
    }

    #[test]
    fn interior_nul_truncates_cstr() {
        let mut fmt_buffer = WdkFormatBuffer::<16>::new();
        assert!(write!(&mut fmt_buffer, "hello\0world").is_ok());
        assert_eq!(fmt_buffer.as_str(), "hello\0world");
        assert_eq!(fmt_buffer.as_cstr(), c"hello");
    }

    #[test]
    fn reset_clears_buffer() {
        let mut fmt_buffer = WdkFormatBuffer::<8>::new();
        assert!(write!(&mut fmt_buffer, "hello").is_ok());
        fmt_buffer.reset();
        assert_eq!(fmt_buffer.used, 0);
        assert_eq!(fmt_buffer.as_str(), "");
        assert_eq!(fmt_buffer.as_cstr(), c"");
    }
}

#[cfg(test)]
mod wdk_flushable_format_buffer_tests {
    extern crate alloc;

    use alloc::{borrow::ToOwned, string::String, vec, vec::Vec};
    use core::fmt::Write;

    use super::WdkFlushableFormatBuffer;

    #[test]
    fn write_fits_in_buffer() {
        let mut flushed: Vec<String> = Vec::new();
        let mut writer = WdkFlushableFormatBuffer::<_, 16>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "hello").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["hello"]);
    }

    #[test]
    fn overflow_triggers_flush() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 7 usable bytes
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        // "0123456789" is 10 bytes — exceeds 7-byte capacity.
        // First 7 bytes fill the buffer, triggering a flush.
        // Remaining "789" goes into the reset buffer.
        assert!(write!(&mut writer, "0123456789").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["0123456", "789"]);
    }

    #[test]
    fn multi_flush() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 3 usable bytes
        let mut writer = WdkFlushableFormatBuffer::<_, 4>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        // "0123456789" is 10 bytes — triggers 3 flushes (3+3+3), leaves "9" in buffer.
        assert!(write!(&mut writer, "0123456789").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["012", "345", "678", "9"]);
    }

    #[test]
    fn empty_write_does_not_flush() {
        let mut flushed: Vec<String> = Vec::new();
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "").is_ok());
        assert!(write!(&mut writer, "").is_ok());
        writer.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn flush_empty_buffer_is_noop() {
        let mut flushed: Vec<String> = Vec::new();
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        writer.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn exact_capacity_fit() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 7 usable bytes
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        // Exactly 7 bytes — fits perfectly, no flush triggered.
        assert!(write!(&mut writer, "0123456").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["0123456"]);
    }

    #[test]
    fn multiple_writes_with_intermittent_overflow() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 7 usable bytes
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "abc").is_ok());
        assert!(write!(&mut writer, "def").is_ok());
        assert!(write!(&mut writer, "ghi").is_ok());
        assert!(write!(&mut writer, "jkl").is_ok());
        assert!(write!(&mut writer, "mno").is_ok());
        writer.flush();
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
        let mut writer = WdkFlushableFormatBuffer::<_, 7>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["❤️", "🧡", "💛", "💚", "💙", "💜"]);
    }

    #[test]
    fn multi_byte_char_triggers_early_flush() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 6 usable bytes.
        // "abcd" (4 bytes) leaves 2 bytes of space — not enough for ❤️ (6 bytes).
        // Flushes "abcd", then chunks the hearts as in the previous test.
        let mut writer = WdkFlushableFormatBuffer::<_, 7>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "abcd").is_ok());
        assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_ok());
        writer.flush();
        assert_eq!(flushed, vec!["abcd", "❤️", "🧡", "💛", "💚", "💙", "💜"]);
    }

    #[test]
    fn multi_byte_char_too_big_for_buffer() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 2 usable bytes.
        // ❤️🧡💛💚💙💜 starts with ❤ (3 bytes) — can never fit.
        let mut writer = WdkFlushableFormatBuffer::<_, 3>::new(|buf| {
            flushed.push(buf.as_str().to_owned());
        });
        assert!(write!(&mut writer, "❤️🧡💛💚💙💜").is_err());
        assert!(flushed.is_empty());
    }
}
