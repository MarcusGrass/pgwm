use crate::config::Action;
use x11rb::protocol::xproto::{ButtonIndex, ModMask};

#[derive(Debug, Hash, Eq, PartialEq)]
pub struct MouseActionKey {
    pub detail: u8,
    pub state: u16,
    pub target: MouseTarget,
}

impl MouseActionKey {
    #[must_use]
    pub fn new(detail: u8, state: u16, target: MouseTarget) -> Self {
        Self {
            detail,
            state,
            target,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(feature = "config-file", serde(tag = "kind", content = "args"))]
pub enum MouseTarget {
    ClientWindow,
    WorkspaceBarComponent(usize),
    WindowTitle,
    ShortcutComponent(usize),
    StatusComponent(usize),
    Tab,
}

impl MouseTarget {
    #[must_use]
    pub fn on_bar(&self) -> bool {
        matches!(
            self,
            MouseTarget::WorkspaceBarComponent(_)
                | MouseTarget::WindowTitle
                | MouseTarget::ShortcutComponent(_)
                | MouseTarget::StatusComponent(_)
        )
    }
}

#[derive(Debug)]
pub struct MouseMapping {
    pub target: MouseTarget,
    pub action: Action,
    pub mods: ModMask,
    pub button: ButtonIndex,
}

impl MouseMapping {
    #[must_use]
    pub const fn new(
        target: MouseTarget,
        mods: ModMask,
        button: ButtonIndex,
        action: Action,
    ) -> Self {
        MouseMapping {
            target,
            action,
            mods,
            button,
        }
    }
}
