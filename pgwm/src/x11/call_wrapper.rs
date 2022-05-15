use crate::error::{Error, Result};
use heapless::FnvIndexMap;
use x11rb::connection::Connection;
use x11rb::cookie::{Cookie, VoidCookie};
use x11rb::properties::{WmHints, WmHintsCookie, WmSizeHints, WmSizeHintsCookie};
use x11rb::protocol::render::{
    CreatePictureAux, Glyphinfo, Glyphset, PictOp, Picture, PolyEdge, PolyMode, Repeat,
};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureRequestEvent,
    ConfigureWindowAux, ConnectionExt, EventMask, GetPropertyReply, GetWindowAttributesReply,
    GrabMode, InputFocus, InternAtomReply, PropMode, QueryPointerReply, Screen, StackMode, Window,
};
use x11rb::protocol::ErrorKind;
use x11rb::rust_connection::{ReplyError, SingleThreadedRustConnection};
use x11rb::{CURRENT_TIME, NONE};

use crate::error::Error::GlyphMismatch;
use pgwm_core::config::{WINDOW_MANAGER_NAME, WINDOW_MANAGER_NAME_BUF_SIZE};
use pgwm_core::geometry::Dimensions;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderVisualInfo};
use pgwm_core::state::workspace::FocusStyle;
use pgwm_core::state::State;

use super::cookies::{
    ClassConvertCookie, DimensionsCookie, FallbackNameConvertCookie, QueryTreeCookie,
    TransientConvertCookie,
};

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
            fn init_maps(connection: &SingleThreadedRustConnection) -> Result<(FnvIndexMap<&'static [u8], ResolvedAtom, 32>, FnvIndexMap<Atom, ResolvedAtom, 32>)> {
                    let mut name_to_atom = FnvIndexMap::new();
                    let mut atom_to_resolved = FnvIndexMap::new();
                    let mut cookies = heapless::Deque::<Cookie<SingleThreadedRustConnection, InternAtomReply>, 32>::new();
        $(
                    cookies.push_back(connection.intern_atom(false, $const_name)?)
                    .expect("Not enough space for intern atoms");

        )*
        $(
                    let atom = cookies.pop_front().unwrap().reply()?.atom;
                    name_to_atom.insert(
                        $const_name,
                        ResolvedAtom {
                            value: atom,
                            intern_atom: SupportedAtom::$enum_name,
                            ewmh: $is_ewmh,
                        },
                    )
                    .expect("Not enough space to store atoms in named AtomWmMap");
                    atom_to_resolved.insert(
                        atom,
                        ResolvedAtom {
                            value: atom,
                            intern_atom: SupportedAtom::$enum_name,
                            ewmh: $is_ewmh,
                        },
                    )
                    .expect("Not enough space to store atoms in named AtomWmMap");
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
    _NET_WM_WINDOW_TYPE,
    NetWmWindowType,
    true,
    _NET_WM_WINDOW_TYPE_DIALOG,
    NetWmWindowTypeDialog,
    true,
    _NET_WM_WINDOW_TYPE_NORMAL,
    NetWmWindowTypeNormal,
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
    _NET_WM_STATE,
    NetWmState,
    true,
    _NET_WM_STATE_FULLSCREEN,
    NetWmStateFullscreen,
    true,
    _NET_WM_STATE_DEMANDS_ATTENTION,
    NetWmStateDemandsAttention,
    true,
    _NET_WM_STATE_MODAL,
    NetWmStateModal,
    true,
    _NET_WM_STATE_ABOVE,
    NetWmStateAbove,
    true
);

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedAtom {
    pub(crate) intern_atom: SupportedAtom,
    pub(crate) value: u32,
    pub(crate) ewmh: bool,
}

pub(crate) struct CallWrapper<'a> {
    connection: &'a SingleThreadedRustConnection,
    name_to_atom: FnvIndexMap<&'static [u8], ResolvedAtom, 32>,
    atom_to_resolved: FnvIndexMap<Atom, ResolvedAtom, 32>,
}

