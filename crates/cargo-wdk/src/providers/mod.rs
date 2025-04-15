//! The `providers` module serves as a centralized abstraction layer for various
//! subsystems used throughout the application. It encapsulates functionality
//! such as file system operations, command execution, metadata handling, and
//! interactions with the `wdk-build` crate. By consolidating these external
//! dependencies, the module promotes cleaner separation of concerns and
//! enhances testability. This design allows external calls to be easily mocked,
//! simplifying unit testing and enabling more robust and maintainable code in
//! the action layer.

pub mod exec;
pub mod fs;
pub mod metadata;
pub mod wdk_build;

// Error definitions for the providers module
pub mod error;
