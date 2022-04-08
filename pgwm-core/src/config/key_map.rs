use crate::config::Action;
use x11rb::protocol::xproto::ModMask;

#[derive(Debug)]
pub struct KeyboardMapping {
    pub modmask: ModMask,
    pub keysym: u32,
    pub action: Action,
}

impl KeyboardMapping {
    #[must_use]
    pub fn new(modmask: ModMask, keysym: u32, action: Action) -> Self {
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
