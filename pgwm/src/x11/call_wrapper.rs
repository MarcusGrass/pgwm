use crate::error::{Error, Result};
use smallmap::Map;
use x11rb::cookie::{Cookie, VoidCookie};
use x11rb::protocol::render::{
    CreatePictureAux, Glyphinfo, Glyphset, PictOp, Picture, PolyEdge, PolyMode, Repeat,
};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureRequestEvent,
    ConfigureWindowAux, EventMask, GetPropertyReply, GetWindowAttributesReply, GrabMode,
    InputFocus, InternAtomReply, PropMode, QueryPointerReply, Screen, StackMode, Timestamp, Window,
};
use x11rb::protocol::ErrorKind;
use x11rb::xcb::xproto;
use x11rb::{CURRENT_TIME, NONE};

use crate::error::Error::GlyphMismatch;
use crate::wm::XorgConnection;
use pgwm_core::config::{
    APPLICATION_WINDOW_LIMIT, WINDOW_MANAGER_NAME, WINDOW_MANAGER_NAME_BUF_SIZE,
    WM_CLASS_NAME_LIMIT, WM_NAME_LIMIT,
};
use pgwm_core::geometry::Dimensions;
use pgwm_core::push_heapless;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderVisualInfo};
use pgwm_core::state::properties::{
    NetWmState, Protocol, WindowProperties, WindowType, WmName, WmState,
};
use pgwm_core::state::workspace::FocusStyle;
use pgwm_core::state::State;
use x11rb::errors::ReplyError;
use x11rb::properties::{WmHints, WmHintsCookie, WmSizeHints, WmSizeHintsCookie};
use x11rb::xcb::xproto::{GetGeometryReply, QueryTreeReply};

const MAX_STORED_ATOMS: usize = 64;

pub(crate) trait PropFirstU32 {
    fn first_u32(&self) -> Option<u32>;
}

impl PropFirstU32 for GetPropertyReply {
    fn first_u32(&self) -> Option<u32> {
        self.value32().and_then(|mut it| it.next())
    }
}

macro_rules! impl_atoms {
    ($( $const_name:ident, $enum_name:ident, $is_ewmh: expr),* ) => {
        // Init consts
        $(
            pub(crate) const $const_name: &[u8] = stringify!($const_name).as_bytes();
        )*
            // Init Enum
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            pub enum SupportedAtom {
        $(
            $enum_name,
        )*
            }
            fn init_maps(connection: &mut XorgConnection) -> Result<(Map<&'static [u8], ResolvedAtom>, Map<Atom, ResolvedAtom>)> {
                    let mut name_to_atom = Map::new();
                    let mut atom_to_resolved = Map::new();
                    let mut cookies = heapless::Deque::<Cookie<InternAtomReply>, 64>::new();
        $(
                    cookies.push_back(xproto::intern_atom(connection, false, $const_name, false)?)
                    .expect("Not enough space for intern atoms");

        )*
        $(
                    let atom = cookies.pop_front().unwrap().reply(connection)?.atom;
                    name_to_atom.insert(
                        $const_name,
                        ResolvedAtom {
                            value: atom,
                            intern_atom: SupportedAtom::$enum_name,
                            ewmh: $is_ewmh,
                        },
                    );
                    atom_to_resolved.insert(
                        atom,
                        ResolvedAtom {
                            value: atom,
                            intern_atom: SupportedAtom::$enum_name,
                            ewmh: $is_ewmh,
                        },
                    );
        )*
                    return Ok((name_to_atom, atom_to_resolved));
        }

    };
}

