use core::{ffi::CStr, fmt, str::Utf8Error};

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
/// let s = buf.as_str().unwrap();
/// assert_eq!(s, "hello 42");
///
/// let c = buf.as_cstr();
/// assert_eq!(c.to_bytes(), b"hello 42");
/// ```
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
    /// Only the bytes successfully written are interpreted as UTF-8.
    ///
    /// # Errors
    /// Returns an error if the written bytes are not valid UTF-8.
    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        core::str::from_utf8(&self.buffer[..self.used])
    }

    /// Returns a C string view up to the first `NUL` byte.
    ///
    /// The buffer always contains a NUL terminator because `write_str`
    /// reserves the last byte.
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

        // Overflow: copy what fits into the remaining space and signal error.
        if s.len() > remaining {
            self.buffer[self.used..self.used + remaining]
                .copy_from_slice(&s.as_bytes()[..remaining]);
            self.used = capacity;
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
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let capacity = N - 1;
        let mut remaining = &s.as_bytes()[..];

        // Fill what fits, flush, continue with the rest.
        while remaining.len() > capacity - self.format_buffer.used {
            let space = capacity - self.format_buffer.used;
            self.format_buffer.buffer[self.format_buffer.used..self.format_buffer.used + space]
                .copy_from_slice(&remaining[..space]);
            self.format_buffer.used = capacity;

            (self.flush_fn)(&self.format_buffer);
            self.format_buffer.reset();

            remaining = &remaining[space..];
        }

        // Remaining bytes fit in the buffer.
        self.format_buffer.buffer
            [self.format_buffer.used..self.format_buffer.used + remaining.len()]
            .copy_from_slice(remaining);
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
        assert!(fmt_buffer.used == 0);
        assert!(fmt_buffer.buffer.len() == 512);
        for x in fmt_buffer.buffer {
            assert!(x == 0);
        }
    }

    #[test]
    fn change_len() {
        let fmt_buffer: WdkFormatBuffer<2> = WdkFormatBuffer::new();
        assert!(fmt_buffer.buffer.len() == 2);
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
        assert_eq!(fmt_buffer.as_str().unwrap(), "Hello world!");
    }

    #[test]
    fn ref_sanity_check() {
        let mut fmt_buffer: WdkFormatBuffer = WdkFormatBuffer::new();
        let world: &str = "world";
        assert!(write!(&mut fmt_buffer, "Hello {world}!").is_ok());

        // borrow fmt_buffer -- while this is in scope we cannot edit fmt_buffer
        let buf_str = fmt_buffer.as_str().unwrap();
        // write!(&mut fmt_buffer, "buf_str is still in scope so we cannot edit!)
        assert_eq!(buf_str, "Hello world!");

        // buf_str cannot be used after this. The backing buffer stays in scope.
        assert!(write!(&mut fmt_buffer, " Second sentence!").is_ok());
        assert_eq!(
            fmt_buffer.as_str().unwrap(),
            "Hello world! Second sentence!"
        );

        // as_cstr now borrows immutably
        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"Hello world! Second sentence!\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);

        // mutable borrow ends here so we can edit the backing buffer.
        assert!(write!(&mut fmt_buffer, " A third sentence!").is_ok());
        assert_eq!(
            fmt_buffer.as_str().unwrap(),
            "Hello world! Second sentence! A third sentence!"
        );
    }

    #[test]
    fn overflow_buffer() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "0123456789").is_err());

        // Usable capacity is N-1 = 7; last byte reserved for NUL
        let buf_str = fmt_buffer.as_str().unwrap();
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

        let buf_str = fmt_buffer.as_str().unwrap();
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

        let buf_str = fmt_buffer.as_str().unwrap();
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

        let buf_str = fmt_buffer.as_str().unwrap();
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

        let buf_str = fmt_buffer.as_str().unwrap();
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

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, "0123456");

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn empty_buffer_strs() {
        let fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();

        let buf_str = fmt_buffer.as_str().unwrap();
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

        assert_eq!(fmt_buffer.as_str().unwrap(), "");

        let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr();
        assert_eq!(buf_c_str, cmp_c_str);
    }
}

#[cfg(test)]
mod wdk_flushable_format_buffer_tests {
    use core::fmt::Write;

    use super::WdkFlushableFormatBuffer;

    #[test]
    fn write_fits_in_buffer() {
        let mut flushed: Vec<String> = Vec::new();
        let mut writer = WdkFlushableFormatBuffer::<_, 16>::new(|buf| {
            flushed.push(buf.as_str().unwrap().to_owned());
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
            flushed.push(buf.as_str().unwrap().to_owned());
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
            flushed.push(buf.as_str().unwrap().to_owned());
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
            flushed.push(buf.as_str().unwrap().to_owned());
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
            flushed.push(buf.as_str().unwrap().to_owned());
        });
        writer.flush();
        assert!(flushed.is_empty());
    }

    #[test]
    fn exact_capacity_fit() {
        let mut flushed: Vec<String> = Vec::new();
        // Capacity is N-1 = 7 usable bytes
        let mut writer = WdkFlushableFormatBuffer::<_, 8>::new(|buf| {
            flushed.push(buf.as_str().unwrap().to_owned());
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
            flushed.push(buf.as_str().unwrap().to_owned());
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
}
