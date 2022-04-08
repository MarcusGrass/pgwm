use crate::error::Result;
use pgwm_core::colors::{Color, Colors, Rgba8};
use pgwm_core::config::USED_DIFFERENT_COLOR_SEGMENTS;
use pgwm_core::push_heapless;
use x11rb::cookie::Cookie;

use x11rb::protocol::xproto::{AllocColorReply, Colormap, ConnectionExt};
use x11rb::rust_connection::RustConnection;

#[allow(clippy::type_complexity)]
pub(crate) fn alloc_colors(
    connection: &RustConnection,
    color_map: Colormap,
    colors: pgwm_core::colors::ColorBuilder,
) -> Result<Colors> {
    pgwm_core::debug!("Allocating colors {colors:?}");
    let mut alloc_rgba_cookies: heapless::Vec<
        ((u8, u8, u8, u8), Cookie<RustConnection, AllocColorReply>),
        USED_DIFFERENT_COLOR_SEGMENTS,
    > = heapless::Vec::new();
    for color in colors.get_all().iter() {
        let color = **color;
        let (r, g, b, _) = color.to_rgba16();
        push_heapless!(
            alloc_rgba_cookies,
            (color, connection.alloc_color(color_map, r, g, b)?)
        )?;
    }
    let mut allocated_colors: heapless::CopyVec<Color, USED_DIFFERENT_COLOR_SEGMENTS> =
        heapless::CopyVec::new();
    for (rgba8, cookie) in alloc_rgba_cookies {
        push_heapless!(
            allocated_colors,
            Color {
                pixel: cookie.reply()?.pixel,
                rgba8
            }
        )?;
    }
    Ok(Colors::from_vec(allocated_colors))
}
