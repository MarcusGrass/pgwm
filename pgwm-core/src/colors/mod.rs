use std::fmt::Debug;

use crate::config::USED_DIFFERENT_COLOR_SEGMENTS;

#[derive(Debug, Copy, Clone)]
pub struct Color {
    pub pixel: u32,
    pub bgra8: [u8; 4],
}

impl Color {
    #[must_use]
    pub fn as_render_color(&self) -> x11rb::protocol::render::Color {
        x11rb::protocol::render::Color {
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

macro_rules! impl_colors {
    ( $( $color_name:ident, $index:literal ),* ) => {
        #[derive(Default)]
        #[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
        #[cfg_attr(test, derive(Eq, PartialEq))]
        #[derive(Copy, Clone, Debug)]
        pub struct ColorBuilder {
            $(
            $color_name: (u8, u8, u8, u8),
            )*
        }
        impl ColorBuilder {
            $(
            #[must_use]
            pub fn $color_name(mut self, value: (u8, u8, u8, u8)) -> Self {
                self.$color_name = value;
                self
            }
            )*
            #[must_use]
            pub fn by_index(&self, ind: usize) -> Option<&(u8, u8, u8, u8)> {
                match ind {
                    $(
                    $index => Some(&self.$color_name),
                    )*
                    _ => None
                }
            }
            #[must_use]
            pub fn get_all(&self) -> heapless::Vec<&(u8, u8, u8, u8), USED_DIFFERENT_COLOR_SEGMENTS> {
                let mut all = heapless::Vec::new();
                $(
                    let _ = all.push(&self.$color_name);
                )*
                all
            }
        }
        #[derive(Copy, Clone, Debug)]
        pub struct Colors {
            $(
                pub $color_name: Color,
            )*
        }
        impl Colors {
            #[must_use]
            pub fn from_vec(source: heapless::Vec<Color, USED_DIFFERENT_COLOR_SEGMENTS>) -> Self {
                Self {
                    $(
                    $color_name: source[$index],
                    )*
                }
            }
            $(
                #[must_use]
                pub fn $color_name(&self) -> &Color {
                    &self.$color_name
                }
            )*

            #[must_use]
            pub fn get_all(&self) -> heapless::Vec<Color, USED_DIFFERENT_COLOR_SEGMENTS> {
                let mut all = heapless::Vec::new();
                $(
                    let _ = all.push(self.$color_name);
                )*
                all
            }
        }
    };
}

impl_colors!(
    window_border,
    0,
    window_border_highlighted,
    1,
    window_border_urgent,
    2,
    workspace_bar_selected_unfocused_workspace_background,
    3,
    workspace_bar_unfocused_workspace_background,
    4,
    workspace_bar_focused_workspace_background,
    5,
    workspace_bar_urgent_workspace_background,
    6,
    workspace_bar_workspace_section_text,
    7,
    workspace_bar_current_window_title_text,
    8,
    workspace_bar_current_window_title_background,
    9,
    status_bar_text,
    10,
    status_bar_background,
    11,
    tab_bar_focused_tab_background,
    12,
    tab_bar_unfocused_tab_background,
    13,
    tab_bar_text,
    14,
    shortcut_background,
    15,
    shortcut_text,
    16
);

const fn convert_up(v: u8) -> u16 {
    v as u16 * 256
}
