# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0](https://github.com/microsoft/windows-drivers-rs/compare/cargo-wdk-v0.1.1...cargo-wdk-v0.2.0) - 2026-10-22

### Added

- *(cargo-wdk)* support hyphen-prefixed signtool args and support empty custom args ([#737](https://github.com/microsoft/windows-drivers-rs/pull/737))
- *(cargo-wdk)* add `--stampinf-args` passthrough to customize `stampinf` options ([#733](https://github.com/microsoft/windows-drivers-rs/pull/733))
- *(cargo-wdk)* add `--infverif-args` passthrough to customize `inverif` options ([#734](https://github.com/microsoft/windows-drivers-rs/pull/734))
- add `--inf2cat-args` passthrough to customize `inf2cat` options ([#702](https://github.com/microsoft/windows-drivers-rs/pull/702))
- *(cargo-wdk)* add `--signtool-args` passthrough to customize driver signing ([#699](https://github.com/microsoft/windows-drivers-rs/pull/699))
- add linker directive for VHF lib + move static linker directives from wdk-build to wdk-sys ([#685](https://github.com/microsoft/windows-drivers-rs/pull/685))
- *(cargo-wdk)* add `--target-platform` option to the `build` command ([#683](https://github.com/microsoft/windows-drivers-rs/pull/683))
- add feature selection options to the `build` command ([#665](https://github.com/microsoft/windows-drivers-rs/pull/665))
- improve user-mode and kernel-mode driver panic handlers ([#642](https://github.com/microsoft/windows-drivers-rs/pull/642))
- add `--locked` option to the `build` command ([#656](https://github.com/microsoft/windows-drivers-rs/pull/656))
- introduce sign mode for driver packaging, allowing skipping of signing steps ([#649](https://github.com/microsoft/windows-drivers-rs/pull/649))
- add `cargo wdk clean` command ([#638](https://github.com/microsoft/windows-drivers-rs/pull/638))
- *(cargo-wdk)* support target architecture override through `config.toml` and `CARGO_BUILD_TARGET` ([#494](https://github.com/microsoft/windows-drivers-rs/pull/494))

### Fixed

- *(cargo-wdk)* use `CARGO` environment variable for nested invocations ([#742](https://github.com/microsoft/windows-drivers-rs/pull/742))
- modify `WinGet` module selection in CI and resolve Clippy errors ([#739](https://github.com/microsoft/windows-drivers-rs/pull/739))
- [**breaking**] remove linker map file from driver packages ([#735](https://github.com/microsoft/windows-drivers-rs/pull/735))
- *(cargo-wdk)* update WDK build number range to skip `InfVerif` when building `sample` class drivers ([#679](https://github.com/microsoft/windows-drivers-rs/pull/679))
- *(cargo-wdk)* use `--message-format=json-render-diagnostics` in the build command to render compiler errors instead of the plain `json` option ([#645](https://github.com/microsoft/windows-drivers-rs/pull/645))
- do not sanitize `CARGO_HOME` & `RUSTUP_HOME` ([#609](https://github.com/microsoft/windows-drivers-rs/pull/609))
- replace direct call to `std::fs::create_dir_all` with `fs` provider `fs::create_dir_all` ([#602](https://github.com/microsoft/windows-drivers-rs/pull/602))
- disable ANSI colors when output is redirected ([#601](https://github.com/microsoft/windows-drivers-rs/pull/601))
- move cargo-wdk output to stderr ([#599](https://github.com/microsoft/windows-drivers-rs/pull/599))
- cargo wdk new now respects default VCS behavior ([#575](https://github.com/microsoft/windows-drivers-rs/pull/575))

### Other

- improve Dependabot and refresh repo-wide dependencies ([#745](https://github.com/microsoft/windows-drivers-rs/pull/745))
- re-format .rs files with the current nightly rustfmt ([#732](https://github.com/microsoft/windows-drivers-rs/pull/732))
- fix assert_is_empty and branches_sharing_code clippy lints ([#713](https://github.com/microsoft/windows-drivers-rs/pull/713))
- add opt out env variable to temporarily prevent using build-dir v2 in nightly runs ([#707](https://github.com/microsoft/windows-drivers-rs/pull/707))
- *(cargo-wdk)* [**breaking**] rename `windows-driver` target-platform value to `windows` ([#705](https://github.com/microsoft/windows-drivers-rs/pull/705))
- update `anyhow` and `crossbeam-epoch` to resolve RUSTSEC-2026-0190 and RUSTSEC-2026-0204 ([#704](https://github.com/microsoft/windows-drivers-rs/pull/704))
- update bindgen to fix compatibility with LLVM 22 ([#682](https://github.com/microsoft/windows-drivers-rs/pull/682))
- Bump serde_json from 1.0.140 to 1.0.143 in /crates/cargo-wdk/tests/wdm-driver ([#547](https://github.com/microsoft/windows-drivers-rs/pull/547))
- fix `install-winget` dependency install loop ([#680](https://github.com/microsoft/windows-drivers-rs/pull/680))
- Bump regex from 1.11.1 to 1.12.2 in /crates/cargo-wdk/tests/wdm-driver ([#548](https://github.com/microsoft/windows-drivers-rs/pull/548))
- Bump regex from 1.11.1 to 1.12.2 in /crates/cargo-wdk/tests/umdf-driver ([#546](https://github.com/microsoft/windows-drivers-rs/pull/546))
- Bump serde_json from 1.0.139 to 1.0.143 in /crates/cargo-wdk/tests/umdf-driver ([#544](https://github.com/microsoft/windows-drivers-rs/pull/544))
- Bump regex from 1.11.1 to 1.12.2 in /crates/cargo-wdk/tests/kmdf-driver ([#539](https://github.com/microsoft/windows-drivers-rs/pull/539))
- Bump serde_json from 1.0.140 to 1.0.143 in /crates/cargo-wdk/tests/kmdf-driver ([#541](https://github.com/microsoft/windows-drivers-rs/pull/541))
- bump remaining crates to 2024 ([#634](https://github.com/microsoft/windows-drivers-rs/pull/634))
- replace cargo-sort with taplo for dependency sorting ([#632](https://github.com/microsoft/windows-drivers-rs/pull/632))
- Bump semver from 1.0.26 to 1.0.27 in /crates/cargo-wdk/tests/wdm-driver ([#549](https://github.com/microsoft/windows-drivers-rs/pull/549))
- Add code coverage ([#556](https://github.com/microsoft/windows-drivers-rs/pull/556))
- Skip build step in `cargo-wdk new` tests for release-plz PRs ([#584](https://github.com/microsoft/windows-drivers-rs/pull/584))
- make cargo wdk integration tests faster ([#553](https://github.com/microsoft/windows-drivers-rs/pull/553))
- add unit tests for cargo wdk new subcommand action flow ([#551](https://github.com/microsoft/windows-drivers-rs/pull/551))

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
