use std::fmt;

pub mod build;
pub mod new;
pub mod package;

#[derive(Debug, Clone)]
pub enum DriverType {
    Kmdf,
    Umdf,
    Wdm,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DriverType::Kmdf => "kmdf",
            DriverType::Umdf => "umdf",
            DriverType::Wdm => "wdm",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub enum Profile {
    Debug,
    Release,
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub enum TargetArch {
    X64,
    Arm64,
}

impl ToString for TargetArch {
    fn to_string(&self) -> String {
        match self {
            TargetArch::X64 => "x86_64".to_string(),
            TargetArch::Arm64 => "aarch64".to_string(),
        }
    }
}