impl_atoms!(
    WM_PROTOCOLS,
    WmProtocols,
    false,
    WM_DELETE_WINDOW,
    WmDeleteWindow,
    false,
    WM_STATE,
    WmState,
    false,
    WM_NAME,
    WmName,
    false,
    WM_CLASS,
    WmClass,
    false,
    WM_HINTS,
    WmHints,
    false,
    WM_TAKE_FOCUS,
    WmTakeFocus,
    false,
    WM_CLIENT_LEADER,
    WmClientLeader,
    false,
    _NET_WM_WINDOW_TYPE,
    NetWmWindowType,
    true,
    _NET_WM_WINDOW_TYPE_DESKTOP,
    NetWmWindowTypeDesktop,
    true,
    _NET_WM_WINDOW_TYPE_DOCK,
    NetWmWindowTypeDock,
    true,
    _NET_WM_WINDOW_TYPE_TOOLBAR,
    NetWmWindowTypeToolbar,
    true,
    _NET_WM_WINDOW_TYPE_MENU,
    NetWmWindowTypeMenu,
    true,
    _NET_WM_WINDOW_TYPE_UTILITY,
    NetWmWindowTypeUtility,
    true,
    _NET_WM_WINDOW_TYPE_SPLASH,
    NetWmWindowTypeSplash,
    true,
    _NET_WM_WINDOW_TYPE_DIALOG,
    NetWmWindowTypeDialog,
    true,
    _NET_WM_WINDOW_TYPE_NORMAL,
    NetWmWindowTypeNormal,
    true,
    _NET_WM_ALLOWED_ACTIONS,
    NetWmAllowedActions,
    true,
    _NET_WM_ACTION_MOVE,
    NetWmActionMove,
    true,
    _NET_WM_ACTION_RESIZE,
    NetWmActionResize,
    true,
    _NET_WM_ACTION_MINIMIZE,
    NetWmActionMinimize,
    true,
    _NET_WM_ACTION_SHADE,
    NetWmActionShade,
    true,
    _NET_WM_ACTION_STICK,
    NetWmActionStick,
    true,
    _NET_WM_ACTION_MAXIMIZE_HORZ,
    NetWmActionMaximizeHorz,
    true,
    _NET_WM_ACTION_MAXIMIZE_VERT,
    NetWmActionMaximizeVert,
    true,
    _NET_WM_ACTION_FULLSCREEN,
    NetWmActionFullscreen,
    true,
    _NET_WM_ACTION_CHANGE_DESKTOP,
    NetWmActionChangeDesktop,
    true,
    _NET_WM_ACTION_CLOSE,
    NetWmActionClose,
    true,
    _NET_WM_STATE,
    NetWmState,
    true,
    _NET_WM_STATE_MODAL,
    NetWmStateModal,
    true,
    _NET_WM_STATE_STICKY,
    NetWmStateSticky,
    true,
    _NET_WM_STATE_MAXIMIZED_VERT,
    NetWmStateMaximizedVert,
    true,
    _NET_WM_STATE_MAXIMIZED_HORZ,
    NetWmStateMaximizedHorz,
    true,
    _NET_WM_STATE_SHADED,
    NetWmStateMaximizedShaded,
    true,
    _NET_WM_STATE_SKIP_TASKBAR,
    NetWmStateSkipTaskbar,
    true,
    _NET_WM_STATE_SKIP_PAGER,
    NetWmStateSkipPager,
    true,
    _NET_WM_STATE_HIDDEN,
    NetWmStateSkipHidden,
    true,
    _NET_WM_STATE_FULLSCREEN,
    NetWmStateFullscreen,
    true,
    _NET_WM_STATE_ABOVE,
    NetWmStateAbove,
    true,
    _NET_WM_STATE_BELOW,
    NetWmStateBelow,
    true,
    _NET_WM_STATE_DEMANDS_ATTENTION,
    NetWmStateDemandsAttention,
    true,
    _NET_WM_NAME,
    NetWmName,
    true,
    _NET_SUPPORTED,
    NetSupported,
    true,
    _NET_CLIENT_LIST,
    NetClientList,
    true,
    _NET_NUMBER_OF_DESKTOPS,
    NetNumberOfDesktops,
    true,
    _NET_DESKTOP,
    NetDesktop,
    true,
    _NET_DESKTOP_NAMES,
    NetDesktopNames,
    true,
    _NET_DESKTOP_GEOMETRY,
    NetDesktopGeometry,
    true,
    _NET_DESKTOP_VIEWPORT,
    NetDesktopViewport,
    true,
    _NET_CURRENT_DESKTOP,
    NetCurrentDesktop,
    true,
    _NET_WORKAREA,
    NetWorkarea,
    true,
    _NET_ACTIVE_WINDOW,
    NetActiveWindow,
    true,
    _NET_SUPPORTING_WM_CHECK,
    NetSupportingWmCheck,
    true,
    _NET_SHOWING_DESKTOP,
    NetShowingDesktop,
    true,
    _NET_CLOSE_WINDOW,
    NetCloseWindow,
    true,
    _NET_FRAME_EXTENTS,
    NetFrameExtents,
    true,
    _NET_REQUEST_FRAME_EXTENTS,
    NetRequestFrameExtents,
    true,
    _NET_WM_PID,
    NetWmPid,
    true,
    _NET_WM_PING,
    NetWmPing,
    true,
    _NET_WM_SYNC_REQUEST,
    NetWmSyncRequest,
    true,
    _NET_WM_SYNC_REQUEST_COUNTER,
    NetWmSyncRequestCounter,
    true
);

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedAtom {
    pub(crate) intern_atom: SupportedAtom,
    pub(crate) value: u32,
    pub(crate) ewmh: bool,
}

pub(crate) struct CallWrapper {
    connection: XorgConnection,
    name_to_atom: Map<&'static [u8], ResolvedAtom>,
    atom_to_resolved: Map<Atom, ResolvedAtom>,
}

impl CallWrapper {
    pub(crate) fn try_become_wm(&mut self, screen: &Screen) -> Result<()> {
        let change = ChangeWindowAttributesAux::default()
            .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY);

