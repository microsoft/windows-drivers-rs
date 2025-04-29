# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.4.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-sys-v0.3.0...wdk-sys-v0.4.0) - 2025-04-18

### Added

- extend coverage in `wdk-sys` to include usb-related headers ([#296](https://github.com/microsoft/windows-drivers-rs/pull/296))
- expand wdk-sys coverage to include gpio and parallel ports related headers ([#278](https://github.com/microsoft/windows-drivers-rs/pull/278))
- add support for Storage API subset in `wdk-sys` ([#287](https://github.com/microsoft/windows-drivers-rs/pull/287))
- expand `wdk-sys` coverage to include spb-related headers ([#263](https://github.com/microsoft/windows-drivers-rs/pull/263))
- [**breaking**] expand `wdk-sys` coverage to include hid-related headers ([#260](https://github.com/microsoft/windows-drivers-rs/pull/260))
- Use stack-based formatter for debug-printing. ([#233](https://github.com/microsoft/windows-drivers-rs/pull/233))

### Fixed

- passing cache tests when WDK config is enabled ([#332](https://github.com/microsoft/windows-drivers-rs/pull/332))
- [**breaking**] specify rust version & edition to wdk-default bindgen::builder ([#314](https://github.com/microsoft/windows-drivers-rs/pull/314))
- use absolute paths for items used in PAGED_CODE macro ([#297](https://github.com/microsoft/windows-drivers-rs/pull/297))

### Other

- update README to clarify community engagement and contact methods ([#312](https://github.com/microsoft/windows-drivers-rs/pull/312))
- [**breaking**] Remove lazy static instances ([#250](https://github.com/microsoft/windows-drivers-rs/pull/250))
- use `is_none_or` for `clippy::nonminimal_bool` and resolve `clippy::needless_raw_string_hashes` ([#231](https://github.com/microsoft/windows-drivers-rs/pull/231))

## [0.3.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-sys-v0.2.0...wdk-sys-v0.3.0) - 2024-09-27

### Added

- add more precise NTSTATUS const fns ([#183](https://github.com/microsoft/windows-drivers-rs/pull/183))
- configure WDK configuration via parsing Cargo manifest metadata ([#186](https://github.com/microsoft/windows-drivers-rs/pull/186))

### Fixed

- typos in Getting Started section of README.md ([#213](https://github.com/microsoft/windows-drivers-rs/pull/213))
- [**breaking**] prevent linking of wdk libraries in tests that depend on `wdk-sys` ([#118](https://github.com/microsoft/windows-drivers-rs/pull/118))

### Other

- Improve doc comments to comply with `too_long_first_doc_paragraph` clippy lint ([#202](https://github.com/microsoft/windows-drivers-rs/pull/202))
- Update README.md ([#180](https://github.com/microsoft/windows-drivers-rs/pull/180))
- update readme to call out bugged LLVM 18 versions  ([#169](https://github.com/microsoft/windows-drivers-rs/pull/169))
- Build perf: Make calls to bindgen run in parallel ([#159](https://github.com/microsoft/windows-drivers-rs/pull/159))
- Bump rustversion from 1.0.14 to 1.0.15 ([#145](https://github.com/microsoft/windows-drivers-rs/pull/145))
- use a standardized workspace lint table ([#134](https://github.com/microsoft/windows-drivers-rs/pull/134))
- Bump anyhow from 1.0.79 to 1.0.82 ([#140](https://github.com/microsoft/windows-drivers-rs/pull/140))
- Bump thiserror from 1.0.56 to 1.0.59 ([#142](https://github.com/microsoft/windows-drivers-rs/pull/142))
- change version bounds for `manual_c_str_literals` and `ref_as_ptr` clippy lints ([#127](https://github.com/microsoft/windows-drivers-rs/pull/127))
- fix `winget` llvm install command option ([#115](https://github.com/microsoft/windows-drivers-rs/pull/115))
- fix various pipeline breakages (nightly rustfmt bug, new nightly clippy lints, upstream winget dependency issue) ([#117](https://github.com/microsoft/windows-drivers-rs/pull/117))
- add lint exceptions for clippy::manual_c_str_literals and clippy::ref_as_ptr ([#108](https://github.com/microsoft/windows-drivers-rs/pull/108))
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
