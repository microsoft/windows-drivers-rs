use std::path::{Path, PathBuf};

use lazy_static::lazy_static;

lazy_static! {
    static ref TESTS_FOLDER_PATH: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests"].iter().collect();
    static ref TRYBUILD_FOLDER_PATH: PathBuf = TESTS_FOLDER_PATH.join("trybuild");
}

mod macro_usage_errors {
    use super::*;

    /// This test leverages `trybuild` to ensure that developer misuse of
    /// the macro cause compilation failures, with an appropriate message
    #[test]
    fn trybuild() {
        trybuild::TestCases::new().compile_fail(
            // canonicalization of this path causes a bug in `glob`: https://github.com/rust-lang/glob/issues/132
            TRYBUILD_FOLDER_PATH // .canonicalize()?
                .join("*.rs"),
        );
    }
}