        let res =
            xproto::change_window_attributes(&mut self.connection, screen.root, &change, false)?
                .check(&mut self.connection);
        if let Err(ReplyError::X11Error(ref error)) = res {
            if error.error_kind == ErrorKind::Access {
                pgwm_core::debug!("Fatal error, Failed to start WM because another WM is running");
                Err(Error::BecomeWm)
            } else {
                Err(Error::BecomeWm)
            }
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn set_default_manager_props(&mut self, state: &State) -> Result<()> {
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_SUPPORTED).unwrap().value,
            AtomEnum::ATOM,
            self.name_to_atom
                .iter()
                .filter(|supported| supported.1.ewmh)
                .map(|val| val.1.value)
                .collect::<heapless::Vec<u32, MAX_STORED_ATOMS>>()
                .as_slice(),
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_CLIENT_LIST).unwrap().value,
            AtomEnum::WINDOW,
            &[],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom
                .get(&_NET_NUMBER_OF_DESKTOPS)
                .unwrap()
                .value,
            AtomEnum::CARDINAL,
            &[0],
            true,
        )?;
        let utf8 = WINDOW_MANAGER_NAME
            .chars()
            .chain(std::iter::once('\u{0}'))
            .map(|ch| ch as u32)
            .collect::<heapless::Vec<u32, WINDOW_MANAGER_NAME_BUF_SIZE>>();
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_DESKTOP_NAMES).unwrap().value,
            AtomEnum::STRING,
            utf8.as_slice(),
            true,
        )?;

        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_CURRENT_DESKTOP).unwrap().value,
            AtomEnum::CARDINAL,
            &[0],
            true,
        )?;

        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_DESKTOP_VIEWPORT).unwrap().value,
            AtomEnum::CARDINAL,
            &[0; 2],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_DESKTOP_GEOMETRY).unwrap().value,
            AtomEnum::CARDINAL,
            &[
                state.screen.width_in_pixels as u32,
                state.screen.height_in_pixels as u32,
            ],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_WORKAREA).unwrap().value,
            AtomEnum::CARDINAL,
            &[
                0,
                state.status_bar_height as u32,
                state.screen.width_in_pixels as u32,
                state.screen.height_in_pixels as u32 - state.status_bar_height as u32,
            ],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_ACTIVE_WINDOW).unwrap().value,
            AtomEnum::CARDINAL,
            &[],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom
                .get(&_NET_SUPPORTING_WM_CHECK)
                .unwrap()
                .value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom
                .get(&_NET_SUPPORTING_WM_CHECK)
                .unwrap()
                .value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
            true,
        )?;
        self.connection.change_property8(
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom.get(&_NET_WM_NAME).unwrap().value,
            AtomEnum::STRING,
            WINDOW_MANAGER_NAME.as_bytes(),
            true,
        )?;
        Ok(())
    }

    pub(crate) fn get_window_properties(
        &mut self,
        window: Window,
    ) -> Result<WindowPropertiesCookie> {
        let wm_name = self.get_wm_name(window)?;
        let net_wm_name = self.get_net_wm_name(window)?;
        let wm_class = self.get_class_names(window)?;
        let wm_state = self.get_wm_state(window)?;
        let net_wm_state = self.get_net_wm_state(window)?;
        let hints = WmHints::get(&mut self.connection, window)?;
        let size_hints = WmSizeHints::get_normal_hints(&mut self.connection, window)?;
        let window_types = self.get_window_types(window)?;
        let leader = self.get_leader(window)?;
        let pid = self.get_pid(window)?;
        let protocols = self.get_protocols(window)?;
        let transient_for = self.get_is_transient_for(window)?;

        Ok(WindowPropertiesCookie {
            wm_state,
            net_wm_state,
            wm_name,
            net_wm_name,
            wm_class,
            hints,
            size_hints,
            window_types,
            protocols,
            pid,
            leader,
            transient_for,
        })
    }

    pub(crate) fn set_base_client_event_mask(&mut self, window: Window) -> Result<()> {
        let cw = ChangeWindowAttributesAux::new().event_mask(
            EventMask::ENTER_WINDOW
                | EventMask::FOCUS_CHANGE
                | EventMask::PROPERTY_CHANGE
                | EventMask::VISIBILITY_CHANGE
                | EventMask::EXPOSURE
                | EventMask::STRUCTURE_NOTIFY,
        );
        xproto::change_window_attributes(&mut self.connection, window, &cw, true)?;
        Ok(())
    }

    pub(crate) fn set_base_client_properties(&mut self, window: Window) -> Result<()> {
        self.connection.change_property32(
            PropMode::REPLACE,
            window,
            self.name_to_atom
                .get(&_NET_WM_ALLOWED_ACTIONS)
                .unwrap()
                .value,
            AtomEnum::ATOM,
            &[
                self.name_to_atom
                    .get(&_NET_WM_ACTION_FULLSCREEN)
                    .unwrap()
                    .value,
                self.name_to_atom.get(&_NET_WM_ACTION_CLOSE).unwrap().value,
            ],
            true,
        )?;
        Ok(())
    }

    pub fn push_to_client_list(&mut self, root: Window, new_win: Window) -> Result<()> {
        self.connection.change_property32(
            PropMode::APPEND,
            root,
            self.name_to_atom.get(&_NET_CLIENT_LIST).unwrap().value,
            AtomEnum::WINDOW,
            &[new_win],
            true,
        )?;
        Ok(())
    }

    pub fn update_client_list(&mut self, managed: &[Window], state: &State) -> Result<()> {
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom.get(&_NET_CLIENT_LIST).unwrap().value,
            AtomEnum::WINDOW,
            managed,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn query_subwindows(&mut self, window: Window) -> Result<QueryTreeCookie> {
        Ok(QueryTreeCookie {
            inner: xproto::query_tree(&mut self.connection, window, false)?,
        })
    }

    pub(crate) fn query_pointer(&mut self, state: &State) -> Result<Cookie<QueryPointerReply>> {
        Ok(xproto::query_pointer(
            &mut self.connection,
            state.screen.root,
            false,
        )?)
    }

    pub(crate) fn get_dimensions(&mut self, window: Window) -> Result<DimensionsCookie> {
        Ok(DimensionsCookie {
            inner: xproto::get_geometry(&mut self.connection, window, false)?,
        })
    }

    pub(crate) fn get_window_attributes(
        &mut self,
        window: Window,
    ) -> Result<Cookie<GetWindowAttributesReply>> {
        Ok(xproto::get_window_attributes(
            &mut self.connection,
            window,
            false,
        )?)
    }

    pub(crate) fn get_class_names(&mut self, win: Window) -> Result<WmClassCookie> {
        let inner = xproto::get_property(
            &mut self.connection,
            false,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            64,
            false,
        )?;
        Ok(WmClassCookie { inner })
    }

    pub(crate) fn get_wm_name(&mut self, win: Window) -> Result<NameCookie> {
        Ok(NameCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                win,
                AtomEnum::WM_NAME,
                AtomEnum::ANY,
                0,
                1028,
                false,
            )?,
        })
    }

    pub(crate) fn get_net_wm_name(&mut self, win: Window) -> Result<NameCookie> {
        Ok(NameCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                win,
                self.name_to_atom.get(&_NET_WM_NAME).unwrap().value,
                AtomEnum::ANY,
                0,
                1028,
                false,
            )?,
        })
    }

    pub(crate) fn get_is_transient_for(&mut self, win: Window) -> Result<SingleCardCookie> {
        let inner = xproto::get_property(
            &mut self.connection,
            false,
            win,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            32,
            false,
        )?;
        Ok(SingleCardCookie { inner })
    }

    pub(crate) fn bulk_set_extents(&mut self, windows: &[Window], border_width: u32) -> Result<()> {
        for win in windows {
            self.set_extents(*win, border_width)?;
        }
        Ok(())
    }

    pub(crate) fn set_extents(&mut self, win: Window, border_width: u32) -> Result<()> {
        self.connection.change_property32(
            PropMode::REPLACE,
            win,
            self.name_to_atom.get(&_NET_FRAME_EXTENTS).unwrap().value,
            AtomEnum::CARDINAL,
            &[border_width; 4],
            true,
        )?;
        Ok(())
    }

    pub(crate) fn set_state(&mut self, win: Window, state: WmState) -> Result<()> {
        self.connection.change_property32(
            PropMode::REPLACE,
            win,
            self.name_to_atom.get(&WM_STATE).unwrap().value,
            self.name_to_atom.get(&WM_STATE).unwrap().value,
            &[state.get_value()],
            true,
        )?;
        Ok(())
    }

    pub(crate) fn set_root_event_mask(
        &mut self,
        cursor_handle: &x11rb::cursor::Handle,
        state: &State,
    ) -> Result<VoidCookie> {
        let change_attrs_aux = ChangeWindowAttributesAux::new()
            .event_mask(
                EventMask::SUBSTRUCTURE_REDIRECT
                    | EventMask::SUBSTRUCTURE_NOTIFY
                    | EventMask::BUTTON_PRESS
                    | EventMask::POINTER_MOTION
                    | EventMask::ENTER_WINDOW
                    | EventMask::LEAVE_WINDOW
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::PROPERTY_CHANGE
                    | EventMask::KEY_PRESS,
            )
            .cursor(cursor_handle.load_cursor(&mut self.connection, state.cursor_name.as_str())?);

        Ok(xproto::change_window_attributes(
            &mut self.connection,
            state.screen.root,
            &change_attrs_aux,
            false,
        )?)
    }

    pub(crate) fn grab_pointer(&mut self, state: &State) -> Result<()> {
        xproto::grab_pointer(
            &mut self.connection,
            true,
            state.screen.root,
            u32::from(
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            ) as u16,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            0u16,
            0u16,
            CURRENT_TIME,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn ungrab_pointer(&mut self) -> Result<()> {
        xproto::ungrab_pointer(&mut self.connection, CURRENT_TIME, true)?;
        Ok(())
    }

    /// Handling x10 style windows becomes strange: <https://tronche.com/gui/x/xlib/ICC/client-to-window-manager/wm-hints.html>
    pub(crate) fn take_focus(
        &mut self,
        root: Window,
        target: Window,
        focus_style: FocusStyle,
        state: &State,
    ) -> Result<()> {
        let target = if target == root {
            // No active window if root gets focused
            xproto::set_input_focus(
                &mut self.connection,
                InputFocus::PARENT,
                target,
                CURRENT_TIME,
                true,
            )?;
            NONE
        } else {
            match focus_style {
                FocusStyle::NoInput => {
                    pgwm_core::debug!("NoInput win {target} take focus");
                    target
                }
                FocusStyle::Passive => {
                    pgwm_core::debug!("Passive win {target} take focus");
                    xproto::set_input_focus(
                        &mut self.connection,
                        InputFocus::PARENT,
                        target,
                        CURRENT_TIME,
                        true,
                    )?;
                    target
                }
                FocusStyle::LocallyActive => {
                    pgwm_core::debug!("Locally active win {target} set input focus");
                    // Setting input focus should only be required if the client's top-level-window
                    // doesn't already have the focus, but whatever just always set it.
                    xproto::set_input_focus(
                        &mut self.connection,
                        InputFocus::PARENT,
                        target,
                        CURRENT_TIME,
                        true,
                    )?;
                    self.send_take_focus(target, state.last_timestamp)?;
                    target
                }
                FocusStyle::GloballyActive => {
                    pgwm_core::debug!("Globally active win {target} take focus");
                    self.send_take_focus(target, state.last_timestamp)?;
                    target
                }
            }
        };
        let data = [target, CURRENT_TIME];
        self.connection.change_property32(
            PropMode::REPLACE,
            root,
            self.name_to_atom.get(&_NET_ACTIVE_WINDOW).unwrap().value,
            AtomEnum::WINDOW,
            &data,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn reset_root_focus(&mut self, state: &State) -> Result<()> {
        xproto::delete_property(
            &mut self.connection,
            state.screen.root,
            self.name_to_atom.get(&_NET_ACTIVE_WINDOW).unwrap().value,
            true,
        )?;
        xproto::set_input_focus(
            &mut self.connection,
            InputFocus::POINTER_ROOT,
            u32::from(InputFocus::POINTER_ROOT),
            CURRENT_TIME,
            true,
        )?;
        Ok(())
    }

    fn send_take_focus(&mut self, win: Window, timestamp: Timestamp) -> Result<()> {
        let event = ClientMessageEvent::new(
            32,
            win,
            self.name_to_atom.get(&WM_PROTOCOLS).unwrap().value,
            [
                self.name_to_atom.get(&WM_TAKE_FOCUS).unwrap().value,
                timestamp,
                0,
                0,
                0,
            ],
        );
        pgwm_core::debug!("Sending WM_TAKE_FOCUS focus for {}", win);
        xproto::send_event(
            &mut self.connection,
            false,
            win,
            EventMask::NO_EVENT,
            &event,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn send_delete(&mut self, win: Window) -> Result<()> {
        let event = ClientMessageEvent::new(
            32,
            win,
            self.name_to_atom.get(&WM_PROTOCOLS).unwrap().value,
            [
                self.name_to_atom.get(&WM_DELETE_WINDOW).unwrap().value,
                0,
                0,
                0,
                0,
            ],
        );
        pgwm_core::debug!("Sending delete for {}", win);
        xproto::send_event(
            &mut self.connection,
            false,
            win,
            EventMask::NO_EVENT,
            &event,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn send_map(&mut self, window: Window, state: &mut State) -> Result<()> {
        let cookie = xproto::map_window(&mut self.connection, window, true)?;
        // Triggers an enter-notify that needs to be ignored
        state.push_sequence(cookie.sequence_number());
        Ok(())
    }

    pub(crate) fn send_unmap(&mut self, window: Window, state: &mut State) -> Result<()> {
        let cookie = xproto::unmap_window(&mut self.connection, window, true)?;
        // Triggers an enter-notify that needs to be ignored, we also don't want to react to an UnmapNotify that we created
        state.push_sequence(cookie.sequence_number());
        Ok(())
    }

    pub(crate) fn send_destroy(&mut self, window: Window) -> Result<()> {
        xproto::destroy_window(&mut self.connection, window, true)?;
        Ok(())
    }

    pub(crate) fn send_kill(&mut self, window: Window) -> Result<()> {
        xproto::kill_client(&mut self.connection, window, true)?;
        Ok(())
    }

    pub(crate) fn push_window_to_top(&mut self, window: Window, state: &mut State) -> Result<()> {
        let cfg = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        self.do_configure(window, &cfg, state)
    }

    pub(crate) fn configure_window(
        &mut self,
        window: Window,
        dimension: Dimensions,
        border_width: u32,
        state: &mut State,
    ) -> Result<()> {
        let cfg = ConfigureWindowAux::new()
            .x(dimension.x as i32)
            .y(dimension.y as i32)
            .width(dimension.width as u32)
            .height(dimension.height as u32)
            .border_width(border_width)
            .stack_mode(StackMode::ABOVE);
        self.set_extents(window, border_width)?;
        self.do_configure(window, &cfg, state)
    }

    // When windows themselves ask to configure resultant mapping should not be ignored, so no need to ignore child-sequences here
    pub(crate) fn configure_from_request(&mut self, event: &ConfigureRequestEvent) -> Result<()> {
        let cfg = ConfigureWindowAux::from_configure_request(event);
        if let Some(border_width) = cfg.border_width {
            self.set_extents(event.window, border_width)?;
        }
        xproto::configure_window(&mut self.connection, event.window, &cfg, true)?;
        Ok(())
    }

    pub(crate) fn move_window(
        &mut self,
        window: Window,
        x: i32,
        y: i32,
        state: &mut State,
    ) -> Result<()> {
        let cfg = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .stack_mode(StackMode::ABOVE);
        self.do_configure(window, &cfg, state)
    }

    pub(crate) fn resize_window(
        &mut self,
        window: Window,
        height: u32,
        width: u32,
        state: &mut State,
    ) -> Result<()> {
        let cfg = ConfigureWindowAux::new().height(height).width(width);
        self.do_configure(window, &cfg, state)
    }

    fn do_configure(
        &mut self,
        window: Window,
        cfg: &ConfigureWindowAux,
        state: &mut State,
    ) -> Result<()> {
        let cookie = xproto::configure_window(&mut self.connection, window, cfg, true)?;
        // Triggers an enter-notify that needs to be ignored
        state.push_sequence(cookie.sequence_number() as u16);
        Ok(())
    }

    pub(crate) fn change_border_color(&mut self, window: Window, pixel: u32) -> Result<()> {
        let cw = ChangeWindowAttributesAux::new().border_pixel(pixel);
        xproto::change_window_attributes(&mut self.connection, window, &cw, true)?;
        Ok(())
    }

    pub(crate) fn window_mapped_picture(
        &mut self,
        win: Window,
        vis_info: &RenderVisualInfo,
    ) -> Result<Picture> {
        let picture = self.connection.generate_id()?;
        x11rb::xcb::render::create_picture(
            &mut self.connection,
            picture,
            win,
            vis_info.root.pict_format,
            &CreatePictureAux::new()
                .polyedge(PolyEdge::SMOOTH)
                .polymode(PolyMode::IMPRECISE),
            true,
        )?;
        Ok(picture)
    }

    pub(crate) fn pixmap_mapped_picture(
        &mut self,
        win: Window,
        vis_info: &RenderVisualInfo,
    ) -> Result<Picture> {
        let picture = self.connection.generate_id()?;
        x11rb::xcb::render::create_picture(
            &mut self.connection,
            picture,
            win,
            vis_info.render.pict_format,
            &CreatePictureAux::new().repeat(Repeat::NORMAL),
            true,
        )?;
        Ok(picture)
    }

    pub(crate) fn create_glyphset(&mut self, vis_info: &RenderVisualInfo) -> Result<Glyphset> {
        let id = self.connection.generate_id()?;
        x11rb::xcb::render::create_glyph_set(
            &mut self.connection,
            id,
            vis_info.render.pict_format,
            true,
        )?;
        Ok(id)
    }

    pub(crate) fn add_glyphs(
        &mut self,
        glyph_set: Glyphset,
        glyph_ids: &[u32],
        glyph_info: &[Glyphinfo],
        rendered_picture_glyphs: &[u8],
    ) -> Result<()> {
        if !glyph_ids.len() == glyph_info.len() {
            return Err(GlyphMismatch);
        }
        x11rb::xcb::render::add_glyphs(
            &mut self.connection,
            glyph_set,
            glyph_ids,
            glyph_info,
            rendered_picture_glyphs,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn fill_xrender_rectangle(
        &mut self,
        picture: Picture,
        color: x11rb::protocol::render::Color,
        dimensions: Dimensions,
    ) -> Result<()> {
        //let (red, green, blue, alpha) = color.to_rgba16();
        x11rb::xcb::render::fill_rectangles(
            &mut self.connection,
            PictOp::SRC,
            picture,
            color,
            &[dimensions.to_rectangle()],
            true,
        )?;
        Ok(())
    }

    // https://lists.freedesktop.org/archives/xcb/2006-October/002157.html
    // Can push everything in one request when switching glyphs instead of chunking
    pub(crate) fn draw_glyphs(
        &mut self,
        x: i16,
        y: i16,
        glyphs: Glyphset,
        dbw: &DoubleBufferedRenderPicture,
        glyph_ids: &[u16],
    ) -> Result<()> {
        let mut buf = Vec::with_capacity(glyph_ids.len());
        let render = if glyph_ids.len() > 254 {
            // 252 for elt8, 254 for elt16 and elt32
            &glyph_ids[..254]
        } else {
            glyph_ids
        };
        buf.extend_from_slice(&[render.len() as u8, 0, 0, 0]); // Pad to 32bit
                                                               //buf.extend_from_slice(&[0u8, 0u8, 0u8, 0u8]); // Actually a delta x and y as u16s encoded as 2 u8s each <- lies
        buf.extend_from_slice(&(x).to_ne_bytes()); // Dest x
        buf.extend_from_slice(&(y).to_ne_bytes()); // Dest y, why is it like this, why is the documentation lying to me?
        for glyph in render {
            buf.extend_from_slice(&glyph.to_ne_bytes()); // Dump to u8s
        }
        x11rb::xcb::render::composite_glyphs16(
            &mut self.connection,
            PictOp::OVER,
            dbw.pixmap.picture,
            dbw.window.picture,
            0,
            glyphs,
            0,
            0,
            &buf,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn resolve_atom(&self, atom: Atom) -> Option<ResolvedAtom> {
        self.atom_to_resolved.get(&atom).copied()
    }

    #[cfg(feature = "debug")]
    pub(crate) fn debug_window(&mut self, win: Window) -> Result<()> {
        use std::fmt::Write;
        let props = xproto::list_properties(&mut self.connection, win, false)?;
        let geom = self.get_dimensions(win)?;
        let attrs = self.get_window_attributes(win)?;
        let name = self.get_wm_name(win)?;
        let class = self.get_class_names(win)?;
        let hints_cookie = WmHints::get(&mut self.connection, win)?;
        let mut base = format!(
            "Debug Window {}, name: {}, classes: {:?}\n",
            win,
            name.await_name(&mut self.connection)
                .unwrap_or_default()
                .unwrap_or_default(),
            class
                .await_class_names(&mut self.connection)
                .unwrap_or_default()
                .unwrap_or_default()
        );
        base.push_str("\tHints: \n");
        if let Ok(hints) = hints_cookie.reply(&mut self.connection) {
            let _ = base.write_fmt(format_args!("\t\t{:?}", hints));
        }
        base.push('\n');
        base.push_str("\tAttributes: \n");
        if let Ok(attributes) = attrs.reply(&mut self.connection) {
            let _ = base.write_fmt(format_args!("\t\t{:?}", attributes));
        }
        base.push('\n');
        base.push_str("\tGeometry: \n");
        if let Ok(dims) = geom.await_dimensions(&mut self.connection) {
            let _ = base.write_fmt(format_args!("\t\t{:?}", dims));
        }
        base.push('\n');
        base.push_str("\tProperties: ");
        if let Ok(props) = props.reply(&mut self.connection) {
            for prop in props.atoms {
                if let Ok(name) = xproto::get_atom_name(&mut self.connection, prop, false)?
                    .reply(&mut self.connection)
                {
                    if let Ok(utf8) = String::from_utf8(name.name) {
                        let post = match utf8.as_str() {
                            "WM_CLIENT_LEADER" => self
                                .get_leader(win)
                                .ok()
                                .and_then(|cc| cc.await_card(self).ok())
                                .unwrap_or_default()
                                .map(|win| win.to_string())
                                .unwrap_or_default(),
                            "WM_PROTOCOLS" => {
                                let protocols = self
                                    .get_protocols(win)?
                                    .await_protocols(self)
                                    .ok()
                                    .unwrap_or_default();
                                format!("{:?}", protocols)
                            }
                            _ => "".to_owned(),
                        };
                        let _ = base.write_fmt(format_args!("\n\t\t{utf8}: {post}"));
                    }
                }
            }
        }
        base.push('\n');
        eprintln!("{base}");
        Ok(())
    }

    #[cfg(feature = "debug")]
    pub(crate) fn get_atom_name(&mut self, atom: Atom) -> Result<String> {
        Ok(String::from_utf8(
            xproto::get_atom_name(&mut self.connection, atom, false)?
                .reply(&mut self.connection)?
                .name,
        )?)
    }

    pub(crate) fn inner_mut(&mut self) -> &mut XorgConnection {
        &mut self.connection
    }

    pub(crate) fn new(mut connection: XorgConnection) -> Result<Self> {
        let (name_to_atom, atom_to_resolved) = init_maps(&mut connection)?;
        Ok(CallWrapper {
            connection,
            name_to_atom,
            atom_to_resolved,
        })
    }

    pub(crate) fn get_wm_state(&mut self, window: Window) -> Result<WmStateCookie> {
        Ok(WmStateCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&WM_STATE).unwrap().value,
                self.name_to_atom.get(&WM_STATE).unwrap().value,
                0,
                4,
                false,
            )?,
        })
    }

    pub(crate) fn get_net_wm_state(&mut self, window: Window) -> Result<NetWmStateCookie> {
        Ok(NetWmStateCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&_NET_WM_STATE).unwrap().value,
                AtomEnum::ATOM,
                0,
                4 * 32,
                false,
            )?,
        })
    }

    pub(crate) fn set_net_wm_state(&mut self, window: Window, new_state: NetWmState) -> Result<()> {
        let mut state = heapless::Vec::<Atom, 12>::new();
        if new_state.modal {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_MODAL).unwrap().value
            )?;
        }
        if new_state.skip_taskbar {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_SKIP_TASKBAR)
                    .unwrap()
                    .value
            )?;
        }
        if new_state.skip_pager {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_SKIP_PAGER)
                    .unwrap()
                    .value
            )?;
        }
        if new_state.sticky {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_STICKY).unwrap().value
            )?;
        }
        if new_state.maximized_vert {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_MAXIMIZED_VERT)
                    .unwrap()
                    .value
            )?;
        }
        if new_state.maximized_horz {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_MAXIMIZED_HORZ)
                    .unwrap()
                    .value
            )?;
        }
        if new_state.hidden {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_HIDDEN).unwrap().value
            )?;
        }
        if new_state.shaded {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_SHADED).unwrap().value
            )?;
        }
        if new_state.above {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_ABOVE).unwrap().value
            )?;
        }
        if new_state.below {
            push_heapless!(
                state,
                self.name_to_atom.get(&_NET_WM_STATE_BELOW).unwrap().value
            )?;
        }
        if new_state.demands_attention {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_DEMANDS_ATTENTION)
                    .unwrap()
                    .value
            )?;
        }
        if new_state.fullscreen {
            push_heapless!(
                state,
                self.name_to_atom
                    .get(&_NET_WM_STATE_FULLSCREEN)
                    .unwrap()
                    .value
            )?;
        }
        self.connection.change_property32(
            PropMode::REPLACE,
            window,
            self.name_to_atom.get(&_NET_WM_STATE).unwrap().value,
            AtomEnum::ATOM,
            state.as_slice(),
            true,
        )?;
        Ok(())
    }

    pub(crate) fn get_window_types(&mut self, window: Window) -> Result<WindowTypesCookie> {
        Ok(WindowTypesCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&_NET_WM_WINDOW_TYPE).unwrap().value,
                AtomEnum::ATOM,
                0,
                4 * 32,
                false,
            )?,
        })
    }

    pub(crate) fn get_leader(&mut self, window: Window) -> Result<SingleCardCookie> {
        Ok(SingleCardCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&WM_CLIENT_LEADER).unwrap().value,
                AtomEnum::WINDOW,
                0,
                4,
                false,
            )?,
        })
    }

    pub(crate) fn get_pid(&mut self, window: Window) -> Result<SingleCardCookie> {
        Ok(SingleCardCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&_NET_WM_PID).unwrap().value,
                AtomEnum::CARDINAL,
                0,
                4,
                false,
            )?,
        })
    }

    pub(crate) fn get_protocols(&mut self, window: Window) -> Result<ProtocolsCookie> {
        Ok(ProtocolsCookie {
            inner: xproto::get_property(
                &mut self.connection,
                false,
                window,
                self.name_to_atom.get(&WM_PROTOCOLS).unwrap().value,
                AtomEnum::ATOM,
                0,
                4 * 32,
                false,
            )?,
        })
    }
}

