// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! `ExAllocatePool2`, resolved at runtime rather than imported.
//!
//! # The problem this solves
//!
//! `ExAllocatePool2` is `NTKERNELAPI` under
//! `#if (NTDDI_VERSION >= NTDDI_WIN10_VB)` (`10.0.26100.0/km/wdm.h:9932`,
//! `10.0.22621.0/km/wdm.h:9530`) — Windows 10 2004 / May 2020. Every WDK
//! citation in this file names the header version it was read from, because
//! the line numbers move between WDKs.
//!
//! Every kernel-mode consumer of this crate that names `ExAllocatePool2` as a
//! plain extern turns it into an *import*, which the kernel's image loader
//! must resolve **before `DriverEntry` runs**. On any Windows before build
//! 19041 the loader refuses to bind the image and the driver fails to load
//! with `STATUS_ENTRYPOINT_NOT_FOUND` — silently, before anything in the
//! driver runs.
//!
//! A driver that wants to load on Windows 10 1703 (RS2) and later cannot
//! import `ExAllocatePool2` directly, because the import would raise its load
//! floor to 2004. Resolving the routine through `MmGetSystemRoutineAddress`
//! and falling back to `ExAllocatePoolWithTag` on machines that lack it keeps
//! the image loadable on the older builds. That is the pattern Microsoft
//! documents for calling a DDI which may be absent on an older kernel while
//! keeping the image loadable on it.
//!
//! # How it behaves
//!
//! - **Resolved** — the kernel is Windows 10 2004 or later. The exported
//!   routine is called with the caller's flags, size and tag unmodified, so
//!   behaviour is byte for byte what a static import would have produced.
//! - **Unresolved** — the kernel is older. The flags are mapped to a
//!   `_POOL_TYPE` (`POOL_FLAG_PAGED` → `PagedPool`, `POOL_FLAG_NON_PAGED` →
//!   `NonPagedPoolNx`, `POOL_FLAG_NON_PAGED_EXECUTE` → `NonPagedPool`) and the
//!   block is allocated through `ExAllocatePoolWithTag`. The returned memory is
//!   then zeroed with `RtlZeroMemory` unless `POOL_FLAG_UNINITIALIZED` is set —
//!   matching `ExAllocatePool2`'s own zero-by-default contract
//!   (`10.0.26100.0/km/wdm.h:9906-9925`).
//!
//! `ExAllocatePoolWithTag` itself is `_IRQL_requires_max_(DISPATCH_LEVEL)`
//! for non-paged pool and `PASSIVE_LEVEL` for paged pool — exactly the IRQL
//! constraints `ExAllocatePool2` documents, so the fallback introduces no
//! IRQL regression.
//!
//! # `bindgen` blocklisting and the fallback extern
//!
//! `wdk-build`'s bindgen builder already blocklists `ExAllocatePoolWithTag`
//! as deprecated (`crates/wdk-build/src/bindgen.rs:103`), so no generated
//! binding names it and driver code cannot reach it directly. This module
//! declares it as a private extern here, alongside `RtlZeroMemory`, so the
//! fallback is available *only* through the shim. `wdk-sys/build.rs`
//! blocklists the generated `ExAllocatePool2` and `ntddk.rs` re-exports this
//! implementation under the same name — every existing call site continues
//! to compile unchanged.
//!
//! # IRQL: the resolution is gated at PASSIVE_LEVEL, the allocation is not
//!
//! `MmGetSystemRoutineAddress` is `_IRQL_requires_max_(PASSIVE_LEVEL)`
//! (`10.0.26100.0/km/wdm.h:13974`). `ExAllocatePool2` is
//! `_IRQL_requires_max_(DISPATCH_LEVEL)`, so a caller may legitimately be
//! above `PASSIVE_LEVEL`. Probing there would be the IRQL violation; taking
//! the fallback for that one call is legitimate, and a later call at
//! `PASSIVE_LEVEL` populates the cache for good. This is the same pattern
//! `lookaside_downlevel.rs` uses for `ExAllocateFromLookasideListEx` /
//! `ExFreeToLookasideListEx`.
//!
//! # What could go wrong, and does not
//!
//! - **Zeroing cost when unresolved**: the fallback zeroes with `RtlZeroMemory`
//!   unless the caller passes `POOL_FLAG_UNINITIALIZED`. On the resolved path
//!   the kernel zeroes for us, so the fallback merely restores parity with the
//!   resolved path — not an extra cost that was avoidable.
//! - **`POOL_FLAG_NON_PAGED_EXECUTE`**: mapped to the deprecated `NonPagedPool`
//!   (executable) rather than `NonPagedPoolNx`. That is deliberate: the caller
//!   asked for executable memory, and silently downgrading to NX would break
//!   code that depends on the executable bit. `wdk-alloc`'s `POOL_FLAGS_USED`
//!   compile-time assertion prevents this crate's global allocator from taking
//!   that path.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    constants::{
        POOL_FLAG_CACHE_ALIGNED,
        POOL_FLAG_NON_PAGED_EXECUTE,
        POOL_FLAG_PAGED,
        POOL_FLAG_RAISE_ON_FAILURE,
        POOL_FLAG_SESSION,
        POOL_FLAG_UNINITIALIZED,
        POOL_FLAG_USE_QUOTA,
    },
    ntddk::{KeGetCurrentIrql, MmGetSystemRoutineAddress},
    types::{POOL_FLAGS, POOL_TYPE, PVOID, SIZE_T, ULONG, UNICODE_STRING, USHORT, WCHAR},
};

