use core::{
    ffi::{CStr, FromBytesUntilNulError},
    fmt,
    str::Utf8Error,
};

const DEFAULT_WDK_FORMAT_BUFFER_SIZE: usize = 512;

/// A fixed-size formatting buffer implementing [`fmt::Write`].
///
/// Zero-initialized, capacity `T` (default 512). Intended for constrained
/// driver environments where heap allocation is undesirable. When reading as a
/// C-style string has capacity `T-1`.
///
/// Append with `write!`/`format_args!`; read via [`WdkFormatBuffer::as_str`]
/// or [`WdkFormatBuffer::as_cstr`].
///
/// # Examples
/// ```
/// use core::fmt::Write;
///
/// use wdk::fmt::WdkFormatBuffer;
///
/// let mut buf = WdkFormatBuffer::<16>::new();
/// write!(&mut buf, "hello {}", 42).unwrap();
///
/// let s = buf.as_str().unwrap();
/// assert_eq!(s, "hello 42");
///
/// let c = buf.as_cstr().unwrap();
/// assert_eq!(c.to_bytes(), b"hello 42");
/// ```
pub struct WdkFormatBuffer<const T: usize = DEFAULT_WDK_FORMAT_BUFFER_SIZE> {
    buffer: [u8; T],
    used: usize,
}

impl<const T: usize> WdkFormatBuffer<T> {
    /// Creates a zeroed formatting buffer with capacity `T`.
    ///
    /// The buffer starts empty (`used == 0`) and is ready for `fmt::Write`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0; T],
            used: 0,
        }
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
    /// Ensures termination by writing a `NUL` if the buffer is completely
    /// filled.
    ///
    /// # Errors
    /// Returns an error only if no terminator is found, e.g. if `T == 0`.
    pub const fn as_cstr(&mut self) -> Result<&CStr, FromBytesUntilNulError> {
        if self.used == T && T != 0 {
            self.buffer[self.used - 1] = 0;
        }
        CStr::from_bytes_until_nul(&self.buffer)
        // PR comments -- I tried to make sure that my implementation never
        // passed a FromBytesUntilNulError, and the function just returned a
        // &CStr. But handling this error here caused a panic which
        // feels too strict given the driver environment since that would cause
        // a bugcheck for kernel drivers. Determined that the most
        // elegant way was for the user to always handle the errors but I'm open
        // to feedback here.
        //
        // // if I do implement this way how should I handle an error case here?
        // feels too strong to panic but this case cannot happen as the current
        // logic stands. if cstr.is_err() {
        //     unreachable!("Buffer should always contain a trailing null
        // byte"); }
        // cstr.unwrap_or_else(| _err | {
        //     // SAFETY: We pass a single null byte into
        // `from_bytes_with_nul_unchecked`. This means that it is null
        // terminated and has no interior null bytes.     unsafe{
        //         CStr::from_bytes_with_nul_unchecked(b"\0")
        //     }
        // })
    }
}

impl<const T: usize> Default for WdkFormatBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const T: usize> fmt::Write for WdkFormatBuffer<T> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() + self.used >= T {
            self.buffer[self.used..T].copy_from_slice(&s.as_bytes()[..T - self.used]);
            self.used = T;
            return Err(fmt::Error);
        }
        self.buffer[self.used..self.used + s.len()].copy_from_slice(s.as_bytes());
        self.used += s.len();
        Ok(())
    }
}

#[cfg(test)]
mod test {
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

        // as_cstr borrows mutably so we cannot edit here
        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"Hello world! Second sentence!\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
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

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "01234567";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn exact_buffer_size() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234567").is_err());

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "01234567";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn overflow_buffer_after_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        assert!(write!(&mut fmt_buffer, "56789").is_err());

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "01234567";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn overflow_buffer_then_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        assert!(write!(&mut fmt_buffer, "56789").is_err());
        assert!(write!(&mut fmt_buffer, "overflow!").is_err());
        assert!(write!(&mut fmt_buffer, "overflow!").is_err());

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "01234567";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn exact_buffer_size_multiple_writes() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "01234").is_ok());
        assert!(write!(&mut fmt_buffer, "567").is_err());

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "01234567";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr =
            core::ffi::CStr::from_bytes_until_nul(b"0123456\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn empty_buffer_strs() {
        let mut fmt_buffer: WdkFormatBuffer<8> = WdkFormatBuffer::new();

        let mut cmp_buffer: [u8; 8] = [0; 8];
        let cmp_str: &str = "";
        cmp_buffer[..cmp_str.len()].copy_from_slice(cmp_str.as_bytes());

        assert_eq!(fmt_buffer.buffer, cmp_buffer);

        let buf_str = fmt_buffer.as_str().unwrap();
        assert_eq!(buf_str, cmp_str);

        let cmp_c_str: &core::ffi::CStr = core::ffi::CStr::from_bytes_until_nul(b"\0").unwrap();
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
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
        let buf_c_str = fmt_buffer.as_cstr().unwrap();
        assert_eq!(buf_c_str, cmp_c_str);
    }

    #[test]
    fn zero_sized_buffer() {
        let mut fmt_buffer: WdkFormatBuffer<0> = WdkFormatBuffer::new();
        assert!(write!(&mut fmt_buffer, "uh oh!").is_err());
        assert_eq!(fmt_buffer.as_str().unwrap(), "");
        assert!(fmt_buffer.as_cstr().is_err());
    }
}
