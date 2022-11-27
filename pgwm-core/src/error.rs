use core::fmt::{Debug, Formatter};

use tiny_std::error::Error as StdError;

use pgwm_utils::from_error;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    InvalidDrawMode,
    HeaplessPush(usize),
    HeaplessIntoArray,
    HeaplessInstantiate,
    Syscall(StdError),
    #[cfg(feature = "status-bar")]
    Check,
    #[cfg(feature = "config-file")]
    ConfigDirFind,
    #[cfg(feature = "config-file")]
    ConfigFileFind,
    #[cfg(feature = "config-file")]
    ConfigParse(toml::de::Error),
    #[cfg(feature = "config-file")]
    ConfigLogic(&'static str),
    #[cfg(feature = "config-file")]
    ConfigP(alloc::string::String),
    #[cfg(feature = "status-bar")]
    ParseFloat(core::num::ParseFloatError),
    #[cfg(feature = "status-bar")]
    ParseInt(core::num::ParseIntError),
    #[cfg(feature = "status-bar")]
    BatParseError,
    #[cfg(feature = "status-bar")]
    MemParseError(&'static str),
    #[cfg(feature = "status-bar")]
    Utf8Convert(alloc::string::FromUtf8Error),
    #[cfg(feature = "status-bar")]
    NetStatParseError,
    #[cfg(feature = "status-bar")]
    ProcStatParseError,
    #[cfg(feature = "status-bar")]
    FontLoad(alloc::string::String),
    #[cfg(feature = "status-bar")]
    Time(alloc::string::String),
}
from_error!(StdError, Error, Syscall);
#[cfg(feature = "config-file")]
from_error!(toml::de::Error, Error, ConfigParse);
#[cfg(feature = "status-bar")]
from_error!(core::num::ParseFloatError, Error, ParseFloat);
#[cfg(feature = "status-bar")]
from_error!(core::num::ParseIntError, Error, ParseInt);
#[cfg(feature = "status-bar")]
from_error!(alloc::string::FromUtf8Error, Error, Utf8Convert);

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::InvalidDrawMode => f.write_str("Failed to save old draw mode when going back to from fullscreen (this is a programming error)"),
            Error::HeaplessPush(u) => f.write_fmt(format_args!("Failed to push to heapless datastructure, limit was {u}")),
            Error::HeaplessIntoArray => f.write_str("Failed to convert heapless vec into array"),
            Error::HeaplessInstantiate => f.write_str("Heapless instantiation impossible, too many items for heapless max_size"),
            #[cfg(feature = "status-bar")]
            Error::Check => f.write_str("Channel error an check"),
            #[cfg(feature = "config-file")]
            Error::ConfigDirFind => f.write_str("Failed to find appropriate user config directory after searching environment variables $XDG_CONFIG_HOME falling back to $HOME/.config"),
            #[cfg(feature = "config-file")]
            Error::ConfigFileFind => f.write_str("Managed to find user config directory but not a config file"),
            #[cfg(feature = "config-file")]
            Error::ConfigParse(e) => f.write_fmt(format_args!("Failed to parse config {e}")),
            #[cfg(feature = "config-file")]
            Error::ConfigLogic(e) => f.write_fmt(format_args!("Invalid configuration {e}")),
            #[cfg(feature = "config-file")]
            Error::ConfigP(e) => f.write_str(e),
            #[cfg(feature = "status-bar")]
            Error::ParseFloat(e) => core::fmt::Display::fmt(e, f),
            #[cfg(feature = "status-bar")]
            Error::ParseInt(e) => core::fmt::Display::fmt(e, f),
            #[cfg(feature = "status-bar")]
            Error::BatParseError => f.write_str("Failed to parse bat info"),
            #[cfg(feature = "status-bar")]
            Error::MemParseError(r) => f.write_fmt(format_args!("Failed to parse mem_info, reason = {r}")),
            #[cfg(feature = "status-bar")]
            Error::Utf8Convert(e) => f.write_fmt(format_args!("Failed to convert bytes to utf8 string {e}")),
            #[cfg(feature = "status-bar")]
            Error::NetStatParseError => f.write_str("Failed to parse netstat, could not find in/out"),
            #[cfg(feature = "status-bar")]
            Error::ProcStatParseError => f.write_str("Failed to parse proc stat, could not find aggregate CPU"),
            #[cfg(feature = "status-bar")]
            Error::Time(t) => f.write_fmt(format_args!("Failed to format time {t}")),
            #[cfg(feature = "status-bar")]
            Error::FontLoad(s) => f.write_fmt(format_args!("Error loading render {s}")),
            Error::Syscall(e) => f.write_fmt(format_args!("Syscall error {e}")),
        }
    }
}
