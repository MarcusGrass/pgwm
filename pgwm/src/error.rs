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
    #[error("Could not become wm, access denied, there is likely another WM running")]
    BecomeWm,
    #[error("Failed to calculate correct tiling dimensions (this is a programming error)")]
    Tiling,
    #[error(transparent)]
    Xft(#[from] XftError),
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
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum XftError {
    #[error("Failed to alloc color {0:?} by rgb")]
    AllocColorByRgb(String),
    #[error("Failed to load font {0}")]
    LoadFont(String),
    #[error("Failed to get glyph info")]
    GetGlyphInfo,
    #[error("Failed to create xft draw")]
    CreateDraw,
    #[error("Failed to open display")]
    OpenDisplay,
}
