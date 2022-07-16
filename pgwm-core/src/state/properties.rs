use crate::config::{WM_CLASS_NAME_LIMIT, WM_NAME_LIMIT};
use x11rb::properties::{WmHints, WmSizeHints};
use x11rb::xcb::xproto::Window;

#[derive(Debug, Clone)]
pub struct WindowProperties {
    pub wm_state: Option<WmState>,
    pub net_wm_state: NetWmState,
    pub hints: Option<WmHints>,
    pub size_hints: Option<WmSizeHints>,
    pub window_types: heapless::Vec<WindowType, 12>,
    pub leader: Option<Window>,
    pub pid: Option<u32>,
    pub class: heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>,
    pub protocols: heapless::Vec<Protocol, 4>,
    pub name: WmName,
    pub transient_for: Option<Window>,
}

impl WindowProperties {
    #[must_use]
    pub fn new(
        wm_state: Option<WmState>,
        net_wm_state: NetWmState,
        hints: Option<WmHints>,
        size_hints: Option<WmSizeHints>,
        window_types: heapless::Vec<WindowType, 12>,
        leader: Option<Window>,
        pid: Option<u32>,
        class: heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>,
        protocols: heapless::Vec<Protocol, 4>,
        name: WmName,
        transient_for: Option<Window>,
    ) -> Self {
        Self {
            wm_state,
            net_wm_state,
            hints,
            size_hints,
            window_types,
            leader,
            pid,
            class,
            protocols,
            name,
            transient_for,
        }
    }
}

/// [`_NET_WM_STATE`](https://specifications.freedesktop.org/wm-spec/1.3/ar01s05.html)
#[allow(clippy::struct_excessive_bools)]
#[derive(PartialEq, Eq, Debug, Default, Copy, Clone)]
pub struct NetWmState {
    pub modal: bool,
    pub sticky: bool,
    pub maximized_vert: bool,
    pub maximized_horz: bool,
    pub shaded: bool,
    pub skip_taskbar: bool,
    pub skip_pager: bool,
    pub hidden: bool,
    pub fullscreen: bool,
    pub above: bool,
    pub below: bool,
    pub demands_attention: bool,
}

/// [`_NET_WM_STATE`](https://specifications.freedesktop.org/wm-spec/1.3/ar01s05.html)
#[allow(clippy::struct_excessive_bools)]
#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub struct NetWmAllowedActions {
    pub move_window: bool,
    pub resize: bool,
    pub minimize: bool,
    pub shade: bool,
    pub stick: bool,
    pub maximize_vert: bool,
    pub maximize_horz: bool,
    pub fullscreen: bool,
    pub change_desktop: bool,
    pub close: bool,
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum WmState {
    Withdrawn,
    Normal,
    Iconic,
}

impl WmState {
    #[must_use]
    pub const fn get_value(&self) -> u32 {
        match self {
            WmState::Withdrawn => 0,
            WmState::Normal => 1,
            WmState::Iconic => 3,
        }
    }
    #[must_use]
    pub fn from_value(val: u32) -> Option<Self> {
        match val {
            0 => Some(WmState::Withdrawn),
            1 => Some(WmState::Normal),
            3 => Some(WmState::Iconic),
            _ => None,
        }
    }
}

/// [`_NET_WM_WINDOW_TYPE`](https://specifications.freedesktop.org/wm-spec/1.3/ar01s05.html)
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum WindowType {
    Desktop,
    Dock,
    Toolbar,
    Menu,
    Utility,
    Splash,
    Dialog,
    Normal,
}

/// [Protocols](https://tronche.com/gui/x/icccm/sec-4.html#WM_PROTOCOLS)
/// 4 Protocols that we actually care about
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Protocol {
    TakeFocus,
    Delete,
    Ping,
    Sync,
}

/// We're essentially saying that we only accept one of these, in practice `WmName` is always set
/// while `NetWmName` is also always set but the latter always is a subset of the former always.
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum WmName {
    WmName(heapless::String<WM_NAME_LIMIT>),
    NetWmName(heapless::String<WM_NAME_LIMIT>),
}

impl WmName {
    #[must_use]
    pub fn get_cloned(&self) -> heapless::String<WM_NAME_LIMIT> {
        match self {
            WmName::NetWmName(n) | WmName::WmName(n) => n.clone(),
        }
    }
}