pub(crate) struct WindowPropertiesCookie {
    wm_state: WmStateCookie,
    net_wm_state: NetWmStateCookie,
    wm_name: NameCookie,
    net_wm_name: NameCookie,
    wm_class: WmClassCookie,
    hints: WmHintsCookie,
    size_hints: WmSizeHintsCookie,
    window_types: WindowTypesCookie,
    protocols: ProtocolsCookie,
    pid: SingleCardCookie,
    leader: SingleCardCookie,
    transient_for: SingleCardCookie,
}

impl WindowPropertiesCookie {
    pub(crate) fn await_properties(
        self,
        call_wrapper: &mut CallWrapper,
    ) -> Result<WindowProperties> {
        let wm_state = self.wm_state.await_state(call_wrapper.inner_mut());
        let net_wm_state = self.net_wm_state.await_net_wm_state(call_wrapper);
        let hints = self.hints.reply(call_wrapper.inner_mut()).ok();
        let size_hints = self.size_hints.reply(call_wrapper.inner_mut()).ok();
        let window_types = self.window_types.await_types(call_wrapper);
        let pid = self.pid.await_card(call_wrapper);
        let leader = self.leader.await_card(call_wrapper);
        let protocols = self.protocols.await_protocols(call_wrapper);
        let transient_for = self.transient_for.await_card(call_wrapper);
        let class = self.wm_class.await_class_names(call_wrapper.inner_mut());
        let name =
            if let Ok(Some(net_wm_name)) = self.net_wm_name.await_name(call_wrapper.inner_mut()) {
                self.wm_name.inner.forget(call_wrapper.inner_mut());
                WmName::NetWmName(net_wm_name)
            } else if let Ok(Some(wm_name)) = self.wm_name.await_name(call_wrapper.inner_mut()) {
                WmName::WmName(wm_name)
            } else {
                WmName::WmName(heapless::String::default())
            };
        Ok(WindowProperties {
            wm_state: wm_state?,
            net_wm_state: net_wm_state?.unwrap_or_default(),
            hints,
            size_hints,
            window_types: window_types?,
            leader: leader?,
            pid: pid?,
            class: class?.unwrap_or_default(),
            protocols: protocols?,
            name,
            transient_for: transient_for?,
        })
    }

