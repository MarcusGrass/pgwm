use crate::config::COLORS;
use core::fmt::Debug;

#[derive(Debug, Copy, Clone)]
pub struct Color {
    pub pixel: u32,
    pub bgra8: [u8; 4],
}

impl Color {
    #[must_use]
    #[inline]
    pub fn as_render_color(&self) -> xcb_rust_protocol::proto::render::Color {
        xcb_rust_protocol::proto::render::Color {
            red: convert_up(self.bgra8[2]),
            green: convert_up(self.bgra8[1]),
            blue: convert_up(self.bgra8[0]),
            alpha: convert_up(self.bgra8[3]),
        }
    }
}

impl Rgba8 for Color {
    fn to_rgba16(&self) -> (u16, u16, u16, u16) {
        (
            convert_up(self.bgra8[2]),
            convert_up(self.bgra8[1]),
            convert_up(self.bgra8[0]),
            convert_up(self.bgra8[3]),
        )
    }
}

pub trait Rgba8 {
    fn to_rgba16(&self) -> (u16, u16, u16, u16);
}

impl Rgba8 for (u8, u8, u8, u8) {
    fn to_rgba16(&self) -> (u16, u16, u16, u16) {
        (
            convert_up(self.0),
            convert_up(self.1),
            convert_up(self.2),
            convert_up(self.3),
        )
    }
}

pub type RGBA = (u8, u8, u8, u8);

/**
Color configuration, Here colors are set for different segments that the WM draws.
Naming is hopefully fairly self-explanatory for what each color does.
A constant can be declared as above for reuse.
 **/
#[derive(Debug, Copy, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ColorBuilder {
    pub window_border: RGBA,
    pub window_border_highlighted: RGBA,
    pub window_border_urgent: RGBA,
    pub workspace_bar_selected_unfocused_workspace_background: RGBA,
    pub workspace_bar_unfocused_workspace_background: RGBA,
    pub workspace_bar_focused_workspace_background: RGBA,
    pub workspace_bar_urgent_workspace_background: RGBA,
    pub workspace_bar_workspace_section_text: RGBA,
    pub workspace_bar_current_window_title_text: RGBA,
    pub workspace_bar_current_window_title_background: RGBA,
    pub status_bar_text: RGBA,
    pub status_bar_background: RGBA,
    pub tab_bar_text: RGBA,
    pub tab_bar_focused_tab_background: RGBA,
    pub tab_bar_unfocused_tab_background: RGBA,
    pub shortcut_text: RGBA,
    pub shortcut_background: RGBA,
}

pub struct Colors {
    pub inner: [Color; COLORS.len()],
}

impl Colors {
    #[inline]
    #[must_use]
    pub const fn window_border(&self) -> Color {
        self.inner[0]
    }
    #[inline]
    #[must_use]
    pub const fn window_border_highlighted(&self) -> Color {
        self.inner[1]
    }
    #[inline]
    #[must_use]
    pub const fn window_border_urgent(&self) -> Color {
        self.inner[2]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_selected_unfocused_workspace_background(&self) -> Color {
        self.inner[3]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_unfocused_workspace_background(&self) -> Color {
        self.inner[4]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_focused_workspace_background(&self) -> Color {
        self.inner[5]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_urgent_workspace_background(&self) -> Color {
        self.inner[6]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_workspace_section_text(&self) -> Color {
        self.inner[7]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_current_window_title_text(&self) -> Color {
        self.inner[8]
    }
    #[inline]
    #[must_use]
    pub const fn workspace_bar_current_window_title_background(&self) -> Color {
        self.inner[9]
    }
    #[inline]
    #[must_use]
    pub const fn status_bar_text(&self) -> Color {
        self.inner[10]
    }
    #[inline]
    #[must_use]
    pub const fn status_bar_background(&self) -> Color {
        self.inner[11]
    }
    #[inline]
    #[must_use]
    pub const fn tab_bar_text(&self) -> Color {
        self.inner[12]
    }
    #[inline]
    #[must_use]
    pub const fn tab_bar_focused_tab_background(&self) -> Color {
        self.inner[13]
    }
    #[inline]
    #[must_use]
    pub const fn tab_bar_unfocused_tab_background(&self) -> Color {
        self.inner[14]
    }
    #[inline]
    #[must_use]
    pub const fn shortcut_text(&self) -> Color {
        self.inner[15]
    }
    #[inline]
    #[must_use]
    pub const fn shortcut_background(&self) -> Color {
        self.inner[16]
    }
}

const fn convert_up(v: u8) -> u16 {
    v as u16 * 256
}