/// Sentinel for "not looked up yet" in [`EX_ALLOCATE_POOL2`].
///
/// Zero cannot be a routine address, so it is free to mean "unknown".
const UNRESOLVED: usize = 0;

/// Sentinel for "looked up, and this kernel does not export it".
///
/// One is not a valid code address on any supported platform — kernel-mode
/// code lives in the upper half of the address space — so it cannot collide
/// with a real resolution. Distinguishing this from [`UNRESOLVED`] is what
/// stops a down-level kernel being re-probed on every allocation.
const ABSENT: usize = 1;

/// `ExAllocatePool2` as resolved through `MmGetSystemRoutineAddress`, or
/// [`UNRESOLVED`] / [`ABSENT`].
static EX_ALLOCATE_POOL2: AtomicUsize = AtomicUsize::new(UNRESOLVED);

/// `ExAllocatePool2` signature, as `10.0.26100.0/km/wdm.h:9936-9944` declares
/// it.
type ExAllocatePool2Fn = unsafe extern "C" fn(POOL_FLAGS, SIZE_T, ULONG) -> PVOID;

/// `_POOL_TYPE` values as `10.0.26100.0/km/wdm.h:9721-9752` defines them.
///
/// Declared as bare `POOL_TYPE` (i32) constants rather than pulled from
/// `types::_POOL_TYPE`, because the generated module's constants are
/// `_POOL_TYPE::Type` values and the free-function fallback needs a POOL_TYPE
/// argument.
const NON_PAGED_POOL: POOL_TYPE = 0;
const PAGED_POOL: POOL_TYPE = 1;
const NON_PAGED_POOL_NX: POOL_TYPE = 512;
/// `POOL_RAISE_IF_ALLOCATION_FAILURE` (`km/wdm.h`), the legacy `_POOL_TYPE`
/// modifier that reproduces `ExAllocatePool2`'s `POOL_FLAG_RAISE_ON_FAILURE`
/// through `ExAllocatePoolWithTag`.
const POOL_RAISE_IF_ALLOCATION_FAILURE: POOL_TYPE = 16;

// The private fallback API. Declared here rather than exposed as a binding
// so that no driver code can name it directly; every allocation flows
// through `ExAllocatePool2`.
//
// `ExAllocatePoolWithTag` is `_IRQL_requires_max_(DISPATCH_LEVEL)` for
// non-paged pool types and `_IRQL_requires_max_(APC_LEVEL)` for paged pool.
// The shim inherits these from `ExAllocatePool2`'s own constraints, so no
// caller change is required.
//
// `RtlZeroMemory` is an ancient export (NT 3.1), available on every supported
// Windows, and used only to restore `ExAllocatePool2`'s zero-by-default
// contract on the fallback path.
unsafe extern "C" {
    fn ExAllocatePoolWithTag(PoolType: POOL_TYPE, NumberOfBytes: SIZE_T, Tag: ULONG) -> PVOID;

    fn RtlZeroMemory(Destination: PVOID, Length: SIZE_T);
}

