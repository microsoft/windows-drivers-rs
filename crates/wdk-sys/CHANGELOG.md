# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0](https://github/microsoft/windows-drivers-rs/compare/wdk-sys-v0.1.0...wdk-sys-v0.2.0) - 2024-02-08

### Added
- generate CStr for c string constants instead of &[u8] ([#72](https://github/microsoft/windows-drivers-rs/pull/72))

### Fixed
- resolve warnings in rust-script blocks and only fail warnings in CI ([#87](https://github/microsoft/windows-drivers-rs/pull/87))

### Other
- update dependencies
- allow multiple_crate_versions in wdk-build (build dependency) ([#98](https://github/microsoft/windows-drivers-rs/pull/98))
- allow exception for clippy::pub_underscore_fields in generated code ([#77](https://github/microsoft/windows-drivers-rs/pull/77))
- Bump thiserror from 1.0.48 to 1.0.55 ([#59](https://github/microsoft/windows-drivers-rs/pull/59))
- reduce noise from bindgen warnings
- fix clippy errors missed due to buggy ci stage
- restrict to one unsafe operation per block ([#24](https://github/microsoft/windows-drivers-rs/pull/24))
- [**breaking**] enable rustdoc lints and resolve errors
- remove extra keywords in cargo manifests
- initial open-source check in
