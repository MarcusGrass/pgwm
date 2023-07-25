use xcb_rust_protocol::connection::xproto::alloc_color;
use xcb_rust_protocol::cookie::FixedCookie;
use xcb_rust_protocol::proto::xproto::{AllocColorReply, Colormap};

use pgwm_core::colors::{Color, Colors, Rgba8};
use pgwm_core::config::COLORS;
use pgwm_core::push_heapless;

use crate::error::Result;
use crate::x11::call_wrapper::CallWrapper;

#[allow(clippy::type_complexity)]
pub(crate) fn alloc_colors(call_wrapper: &mut CallWrapper, color_map: Colormap) -> Result<Colors> {
    let mut alloc_rgba_cookies: heapless::Vec<
        ((u8, u8, u8, u8), FixedCookie<AllocColorReply, 20>),
        { COLORS.len() },
    > = heapless::Vec::new();
    for color in COLORS {
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
    let mut allocated_colors: [Color; 17] = [Color {
        pixel: 0,
        bgra8: [0, 0, 0, 0],
    }; 17];
    for (ind, ((r, g, b, a), cookie)) in alloc_rgba_cookies.into_iter().enumerate() {
        allocated_colors[ind] = Color {
            pixel: cookie
                .reply(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?
                .pixel,
            bgra8: [b, g, r, a],
        };
    }
    Ok(Colors {
        inner: allocated_colors,
    })
}