    pub(crate) fn forget(self, callwrapper: &mut CallWrapper) {
        self.wm_name.inner.forget(callwrapper.inner_mut());
        self.net_wm_name.inner.forget(callwrapper.inner_mut());
        self.wm_state.inner.forget(callwrapper.inner_mut());
        self.net_wm_state.inner.forget(callwrapper.inner_mut());
        self.wm_class.inner.forget(callwrapper.inner_mut());
        self.pid.inner.forget(callwrapper.inner_mut());
        self.leader.inner.forget(callwrapper.inner_mut());
        self.hints.forget(callwrapper.inner_mut());
        self.size_hints.forget(callwrapper.inner_mut());
        self.protocols.inner.forget(callwrapper.inner_mut());
        self.window_types.inner.forget(callwrapper.inner_mut());
        self.transient_for.inner.forget(callwrapper.inner_mut());
    }
}

pub(crate) struct NameCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl NameCookie {
    pub(crate) fn await_name(
        self,
        con: &mut XorgConnection,
    ) -> Result<Option<heapless::String<WM_NAME_LIMIT>>> {
        if let Ok(wm) = self.inner.reply(con) {
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

pub(crate) struct WmClassCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl WmClassCookie {
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

pub(crate) struct WmStateCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl WmStateCookie {
    pub(crate) fn await_state(self, con: &mut XorgConnection) -> Result<Option<WmState>> {
        Ok(self
            .inner
            .reply(con)?
            .first_u32()
            .and_then(WmState::from_value))
    }
}

pub(crate) struct NetWmStateCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl NetWmStateCookie {
    pub(crate) fn await_net_wm_state(
        self,
        call_wrapper: &mut CallWrapper,
    ) -> Result<Option<NetWmState>> {
        let state = self
            .inner
            .reply(call_wrapper.inner_mut())?
            .value32()
            .map(|it| {
                let mut net_wm_state = NetWmState::default();
                for atom in it {
                    if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_MODAL)
                            .unwrap()
                            .value
                    {
                        net_wm_state.modal = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_FULLSCREEN)
                            .unwrap()
                            .value
                    {
                        net_wm_state.fullscreen = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_DEMANDS_ATTENTION)
                            .unwrap()
                            .value
                    {
                        net_wm_state.demands_attention = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_HIDDEN)
                            .unwrap()
                            .value
                    {
                        net_wm_state.hidden = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_ABOVE)
                            .unwrap()
                            .value
                    {
                        net_wm_state.above = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_BELOW)
                            .unwrap()
                            .value
                    {
                        net_wm_state.below = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_MAXIMIZED_HORZ)
                            .unwrap()
                            .value
                    {
                        net_wm_state.maximized_horz = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_MAXIMIZED_VERT)
                            .unwrap()
                            .value
                    {
                        net_wm_state.maximized_vert = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_SHADED)
                            .unwrap()
                            .value
                    {
                        net_wm_state.shaded = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_STICKY)
                            .unwrap()
                            .value
                    {
                        net_wm_state.sticky = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_SKIP_PAGER)
                            .unwrap()
                            .value
                    {
                        net_wm_state.skip_pager = true;
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_STATE_SKIP_TASKBAR)
                            .unwrap()
                            .value
                    {
                        net_wm_state.skip_taskbar = true;
                    }
                }
                net_wm_state
            });
        Ok(state)
    }
}

pub(crate) struct WindowTypesCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl WindowTypesCookie {
    pub(crate) fn await_types(
        self,
        call_wrapper: &mut CallWrapper,
    ) -> Result<heapless::Vec<WindowType, 12>> {
        let types = self
            .inner
            .reply(call_wrapper.inner_mut())?
            .value32()
            .map(|it| {
                let mut window_types = heapless::Vec::new();
                for atom in it {
                    if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_DESKTOP)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Desktop);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_DIALOG)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Dialog);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_DOCK)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Dock);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_MENU)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Menu);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_SPLASH)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Splash);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_TOOLBAR)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Toolbar);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_UTILITY)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Utility);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_WINDOW_TYPE_NORMAL)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(window_types, WindowType::Normal);
                    }
                }
                window_types
            })
            .unwrap_or_default();
        Ok(types)
    }
}

