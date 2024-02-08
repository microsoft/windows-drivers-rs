# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0](https://github/microsoft/windows-drivers-rs/compare/wdk-build-v0.1.0...wdk-build-v0.2.0) - 2024-02-08

### Added
- package rust-driver-makefile.toml with wdk-build package ([#36](https://github/microsoft/windows-drivers-rs/pull/36))
- support multiple drivers (of same type) in same cargo workspace
- cargo-make argument forwarding
- generate CStr for c string constants instead of &[u8] ([#72](https://github/microsoft/windows-drivers-rs/pull/72))

### Fixed
- resolve warnings in rust-script blocks and only fail warnings in CI ([#87](https://github/microsoft/windows-drivers-rs/pull/87))
- add missing cpu-arch macro defintions
- fix wdk path regkey detection

### Other
- update versions in readme and rust-driver-makefile.toml
- update dependencies
- allow multiple_crate_versions in wdk-build (build dependency) ([#98](https://github/microsoft/windows-drivers-rs/pull/98))
- update cargo-make tasks with arch-specific tools
- Bump thiserror from 1.0.48 to 1.0.55 ([#59](https://github/microsoft/windows-drivers-rs/pull/59))
- restrict to one unsafe operation per block ([#24](https://github/microsoft/windows-drivers-rs/pull/24))
- [**breaking**] enable rustdoc lints and resolve errors
- initial open-source check in
