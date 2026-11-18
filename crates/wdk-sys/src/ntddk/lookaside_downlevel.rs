// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

//! `ExAllocateFromLookasideListEx` and `ExFreeToLookasideListEx`, which cannot
//! be bound as plain externs without raising every consumer's minimum OS to
//! Windows 11 22H2.
//!
//! # The problem these solve
//!
//! `10.0.26100.0/km/wdm.h:26324` is `#if (NTDDI_VERSION >= NTDDI_WIN10_NI)`.
//! Above it the two routines are `NTKERNELAPI` declarations
//! (`10.0.26100.0/km/wdm.h:26331` and `:26338`); below it, in the `#else` at
//! `:26343-26433`, the WDK supplies `FORCEINLINE` bodies instead.
//! `NTDDI_WIN10_NI` is Windows 11 22H2 / Windows Server 2025.
//!
//! Every WDK citation in this file names the header version it was read from,
//! because the line numbers move by roughly a thousand lines between WDKs: the
//! same gate is at `10.0.22621.0/km/wdm.h:25341`. An unqualified `km/wdm.h:NNN`
//! resolves to unrelated code against any other kit, which for this file is a
//! hazard rather than an inconvenience — the justification below is the only
//! thing standing between a reader and "simplifying" these two back into plain
//! externs, which would silently restore the load floor this module exists to
//! remove.
//!
//! MSVC honours the `FORCEINLINE` and emits no import, which is why a C driver
//! built against a current WDK still loads on Windows 10. bindgen cannot inline
//! a `FORCEINLINE`, so it emits ordinary externs — and an extern is an import
//! the kernel's image loader must resolve **before `DriverEntry` is entered**.
//! An image importing these two therefore fails to load on every build of
//! Windows 10 and on Server 2016, 2019 and 2022, with no trace and nothing in
//! any log attributing it.
//!
//! The asymmetry is an artefact of binding generation rather than a design
//! choice: a C driver built against the same WDK imports neither routine.
//!
//! `build.rs` blocklists the two functions from the `ntddk` bindings and
//! `ntddk.rs` re-exports these in their place, so consumers need no change.
//!
//! # How they behave
//!
//! Each resolves its own routine once, through `MmGetSystemRoutineAddress`, and
//! caches the result. That is the pattern Microsoft documents for calling a DDI
//! that may be absent on an older kernel while keeping the image loadable on
//! it.
//!
//! - **Resolved** — the kernel is Windows 11 22H2 or later, which is every
//!   kernel a consumer of this crate could previously load on at all. The
//!   exported routine is called, so behaviour is byte for byte what it was.
//! - **Unresolved** — the kernel is older. This module reproduces the WDK's own
//!   `FORCEINLINE` body at `10.0.26100.0/km/wdm.h:26343-26433`, including the
//!   `SLIST` cache in `Lookaside->L.ListHead`. That is the shape a C driver
//!   built against a current WDK for a downlevel kernel runs anyway — MSVC
//!   inlines the same body verbatim — and it is what makes a lookaside list
//!   actually cache blocks rather than degenerating into a per-op pool round
//!   trip.
//!
//! # The `SLIST` cache, and why it is now here
//!
//! The `FORCEINLINE` bodies keep an `SLIST` cache in `Lookaside->L.ListHead`.
//! An earlier revision of this file deliberately did not reproduce it,
//! degrading the miss arm to `L.AllocateEx`/`L.FreeEx` on every operation.
//! That is measurable pool pressure on hot paths: a consumer that allocates
//! one lookaside block per monitored FS operation pays a pool round trip on
//! every one of them.
//!
//! Two reasons the earlier revision gave for deferring the cache do not
//! survive checking, and are corrected here rather than deleted so that the
//! next reader does not re-derive them:
//!
//! - It is **not** true that restoring the cache requires reading a bitfield
//!   through a union. `#define ExQueryDepthSList(_listhead_)
//!   (_listhead_)->Depth` is the **x86** arm: it sits in the `#else` of `#if
//!   !defined(_X86_)` at `10.0.26100.0/km/wdm.h:26104`, with the macro at
//!   `:26154`. On x64, with `_NTDDK_`/`_NTIFS_` defined as a kernel driver
//!   build defines them (`:26106`), `ExQueryDepthSList` is an ordinary
//!   `NTKERNELAPI USHORT ExQueryDepthSList(PSLIST_HEADER)` export at
//!   `:26108-26112` — which is why the generated `ntddk` bindings already
//!   contain it, as they do `ExpInterlockedPopEntrySList` and
//!   `ExpInterlockedPushEntrySList` (`:26197` and `:26203`, the two the
//!   `InterlockedPop/PushEntrySList` macros expand to at `:26184-26188`). No
//!   union access is involved.
//! - It is **not** true that the cache would only buy speed on kernels a
//!   consumer cannot load on anyway. Removing the miss-arm `NTDDI_WIN10_NI`
//!   imports is what made the pre-NI window loadable in the first place, and
//!   `ExAllocatePool2` — which used to set the RS2..2004 floor — is now
//!   resolved through `MmGetSystemRoutineAddress` too (see
//!   `allocate_pool_downlevel.rs`). The loadable window therefore spans Windows
//!   10 RS2 (1703) through current builds on both client and Server SKUs, and
//!   the cache miss arm is live across that entire window.
//!
//! # Correctness argument for the SLIST cache
//!
//! Both the WDK-exported routines and the WDK's `FORCEINLINE` body operate on
//! the same `Lookaside->L.ListHead` field, and both use the same
//! `InterlockedPop/PushEntrySList` primitives. When the kernel exports the
//! routines, this module delegates to them (the `resolved != ABSENT` arm)
//! rather than duplicating their work — because the exported routine may
//! carry kernel-internal accounting this module cannot see.
//!
//! When the kernel does **not** export them, `ExInitializeLookasideListEx`
//! still zero-initialises the `SLIST_HEADER` and populates the same
//! `AllocateEx`/`FreeEx` callbacks the miss arm uses. So the WDK's own
//! `FORCEINLINE` body — reproduced below — is what a C driver built against
//! the same WDK header runs, byte for byte.
//!
//! What remains true is that the SLIST arm cannot be exercised without a
//! pre-NI machine. The counters `AllocateHits`, `AllocateMisses`, `FreeHits`
//! and `FreeMisses` are maintained so that `!lookaside` in a debugger shows
//! the hit rate that motivates it, giving a positive artefact for any future
//! test.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::{
    ntddk::{
        ExQueryDepthSList,
        ExpInterlockedPopEntrySList,
        ExpInterlockedPushEntrySList,
        KeGetCurrentIrql,
        MmGetSystemRoutineAddress,
    },
    types::{
        PLOOKASIDE_LIST_EX,
        PSLIST_ENTRY,
        PSLIST_HEADER,
        PVOID,
        UNICODE_STRING,
        USHORT,
        WCHAR,
    },
};

