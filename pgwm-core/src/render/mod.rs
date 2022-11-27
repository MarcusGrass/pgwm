use xcb_rust_protocol::proto::render::{Directformat, Pictformat, Picture};
use xcb_rust_protocol::proto::xproto::{Drawable, Visualid};

pub struct RenderPicture {
    pub drawable: Drawable,
    pub picture: Picture,
    pub format: Pictformat,
}

pub struct DoubleBufferedRenderPicture {
    pub window: RenderPicture,
    pub pixmap: RenderPicture,
}

#[derive(Debug, Copy, Clone)]
pub struct RenderVisualInfo {
    pub root: VisualInfo,
    pub render: VisualInfo,
}

#[derive(Debug, Copy, Clone)]
pub struct VisualInfo {
    pub visual_id: Visualid,
    pub pict_format: Pictformat,
    pub direct_format: Directformat,
    pub depth: u8,
}