/// `"ExAllocatePool2"` as UTF-16, for [`MmGetSystemRoutineAddress`].
///
/// A `static` rather than a local because `MmGetSystemRoutineAddress` takes a
/// `UNICODE_STRING` that points *at* the buffer and reads it during the call;
/// a local would work for that, but a `static` is also what makes the
/// descriptor cheap to build and impossible to get wrong.
static NAME: [WCHAR; 15] = wide(b"ExAllocatePool2");

/// Widens an ASCII routine name at compile time.
///
/// A `const fn` so the name above is data in the image rather than work at
/// run time, and so a non-ASCII byte is a build failure rather than a lookup
/// that silently misses.
const fn wide<const N: usize>(name: &[u8]) -> [WCHAR; N] {
    assert!(name.len() == N, "buffer length should match the name");

    let mut wide = [0; N];
    let mut index = 0;
    while index < N {
        assert!(name[index] < 0x80, "a routine name should be ASCII");
        wide[index] = name[index] as WCHAR;
        index += 1;
    }

    wide
}

/// Builds the `UNICODE_STRING` `MmGetSystemRoutineAddress` wants over
/// [`NAME`].
///
/// The `Length` is a byte count, which is what a `UNICODE_STRING` counts, and
/// the buffer is deliberately not NUL terminated: a `UNICODE_STRING` is
/// explicitly counted.
fn name_of(name: &'static [WCHAR]) -> UNICODE_STRING {
    // [`NAME`] is 15 elements, so its byte count is 30 — far short of
    // `USHORT::MAX`. The narrowing cannot truncate.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the sole caller passes a `static` of 15 elements, so the byte count is 30 and \
                  cannot exceed `USHORT::MAX`"
    )]
    let length_in_bytes = (name.len() * size_of::<WCHAR>()) as USHORT;

    UNICODE_STRING {
        Length: length_in_bytes,
        MaximumLength: length_in_bytes,
        Buffer: name.as_ptr().cast_mut(),
    }
}

/// Resolves `ExAllocatePool2` through `MmGetSystemRoutineAddress`, caching
/// the answer in [`EX_ALLOCATE_POOL2`], and returns the address or
/// [`ABSENT`].
///
/// # Why the IRQL is tested
///
/// `MmGetSystemRoutineAddress` is `_IRQL_requires_max_(PASSIVE_LEVEL)`
/// (`10.0.26100.0/km/wdm.h:13974`) but `ExAllocatePool2` is
/// `_IRQL_requires_max_(DISPATCH_LEVEL)`, so a caller may legitimately be
/// above `PASSIVE_LEVEL`. Probing there would be the IRQL violation; taking
/// the fallback for that one call is legitimate, and a later call at
/// `PASSIVE_LEVEL` populates the cache for good.
///
/// This is what makes the shim self-contained: there is no initialisation a
/// consumer has to remember to call from `DriverEntry`, and therefore no way
/// to get an unresolved-pointer call by forgetting it.
fn resolve() -> usize {
    let cached = EX_ALLOCATE_POOL2.load(Ordering::Relaxed);
    if cached != UNRESOLVED {
        return cached;
    }

    // SAFETY: `KeGetCurrentIrql` reads the current processor's IRQL and
    // takes no arguments
    let irql = unsafe { KeGetCurrentIrql() };

    // `PASSIVE_LEVEL` is 0 (`km/wdm.h`), and `crate::constants::PASSIVE_LEVEL`
    // is a `u32` while `KIRQL` is a `UCHAR`, so the comparison is written
    // against 0 rather than through a cast that could hide a widening.
    //
    // Do NOT cache the answer at this branch — the routine may exist and
    // simply be unreachable from this IRQL. A later call at `PASSIVE_LEVEL`
    // must be able to resolve it for real.
    if irql != 0 {
        return ABSENT;
    }

    let mut routine_name = name_of(&NAME);

    // SAFETY: `routine_name` is a valid `UNICODE_STRING` over a `static`
    // buffer that outlives the call, and the IRQL test above establishes the
    // `PASSIVE_LEVEL` precondition
    let address = unsafe { MmGetSystemRoutineAddress(&raw mut routine_name) };

    let resolved = if address.is_null() {
        ABSENT
    } else {
        address as usize
    };

    // `Relaxed` is sufficient: the value published is a plain integer with
    // no memory it guards, two threads resolving concurrently compute the
    // same answer, and a store lost to a race merely costs one more lookup.
    EX_ALLOCATE_POOL2.store(resolved, Ordering::Relaxed);

    resolved
}