/// Sentinel for "not looked up yet" in the caches below.
///
/// Zero cannot be a routine address, so it is free to mean "unknown".
const UNRESOLVED: usize = 0;

/// Sentinel for "looked up, and this kernel does not export it".
///
/// One is not a valid code address on any supported platform — kernel-mode code
/// lives in the upper half of the address space — so it cannot collide with a
/// real resolution. Distinguishing this from [`UNRESOLVED`] is what stops a
/// down-level kernel being re-probed on every allocation.
const ABSENT: usize = 1;

/// `NTDDI_VERSION >= NTDDI_WIN10_NI` build of `ExAllocateFromLookasideListEx`,
/// or [`UNRESOLVED`]/[`ABSENT`].
static ALLOCATE_FROM_LOOKASIDE: AtomicUsize = AtomicUsize::new(UNRESOLVED);

/// As [`ALLOCATE_FROM_LOOKASIDE`], for `ExFreeToLookasideListEx`.
static FREE_TO_LOOKASIDE: AtomicUsize = AtomicUsize::new(UNRESOLVED);

/// `ExAllocateFromLookasideListEx`, as `10.0.26100.0/km/wdm.h:26326-26333`
/// declares it.
type AllocateFromLookaside = unsafe extern "C" fn(PLOOKASIDE_LIST_EX) -> PVOID;

/// `ExFreeToLookasideListEx`, as `10.0.26100.0/km/wdm.h:26335-26341` declares
/// it.
type FreeToLookaside = unsafe extern "C" fn(PLOOKASIDE_LIST_EX, PVOID);

/// `"ExAllocateFromLookasideListEx"` as UTF-16, for
/// `MmGetSystemRoutineAddress`.
///
/// A `static` rather than a local because `MmGetSystemRoutineAddress` takes a
/// `UNICODE_STRING` that points *at* the buffer and reads it during the call; a
/// local would be fine for that, but a `static` is also what makes the
/// descriptor cheap to build and impossible to get wrong.
static ALLOCATE_NAME: [WCHAR; 29] = wide(b"ExAllocateFromLookasideListEx");

