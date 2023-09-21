# Contributing to windows-drivers-rs

This project welcomes contributions and suggestions.  Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit <https://cla.opensource.microsoft.com>.

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

* [Code of Conduct](#coc)
* [Issues and Bugs](#issue)
* [Feature Requests](#feature)
* [Submission Guidelines](#submit)
* [Getting Started with windows-drivers-rs Development](#development)

## <a name="coc"></a> Code of Conduct

Help us keep this project open and inclusive. Please read and follow our [Code of Conduct](https://opensource.microsoft.com/codeofconduct/).

## <a name="issue"></a> Found an Issue?

If you find a bug in the source code or a mistake in the documentation, you can help us by
[submitting an issue](#submit-issue) to the GitHub Repository. Even better, you can
[submit a Pull Request](#submit-pr) with a fix.

## <a name="feature"></a> Want a Feature?

You can *request* a new feature by [submitting an issue](#submit-issue) to the GitHub
Repository. If you would like to *implement* a new feature, please submit an issue with
a proposal for your work first, to be sure that we can use it.

* **Small Features** can be crafted and directly [submitted as a Pull Request](#submit-pr).

## <a name="submit"></a> Submission Guidelines

### <a name="submit-issue"></a> Submitting an Issue

Before you submit an issue, search the archive, maybe your question was already answered.

If your issue appears to be a bug, and hasn't been reported, open a new issue.
Help us to maximize the effort we can spend fixing issues and adding new
features, by not reporting duplicate issues.  Providing the following information will increase the
chances of your issue being dealt with quickly:

* **Overview of the Issue** - if an error is being thrown a non-minified stack trace helps
* **Version** - what version is affected (e.g. 0.1.2)
* **Motivation for or Use Case** - explain what are you trying to do and why the current behavior is a bug for you
* **Browsers and Operating System** - is this a problem with all browsers?
* **Reproduce the Error** - provide a live example or a unambiguous set of steps
* **Related Issues** - has a similar issue been reported before?
* **Suggest a Fix** - if you can't fix the bug yourself, perhaps you can point to what might be
  causing the problem (line of code or commit)

You can file new issues by providing the above information at the corresponding repository's issues link: <https://github.com/microsoft/windows-drivers-rs/issues/new>].

### <a name="submit-pr"></a> Submitting a Pull Request (PR)

Before you submit your Pull Request (PR) consider the following guidelines:

* Search the repository (<https://github.com/microsoft/windows-drivers-rs>) for an open or closed PR
  that relates to your submission. You don't want to duplicate effort.

* Make your changes in a new git fork:

* Commit your changes using a descriptive commit message
* Push your fork to GitHub:
* In GitHub, create a pull request
* If we suggest changes then:
  * Make the required updates.
  * Rebase your fork and force push to your GitHub repository (this will update your Pull Request):

    ```shell
    git rebase master -i
    git push -f
    ```

That's it! Thank you for your contribution!

## <a name="development"></a> Getting Started with windows-drivers-rs Development

### Development Requirements

The following tools should be installed as a part of the `windows-drivers-rs` developer workflow:

* `cargo-expand`: `cargo install --locked cargo-expand`
* `cargo-audit`: `cargo install --locked cargo-audit`
* `cargo-udeps`: `cargo install --locked cargo-udeps`
* `taplo-cli`: `cargo install --locked taplo-cli`

**Note on arm64:** ARM64 support for ring is [not released yet](https://github.com/briansmith/ring/issues/1167), so TLS features must be disabled until arm64 is officially supported by ring (probably in 0.17.0 release)

### Generating Documentation

* To compile and open documentation: `cargo doc --locked --open`
  * To include nightly features: `cargo +nightly doc --locked --open --features nightly`

### Policy on using Nightly/Unstable Features

#### In `lib` and `bin` targets

The crates in this repository are designed to work with `stable` rust. Some of the crates expose a `nightly` feature that adds additional functionality that requires unstable rust features in the `nightly` toolchains.

#### In `test` targets and unit tests

`test` targets and unit tests in other targets will automatically enable nightly features when a nightly toolchain is detected. This is done via the `nightly_toolchain` `cfg` value. This allows us to take advantage of unstable features (ex. [`assert_matches`](https://doc.rust-lang.org/std/assert_matches/macro.assert_matches.html)) in tests.

### Build and Test

To **only build** the workspace: `cargo build`

To **both** build and package the samples in the workspace: `cargo make --cwd .\crates\<driver-name>`

### Quality

To maintain the quality of code, tests and tools are required to pass before contributions are accepted. This is a suggested list of things that should be run before contributions will be accepted:

**_Functional Correctness:_**

* `cargo test --locked --workspace --exclude sample-*`
  * To test `nightly` features: `cargo +nightly test --locked --workspace --exclude sample-* --features nightly`

**_Static Analysis and Linting:_**

* `cargo clippy --locked --all-targets -- -D warnings`
  * To lint `nightly` features: `cargo +nightly clippy --locked --all-targets --features nightly -- -D warnings`

**_Formatting:_**

* Check for consistent `.rs` file formatting: `cargo +nightly fmt --all -- --check`
  * Running `cargo +nightly fmt --all` resolves these formatting inconsistencies usually
  * `+nightly` is required to use some `nightly` configuration features in [the `rustfmt.toml` config](./rustfmt.toml)
* Check for consistent `.toml` file formatting: `taplo fmt --check --diff`
  * Running `taplo fmt` resolves these formatting inconsistencies usually

**_Dependency Analysis:_**

* Scan for security advisories in dependent crates: `cargo audit --deny warnings`
* Scan for unused dependencies: `cargo +nightly udeps --locked --all-targets`
  * `cargo udeps` requires `nightly` to function

**_Rust Documentation Build Test_**

* `cargo doc --locked`
  * To build docs for `nightly` features: `cargo +nightly doc --locked --features nightly`

### A Note on Code-Style

Any bindings generated to C code maintains their original names, including their original style conventions(ex. PascalCase for functions). These bindings should all reside in `wdk-sys` and are marked as `unsafe` since all ffi is inherently `unsafe`. `wdk-sys` also retains manual implementations of wdk code (ex. because `bindgen` fails to resolve some macros). These should also maintain their original names and style.

Any Rust wrappers written around the bindings should follow [Rust style and naming conventions](https://rust-lang.github.io/api-guidelines/naming.html) per RFC-430. Any wrappers around the FFI bindings should also be written to guarantee safety. Refer to [this](https://doc.rust-lang.org/nomicon/ffi.html#creating-a-safe-interface) for more information on writing safe rust wrappers to ffi code.
