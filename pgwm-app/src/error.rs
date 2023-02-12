use alloc::ffi::NulError;
use alloc::string::{FromUtf8Error, String};
use core::fmt::Formatter;

use rusl::Error as RuslError;
use tiny_std::error::Error as StdError;
use xcb_rust_connection::{ConnectError, ConnectionError};

use pgwm_utils::from_error;

pub(crate) type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub(crate) enum Error {
    Core(pgwm_core::error::Error),
    X11Connect(ConnectError),
    X11Connection(ConnectionError),
    XcbProto(xcb_rust_protocol::Error),
    X11EventParse,
    GlyphMismatch,
    BecomeWm,
    Tiling,
    NoAppropriateVisual,
    ContentToCstr(NulError),
    ConvertToUtf8(FromUtf8Error),
    ConvertCoreToUtf8(core::str::Utf8Error),
    StateInvalidated,
    GracefulShutdown,
    FullRestart,
    ParseFloat,
    FontLoad(&'static str),
    BadCharFontMapping(&'static str),
    Uring(String),
    Syscall(StdError),
    Rusl(RuslError),
}
from_error!(pgwm_core::error::Error, Error, Core);
from_error!(ConnectError, Error, X11Connect);
from_error!(ConnectionError, Error, X11Connection);
from_error!(xcb_rust_protocol::Error, Error, XcbProto);
from_error!(NulError, Error, ContentToCstr);
from_error!(FromUtf8Error, Error, ConvertToUtf8);
from_error!(core::str::Utf8Error, Error, ConvertCoreToUtf8);
from_error!(StdError, Error, Syscall);
from_error!(RuslError, Error, Rusl);

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Core(e) => core::fmt::Display::fmt(e, f),
            Error::X11Connect(e) => core::fmt::Display::fmt(e, f),
            Error::X11Connection(e) => core::fmt::Display::fmt(e, f),
            Error::XcbProto(e) => core::fmt::Display::fmt(e, f),
            Error::X11EventParse => f.write_str("Failed to parse event"),
            Error::GlyphMismatch => {
                f.write_str("Number of glyph ids not corresponding to number of metrics")
            }
            Error::BecomeWm => f.write_str(
                "Could not become wm, access denied, there is likely another WM running",
            ),
            Error::Tiling => f.write_str(
                "Failed to calculate correct tiling dimensions (this is a programming error)",
            ),
            Error::NoAppropriateVisual => {
                f.write_str("Failed to find an appropriate 32 bit depth visual")
            }
            Error::ContentToCstr(e) => core::fmt::Display::fmt(e, f),
            Error::ConvertToUtf8(e) => core::fmt::Display::fmt(e, f),
            Error::ConvertCoreToUtf8(e) => core::fmt::Display::fmt(e, f),
            Error::StateInvalidated => f.write_str("State Invalidated"),
            Error::GracefulShutdown => f.write_str("Exit triggered"),
            Error::FullRestart => f.write_str("Restart triggered"),
            Error::ParseFloat => f.write_str("Size not parseable as f32"),
            Error::FontLoad(s) => f.write_fmt(format_args!("Failed to load font {s}")),
            Error::BadCharFontMapping(s) => {
                f.write_fmt(format_args!("Invalid char to font mapping {s}"))
            }
            Error::Syscall(e) => f.write_fmt(format_args!("Syscall error {e}")),
            Error::Rusl(e) => f.write_fmt(format_args!("Rusl error {e}")),
            Error::Uring(e) => f.write_fmt(format_args!("Uring error {e}")),
        }
    }
}