/// `"ExFreeToLookasideListEx"` as UTF-16.
static FREE_NAME: [WCHAR; 23] = wide(b"ExFreeToLookasideListEx");

/// Widens an ASCII routine name at compile time.
///
/// A `const fn` so the two names above are data in the image rather than work
/// at run time, and so a non-ASCII byte is a build failure rather than a lookup
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

/// Builds the `UNICODE_STRING` `MmGetSystemRoutineAddress` wants over `name`.
///
/// The `Length` is a byte count, which is what a `UNICODE_STRING` counts, and
/// the buffer is deliberately not NUL terminated: a `UNICODE_STRING` is
/// explicitly counted.
fn name_of(name: &'static [WCHAR]) -> UNICODE_STRING {
    // `name` is one of the two `static`s above, both far shorter than
    // `USHORT::MAX`, so this cannot truncate.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "both callers pass a `static` of fewer than 30 elements, so the byte count is \
                  under 60 and cannot exceed `USHORT::MAX`"
    )]
    let length_in_bytes = (name.len() * size_of::<WCHAR>()) as USHORT;

    UNICODE_STRING {
        Length: length_in_bytes,
        MaximumLength: length_in_bytes,
        Buffer: name.as_ptr().cast_mut(),
    }
}

/// Resolves `name` through `MmGetSystemRoutineAddress`, caching the answer in
/// `cache`, and returns the address or [`ABSENT`].
///
/// # Why the IRQL is tested
///
/// `MmGetSystemRoutineAddress` is `_IRQL_requires_max_(PASSIVE_LEVEL)`
/// (`10.0.26100.0/km/wdm.h:13974`) and both lookaside routines are
/// `_IRQL_requires_max_(DISPATCH_LEVEL)`, so a caller may legitimately be above
/// `PASSIVE_LEVEL`. Probing there would be the IRQL violation; taking the
/// down-level path for that one call is merely slow, and a later call at
/// `PASSIVE_LEVEL` populates the cache for good.
///
/// This is what makes the shims self-contained: there is no initialisation a
/// consumer has to remember to call from `DriverEntry`, and therefore no way to
/// get an unresolved-pointer call by forgetting it.
fn resolve(cache: &AtomicUsize, name: &'static [WCHAR]) -> usize {
    let cached = cache.load(Ordering::Relaxed);
    if cached != UNRESOLVED {
        return cached;
    }

    // SAFETY: `KeGetCurrentIrql` reads the current processor's IRQL and takes
    // no arguments
    let irql = unsafe { KeGetCurrentIrql() };

    // `PASSIVE_LEVEL` is 0 (`km/wdm.h`), and `crate::constants::PASSIVE_LEVEL`
    // is a `u32` while `KIRQL` is a `UCHAR`, so the comparison is written
    // against 0 rather than through a cast that could hide a widening.
    if irql != 0 {
        return ABSENT;
    }

    let mut routine_name = name_of(name);

    // SAFETY: `routine_name` is a valid `UNICODE_STRING` over a `static` buffer
    // that outlives the call, and the IRQL test above establishes the
    // `PASSIVE_LEVEL` precondition
    let address = unsafe { MmGetSystemRoutineAddress(&raw mut routine_name) };

    let resolved = if address.is_null() {
        ABSENT
    } else {
        address as usize
    };

    // `Relaxed` is sufficient: the value published is a plain integer with no
    // memory it guards, two threads resolving concurrently compute the same
    // answer, and a store lost to a race merely costs one more lookup.
    cache.store(resolved, Ordering::Relaxed);

    resolved
}

