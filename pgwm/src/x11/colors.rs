use crate::error::Result;
use pgwm_core::colors::{Color, Colors, Rgba8};
use pgwm_core::config::USED_DIFFERENT_COLOR_SEGMENTS;
use pgwm_core::push_heapless;
use x11rb::cookie::Cookie;

use crate::wm::XorgConnection;
use x11rb::protocol::xproto::{AllocColorReply, Colormap};
use x11rb::xcb::xproto;

#[allow(clippy::type_complexity)]
pub(crate) fn alloc_colors(
    connection: &mut XorgConnection,
    color_map: Colormap,
    colors: pgwm_core::colors::ColorBuilder,
) -> Result<Colors> {
    let mut alloc_rgba_cookies: heapless::Vec<
        ((u8, u8, u8, u8), Cookie<AllocColorReply>),
        USED_DIFFERENT_COLOR_SEGMENTS,
    > = heapless::Vec::new();
    for color in colors.get_all().iter() {
        let color = **color;
        let (r, g, b, _) = color.to_rgba16();
        push_heapless!(
            alloc_rgba_cookies,
            (
                color,
                xproto::alloc_color(connection, color_map, r, g, b, false)?
            )
        )?;
    }
    eprintln!("Sent alloc for {} colors", alloc_rgba_cookies.len());
    let mut allocated_colors: heapless::Vec<Color, USED_DIFFERENT_COLOR_SEGMENTS> =
        heapless::Vec::new();
    for ((r, g, b, a), cookie) in alloc_rgba_cookies {
        eprintln!("checking alloc color for seq {}", cookie.sequence_number());
        push_heapless!(
            allocated_colors,
            Color {
                pixel: cookie.reply(connection)?.pixel,
                bgra8: [b, g, r, a]
            }
        )?;
    }
    Ok(Colors::from_vec(allocated_colors))
}
