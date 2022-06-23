use crate::error::Result;
use crate::wm::XorgConnection;
use pgwm_core::config::{APPLICATION_WINDOW_LIMIT, WM_CLASS_NAME_LIMIT, WM_NAME_LIMIT};
use pgwm_core::geometry::Dimensions;
use x11rb::{
    cookie::Cookie,
    protocol::xproto::{GetGeometryReply, GetPropertyReply, QueryTreeReply, Window},
};

pub(crate) struct QueryTreeCookie {
    pub(crate) inner: Cookie<QueryTreeReply>,
}

impl QueryTreeCookie {
    pub(crate) fn await_children(
        self,
        con: &mut XorgConnection,
    ) -> Result<heapless::Vec<Window, APPLICATION_WINDOW_LIMIT>> {
        let tree_reply = self.inner.reply(con)?;
        Ok(heapless::Vec::from_slice(tree_reply.children.as_slice())
            .map_err(|_| pgwm_core::error::Error::HeaplessInstantiate)?)
    }
}

pub(crate) struct DimensionsCookie {
    pub(crate) inner: Cookie<GetGeometryReply>,
}

impl DimensionsCookie {
    pub(crate) fn await_dimensions(self, con: &mut XorgConnection) -> Result<Dimensions> {
        let reply = self.inner.reply(con)?;
        Ok(Dimensions {
            height: reply.height as i16,
            width: reply.width as i16,
            x: reply.x,
            y: reply.y,
        })
    }
}

pub(crate) struct ClassConvertCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl ClassConvertCookie {
    pub(crate) fn await_class_names(
        self,
        con: &mut XorgConnection,
    ) -> Result<Option<heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>>> {
        Ok(extract_wm_class(self.inner.reply(con)?))
    }
}

fn extract_wm_class(
    class_response: GetPropertyReply,
) -> Option<heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>> {
    // Already allocated vec
    let raw_utf8 = String::from_utf8(class_response.value);
    if let Ok(raw_utf8) = &raw_utf8 {
        let complete_names = raw_utf8
            .split('\u{0}')
            .filter(|s| !s.is_empty())
            .map(heapless::String::from)
            // Avoiding another alloc here
            .collect::<heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>>();
        Some(complete_names)
    } else {
        pgwm_core::debug!("Failed to parse class response value as utf-8");
        None
    }
}

pub(crate) struct FallbackNameConvertCookie {
    pub(crate) wm_inner: Cookie<GetPropertyReply>,
    pub(crate) ewmh_inner: Cookie<GetPropertyReply>,
}

impl FallbackNameConvertCookie {
    pub(crate) fn await_name(
        self,
        con: &mut XorgConnection,
    ) -> Result<Option<heapless::String<WM_NAME_LIMIT>>> {
        let ewmh = self.ewmh_inner.reply(con)?;
        if ewmh.value.is_empty() {
            let wm = self.wm_inner.reply(con)?;
            if wm.value.is_empty() {
                Ok(None)
            } else {
                utf8_heapless(wm.value)
            }
        } else if let Ok(utf8) = utf8_heapless(ewmh.value) {
            self.wm_inner.forget(con);
            Ok(utf8)
        } else if let Ok(wm) = self.wm_inner.reply(con) {
            utf8_heapless(wm.value)
        } else {
            Ok(None)
        }
    }
}

fn utf8_heapless<const N: usize>(bytes: Vec<u8>) -> Result<Option<heapless::String<N>>> {
    let slice = &bytes[..N.min(bytes.len())];
    Ok(std::str::from_utf8(slice).map(|s| Some(heapless::String::from(s)))?)
}

pub(crate) struct TransientConvertCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl TransientConvertCookie {
    pub(crate) fn await_is_transient_for(self, con: &mut XorgConnection) -> Result<Option<Window>> {
        let prop = self.inner.reply(con)?;
        if prop.value_len == 0 {
            Ok(None)
        } else {
            Ok(prop.value32().and_then(|mut val| val.next()))
        }
    }
}