/// Removes (pops) the first entry from the specified lookaside list.
///
/// `10.0.26100.0/km/wdm.h:26326-26333` when the kernel exports it, and the
/// full `FORCEINLINE` body of `:26619-26657` when it does not. See the module
/// documentation for why this is not a plain extern.
///
/// # Safety
///
/// As `ExAllocateFromLookasideListEx`: `Lookaside` must point to a
/// `LOOKASIDE_LIST_EX` initialised by `ExInitializeLookasideListEx` and not yet
/// deleted, and the caller must be at `IRQL <= DISPATCH_LEVEL`.
#[expect(
    non_snake_case,
    reason = "bindings in this crate retain their original C names, and this one replaces a \
              bindgen-generated extern of the same name"
)]
#[must_use]
pub unsafe extern "C" fn ExAllocateFromLookasideListEx(Lookaside: PLOOKASIDE_LIST_EX) -> PVOID {
    let resolved = resolve(&ALLOCATE_FROM_LOOKASIDE, &ALLOCATE_NAME);

    if resolved != ABSENT {
        // SAFETY: `resolved` is neither `UNRESOLVED` nor `ABSENT`, so it is the
        // address `MmGetSystemRoutineAddress` returned for
        // `ExAllocateFromLookasideListEx`, whose declared signature is
        // `AllocateFromLookaside`
        let routine = unsafe { core::mem::transmute::<usize, AllocateFromLookaside>(resolved) };

        // SAFETY: the caller's contract is this routine's contract, unchanged
        return unsafe { routine(Lookaside) };
    }

    // The WDK's `FORCEINLINE` body, `10.0.26100.0/km/wdm.h:26642-26657`, in
    // order.
    //
    // SAFETY: `Lookaside` points to a live `LOOKASIDE_LIST_EX` per this
    // function's contract, so `L` is a valid `GENERAL_LOOKASIDE_POOL`. A raw
    // pointer is used rather than a reference because the list is shared kernel
    // state that other CPUs and the kernel itself access concurrently.
    let list = unsafe { &raw mut (*Lookaside).L };

    // `10.0.26100.0/km/wdm.h:26646`. Both counters are `ULONG` and the WDK
    // increments them unsynchronized too -- they are diagnostics for
    // `!lookaside`, not accounting -- so a wrap is the WDK's behaviour and not
    // a defect here.
    //
    // SAFETY: `list` is a valid pointer per the SAFETY above, so the counter is
    // a live `ULONG` (u32) at a valid, aligned address.
    let total_allocates_ptr = unsafe { &raw mut (*list).TotalAllocates };
    // SAFETY: `total_allocates_ptr` is a valid, aligned `*mut u32` per the
    // SAFETY above, and `AtomicU32` has the same size and alignment. The atomic
    // access is what makes the unsynchronized increment race-free.
    let total_allocates = unsafe { AtomicU32::from_ptr(total_allocates_ptr) };
    total_allocates.fetch_add(1, Ordering::Relaxed);

    // Fast path: pop from the SLIST cache. `InterlockedPopEntrySList` on x64
    // expands to `ExpInterlockedPopEntrySList` (`10.0.26100.0/km/wdm.h:26197`
    // is the export, `:26184-26188` is the macro definition), which returns
    // the freed entry if the cache is non-empty and null if it is empty. The
    // SLIST is 8-byte-aligned, and every `Lookaside->Type/Size/Tag`-shaped
    // allocation from `L.AllocateEx` satisfies that alignment because the
    // pool returns 16-aligned blocks -- so the popped pointer is safe to
    // return without further alignment adjustment.
    //
    // SAFETY: `ListHead` and `SingleListHead` are the two arms of the same
    // anonymous union and both are `SLIST_HEADER`; the field is initialised
    // to zero by `ExInitializeLookasideListEx` and is the same field the
    // exported `ExAllocateFromLookasideListEx` operates on. Taking a mutable
    // pointer to it through the union arm is well-defined
    let list_head: PSLIST_HEADER = unsafe { &raw mut (*list).__bindgen_anon_1.ListHead };

    // SAFETY: `list_head` is a valid `PSLIST_HEADER` per the SAFETY above; the
    // primitive is atomic and callable at or below `DISPATCH_LEVEL` (matching
    // this routine's own IRQL requirement)
    let entry = unsafe { ExpInterlockedPopEntrySList(list_head) };

    if !entry.is_null() {
        // Cache hit: `AllocateHits` and `AllocateMisses` are the two arms of
        // the same anonymous union, so writing `AllocateHits` shares its
        // storage with `AllocateMisses`. The WDK's `FORCEINLINE` body does not
        // increment `AllocateHits` on the hit arm -- it leaves that field
        // untouched -- so this code does not either. Total hits are inferred
        // as `TotalAllocates - AllocateMisses`, which is what `!lookaside`
        // does.
        return entry.cast::<core::ffi::c_void>();
    }

    // Miss: bump the counter and call through the callback the kernel installed
    // by `ExInitializeLookasideListEx`. `10.0.26100.0/km/wdm.h:26649`.
    //
    // SAFETY: `AllocateMisses` and `AllocateHits` are the two arms of the same
    // anonymous union and both are `ULONG`, so reading and writing either is
    // valid whichever the kernel last wrote. The atomic access makes the
    // unsynchronized increment race-free.
    let allocate_misses_ptr = unsafe { &raw mut (*list).__bindgen_anon_2.AllocateMisses };
    // SAFETY: `allocate_misses_ptr` is a valid, aligned `*mut u32` per the
    // SAFETY above, and `AtomicU32` has the same size and alignment.
    let allocate_misses = unsafe { AtomicU32::from_ptr(allocate_misses_ptr) };
    allocate_misses.fetch_add(1, Ordering::Relaxed);

    // SAFETY: `AllocateEx` and `Allocate` are the two arms of one anonymous
    // union; `ExInitializeLookasideListEx` is documented to fill the `Ex` form,
    // which is also the arm the WDK's own `FORCEINLINE` body calls with four
    // arguments at `10.0.26100.0/km/wdm.h:26650-26653`
    let allocate = unsafe { (*list).__bindgen_anon_4.AllocateEx };

    let Some(allocate) = allocate else {
        // `ExInitializeLookasideListEx` installs a default when the caller
        // passes none, so this is unreachable for an initialised list.
        // Returning null rather than calling through it is what keeps that
        // unreachability from being a bugcheck if it is ever wrong -- a failed
        // lookaside allocation is a status every caller already handles.
        return core::ptr::null_mut();
    };

    // `10.0.26100.0/km/wdm.h:26650-26653`, argument for argument.
    //
    // SAFETY: `list` is a valid pointer per the SAFETY above; `Type` is the
    // descriptor's own field, which the WDK's body reads for the callback.
    let pool_type = unsafe { (*list).Type };
    // SAFETY: `list` is a valid pointer per the SAFETY above; `Size` is the
    // descriptor's own field, which the WDK's body reads for the callback.
    let size = unsafe { (*list).Size as crate::types::SIZE_T };
    // SAFETY: `list` is a valid pointer per the SAFETY above; `Tag` is the
    // descriptor's own field, which the WDK's body reads for the callback.
    let tag = unsafe { (*list).Tag };

    // SAFETY: `allocate` is the non-null callback the kernel installed in this
    // descriptor, and the four arguments are the descriptor's own `Type`,
    // `Size` and `Tag` plus the descriptor itself, which is what the WDK passes
    unsafe { allocate(pool_type, size, tag, Lookaside) }
}