impl<'a> CallWrapper<'a> {
    pub(crate) fn try_become_wm(&self, screen: &Screen) -> Result<()> {
        let change = ChangeWindowAttributesAux::default()
            .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY);
        let res = self
            .connection
            .change_window_attributes(screen.root, &change)?
            .check();
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
    pub(crate) fn set_default_manager_props(&self, state: &State) -> Result<()> {
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_SUPPORTED].value,
            AtomEnum::ATOM,
            self.name_to_atom
                .iter()
                .filter(|supported| supported.1.ewmh)
                .map(|val| val.1.value)
                .collect::<heapless::Vec<u32, 32>>()
                .as_slice(),
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_CLIENT_LIST].value,
            AtomEnum::WINDOW,
            &[],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_NUMBER_OF_DESKTOPS].value,
            AtomEnum::CARDINAL,
            &[0],
        )?;
        let utf8 = WINDOW_MANAGER_NAME
            .chars()
            .chain(std::iter::once('\u{0}'))
            .map(|ch| ch as u32)
            .collect::<heapless::Vec<u32, WINDOW_MANAGER_NAME_BUF_SIZE>>();
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_DESKTOP_NAMES].value,
            AtomEnum::STRING,
            utf8.as_slice(),
        )?;

        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_CURRENT_DESKTOP].value,
            AtomEnum::CARDINAL,
            &[0],
        )?;

        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_DESKTOP_VIEWPORT].value,
            AtomEnum::CARDINAL,
            &[0; 2],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_DESKTOP_GEOMETRY].value,
            AtomEnum::CARDINAL,
            &[
                state.screen.width_in_pixels as u32,
                state.screen.height_in_pixels as u32,
            ],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_WORKAREA].value,
            AtomEnum::CARDINAL,
            &[
                0,
                state.status_bar_height as u32,
                state.screen.width_in_pixels as u32,
                state.screen.height_in_pixels as u32 - state.status_bar_height as u32,
            ],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
            AtomEnum::CARDINAL,
            &[],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_SUPPORTING_WM_CHECK].value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
        )?;
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom[_NET_SUPPORTING_WM_CHECK].value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
        )?;
        x11rb::wrapper::ConnectionExt::change_property8(
            self.connection,
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom[_NET_WM_NAME].value,
            AtomEnum::STRING,
            WINDOW_MANAGER_NAME.as_bytes(),
        )?;
        Ok(())
    }

    pub(crate) fn set_base_client_event_mask(
        &self,
        window: Window,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cw = ChangeWindowAttributesAux::new().event_mask(
            EventMask::ENTER_WINDOW
                | EventMask::FOCUS_CHANGE
                | EventMask::PROPERTY_CHANGE
                | EventMask::VISIBILITY_CHANGE
                | EventMask::EXPOSURE
                | EventMask::STRUCTURE_NOTIFY,
        );
        Ok(self.connection.change_window_attributes(window, &cw)?)
    }

    pub fn push_to_client_list(&self, root: Window, new_win: Window) -> Result<()> {
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::APPEND,
            root,
            self.name_to_atom[_NET_CLIENT_LIST].value,
            AtomEnum::WINDOW,
            &[new_win],
        )?;
        Ok(())
    }

    pub fn update_client_list(
        &self,
        managed: &[Window],
        state: &State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        Ok(x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_CLIENT_LIST].value,
            AtomEnum::WINDOW,
            managed,
        )?)
    }

    pub(crate) fn get_hints(
        &self,
        window: Window,
    ) -> Result<WmHintsCookie<'a, SingleThreadedRustConnection>> {
        Ok(WmHints::get(self.connection, window)?)
    }

    pub(crate) fn query_subwindows(&self, window: Window) -> Result<QueryTreeCookie> {
        Ok(QueryTreeCookie {
            inner: self.connection.query_tree(window)?,
        })
    }

    pub(crate) fn query_pointer(
        &self,
        state: &State,
    ) -> Result<Cookie<'a, SingleThreadedRustConnection, QueryPointerReply>> {
        Ok(self.connection.query_pointer(state.screen.root)?)
    }

    pub(crate) fn get_dimensions(&self, window: Window) -> Result<DimensionsCookie> {
        Ok(DimensionsCookie {
            inner: self.connection.get_geometry(window)?,
        })
    }

    pub(crate) fn get_window_attributes(
        &self,
        window: Window,
    ) -> Result<Cookie<'a, SingleThreadedRustConnection, GetWindowAttributesReply>> {
        Ok(self.connection.get_window_attributes(window)?)
    }

    pub(crate) fn get_class_names(&self, win: Window) -> Result<ClassConvertCookie> {
        let inner = self.connection.get_property(
            false,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            64,
        )?;
        Ok(ClassConvertCookie { inner })
    }
    pub(crate) fn get_name(&self, win: Window) -> Result<FallbackNameConvertCookie> {
        Ok(FallbackNameConvertCookie {
            ewmh_inner: self.connection.get_property(
                false,
                win,
                self.name_to_atom[_NET_WM_NAME].value,
                AtomEnum::ANY,
                0,
                1028,
            )?,
            wm_inner: self.connection.get_property(
                false,
                win,
                AtomEnum::WM_NAME,
                AtomEnum::ANY,
                0,
                1028,
            )?,
        })
    }

    pub(crate) fn get_is_transient_for(&self, win: Window) -> Result<TransientConvertCookie> {
        let inner = self.connection.get_property(
            false,
            win,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            32,
        )?;
        Ok(TransientConvertCookie { inner })
    }

    pub(crate) fn get_float_indicators(&self, window: Window) -> Result<FloatDeductionCookie> {
        let transient_for = self.connection.get_property(
            false,
            window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            32,
        )?;
        // There are 12 possible WM_STATES represented at Atoms (u32), we'll go with 16 for safety.
        let long_length = 32 * 16u32;

        let modality = self.connection.get_property(
            false,
            window,
            self.name_to_atom[_NET_WM_STATE].value,
            AtomEnum::ATOM,
            0,
            long_length,
        )?;

        // There are 8 possible window types, 8 is a good number and we'll stick with that
        let long_length = 32 * 8u32;
        let window_type = self.connection.get_property(
            false,
            window,
            self.name_to_atom[_NET_WM_WINDOW_TYPE].value,
            AtomEnum::ATOM,
            0,
            long_length,
        )?;

        Ok(FloatDeductionCookie {
            call_wrapper: self,
            size_hints: WmSizeHints::get_normal_hints(self.connection, window)?,
            transient_convert: transient_for,
            ewmh_state_convert: modality,
            ewmh_window_type_convert: window_type,
        })
    }

    fn convert_atom_vector(&self, reply: GetPropertyReply) -> heapless::Vec<SupportedAtom, 16> {
        reply
            .value32()
            .into_iter()
            .flatten()
            .filter_map(|atom| {
                self.atom_to_resolved
                    .get(&atom)
                    .map(|resolved| resolved.intern_atom)
            })
            .collect()
    }

    pub(crate) fn bulk_set_extents(&self, windows: &[Window], border_width: u32) -> Result<()> {
        for win in windows {
            self.set_extents(*win, border_width)?;
        }
        Ok(())
    }

    pub(crate) fn set_extents(&self, win: Window, border_width: u32) -> Result<()> {
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            win,
            self.name_to_atom[_NET_FRAME_EXTENTS].value,
            AtomEnum::CARDINAL,
            &[border_width; 4],
        )?;
        Ok(())
    }

    pub(crate) fn set_state(
        &self,
        win: Window,
        state: WmState,
    ) -> Result<VoidCookie<SingleThreadedRustConnection>> {
        Ok(x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            win,
            self.name_to_atom[WM_STATE].value,
            self.name_to_atom[WM_STATE].value,
            &[state.get_value()],
        )?)
    }

    pub(crate) fn get_state(&self, win: Window) -> Result<Option<WmState>> {
        let prop = self.connection.get_property(
            false,
            win,
            self.name_to_atom[WM_STATE].value,
            self.name_to_atom[WM_STATE].value,
            0,
            4,
        )?;
        if let Some(val) = prop
            .reply()
            .ok()
            .and_then(|val| val.value32().and_then(|mut v| v.next()))
        {
            Ok(WmState::from_value(val))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn set_root_event_mask(
        &self,
        cursor_handle: &x11rb::cursor::Handle,
        state: &State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
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
            .cursor(cursor_handle.load_cursor(self.connection, state.cursor_name.as_str())?);

        Ok(self
            .connection
            .change_window_attributes(state.screen.root, &change_attrs_aux)?)
    }

    pub(crate) fn grab_pointer(&self, state: &State) -> Result<()> {
        self.connection.grab_pointer(
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
        )?;
        Ok(())
    }

    pub(crate) fn ungrab_pointer(&self) -> Result<()> {
        self.connection.ungrab_pointer(CURRENT_TIME)?;
        Ok(())
    }

    /// Handling x10 style windows becomes strange: <https://tronche.com/gui/x/xlib/ICC/client-to-window-manager/wm-hints.html>
    pub(crate) fn take_focus(
        &self,
        root: Window,
        target: Window,
        focus_style: FocusStyle,
    ) -> Result<()> {
        match focus_style {
            FocusStyle::Push { group_leader } => {
                if let Some(leader) = group_leader {
                    self.connection
                        .set_input_focus(InputFocus::PARENT, leader, CURRENT_TIME)?;
                } else {
                    pgwm_core::debug!("Found window with focus_style pull with no group leader");
                    self.connection
                        .set_input_focus(InputFocus::PARENT, target, CURRENT_TIME)?;
                }
            }
            FocusStyle::Pull => {
                self.connection
                    .set_input_focus(InputFocus::PARENT, target, CURRENT_TIME)?;
            }
        }
        let target = if target == root {
            // No active window if root gets focused
            NONE
        } else {
            self.send_take_focus(target)?;
            target
        };
        let data = [target, CURRENT_TIME];
        x11rb::wrapper::ConnectionExt::change_property32(
            self.connection,
            PropMode::REPLACE,
            root,
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
            AtomEnum::WINDOW,
            &data,
        )?;
        Ok(())
    }

    pub(crate) fn reset_root_focus(
        &self,
        state: &State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        self.connection.delete_property(
            state.screen.root,
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
        )?;
        Ok(self.connection.set_input_focus(
            InputFocus::POINTER_ROOT,
            u32::from(InputFocus::POINTER_ROOT),
            CURRENT_TIME,
        )?)
    }

    fn send_take_focus(&self, win: Window) -> Result<VoidCookie<SingleThreadedRustConnection>> {
        let event = ClientMessageEvent::new(
            32,
            win,
            self.name_to_atom[WM_PROTOCOLS].value,
            [
                self.name_to_atom[WM_TAKE_FOCUS].value,
                CURRENT_TIME,
                0,
                0,
                0,
            ],
        );
        pgwm_core::debug!("Sending WM_TAKE_FOCUS focus for {}", win);
        Ok(self
            .connection
            .send_event(false, win, EventMask::NO_EVENT, &event)?)
    }

    pub(crate) fn send_delete(
        &self,
        win: Window,
    ) -> Result<VoidCookie<SingleThreadedRustConnection>> {
        let event = ClientMessageEvent::new(
            32,
            win,
            self.name_to_atom[WM_PROTOCOLS].value,
            [self.name_to_atom[WM_DELETE_WINDOW].value, 0, 0, 0, 0],
        );
        pgwm_core::debug!("Sending delete for {}", win);
        Ok(self
            .connection
            .send_event(false, win, EventMask::NO_EVENT, &event)?)
    }

    pub(crate) fn map_window(
        &self,
        window: Window,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cookie = self.connection.map_window(window)?;
        // Triggers an enter-notify that needs to be ignored
        state.push_sequence(cookie.sequence_number() as u16);
        Ok(cookie)
    }

    pub(crate) fn unmap_window(
        &self,
        window: Window,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cookie = self.connection.unmap_window(window)?;
        // Triggers an enter-notify that needs to be ignored, we also don't want to react to an UnmapNotify that we created
        state.push_sequence(cookie.sequence_number() as u16);
        Ok(cookie)
    }

    pub(crate) fn destroy_window(
        &self,
        window: Window,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        Ok(self.connection.destroy_window(window)?)
    }

    pub(crate) fn kill_client(
        &self,
        window: Window,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        Ok(self.connection.kill_client(window)?)
    }

    pub(crate) fn push_window_to_top(
        &self,
        window: Window,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cfg = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        self.do_configure(window, &cfg, state)
    }

    pub(crate) fn configure_window(
        &self,
        window: Window,
        dimension: Dimensions,
        border_width: u32,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
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
    pub(crate) fn configure_from_request(
        &self,
        event: &ConfigureRequestEvent,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cfg = ConfigureWindowAux::from_configure_request(event);
        if let Some(border_width) = cfg.border_width {
            self.set_extents(event.window, border_width)?;
        }
        Ok(self.connection.configure_window(event.window, &cfg)?)
    }

    pub(crate) fn move_window(
        &self,
        window: Window,
        x: i32,
        y: i32,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cfg = ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .stack_mode(StackMode::ABOVE);
        self.do_configure(window, &cfg, state)
    }

    pub(crate) fn resize_window(
        &self,
        window: Window,
        height: u32,
        width: u32,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cfg = ConfigureWindowAux::new().height(height).width(width);
        self.do_configure(window, &cfg, state)
    }

    fn do_configure(
        &self,
        window: Window,
        cfg: &ConfigureWindowAux,
        state: &mut State,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cookie = self.connection.configure_window(window, cfg)?;
        // Triggers an enter-notify that needs to be ignored
        state.push_sequence(cookie.sequence_number() as u16);
        Ok(cookie)
    }

    pub(crate) fn change_border_color(
        &self,
        window: Window,
        pixel: u32,
    ) -> Result<VoidCookie<'a, SingleThreadedRustConnection>> {
        let cw = ChangeWindowAttributesAux::new().border_pixel(pixel);
        Ok(self.connection.change_window_attributes(window, &cw)?)
    }

    pub(crate) fn window_mapped_picture(
        &self,
        win: Window,
        vis_info: &RenderVisualInfo,
    ) -> Result<Picture> {
        let picture = self.connection.generate_id()?;
        x11rb::protocol::render::create_picture(
            self.connection,
            picture,
            win,
            vis_info.root.pict_format,
            &CreatePictureAux::new()
                .polyedge(PolyEdge::SMOOTH)
                .polymode(PolyMode::IMPRECISE),
        )?;
        Ok(picture)
    }

    pub(crate) fn pixmap_mapped_picture(
        &self,
        win: Window,
        vis_info: &RenderVisualInfo,
    ) -> Result<Picture> {
        let picture = self.connection.generate_id()?;
        x11rb::protocol::render::create_picture(
            self.connection,
            picture,
            win,
            vis_info.render.pict_format,
            &CreatePictureAux::new().repeat(Repeat::NORMAL),
        )?;
        Ok(picture)
    }

    pub(crate) fn create_glyphset(&self, vis_info: &RenderVisualInfo) -> Result<Glyphset> {
        let id = self.connection.generate_id()?;
        x11rb::protocol::render::create_glyph_set(
            self.connection,
            id,
            vis_info.render.pict_format,
        )?;
        Ok(id)
    }

    pub(crate) fn add_glyphs(
        &self,
        glyph_set: Glyphset,
        glyph_ids: &[u32],
        glyph_info: &[Glyphinfo],
        rendered_picture_glyphs: &[u8],
    ) -> Result<()> {
        if !glyph_ids.len() == glyph_info.len() {
            return Err(GlyphMismatch);
        }
        x11rb::protocol::render::add_glyphs(
            self.connection,
            glyph_set,
            glyph_ids,
            glyph_info,
            rendered_picture_glyphs,
        )?;
        Ok(())
    }

    pub(crate) fn fill_xrender_rectangle(
        &self,
        picture: Picture,
        color: x11rb::protocol::render::Color,
        dimensions: Dimensions,
    ) -> Result<()> {
        //let (red, green, blue, alpha) = color.to_rgba16();
        x11rb::protocol::render::fill_rectangles(
            self.connection,
            PictOp::SRC,
            picture,
            color,
            &[dimensions.to_rectangle()],
        )?;
        Ok(())
    }

    // https://lists.freedesktop.org/archives/xcb/2006-October/002157.html
    // Can push everything in one request when switching glyphs instead of chunking
    pub(crate) fn draw_glyphs(
        &self,
        x: i16,
        y: i16,
        glyphs: Glyphset,
        dbw: &DoubleBufferedRenderPicture,
        glyph_ids: &[u32],
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
        x11rb::protocol::render::composite_glyphs32(
            self.connection,
            PictOp::OVER,
            dbw.pixmap.picture,
            dbw.window.picture,
            0,
            glyphs,
            0,
            0,
            &buf,
        )?;
        Ok(())
    }

    pub(crate) fn resolve_atom(&self, atom: Atom) -> Option<ResolvedAtom> {
        self.atom_to_resolved.get(&atom).copied()
    }

    pub fn new(connection: &'a SingleThreadedRustConnection) -> Result<Self> {
        let (name_to_atom, atom_to_resolved) = init_maps(connection)?;
        Ok(CallWrapper {
            connection,
            name_to_atom,
            atom_to_resolved,
        })
    }
}

fn convert_first_u32(reply: GetPropertyReply) -> Option<u32> {
    reply.value32().and_then(|mut it| it.next())
}
#[derive(PartialEq, Eq, Debug)]
pub enum WmState {
    Withdrawn,
    Normal,
    Iconic,
}

impl WmState {
    const fn get_value(&self) -> u32 {
        match self {
            WmState::Withdrawn => 0,
            WmState::Normal => 1,
            WmState::Iconic => 3,
        }
    }
    fn from_value(val: u32) -> Option<Self> {
        match val {
            0 => Some(WmState::Withdrawn),
            1 => Some(WmState::Normal),
            3 => Some(WmState::Iconic),
            _ => None,
        }
    }
}

pub(crate) struct FloatDeductionCookie<'a> {
    call_wrapper: &'a CallWrapper<'a>,
    size_hints: WmSizeHintsCookie<'a, SingleThreadedRustConnection>,
    transient_convert: Cookie<'a, SingleThreadedRustConnection, GetPropertyReply>,
    ewmh_state_convert: Cookie<'a, SingleThreadedRustConnection, GetPropertyReply>,
    ewmh_window_type_convert: Cookie<'a, SingleThreadedRustConnection, GetPropertyReply>,
}

