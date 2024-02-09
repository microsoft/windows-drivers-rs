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