/// Inserts (pushes) the specified entry into the specified lookaside list.
///
/// `10.0.26100.0/km/wdm.h:26335-26341` when the kernel exports it, and the
/// full `FORCEINLINE` body of `:26662-26699` when it does not.
///
/// # Safety
///
/// As `ExFreeToLookasideListEx`: `Lookaside` must point to a live
/// `LOOKASIDE_LIST_EX`, `Entry` must be a block obtained from that same list,
/// and the caller must be at `IRQL <= DISPATCH_LEVEL`.
#[expect(
    non_snake_case,
    reason = "bindings in this crate retain their original C names, and this one replaces a \
              bindgen-generated extern of the same name"
)]
pub unsafe extern "C" fn ExFreeToLookasideListEx(Lookaside: PLOOKASIDE_LIST_EX, Entry: PVOID) {
    let resolved = resolve(&FREE_TO_LOOKASIDE, &FREE_NAME);

    if resolved != ABSENT {
        // SAFETY: `resolved` is the address `MmGetSystemRoutineAddress`
        // returned for `ExFreeToLookasideListEx`, whose declared signature is
        // `FreeToLookaside`
        let routine = unsafe { core::mem::transmute::<usize, FreeToLookaside>(resolved) };

        // SAFETY: the caller's contract is this routine's contract, unchanged
        unsafe { routine(Lookaside, Entry) };
        return;
    }

    // The WDK's `FORCEINLINE` body, `10.0.26100.0/km/wdm.h:26687-26698`, in
    // order.
    //
    // SAFETY: `Lookaside` points to a live `LOOKASIDE_LIST_EX` per this
    // function's contract, so `L` is a valid `GENERAL_LOOKASIDE_POOL`. A raw
    // pointer is used rather than a reference because the list is shared kernel
    // state that other CPUs and the kernel itself access concurrently.
    let list = unsafe { &raw mut (*Lookaside).L };

    // `10.0.26100.0/km/wdm.h:26689`.
    //
    // SAFETY: `list` is a valid pointer per the SAFETY above, so the counter is
    // a live `ULONG` (u32) at a valid, aligned address.
    let total_frees_ptr = unsafe { &raw mut (*list).TotalFrees };
    // SAFETY: `total_frees_ptr` is a valid, aligned `*mut u32` per the SAFETY
    // above, and `AtomicU32` has the same size and alignment. The atomic access
    // is what makes the unsynchronized increment race-free.
    let total_frees = unsafe { AtomicU32::from_ptr(total_frees_ptr) };
    total_frees.fetch_add(1, Ordering::Relaxed);

    // SAFETY: `ListHead` and `SingleListHead` are the two arms of the same
    // anonymous union and both are `SLIST_HEADER`; the field is initialised to
    // zero by `ExInitializeLookasideListEx`. Taking a mutable pointer to it
    // through the union arm is well-defined
    let list_head: PSLIST_HEADER = unsafe { &raw mut (*list).__bindgen_anon_1.ListHead };

    // Cache is full when its depth reaches or exceeds `L.Depth`; push to it
    // otherwise. `L.Depth` is the tunable "grow up to but not past" bound the
    // WDK maintains -- `10.0.26100.0/km/wdm.h:26690`.
    //
    // SAFETY: `list_head` is a valid `PSLIST_HEADER` per the SAFETY above;
    // `ExQueryDepthSList` is a pure read of the depth field and callable at
    // any IRQL
    let depth = unsafe { ExQueryDepthSList(list_head) };

    // SAFETY: `list` is a valid pointer per the SAFETY above; `Depth` is a
    // plain read of a field the kernel maintains.
    if depth >= unsafe { (*list).Depth } {
        // Cache is at capacity: bump the miss counter and free through the
        // callback the kernel installed. `10.0.26100.0/km/wdm.h:26691-26692`.
        //
        // SAFETY: `FreeMisses` and `FreeHits` are the two arms of one anonymous
        // union and both are `ULONG`. The atomic access makes the
        // unsynchronized increment race-free.
        let free_misses_ptr = unsafe { &raw mut (*list).__bindgen_anon_3.FreeMisses };
        // SAFETY: `free_misses_ptr` is a valid, aligned `*mut u32` per the
        // SAFETY above, and `AtomicU32` has the same size and alignment.
        let free_misses = unsafe { AtomicU32::from_ptr(free_misses_ptr) };
        free_misses.fetch_add(1, Ordering::Relaxed);

        // SAFETY: `FreeEx` and `Free` are the two arms of one anonymous union;
        // `ExInitializeLookasideListEx` fills the `Ex` form, which is the arm
        // the WDK's own body calls with two arguments at
        // `10.0.26100.0/km/wdm.h:26692`
        let free = unsafe { (*list).__bindgen_anon_5.FreeEx };

        let Some(free) = free else {
            // Unreachable for an initialised list, as in the allocate path.
            // Leaking the block is the only safe response: the alternative is
            // guessing which pool it came from and calling `ExFreePool` on a
            // block the kernel may own.
            return;
        };

        // `10.0.26100.0/km/wdm.h:26692`.
        // SAFETY: `free` is the non-null callback the kernel installed in this
        // descriptor, and `Entry` came from the matching allocate per this
        // function's contract
        unsafe { free(Entry, Lookaside) };
        return;
    }

    // Cache has room: push `Entry` onto it. The WDK's body casts `Entry` to
    // `PSLIST_ENTRY` -- the block's first 16 bytes are the `SLIST_ENTRY` next
    // pointer, which the caller's block already reserves because
    // `ExInitializeLookasideListEx` requires `Size >= sizeof(SLIST_ENTRY)`.
    //
    // The WDK's body does not increment `FreeHits` on this arm; total hits are
    // inferred as `TotalFrees - FreeMisses` at diagnostic time. This module
    // matches that.
    //
    // SAFETY: `Entry` came from a matching allocate on this list per this
    // function's contract, so its first `sizeof(SLIST_ENTRY)` bytes are a
    // writable region the SLIST primitive can chain
    let entry_slist: PSLIST_ENTRY = Entry.cast::<crate::types::SLIST_ENTRY>();

    // SAFETY: `list_head` and `entry_slist` are both valid per the SAFETY
    // above; the primitive is atomic and callable at or below `DISPATCH_LEVEL`
    // (matching this routine's own IRQL requirement)
    unsafe {
        ExpInterlockedPushEntrySList(list_head, entry_slist);
    }
}
