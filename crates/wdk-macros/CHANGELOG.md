# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0](https://github/microsoft/windows-drivers-rs/compare/wdk-macros-v0.1.0...wdk-macros-v0.2.0) - 2024-02-08

### Fixed
- resolve warnings in rust-script blocks and only fail warnings in CI ([#87](https://github/microsoft/windows-drivers-rs/pull/87))

### Other
- allow multiple_crate_versions in wdk-build (build dependency) ([#98](https://github/microsoft/windows-drivers-rs/pull/98))
- use owo-colors for colored output in tests ([#73](https://github/microsoft/windows-drivers-rs/pull/73))
- Bump proc-macro2 from 1.0.66 to 1.0.74 ([#60](https://github/microsoft/windows-drivers-rs/pull/60))
- Bump trybuild from 1.0.84 to 1.0.86 ([#52](https://github/microsoft/windows-drivers-rs/pull/52))
- fix clippy errors missed due to buggy ci stage
- restrict to one unsafe operation per block ([#24](https://github/microsoft/windows-drivers-rs/pull/24))
- [**breaking**] enable rustdoc lints and resolve errors
- remove extra keyword for wdk-macros
- initial open-source check in