/// Maps `POOL_FLAGS` to a `_POOL_TYPE` for the fallback path.
///
/// Extracted so the mapping is one place to read and one place to change.
/// The precedence — paged first, then executable, then NX non-paged — is the
/// one the WDK header follows in its own `#if` cascade: `POOL_FLAG_PAGED` is
/// documented as mutually exclusive with the non-paged flags, and
/// `POOL_FLAG_NON_PAGED_EXECUTE` is what an intentional caller specifies
/// when they want the executable bit.
///
/// The only optional refinement is special pool, which changes no observable
/// contract for the down-level fallback. `POOL_FLAG_RAISE_ON_FAILURE` is
/// preserved: it is reproduced by OR-ing the legacy
/// `POOL_RAISE_IF_ALLOCATION_FAILURE` modifier into the returned type. The
/// pool-type bits — which decide *where* the memory comes from — cannot be
/// lost, so they are what this function preserves. Requests that set the
/// cache-aligned, quota-charged, or session-pool flags are rejected by the
/// caller before reaching this function.
const fn pool_type_from_flags(flags: POOL_FLAGS) -> POOL_TYPE {
    let pool_type = if flags & POOL_FLAG_PAGED != 0 {
        PAGED_POOL
    } else if flags & POOL_FLAG_NON_PAGED_EXECUTE != 0 {
        // Executable non-paged pool. Down-level `_POOL_TYPE` uses the plain
        // `NonPagedPool` variant for this, which was executable by default
        // before `NonPagedPoolNx` was added.
        NON_PAGED_POOL
    } else {
        // `POOL_FLAG_NON_PAGED` or unspecified. NX non-paged is the modern
        // default and what `wdk-alloc` uses. Written as the fall-through
        // rather than a third `else if` so that a caller who passes no
        // pool-type bit still gets a defined answer instead of a "match
        // fell off the end" left to a future maintainer.
        NON_PAGED_POOL_NX
    };

    if flags & POOL_FLAG_RAISE_ON_FAILURE != 0 {
        pool_type | POOL_RAISE_IF_ALLOCATION_FAILURE
    } else {
        pool_type
    }
}

/// Item-level `const` block that fails the build if the mapping ever
/// disagrees with what its documentation and callers depend on. `cargo
/// check` evaluates item-level `const _` blocks, so this is a build error
/// rather than a runtime assertion — the shim reaches no `assert!` in the
/// binary.
const _: () = {
    // The three pool-type flag paths, evaluated against known constants.
    assert!(pool_type_from_flags(POOL_FLAG_PAGED) == PAGED_POOL);
    assert!(pool_type_from_flags(POOL_FLAG_NON_PAGED_EXECUTE) == NON_PAGED_POOL);
    // No pool-type bits — the fall-through path.
    assert!(pool_type_from_flags(0) == NON_PAGED_POOL_NX);
    // Paged wins over executable, matching the WDK's own mutual-exclusion
    // documentation.
    assert!(pool_type_from_flags(POOL_FLAG_PAGED | POOL_FLAG_NON_PAGED_EXECUTE) == PAGED_POOL);
    // Raise-on-failure ORs the legacy modifier into the pool type.
    assert!(
        pool_type_from_flags(POOL_FLAG_PAGED | POOL_FLAG_RAISE_ON_FAILURE)
            == PAGED_POOL | POOL_RAISE_IF_ALLOCATION_FAILURE
    );
};

