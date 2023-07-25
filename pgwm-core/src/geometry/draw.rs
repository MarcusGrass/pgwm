use xcb_rust_protocol::proto::xproto::Window;

use crate::error::Error;
use crate::error::Result;
use crate::geometry::layout::Layout;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Mode {
    Tiled(Layout),
    Tabbed(usize),
    Fullscreen {
        window: Window,
        last_draw_mode: OldDrawMode,
    },
}

// Infinite cycles... this is fine...
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OldDrawMode {
    Tiled(Layout),
    Tabbed(usize),
}

impl OldDrawMode {
    #[must_use]
    pub fn to_draw_mode(self) -> Mode {
        match self {
            OldDrawMode::Tiled(layout) => Mode::Tiled(layout),
            OldDrawMode::Tabbed(n) => Mode::Tabbed(n),
        }
    }
    pub fn from_draw_mode(draw_mode: Mode) -> Result<Self> {
        match draw_mode {
            Mode::Tiled(layout) => Ok(OldDrawMode::Tiled(layout)),
            Mode::Tabbed(win_ind) => Ok(OldDrawMode::Tabbed(win_ind)),
            Mode::Fullscreen { .. } => Err(Error::InvalidDrawMode),
        }
    }
}
