# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.5.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-build-v0.4.0...wdk-build-v0.5.0) - 2025-11-06

### Added

- enhance error handling with IoError and IoErrorMetadata for improved std::io::Error diagnostics for fs errors ([#480](https://github.com/microsoft/windows-drivers-rs/pull/480))
- add color to cargo wdk and cargo make argument forwarding ([#519](https://github.com/microsoft/windows-drivers-rs/pull/519))
- enhance debug tracing in bindgen and config modules ([#455](https://github.com/microsoft/windows-drivers-rs/pull/455))
- enhance cargo metadata parsing to respect config.toml ([#451](https://github.com/microsoft/windows-drivers-rs/pull/451))
- *(ci)* install and use `nuget` packages in CI workflows ([#406](https://github.com/microsoft/windows-drivers-rs/pull/406))
- add `cargo-wdk` cargo extension ([#306](https://github.com/microsoft/windows-drivers-rs/pull/306))
- make `emit_check_cfg_settings` function public ([#352](https://github.com/microsoft/windows-drivers-rs/pull/352))

### Fixed

- use latest version of ucx in the WDKContent as default ([#411](https://github.com/microsoft/windows-drivers-rs/pull/411))
- improve error reporting when no wdk-build package is found ([#339](https://github.com/microsoft/windows-drivers-rs/pull/339))

### Other

- Prepare cargo-wdk for release ([#560](https://github.com/microsoft/windows-drivers-rs/pull/560))
- [**breaking**] bump to Rust 2024 Edition ([#430](https://github.com/microsoft/windows-drivers-rs/pull/430))
- use `std::path::absolute` instead of canonicalize + strip_extended_path_prefix ([#462](https://github.com/microsoft/windows-drivers-rs/pull/462))
- Bump tracing-subscriber from 0.3.19 to 0.3.20 ([#492](https://github.com/microsoft/windows-drivers-rs/pull/492))
- enforce typo checking ([#452](https://github.com/microsoft/windows-drivers-rs/pull/452))
- update crate references for consistency in documentation ([#440](https://github.com/microsoft/windows-drivers-rs/pull/440))
- improve cargo-wdk tests ([#429](https://github.com/microsoft/windows-drivers-rs/pull/429))

## [0.4.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-build-v0.3.0...wdk-build-v0.4.0) - 2025-04-18

### Added

- extend coverage in `wdk-sys` to include usb-related headers ([#296](https://github.com/microsoft/windows-drivers-rs/pull/296))
- expand wdk-sys coverage to include gpio and parallel ports related headers ([#278](https://github.com/microsoft/windows-drivers-rs/pull/278))
- add support for Storage API subset in `wdk-sys` ([#287](https://github.com/microsoft/windows-drivers-rs/pull/287))
- expand `wdk-sys` coverage to include spb-related headers ([#263](https://github.com/microsoft/windows-drivers-rs/pull/263))
- [**breaking**] expand `wdk-sys` coverage to include hid-related headers ([#260](https://github.com/microsoft/windows-drivers-rs/pull/260))

### Fixed

- [**breaking**] specify rust version & edition to wdk-default bindgen::builder ([#314](https://github.com/microsoft/windows-drivers-rs/pull/314))
- [**breaking**] explicitly mark `_KGDTENTRY64` and `_KIDTENTRY64` as opaque types in `bindgen` ([#277](https://github.com/microsoft/windows-drivers-rs/pull/277))
- suppress linker warnings exposed by nightly rustc change ([#279](https://github.com/microsoft/windows-drivers-rs/pull/279))
- add missing arm64rt library to linker flags for arm64 kernel-mode builds ([#261](https://github.com/microsoft/windows-drivers-rs/pull/261))

### Other

- update README to clarify community engagement and contact methods ([#312](https://github.com/microsoft/windows-drivers-rs/pull/312))
- remove noop `must_use` on trait impl ([#302](https://github.com/microsoft/windows-drivers-rs/pull/302))
- [**breaking**] Remove lazy static instances ([#250](https://github.com/microsoft/windows-drivers-rs/pull/250))
- fix panic condition docs for `package_driver_flow_condition_script` ([#264](https://github.com/microsoft/windows-drivers-rs/pull/264))
- port certificate-generation condition script to Rust ([#259](https://github.com/microsoft/windows-drivers-rs/pull/259))
- remove redundant code-path in `detect_wdk_content_root` ([#249](https://github.com/microsoft/windows-drivers-rs/pull/249))
- use `next_back` instead of `last` on double-ended iterators (`clippy::double_ended_iterator_last`) ([#262](https://github.com/microsoft/windows-drivers-rs/pull/262))
- use `is_none_or` for `clippy::nonminimal_bool` and resolve `clippy::needless_raw_string_hashes` ([#231](https://github.com/microsoft/windows-drivers-rs/pull/231))
- fix `clippy::nonminimal_bool` and `clippy::ref_option` issues ([#230](https://github.com/microsoft/windows-drivers-rs/pull/230))

## [0.3.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-build-v0.2.0...wdk-build-v0.3.0) - 2024-09-27

### Added

- add `skip_umdf_static_crt_check` unstable option to prevent static crt linkage check ([#217](https://github.com/microsoft/windows-drivers-rs/pull/217))
- [**breaking**] add 'ExAllocatePool' to blocklist due to deprecation ([#190](https://github.com/microsoft/windows-drivers-rs/pull/190))
- configure WDK configuration via parsing Cargo manifest metadata ([#186](https://github.com/microsoft/windows-drivers-rs/pull/186))

### Fixed

- typos in Getting Started section of README.md ([#213](https://github.com/microsoft/windows-drivers-rs/pull/213))
- skip infverif task for sample drivers built with certain GE WDK versions ([#143](https://github.com/microsoft/windows-drivers-rs/pull/143))
- [**breaking**] prevent linking of wdk libraries in tests that depend on `wdk-sys` ([#118](https://github.com/microsoft/windows-drivers-rs/pull/118))

### Other

- fix `clippy::empty-line-after-doc-comments` lint issues ([#221](https://github.com/microsoft/windows-drivers-rs/pull/221))
- move infverif task's condition script logic to cargo_make.rs ([#216](https://github.com/microsoft/windows-drivers-rs/pull/216))
- remove unstable `rustfmt` `version` setting (replaced by auto-detected `edition`) ([#220](https://github.com/microsoft/windows-drivers-rs/pull/220))
- replace directory substitution plugin with condition_script_runner_args ([#208](https://github.com/microsoft/windows-drivers-rs/pull/208))
- use cargo-make's built-in arg expansion instead of custom plugin support in `nested-cargo-workspace-in-cargo-make-emulated-workspace-support` ([#201](https://github.com/microsoft/windows-drivers-rs/pull/201))
- Improve doc comments to comply with `too_long_first_doc_paragraph` clippy lint ([#202](https://github.com/microsoft/windows-drivers-rs/pull/202))
- Update README.md ([#180](https://github.com/microsoft/windows-drivers-rs/pull/180))
- update readme to call out bugged LLVM 18 versions  ([#169](https://github.com/microsoft/windows-drivers-rs/pull/169))
- Build perf: Make calls to bindgen run in parallel ([#159](https://github.com/microsoft/windows-drivers-rs/pull/159))
- add support for rustc-check-cfg ([#150](https://github.com/microsoft/windows-drivers-rs/pull/150))
- Bump windows from 0.52.0 to 0.56.0 ([#144](https://github.com/microsoft/windows-drivers-rs/pull/144))
- Bump rustversion from 1.0.14 to 1.0.15 ([#145](https://github.com/microsoft/windows-drivers-rs/pull/145))
- use a standardized workspace lint table ([#134](https://github.com/microsoft/windows-drivers-rs/pull/134))
- Bump clap from 4.4.18 to 4.5.4 ([#130](https://github.com/microsoft/windows-drivers-rs/pull/130))
- Bump thiserror from 1.0.56 to 1.0.59 ([#142](https://github.com/microsoft/windows-drivers-rs/pull/142))
- fix `winget` llvm install command option ([#115](https://github.com/microsoft/windows-drivers-rs/pull/115))
- fix various pipeline breakages (nightly rustfmt bug, new nightly clippy lints, upstream winget dependency issue) ([#117](https://github.com/microsoft/windows-drivers-rs/pull/117))
- add lint exceptions for clippy::manual_c_str_literals and clippy::ref_as_ptr ([#108](https://github.com/microsoft/windows-drivers-rs/pull/108))
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
- add missing cpu-arch macro definitions
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
