use crate::error::{Error, Result};
use heapless::FnvIndexMap;
use x11rb::cookie::{Cookie, VoidCookie};
use x11rb::protocol::render::{
    CreatePictureAux, Glyphinfo, Glyphset, PictOp, Picture, PolyEdge, PolyMode, Repeat,
};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureRequestEvent,
    ConfigureWindowAux, EventMask, GetPropertyReply, GetWindowAttributesReply, GrabMode,
    InputFocus, InternAtomReply, PropMode, QueryPointerReply, Screen, StackMode, Window,
};
use x11rb::protocol::ErrorKind;
use x11rb::xcb::xproto;
use x11rb::{CURRENT_TIME, NONE};

use crate::error::Error::GlyphMismatch;
use crate::wm::XorgConnection;
use pgwm_core::config::{WINDOW_MANAGER_NAME, WINDOW_MANAGER_NAME_BUF_SIZE};
use pgwm_core::geometry::Dimensions;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderVisualInfo};
use pgwm_core::state::workspace::FocusStyle;
use pgwm_core::state::State;
use x11rb::errors::ReplyError;
use x11rb::properties::{WmHints, WmHintsCookie, WmSizeHints, WmSizeHintsCookie};

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
            fn init_maps(connection: &mut XorgConnection) -> Result<(FnvIndexMap<&'static [u8], ResolvedAtom, 32>, FnvIndexMap<Atom, ResolvedAtom, 32>)> {
                    let mut name_to_atom = FnvIndexMap::new();
                    let mut atom_to_resolved = FnvIndexMap::new();
                    let mut cookies = heapless::Deque::<Cookie<InternAtomReply>, 32>::new();
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

pub(crate) struct CallWrapper {
    pub(crate) connection: XorgConnection,
    name_to_atom: FnvIndexMap<&'static [u8], ResolvedAtom, 32>,
    atom_to_resolved: FnvIndexMap<Atom, ResolvedAtom, 32>,
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
            self.name_to_atom[_NET_SUPPORTED].value,
            AtomEnum::ATOM,
            self.name_to_atom
                .iter()
                .filter(|supported| supported.1.ewmh)
                .map(|val| val.1.value)
                .collect::<heapless::Vec<u32, 32>>()
                .as_slice(),
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_CLIENT_LIST].value,
            AtomEnum::WINDOW,
            &[],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_NUMBER_OF_DESKTOPS].value,
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
            self.name_to_atom[_NET_DESKTOP_NAMES].value,
            AtomEnum::STRING,
            utf8.as_slice(),
            true,
        )?;

        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_CURRENT_DESKTOP].value,
            AtomEnum::CARDINAL,
            &[0],
            true,
        )?;

        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_DESKTOP_VIEWPORT].value,
            AtomEnum::CARDINAL,
            &[0; 2],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_DESKTOP_GEOMETRY].value,
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
            self.name_to_atom[_NET_WORKAREA].value,
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
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
            AtomEnum::CARDINAL,
            &[],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.screen.root,
            self.name_to_atom[_NET_SUPPORTING_WM_CHECK].value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
            true,
        )?;
        self.connection.change_property32(
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom[_NET_SUPPORTING_WM_CHECK].value,
            AtomEnum::WINDOW,
            &[state.wm_check_win],
            true,
        )?;
        self.connection.change_property8(
            PropMode::REPLACE,
            state.wm_check_win,
            self.name_to_atom[_NET_WM_NAME].value,
            AtomEnum::STRING,
            WINDOW_MANAGER_NAME.as_bytes(),
            true,
        )?;
        Ok(())
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

    pub fn push_to_client_list(&mut self, root: Window, new_win: Window) -> Result<()> {
        self.connection.change_property32(
            PropMode::APPEND,
            root,
            self.name_to_atom[_NET_CLIENT_LIST].value,
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
            self.name_to_atom[_NET_CLIENT_LIST].value,
            AtomEnum::WINDOW,
            managed,
            true,
        )?;
        Ok(())
    }

    pub(crate) fn get_hints(&mut self, window: Window) -> Result<WmHintsCookie> {
        Ok(WmHints::get(&mut self.connection, window)?)
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

    pub(crate) fn get_class_names(&mut self, win: Window) -> Result<ClassConvertCookie> {
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
        Ok(ClassConvertCookie { inner })
    }
    pub(crate) fn get_name(&mut self, win: Window) -> Result<FallbackNameConvertCookie> {
        Ok(FallbackNameConvertCookie {
            ewmh_inner: xproto::get_property(
                &mut self.connection,
                false,
                win,
                self.name_to_atom[_NET_WM_NAME].value,
                AtomEnum::ANY,
                0,
                1028,
                false,
            )?,
            wm_inner: xproto::get_property(
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

    pub(crate) fn get_is_transient_for(&mut self, win: Window) -> Result<TransientConvertCookie> {
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
        Ok(TransientConvertCookie { inner })
    }

    pub(crate) fn get_float_indicators(&mut self, window: Window) -> Result<FloatDeductionCookie> {
        let transient_for = xproto::get_property(
            &mut self.connection,
            false,
            window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            32,
            false,
        )?;
        // There are 12 possible WM_STATES represented at Atoms (u32), we'll go with 16 for safety.
        let long_length = 32 * 16u32;

        let modality = xproto::get_property(
            &mut self.connection,
            false,
            window,
            self.name_to_atom[_NET_WM_STATE].value,
            AtomEnum::ATOM,
            0,
            long_length,
            false,
        )?;

        // There are 8 possible window types, 8 is a good number and we'll stick with that
        let long_length = 32 * 8u32;
        let window_type = xproto::get_property(
            &mut self.connection,
            false,
            window,
            self.name_to_atom[_NET_WM_WINDOW_TYPE].value,
            AtomEnum::ATOM,
            0,
            long_length,
            false,
        )?;

        Ok(FloatDeductionCookie {
            size_hints: WmSizeHints::get_normal_hints(&mut self.connection, window)?,
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
            self.name_to_atom[_NET_FRAME_EXTENTS].value,
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
            self.name_to_atom[WM_STATE].value,
            self.name_to_atom[WM_STATE].value,
            &[state.get_value()],
            true,
        )?;
        Ok(())
    }

    pub(crate) fn get_state(&mut self, win: Window) -> Result<Option<WmState>> {
        let prop = xproto::get_property(
            &mut self.connection,
            false,
            win,
            self.name_to_atom[WM_STATE].value,
            self.name_to_atom[WM_STATE].value,
            0,
            4,
            false,
        )?;
        if let Some(val) = prop
            .reply(&mut self.connection)
            .ok()
            .and_then(|val| val.value32().and_then(|mut v| v.next()))
        {
            Ok(WmState::from_value(val))
        } else {
            Ok(None)
        }
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
    ) -> Result<()> {
        match focus_style {
            FocusStyle::Push { group_leader } => {
                if let Some(leader) = group_leader {
                    xproto::set_input_focus(
                        &mut self.connection,
                        InputFocus::PARENT,
                        leader,
                        CURRENT_TIME,
                        true,
                    )?;
                } else {
                    pgwm_core::debug!("Found window with focus_style pull with no group leader");
                    xproto::set_input_focus(
                        &mut self.connection,
                        InputFocus::PARENT,
                        target,
                        CURRENT_TIME,
                        true,
                    )?;
                }
            }
            FocusStyle::Pull => {
                xproto::set_input_focus(
                    &mut self.connection,
                    InputFocus::PARENT,
                    target,
                    CURRENT_TIME,
                    true,
                )?;
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
        self.connection.change_property32(
            PropMode::REPLACE,
            root,
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
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
            self.name_to_atom[_NET_ACTIVE_WINDOW].value,
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

    fn send_take_focus(&mut self, win: Window) -> Result<()> {
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
            self.name_to_atom[WM_PROTOCOLS].value,
            [self.name_to_atom[WM_DELETE_WINDOW].value, 0, 0, 0, 0],
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

    pub fn new(mut connection: XorgConnection) -> Result<Self> {
        let (name_to_atom, atom_to_resolved) = init_maps(&mut connection)?;
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

pub(crate) struct FloatDeductionCookie {
    size_hints: WmSizeHintsCookie,
    transient_convert: Cookie<GetPropertyReply>,
    ewmh_state_convert: Cookie<GetPropertyReply>,
    ewmh_window_type_convert: Cookie<GetPropertyReply>,
}

impl FloatDeductionCookie {
    pub(crate) fn await_float_indicators(self, call_wrapper: &mut CallWrapper) -> FloatIndicators {
        let transient_for = self
            .transient_convert
            .reply(&mut call_wrapper.connection)
            .ok()
            .and_then(convert_first_u32);
        let (no_window_type, is_normal, is_modal) = self
            .ewmh_window_type_convert
            .reply(&mut call_wrapper.connection)
            .ok()
            .map(|reply| call_wrapper.convert_atom_vector(reply))
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
            .reply(&mut call_wrapper.connection)
            .map(|reply| call_wrapper.convert_atom_vector(reply))
            .map(|states| states.contains(&SupportedAtom::NetWmStateModal))
            .unwrap_or(false);
        // Could just make a frame window, the main problem here is that undersized windows look weird
        let size_hints = self.size_hints.reply(&mut call_wrapper.connection).ok();
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
