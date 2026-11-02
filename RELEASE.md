## Release Process

The release process for this repository leverages [`release-plz`](https://github.com/MarcoIeni/release-plz) to automate some of the steps. Only maintainers with the necessary permissions can release new versions of this repository.

1. Install `release-plz`: One option is by running `cargo install --locked release-plz`
   1. Ensure you are running the latest version of `release-plz`
1. Install `cargo-semver-checks`: One option is by running `cargo install --locked cargo-semver-checks`
   1. Ensure you are running the latest version of `cargo-semver-checks`    
1. Checkout the latest code on `main` branch
1. From the repository root, set `$env:CARGO_TARGET_DIR = './target'` (or an absolute path to that directory). This overrides `release-plz`'s [temporary target directory](https://github.com/release-plz/release-plz/pull/3243), since windows-drivers-rs [does not yet support custom target directories outside the workspace](https://github.com/microsoft/windows-drivers-rs/issues/252).
1. Create a release PR: `release-plz release-pr --git-token <Github Token>`
1. In the Pull Request:
   1. Check that the new versions automatically determined by `release-plz` make sense
      1. In the PR description, if there are API-breaking changes detected for a crate, make sure that that crate shows a major semver bump
      1. If no API-breaking changes are detected, [conventional commits](https://www.conventionalcommits.org/) will be used to determine what type of version bump it should be. This means that `release-plz` may say there are no API-breaking changes detected, but there may still be a major version bump if the commit titles suggest there is a change that warrants it.
      1. If the versions are not what you expect, you can manually override them by running `release-plz set-version <new semver version>`. This will update the package versions and the changelogs, but the PR description may need manual editing.
1. Checkout the generated release branch locally in order to:
   1. Update all docs to reflect new versions
   1. Update all `cargo-make` files to use the new versions (ex. for `wdk-build` in [rust-driver-makefile.toml](./crates/wdk-build/rust-driver-makefile.toml))
   1. Refresh the independent `Cargo.lock` files for the [example drivers](./examples/), [workspace-level tests](./tests/), and [cargo-wdk test fixtures](./crates/cargo-wdk/tests/). These use local path dependencies but are outside the root Cargo workspace, so `release-plz` does not update their lockfiles.
   1. Update the dependency versions in all [cargo-wdk templates](./crates/cargo-wdk/templates/) to match the workspace versions; CI checks this consistency. The templates use crates.io dependencies, so CI sets `SKIP_BUILD_IN_CARGO_WDK_NEW_TESTS=1` for the release branch's PR, push, and merge queue runs to skip building generated drivers until those versions are published. Project-creation checks still run.
   1. Check that the release notes are correct and edit as needed
1. Do a last sanity check of all of the [example drivers](./examples/) (i.e. install and verify that all logs are as expected)
1. Run `release-plz release --dry-run` to check that the release will be successful
1. Merge the release Pull Request
1. Run `release-plz release`
   1. This will release the crates to crates.io and create draft releases on Github
1. Re-run the failed `Test` and `Local Development Makefile` workflow runs for the release merge commit on `main`. They build generated drivers against crates.io, so they fail until the release is published.
1. Publish all the Github releases
