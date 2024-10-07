# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


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