/// Allocates a block of kernel pool memory.
///
/// This is the [`wdk_sys::ntddk`](crate::ntddk) shim of `ExAllocatePool2`.
/// On Windows 10 2004 and later the routine is resolved through
/// `MmGetSystemRoutineAddress` and called with the arguments unchanged; on
/// older kernels the flags are translated to a `_POOL_TYPE` and the block is
/// allocated through `ExAllocatePoolWithTag`, then zeroed unless the caller
/// set `POOL_FLAG_UNINITIALIZED`.
///
/// Resolving through `MmGetSystemRoutineAddress` rather than importing keeps
/// a driver built against this crate loadable on Windows 10 RS2 (1703) and
/// later, instead of raising its load floor to 2004.
///
/// # Return value
///
/// Non-null on success. Null on allocation failure or when the fallback
/// receives a flag combination it cannot honour.
///
/// # Safety
///
/// As `ExAllocatePool2`: `NumberOfBytes` must be non-zero, the caller must
/// be at IRQL <= DISPATCH_LEVEL for non-paged pool types and IRQL <=
/// APC_LEVEL for paged pool types, and the returned block must be released
/// with `ExFreePool` / `ExFreePoolWithTag`.
#[expect(
    non_snake_case,
    reason = "bindings in this crate retain their original C names, and this one replaces a \
              bindgen-generated extern of the same name"
)]
#[must_use]
pub unsafe extern "C" fn ExAllocatePool2(
    Flags: POOL_FLAGS,
    NumberOfBytes: SIZE_T,
    Tag: ULONG,
) -> PVOID {
    let resolved = resolve();

    if resolved != ABSENT {
        // SAFETY: `resolved` is neither `UNRESOLVED` nor `ABSENT`, so it is
        // the address `MmGetSystemRoutineAddress` returned for
        // `ExAllocatePool2`, whose declared signature is
        // `ExAllocatePool2Fn`
        let routine = unsafe { core::mem::transmute::<usize, ExAllocatePool2Fn>(resolved) };

        // SAFETY: the caller's contract is this routine's contract,
        // unchanged
        return unsafe { routine(Flags, NumberOfBytes, Tag) };
    }

    // Fallback: down-level pool type + `ExAllocatePoolWithTag` + optional
    // zero.
    //
    // `ExAllocatePoolWithTag` cannot honour the cache-aligned, quota-charged,
    // or session-pool requests, so those are rejected rather than silently
    // returning memory that does not match the caller's contract.
    // `pool_type_from_flags` already folds `POOL_FLAG_RAISE_ON_FAILURE` into
    // the pool type via the legacy `POOL_RAISE_IF_ALLOCATION_FAILURE` modifier,
    // so the raise-on-failure contract survives the fallback; only special pool
    // (unobservable here) is dropped.
    const UNSUPPORTED_REQUIRED_FLAGS: POOL_FLAGS =
        POOL_FLAG_CACHE_ALIGNED | POOL_FLAG_USE_QUOTA | POOL_FLAG_SESSION;
    if Flags & UNSUPPORTED_REQUIRED_FLAGS != 0 {
        return core::ptr::null_mut();
    }

    let pool_type = pool_type_from_flags(Flags);

    // SAFETY: `ExAllocatePoolWithTag`'s IRQL contract is a superset of
    // `ExAllocatePool2`'s (paged pool: APC_LEVEL, non-paged: DISPATCH_LEVEL),
    // so the caller's IRQL discipline is preserved. `NumberOfBytes` is the
    // caller's, `Tag` is a 4-byte value, and the returned pointer is either
    // null or the address of a pool block of the requested size
    let ptr = unsafe { ExAllocatePoolWithTag(pool_type, NumberOfBytes, Tag) };

    // `ExAllocatePool2` zeroes returned memory unless `POOL_FLAG_UNINITIALIZED`
    // is set (`10.0.26100.0/km/wdm.h:9906-9925`). `ExAllocatePoolWithTag`
    // does not, so the fallback restores parity.
    if !ptr.is_null() && (Flags & POOL_FLAG_UNINITIALIZED) == 0 {
        // SAFETY: `ptr` is a non-null pool block of `NumberOfBytes` bytes,
        // just returned by `ExAllocatePoolWithTag`. `RtlZeroMemory` writes
        // that many bytes starting there.
        unsafe { RtlZeroMemory(ptr, NumberOfBytes) };
    }

    ptr
}
