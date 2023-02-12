use xcb_rust_protocol::connection::xproto::alloc_color;
use xcb_rust_protocol::cookie::FixedCookie;
use xcb_rust_protocol::proto::xproto::{AllocColorReply, Colormap};

use pgwm_core::colors::{Color, Colors, Rgba8};
use pgwm_core::config::USED_DIFFERENT_COLOR_SEGMENTS;
use pgwm_core::push_heapless;

use crate::error::Result;
use crate::x11::call_wrapper::CallWrapper;

#[allow(clippy::type_complexity)]
pub(crate) fn alloc_colors(
    call_wrapper: &mut CallWrapper,
    color_map: Colormap,
    colors: pgwm_core::colors::ColorBuilder,
) -> Result<Colors> {
    let mut alloc_rgba_cookies: heapless::Vec<
        ((u8, u8, u8, u8), FixedCookie<AllocColorReply, 20>),
        USED_DIFFERENT_COLOR_SEGMENTS,
    > = heapless::Vec::new();
    for color in colors.get_all().iter() {
        let color = **color;
        let (r, g, b, _) = color.to_rgba16();
        push_heapless!(
            alloc_rgba_cookies,
            (
                color,
                alloc_color(
                    &mut call_wrapper.uring,
                    &mut call_wrapper.xcb_state,
                    color_map,
                    r,
                    g,
                    b,
                    false
                )?
            )
        )?;
    }
    let mut allocated_colors: heapless::Vec<Color, USED_DIFFERENT_COLOR_SEGMENTS> =
        heapless::Vec::new();
    for ((r, g, b, a), cookie) in alloc_rgba_cookies {
        push_heapless!(
            allocated_colors,
            Color {
                pixel: cookie
                    .reply(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?
                    .pixel,
                bgra8: [b, g, r, a]
            }
        )?;
    }
    Ok(Colors::from_vec(allocated_colors))
}