pub(crate) struct ProtocolsCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl ProtocolsCookie {
    pub(crate) fn await_protocols(
        self,
        call_wrapper: &mut CallWrapper,
    ) -> Result<heapless::Vec<Protocol, 4>> {
        let protocols = self
            .inner
            .reply(&mut call_wrapper.connection)?
            .value32()
            .map(|it| {
                let mut protocols = heapless::Vec::new();
                for atom in it {
                    if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&WM_DELETE_WINDOW)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(protocols, Protocol::Delete);
                    } else if atom == call_wrapper.name_to_atom.get(&WM_TAKE_FOCUS).unwrap().value {
                        let _ = push_heapless!(protocols, Protocol::TakeFocus);
                    } else if atom
                        == call_wrapper
                            .name_to_atom
                            .get(&_NET_WM_SYNC_REQUEST)
                            .unwrap()
                            .value
                    {
                        let _ = push_heapless!(protocols, Protocol::Sync);
                    } else if atom == call_wrapper.name_to_atom.get(&_NET_WM_PING).unwrap().value {
                        let _ = push_heapless!(protocols, Protocol::Ping);
                    }
                }
                protocols
            })
            .unwrap_or_default();
        Ok(protocols)
    }
}

pub(crate) struct SingleCardCookie {
    pub(crate) inner: Cookie<GetPropertyReply>,
}

impl SingleCardCookie {
    pub(crate) fn await_card(self, call_wrapper: &mut CallWrapper) -> Result<Option<u32>> {
        Ok(self.inner.reply(call_wrapper.inner_mut())?.first_u32())
    }
}

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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum WindowFloatDeduction {
    Floating { parent: Option<Window> },
    Docked { parent: Option<Window> },
}
