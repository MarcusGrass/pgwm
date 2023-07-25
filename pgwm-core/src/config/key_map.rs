use xcb_rust_protocol::proto::xproto::ModMask;

use crate::config::Action;

#[derive(Debug, Copy, Clone)]
pub struct KeyboardMapping {
    pub modmask: ModMask,
    pub keysym: u32,
    pub action: Action,
}

impl KeyboardMapping {
    #[must_use]
    pub const fn new(modmask: ModMask, keysym: u32, action: Action) -> Self {
        KeyboardMapping {
            modmask,
            keysym,
            action,
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct KeyBoardMappingKey {
    pub code: u8,
    pub mods: u16,
}

impl KeyBoardMappingKey {
    #[must_use]
    pub fn new(code: u8, mods: u16) -> Self {
        KeyBoardMappingKey { code, mods }
    }
}
