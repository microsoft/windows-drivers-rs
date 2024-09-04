# wdk-macros-tests

This crate allows for testing the `wdk-macros` crate, specifically containing tests for macro expansion and error handling. The tests are written using the `macrotest` and `trybuild` crates, and executed for specific wdk configurations in the [`config-kmdf`](./config-kmdf/) and [`config-umdf`](./config-umdf/) crate tests.

## Tests

In order to update the tests in [`macrotest` folder](./tests/inputs/macrotest/) due to a change in the macro expansion, refer to [this section in the macrotest documentation](https://docs.rs/macrotest/latest/macrotest/#updating-expandedrs).

In order to update the tests in [`trybuild` folder](./tests/inputs/trybuild/) due to a change in the error handling, refer to [this section in the trybuild documentation](https://docs.rs/trybuild/latest/trybuild/#workflow).
