# Check Cargo Lockfiles

Checks that tracked `Cargo.lock` files are consistent with their `Cargo.toml` manifests using `cargo +<toolchain> metadata --locked --all-features --format-version 1`. Each check runs from the lockfile's directory to honor local `.cargo/config.toml` settings and verifies that validation did not change the lockfile. Valid uncommitted lockfile updates are allowed.

This checks manifest/lockfile agreement, not identical dependency versions across workspaces. The consuming repository must have a tracked root `Cargo.lock`; nested lockfiles are discovered from Git's tracked files.

## Workflow usage

The action requires Git, PowerShell 7 or later, and an already-installed Rust toolchain. The calling workflow owns toolchain installation:

```yaml
- name: Install Rust Toolchain (Stable)
  uses: dtolnay/rust-toolchain@e2a55d2ffb04f378e9626c28d38b36d230d1e12f # master
  with:
    toolchain: stable

- name: Check Cargo lockfiles
  uses: ./.github/actions/check-cargo-lockfiles
```

Run the action after checking out the consuming repository and before build or cargo-make commands. Cargo-make can refresh stale lockfiles during startup, before any task executes.

The checker script is bundled with the action, so repositories that copy `.github/actions` can reuse it without copying other repository files. It checks the consuming checkout, not the action's source repository.

## Inputs

| Input | Default | Meaning |
| --- | --- | --- |
| `lockfile` | All tracked lockfiles | One repository-relative tracked `Cargo.lock` path, or omit it to check all tracked lockfiles. |
| `toolchain` | `stable` | An installed Rust toolchain specifier, such as `1.93`, a channel, or a dated and host-qualified nightly. |

## Direct script usage

The script can reproduce a single check without WDK initialization. Install the requested toolchain first:

```shell
pwsh -NoProfile -File .github/actions/check-cargo-lockfiles/check-lockfiles.ps1 -Lockfile tests/config-kmdf/Cargo.lock -Toolchain 1.93
```

The script's `-List` mode emits the tracked paths as a JSON array for workflow discovery without invoking Cargo.
