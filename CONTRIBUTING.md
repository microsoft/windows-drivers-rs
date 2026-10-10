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

* `cargo-audit`: `cargo install --locked cargo-audit`
* `cargo-expand`: `cargo install --locked cargo-expand --version 1.0.85`
* `cargo-machete`: `cargo install --locked cargo-machete`
* `taplo-cli`: `cargo install --git https://github.com/tamasfe/taplo --rev b673b44d taplo-cli --locked --force`
* `typos-cli`: `cargo install --locked typos-cli`

**Note on arm64:** ARM64 support for ring is [not released yet](https://github.com/briansmith/ring/issues/1167), so TLS features must be disabled until arm64 is officially supported by ring (probably in 0.17.0 release)

### Generating Documentation

* To compile and open documentation: `cargo doc --locked --all-features --open`
  * To open docs for non-host architecture: `cargo doc --locked --all-features --open --target <target-triple> --workspace --exclude wdk-macros`
    * `--target` is incompatible with `proc-macro` crates due to a [cargo bug](https://github.com/rust-lang/cargo/issues/10368)

### Policy on using Nightly/Unstable Features

#### In `lib` and `bin` targets

The crates in this repository are designed to work with `stable` rust. Some of the crates expose a `nightly` feature that adds additional functionality that requires unstable rust features in the `nightly` toolchains.

### Build and Test

To **only build** the workspace: `cargo build --locked --all-features`

To **both** build and package the samples in the workspace: `cargo make --cwd .\crates\<driver-name>`

### Quality

To maintain the quality of code, tests and tools are required to pass before contributions are accepted. This is a suggested list of things that should be run before contributions will be accepted:

**_Functional Correctness:_**

* `cargo test --locked --all-features`
  * To test `nightly` features: `cargo +nightly test --locked --features nightly`

**_Static Analysis and Linting:_**

* `cargo clippy --locked --all-features --all-targets -- -D warnings`
  * To lint `nightly` features: `cargo +nightly clippy --locked --all-features --all-targets --features nightly -- -D warnings`

**_Formatting:_**

* Check for consistent `.rs` file formatting: `cargo +nightly fmt --all -- --check`
  * Running `cargo +nightly fmt --all` resolves these formatting inconsistencies usually
  * `+nightly` is required to use some `nightly` configuration features in [the `rustfmt.toml` config](./rustfmt.toml)
* Check for consistent `.toml` file formatting and sorted dependencies: `taplo fmt --check --diff`
  * Running `taplo fmt` resolves these formatting inconsistencies usually

**_Spelling (typos-cli):_**

* Check spelling with typos-cli: `typos`
  * Auto-fix locally before committing: `typos --write-changes`
  * CI runs `typos` on every PR to `main` and will fail on finding any typos
  * Configuration lives in `.typos.toml`; we allowlist specific Windows API identifiers flagged by typos to preserve upstream names and binding compatibility (no file-level excludes)

**_Dependency Analysis:_**

* Check lockfile consistency before running build or cargo-make commands: `pwsh -NoProfile -File .github/actions/check-cargo-lockfiles/check-lockfiles.ps1`
  * This checks every tracked `Cargo.lock` from its own directory so Cargo loads that project's `.cargo/config.toml`. Valid uncommitted lockfile updates are allowed; validation must not modify them.
  * The Lockfile Consistency Checks workflow runs a separate job for each tracked lockfile on stable, beta, and nightly, with a fail-closed summary check for branch protection.
  * The repository-only `cargo make check-lockfiles` task also runs this checker and is included in `wdk-pre-commit-flow`. Run the script directly for a preflight check because cargo-make's startup metadata queries can refresh stale lockfiles before any task executes. CI runs the action before invoking cargo-make for the same reason.
  * To repair a stale lockfile, run `cargo +stable update --workspace` from its directory and commit the updated lockfile.
  * This checks that manifests and lockfiles agree; it does not require every workspace to use the same dependency versions.
* Scan for security advisories in dependent crates: `cargo audit --deny warnings`
* Scan for unused dependencies: `cargo machete --skip-target-dir`

**_Rust Documentation Build Test_**

* `cargo doc --locked --all-features`

### Reusing the Lockfile Check

The `check-cargo-lockfiles` composite action contains its checker script, so repositories that copy `.github/actions` from windows-drivers-rs can use it without copying other repository files:

```yaml
- name: Install Rust Toolchain (Stable)
  uses: dtolnay/rust-toolchain@e2a55d2ffb04f378e9626c28d38b36d230d1e12f # master
  with:
    toolchain: stable

- name: Check Cargo lockfiles
  uses: ./.github/actions/check-cargo-lockfiles
```

Run it after checking out the consuming repository, copying the shared actions, and installing the selected Rust channel, before build or cargo-make commands. The action does not install toolchains. By default, it checks every tracked lockfile on stable in the consuming checkout, not the action's source repository. The checker requires a tracked root `Cargo.lock`.

The action's `lockfile` input selects one repository-relative tracked path, and `toolchain` selects `stable`, `beta`, or `nightly`. The repository workflow uses both inputs for its lockfile-by-channel matrix. To reproduce an individual check locally:

```shell
pwsh -NoProfile -File .github/actions/check-cargo-lockfiles/check-lockfiles.ps1 -Lockfile tests/config-kmdf/Cargo.lock -Toolchain beta
```

The script's `-List` mode emits the tracked paths as a JSON array for workflow discovery without invoking Cargo.

### A Note on Code-Style

Any bindings generated to C code maintains their original names, including their original style conventions(ex. PascalCase for functions). These bindings should all reside in `wdk-sys` and are marked as `unsafe` since all ffi is inherently `unsafe`. `wdk-sys` also retains manual implementations of wdk code (ex. because `bindgen` fails to resolve some macros). These should also maintain their original names and style.

Any Rust wrappers written around the bindings should follow [Rust style and naming conventions](https://rust-lang.github.io/api-guidelines/naming.html) per RFC-430. Any wrappers around the FFI bindings should also be written to guarantee safety. Refer to [this](https://doc.rust-lang.org/nomicon/ffi.html#creating-a-safe-interface) for more information on writing safe rust wrappers to ffi code.
