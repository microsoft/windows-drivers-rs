# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.1.1](https://github.com/microsoft/windows-drivers-rs/compare/cargo-wdk-v0.1.0...cargo-wdk-v0.1.1) - 2025-11-13

### Other

- update cargo-wdk templates to use latest crate versions ([#573](https://github.com/microsoft/windows-drivers-rs/pull/573))
- update cargo-wdk `Cargo.toml` description to align better with `README.md` ([#569](https://github.com/microsoft/windows-drivers-rs/pull/569))

## [0.1.0](https://github.com/microsoft/windows-drivers-rs/compare/cargo-wdk-v0.0.0...cargo-wdk-v0.1.0) - 2025-11-06

### Added

- support stampinf version override ([#520](https://github.com/microsoft/windows-drivers-rs/pull/520))
- add color to cargo wdk and cargo make argument forwarding ([#519](https://github.com/microsoft/windows-drivers-rs/pull/519))
- enhance cargo metadata parsing to respect config.toml ([#451](https://github.com/microsoft/windows-drivers-rs/pull/451))
- add `cargo-wdk` cargo extension ([#306](https://github.com/microsoft/windows-drivers-rs/pull/306))

### Fixed

- remove `--cwd` arg from `cargo-wdk` ([#437](https://github.com/microsoft/windows-drivers-rs/pull/437))
- remove from `NewArgs::driver_type()` the unnecessary check based on `usize` casts ([#421](https://github.com/microsoft/windows-drivers-rs/pull/421))
- remove cdylib test exclusion from Cargo.toml files ([#379](https://github.com/microsoft/windows-drivers-rs/pull/379))

### Other

- Prepare cargo-wdk for release ([#560](https://github.com/microsoft/windows-drivers-rs/pull/560))
- [**breaking**] bump to Rust 2024 Edition ([#430](https://github.com/microsoft/windows-drivers-rs/pull/430))
- Bump proc-macro2 from 1.0.94 to 1.0.101 in /crates/cargo-wdk/tests/kmdf-driver ([#530](https://github.com/microsoft/windows-drivers-rs/pull/530))
- Bump proc-macro2 from 1.0.94 to 1.0.101 in /crates/cargo-wdk/tests/wdm-driver ([#532](https://github.com/microsoft/windows-drivers-rs/pull/532))
- Bump proc-macro2 from 1.0.94 to 1.0.101 in /crates/cargo-wdk/tests/umdf-driver ([#531](https://github.com/microsoft/windows-drivers-rs/pull/531))
- Bump syn from 2.0.100 to 2.0.106 in /crates/cargo-wdk/tests/kmdf-driver ([#472](https://github.com/microsoft/windows-drivers-rs/pull/472))
- Bump syn from 2.0.100 to 2.0.106 in /crates/cargo-wdk/tests/wdm-driver ([#474](https://github.com/microsoft/windows-drivers-rs/pull/474))
- Bump syn from 2.0.100 to 2.0.106 in /crates/cargo-wdk/tests/umdf-driver ([#469](https://github.com/microsoft/windows-drivers-rs/pull/469))
- Bump cfg-if from 1.0.0 to 1.0.3 in /crates/cargo-wdk/tests/umdf-driver ([#475](https://github.com/microsoft/windows-drivers-rs/pull/475))
- Bump cfg-if from 1.0.0 to 1.0.3 in /crates/cargo-wdk/tests/wdm-driver ([#470](https://github.com/microsoft/windows-drivers-rs/pull/470))
- Bump cfg-if from 1.0.0 to 1.0.3 in /crates/cargo-wdk/tests/kmdf-driver ([#465](https://github.com/microsoft/windows-drivers-rs/pull/465))
- Bump cc from 1.2.17 to 1.2.39 in /crates/cargo-wdk/tests/umdf-driver ([#523](https://github.com/microsoft/windows-drivers-rs/pull/523))
- Bump cc from 1.2.17 to 1.2.39 in /crates/cargo-wdk/tests/kmdf-driver ([#522](https://github.com/microsoft/windows-drivers-rs/pull/522))
- Bump cc from 1.2.17 to 1.2.39 in /crates/cargo-wdk/tests/wdm-driver ([#524](https://github.com/microsoft/windows-drivers-rs/pull/524))
- improve logging for build action ([#495](https://github.com/microsoft/windows-drivers-rs/pull/495))
- use `std::path::absolute` instead of canonicalize + strip_extended_path_prefix ([#462](https://github.com/microsoft/windows-drivers-rs/pull/462))
- Bump tracing-subscriber from 0.3.19 to 0.3.20 ([#492](https://github.com/microsoft/windows-drivers-rs/pull/492))
- enforce typo checking ([#452](https://github.com/microsoft/windows-drivers-rs/pull/452))
- change categories in cargo-wdk to known slugs ([#441](https://github.com/microsoft/windows-drivers-rs/pull/441))
- update crate references for consistency in documentation ([#440](https://github.com/microsoft/windows-drivers-rs/pull/440))
- improve cargo-wdk tests ([#429](https://github.com/microsoft/windows-drivers-rs/pull/429))
- update dependencies to avoid double windows-sys dependency ([#393](https://github.com/microsoft/windows-drivers-rs/pull/393))
- fix invalid argument in cargo-wdk command in README.md ([#377](https://github.com/microsoft/windows-drivers-rs/pull/377))
