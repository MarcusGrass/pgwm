pub type Result<T> = std::result::Result<T, Error>;
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to save old draw mode when going back to from fullscreen (this is a programming error)")]
    InvalidDrawMode,
    #[error("Failed to push to heapless datastructure, limit was {0}")]
    HeaplessPush(usize),
    #[error("Failed to convert heapless vec into array")]
    HeaplessIntoArray,
    #[error("Heapless instantiation impossible, too many items for heapless max_size")]
    HeaplessInstantiate,
    #[cfg(feature = "status-bar")]
    #[error("Channel error an check")]
    Check,
    #[cfg(feature = "config-file")]
    #[error("Failed to find appropriate user config directory after searching environment variables $XDG_CONFIG_HOME falling back to $HOME/.config")]
    ConfigDirFind,
    #[cfg(feature = "config-file")]
    #[error("Managed to find user config directory but not a config file")]
    ConfigFileFind,
    #[cfg(any(feature = "config-file", feature = "status-bar"))]
    #[error("Failed to read config from disk")]
    Io(#[from] std::io::Error),
    #[cfg(feature = "config-file")]
    #[error("Failed to parse config")]
    ConfigParse(#[from] toml::de::Error),
    #[cfg(feature = "config-file")]
    #[error("Unsound configuration: Reason {0}")]
    ConfigLogic(&'static str),
    #[cfg(feature = "status-bar")]
    #[error(transparent)]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[cfg(feature = "status-bar")]
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[cfg(feature = "status-bar")]
    #[error("Failed to parse bat info")]
    BatParseError,
    #[cfg(feature = "status-bar")]
    #[error("Failed to parse mem_info, reason = {0}")]
    MemParseError(&'static str),
    #[cfg(feature = "status-bar")]
    #[error("Failed to convert bytes to utf8 string")]
    Utf8Convert(#[from] std::string::FromUtf8Error),
    #[cfg(feature = "status-bar")]
    #[error("Failed to parse netstat, could not find in/out")]
    NetStatParseError,
    #[cfg(feature = "status-bar")]
    #[error("Failed to parse proc stat, could not find aggregate CPU")]
    ProcStatParseError,
    #[error("Error loading render {0}")]
    FontLoad(String),
}
