// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

#[cfg(test)]
mod tests {
    use wdk_sys::{
        NT_ERROR,
        NT_INFORMATION,
        NT_SUCCESS,
        NT_WARNING,
        STATUS_BREAKPOINT,
        STATUS_HIBERNATED,
        STATUS_PRIVILEGED_INSTRUCTION,
        STATUS_SUCCESS,
    };
    #[test]
    pub const fn nt_status_validation() {
        assert!(NT_SUCCESS(STATUS_SUCCESS));
        assert!(NT_SUCCESS(STATUS_HIBERNATED));
        assert!(NT_INFORMATION(STATUS_HIBERNATED));
        assert!(NT_WARNING(STATUS_BREAKPOINT));
        assert!(NT_ERROR(STATUS_PRIVILEGED_INSTRUCTION));
        assert!(!NT_SUCCESS(STATUS_BREAKPOINT));
        assert!(!NT_SUCCESS(STATUS_PRIVILEGED_INSTRUCTION));
        assert!(!NT_INFORMATION(STATUS_SUCCESS));
        assert!(!NT_INFORMATION(STATUS_BREAKPOINT));
        assert!(!NT_INFORMATION(STATUS_PRIVILEGED_INSTRUCTION));
        assert!(!NT_WARNING(STATUS_SUCCESS));
        assert!(!NT_WARNING(STATUS_HIBERNATED));
        assert!(!NT_WARNING(STATUS_PRIVILEGED_INSTRUCTION));
        assert!(!NT_ERROR(STATUS_SUCCESS));
        assert!(!NT_ERROR(STATUS_HIBERNATED));
        assert!(!NT_ERROR(STATUS_BREAKPOINT));
    }
}
