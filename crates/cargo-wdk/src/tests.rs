use std::sync::Mutex;

// Serialize tests in this module to avoid concurrent global mock expectation overrides
// due to the use of associated functions for providers.
pub static TEST_MUTEX: Mutex<()> = Mutex::new(());
