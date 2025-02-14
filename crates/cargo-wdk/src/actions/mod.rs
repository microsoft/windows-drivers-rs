use std::fmt;

mod build;
pub mod new;
pub mod package;

pub enum DriverType {
    KMDF,
    UMDF,
    WDM,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DriverType::KMDF => "kmdf",
            DriverType::UMDF => "umdf",
            DriverType::WDM => "wdm",
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
    X86_64,
    Aarch64,
}

impl ToString for TargetArch {
    fn to_string(&self) -> String {
        match self {
            TargetArch::X86_64 => "x86_64".to_string(),
            TargetArch::Aarch64 => "aarch64".to_string(),
        }
    }
}
