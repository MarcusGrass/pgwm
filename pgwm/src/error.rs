use std::io;
use std::{ffi::NulError, string::FromUtf8Error};

use x11rb::errors::{ConnectError, ConnectionError, ReplyError, ReplyOrIdError};

pub(crate) type Result<T> = std::result::Result<T, Error>;
#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error(transparent)]
    Core(#[from] pgwm_core::error::Error),
    #[error(transparent)]
    X11Connect(#[from] ConnectError),
    #[error(transparent)]
    X11Connection(#[from] ConnectionError),
    #[error(transparent)]
    X11Reply(#[from] ReplyError),
    #[error(transparent)]
    X11IdCreation(#[from] ReplyOrIdError),
    #[error("Number of glyph ids not corresponding to number of metrics")]
    GlyphMismatch,
    #[error("Could not become wm, access denied, there is likely another WM running")]
    BecomeWm,
    #[error("Failed to calculate correct tiling dimensions (this is a programming error)")]
    Tiling,
    #[error("Failed to find an appropriate 32 bit depth visual")]
    NoAppropriateVisual,
    #[error(transparent)]
    ContentToCstr(#[from] NulError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    NixPoll(#[from] nix::Error),
    #[error(transparent)]
    ConvertToUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    ConvertCoreToUtf8(#[from] core::str::Utf8Error),
    #[error("StateInvalidated")]
    StateInvalidated,
    #[error("Exit triggered")]
    GracefulShutdown,
    #[error("Size not parseable as f32")]
    ParseFloat,
    #[error("Failed to load font {0}")]
    FontLoad(&'static str),
    #[error("Invalid char to font mapping {0}")]
    BadCharFontMapping(&'static str),
}
