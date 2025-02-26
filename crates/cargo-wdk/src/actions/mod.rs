use std::fmt;

/// Business logic is divided into the following action modules
/// * `new` - New action module
/// * `build` - Build action module
/// * `package` - Package action module
pub mod build;
pub mod new;
pub mod package;

/// `DriverType` for the action layer
#[derive(Debug, Clone)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Kmdf => "kmdf",
            Self::Umdf => "umdf",
            Self::Wdm => "wdm",
        };
        write!(f, "{s}")
    }
}

/// `Profile` for the action layer
#[derive(Debug, Clone, Copy)]
pub enum Profile {
    Debug,
    Release,
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Debug => "debug",
            Self::Release => "release",
        };
        write!(f, "{s}")
    }
}

/// `TargetArch` for the action layer
#[derive(Debug, Clone, Copy)]
pub enum TargetArch {
    X64,
    Arm64,
}

impl ToString for TargetArch {
    fn to_string(&self) -> String {
        match self {
            Self::X64 => "x86_64".to_string(),
            Self::Arm64 => "aarch64".to_string(),
        }
    }
}
