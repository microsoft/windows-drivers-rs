# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.4.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-macros-v0.3.0...wdk-macros-v0.4.0) - 2025-04-18

### Added

- Cache parameters & return type during `call_unsafe_wdf_function_binding` macro expansion ([#295](https://github.com/microsoft/windows-drivers-rs/pull/295))

### Fixed

- passing cache tests when WDK config is enabled ([#332](https://github.com/microsoft/windows-drivers-rs/pull/332))

### Other

- update README to clarify community engagement and contact methods ([#312](https://github.com/microsoft/windows-drivers-rs/pull/312))
- [**breaking**] Remove lazy static instances ([#250](https://github.com/microsoft/windows-drivers-rs/pull/250))

## [0.3.0](https://github.com/microsoft/windows-drivers-rs/compare/wdk-macros-v0.2.0...wdk-macros-v0.3.0) - 2024-09-27

### Added

- configure WDK configuration via parsing Cargo manifest metadata ([#186](https://github.com/microsoft/windows-drivers-rs/pull/186))

### Fixed

- typos in Getting Started section of README.md ([#213](https://github.com/microsoft/windows-drivers-rs/pull/213))
- prevent unused import warning in arguments to `call_unsafe_wdf_function_binding` ([#207](https://github.com/microsoft/windows-drivers-rs/pull/207))
- prevent `E0530 function parameters cannot shadow tuple structs` error when using `call_unsafe_wdf_function_binding`  ([#200](https://github.com/microsoft/windows-drivers-rs/pull/200))
- only emit must_use hint when wdf function has return type ([#122](https://github.com/microsoft/windows-drivers-rs/pull/122))
- [**breaking**] prevent linking of wdk libraries in tests that depend on `wdk-sys` ([#118](https://github.com/microsoft/windows-drivers-rs/pull/118))

### Other

- Improve doc comments to comply with `too_long_first_doc_paragraph` clippy lint ([#202](https://github.com/microsoft/windows-drivers-rs/pull/202))
- Update README.md ([#180](https://github.com/microsoft/windows-drivers-rs/pull/180))
- update readme to call out bugged LLVM 18 versions  ([#169](https://github.com/microsoft/windows-drivers-rs/pull/169))
- Bump paste from 1.0.14 to 1.0.15 ([#152](https://github.com/microsoft/windows-drivers-rs/pull/152))
- Bump proc-macro2 from 1.0.81 to 1.0.82 ([#151](https://github.com/microsoft/windows-drivers-rs/pull/151))
- Bump rustversion from 1.0.14 to 1.0.15 ([#145](https://github.com/microsoft/windows-drivers-rs/pull/145))
- Bump macrotest from 1.0.11 to 1.0.12 ([#146](https://github.com/microsoft/windows-drivers-rs/pull/146))
- Bump proc-macro2 from 1.0.78 to 1.0.81 ([#147](https://github.com/microsoft/windows-drivers-rs/pull/147))
- Bump trybuild from 1.0.89 to 1.0.91 ([#148](https://github.com/microsoft/windows-drivers-rs/pull/148))
- use a standardized workspace lint table ([#134](https://github.com/microsoft/windows-drivers-rs/pull/134))
- Bump syn from 2.0.48 to 2.0.58 ([#135](https://github.com/microsoft/windows-drivers-rs/pull/135))
- fix `winget` llvm install command option ([#115](https://github.com/microsoft/windows-drivers-rs/pull/115))
- fix various pipeline breakages (nightly rustfmt bug, new nightly clippy lints, upstream winget dependency issue) ([#117](https://github.com/microsoft/windows-drivers-rs/pull/117))
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
