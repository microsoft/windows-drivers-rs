# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.3.1](https://github.com/microsoft/windows-drivers-rs/compare/wdk-v0.3.0...wdk-v0.3.1) - 2025-04-18

### Other

- Use stack-based formatter for debug-printing. ([#233](https://github.com/microsoft/windows-drivers-rs/pull/233))
- update README to clarify community engagement and contact methods ([#312](https://github.com/microsoft/windows-drivers-rs/pull/312))

## [0.3.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-v0.2.0...wdk-v0.3.0) - 2024-09-27

### Added

- configure WDK configuration via parsing Cargo manifest metadata ([#186](https://github.com/microsoft/windows-drivers-rs/pull/186))

### Fixed

- typos in Getting Started section of README.md ([#213](https://github.com/microsoft/windows-drivers-rs/pull/213))
- only emit must_use hint when wdf function has return type ([#122](https://github.com/microsoft/windows-drivers-rs/pull/122))
- [**breaking**] prevent linking of wdk libraries in tests that depend on `wdk-sys` ([#118](https://github.com/microsoft/windows-drivers-rs/pull/118))

### Other

- Update README.md ([#180](https://github.com/microsoft/windows-drivers-rs/pull/180))
- update readme to call out bugged LLVM 18 versions  ([#169](https://github.com/microsoft/windows-drivers-rs/pull/169))
- use a standardized workspace lint table ([#134](https://github.com/microsoft/windows-drivers-rs/pull/134))
- fix `winget` llvm install command option ([#115](https://github.com/microsoft/windows-drivers-rs/pull/115))
- fix various pipeline breakages (nightly rustfmt bug, new nightly clippy lints, upstream winget dependency issue) ([#117](https://github.com/microsoft/windows-drivers-rs/pull/117))
# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0](https://github/microsoft/windows-drivers-rs/compare/wdk-v0.1.0...wdk-v0.2.0) - 2024-02-08

### Fixed
- resolve warnings in rust-script blocks and only fail warnings in CI ([#87](https://github/microsoft/windows-drivers-rs/pull/87))
- fix wrong instruction used for arm64 breakpoint

### Other
- restrict to one unsafe operation per block ([#24](https://github/microsoft/windows-drivers-rs/pull/24))
- [**breaking**] enable rustdoc lints and resolve errors
- remove extra keywords in cargo manifests
- initial open-source check in