impl<'a> FloatDeductionCookie<'a> {
    pub(crate) fn await_float_indicators(self) -> FloatIndicators {
        let transient_for = self
            .transient_convert
            .reply()
            .ok()
            .and_then(convert_first_u32);
        let (no_window_type, is_normal, is_modal) = self
            .ewmh_window_type_convert
            .reply()
            .ok()
            .map(|reply| self.call_wrapper.convert_atom_vector(reply))
            .and_then(|window_types| {
                pgwm_core::debug!("Window types {window_types:?}");
                for wt in window_types {
                    match wt {
                        SupportedAtom::NetWmWindowTypeDialog => {
                            return Some((false, false, true));
                        }
                        SupportedAtom::NetWmWindowTypeNormal => {
                            return Some((false, true, false));
                        }
                        _ => {}
                    }
                }
                None
            })
            .unwrap_or((true, false, false));
        let is_dialog = self
            .ewmh_state_convert
            .reply()
            .map(|reply| self.call_wrapper.convert_atom_vector(reply))
            .map(|states| states.contains(&SupportedAtom::NetWmStateModal))
            .unwrap_or(false);
        // Could just make a frame window, the main problem here is that undersized windows look weird
        let size_hints = self.size_hints.reply().ok();
        FloatIndicators {
            transient_for,
            no_window_type,
            is_normal,
            is_modal,
            is_dialog,
            size_hints,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
pub(crate) struct FloatIndicators {
    pub(crate) transient_for: Option<Window>,
    pub(crate) no_window_type: bool,
    pub(crate) is_normal: bool,
    pub(crate) is_modal: bool,
    pub(crate) is_dialog: bool,
    pub(crate) size_hints: Option<WmSizeHints>,
}

pub(crate) enum WindowFloatDeduction {
    Floating { parent: Option<Window> },
    Docked { parent: Option<Window> },
}
