pub(crate) mod bar;
pub(crate) mod draw;
pub(crate) mod font;

use crate::error::{Error, Result};
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::x11::call_wrapper::{
    CallWrapper, FloatIndicators, Protocol, WindowFloatDeduction, WmState,
};
use crate::x11::client_message::{ChangeType, ClientMessage, PropertyChangeMessage};
use crate::x11::cookies::{FallbackNameConvertCookie, TransientConvertCookie};
use pgwm_core::config::mouse_map::MouseTarget;
#[cfg(feature = "status-bar")]
use pgwm_core::config::STATUS_BAR_CHECK_CONTENT_LIMIT;
use pgwm_core::config::{Action, APPLICATION_WINDOW_LIMIT, WM_CLASS_NAME_LIMIT, WM_NAME_LIMIT};
use pgwm_core::geometry::draw::Mode;
use pgwm_core::geometry::layout::Layout;
use pgwm_core::geometry::Dimensions;
use pgwm_core::push_heapless;
use pgwm_core::state::workspace::{
    ArrangeKind, DeleteResult, FocusStyle, ManagedWindow, Workspaces,
};
use pgwm_core::state::{DragPosition, State, WinMarkedForDeath};
use x11rb::cookie::Cookie;
use x11rb::properties::{WmHints, WmHintsCookie};
use x11rb::protocol::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ConfigureNotifyEvent, ConfigureRequestEvent,
    DestroyNotifyEvent, EnterNotifyEvent, GetWindowAttributesReply, KeyPressEvent, MapRequestEvent,
    MapState, MotionNotifyEvent, NotifyMode, PropertyNotifyEvent, QueryPointerReply,
    UnmapNotifyEvent, Visibility, VisibilityNotifyEvent, Window,
};

pub(crate) struct Manager<'a> {
    drawer: Drawer<'a>,
    bar_manager: BarManager<'a>,
    cursor_handle: x11rb::cursor::Handle,
}

impl<'a> Manager<'a> {
    pub(crate) fn new(
        drawer: Drawer<'a>,
        bar_manager: BarManager<'a>,
        cursor_handle: x11rb::cursor::Handle,
    ) -> Self {
        Self {
            drawer,
            bar_manager,
            cursor_handle,
        }
    }

    pub(crate) fn init(&self, call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        let ch_wa = call_wrapper.set_root_event_mask(&self.cursor_handle, state)?;
        ch_wa.check(call_wrapper.inner_mut())?;
        pgwm_core::debug!("Set root event mask");
        self.bar_manager.draw_static(call_wrapper, state)?;
        pgwm_core::debug!("Drew workspace sections");
        call_wrapper.set_default_manager_props(state)?;
        pgwm_core::debug!("Drew default manager properties");
        Ok(())
    }

    /// Scan for already existing windows and manage them
    pub(crate) fn scan(&self, call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        let subwindows = call_wrapper
            .query_subwindows(state.screen.root)?
            .await_children(call_wrapper.inner_mut())?;
        let mut children_with_properties: heapless::Vec<ScanProperties, APPLICATION_WINDOW_LIMIT> =
            heapless::Vec::new();
        for win in subwindows {
            let state = call_wrapper.get_state(win)?;
            let attr_cookie = call_wrapper.get_window_attributes(win)?;
            let is_transient = call_wrapper.get_is_transient_for(win)?;
            let hints = call_wrapper.get_hints(win)?;
            push_heapless!(
                children_with_properties,
                ScanProperties {
                    window: win,
                    attributes: attr_cookie,
                    transient_cookie: is_transient,
                    wm_state: state,
                    hints,
                }
            )?;
        }
        let mut transients: heapless::Vec<(Window, WmHintsCookie), APPLICATION_WINDOW_LIMIT> =
            heapless::Vec::new();
        let mut non_transients: heapless::Vec<(Window, WmHintsCookie), APPLICATION_WINDOW_LIMIT> =
            heapless::Vec::new();
        for ScanProperties {
            window,
            attributes,
            transient_cookie,
            wm_state,
            hints,
        } in children_with_properties
        {
            if let Ok(attr) = attributes.reply(call_wrapper.inner_mut()) {
                if !attr.override_redirect
                    // If the window is a viewable top level -> manage
                    // Additionally, when the WM starts up, if a WM state is set that's a pretty good
                    // heuristic for whether or not to manage.
                    && (attr.map_state == MapState::VIEWABLE || wm_state.is_some())
                    && !state.intern_created_windows.contains(&window)
                {
                    if transient_cookie
                        .await_is_transient_for(call_wrapper.inner_mut())?
                        .is_some()
                    {
                        push_heapless!(transients, (window, hints))?;
                    } else {
                        push_heapless!(non_transients, (window, hints))?;
                    }
                } else {
                    transient_cookie.inner.forget(call_wrapper.inner_mut());
                    hints.forget(call_wrapper.inner_mut());
                    continue;
                }
            } else {
                transient_cookie.inner.forget(call_wrapper.inner_mut());
                hints.forget(call_wrapper.inner_mut());
            }
        }

        for (win, hints) in non_transients {
            self.manage_window(call_wrapper, win, hints, state)?;
        }
        for (win, hints) in transients {
            self.manage_window(call_wrapper, win, hints, state)?;
        }
        Ok(())
    }

    pub(crate) fn pick_up_state(
        &self,
        call_wrapper: &mut CallWrapper,
        state: &mut State,
    ) -> Result<()> {
        for mon_ind in 0..state.monitors.len() {
            self.drawer.draw_on(call_wrapper, mon_ind, true, state)?;
        }
        self.bar_manager.draw_static(call_wrapper, state)?;
        Ok(())
    }

    pub(crate) fn handle_key_press(
        &self,
        call_wrapper: &mut CallWrapper,
        event: KeyPressEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        if let Some(action) = state.get_key_action(event.detail, event.state) {
            self.exec_action(
                call_wrapper,
                event.event,
                InputSource::Keyboard,
                action.clone(),
                state,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn exec_action(
        &self,
        call_wrapper: &mut CallWrapper,
        origin: Window,
        source: InputSource,
        action: Action,
        state: &mut State,
    ) -> Result<()> {
        match action {
            Action::Restart => {
                Self::cleanup(call_wrapper, state)?;
                return Err(Error::FullRestart);
            }
            Action::Quit => {
                Self::cleanup(call_wrapper, state)?;
                return Err(Error::GracefulShutdown);
            }
            #[cfg_attr(feature = "perf-test", allow(unused_variables))]
            Action::Spawn(cmd, args) => {
                pgwm_core::debug!("Spawning {} with args {:?}", cmd, args);
                #[cfg(not(feature = "perf-test"))]
                std::process::Command::new(cmd)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .args(args)
                    .spawn()?;
            }
            Action::Close => {
                let win = focus_fallback_origin(origin, state);
                self.unmanage_and_kill(call_wrapper, win, state)?;
            }
            Action::ToggleWorkspace(num) => {
                self.toggle_workspace(call_wrapper, num, state.focused_mon, state)?;
            }
            Action::NextTilingMode => {
                let window = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(window) {
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        state.workspaces.cycle_tiling_mode(ws_ind);
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        self.focus_mon(call_wrapper, mon_ind, state)?;
                    }
                }
            }
            Action::CycleDrawMode => {
                let window = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(window) {
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        match state.workspaces.get_draw_mode(ws_ind) {
                            Mode::Tiled(_) => {
                                toggle_tabbed(mon_ind, ws_ind, state)?;
                            }
                            Mode::Tabbed(_) => {
                                state
                                    .workspaces
                                    .set_draw_mode(ws_ind, Mode::Tiled(Layout::LeftLeader));
                            }
                            Mode::Fullscreen { last_draw_mode, .. } => {
                                state
                                    .workspaces
                                    .set_draw_mode(ws_ind, last_draw_mode.to_draw_mode());
                            }
                        }
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        self.focus_mon(call_wrapper, mon_ind, state)?;
                    }
                }
            }
            Action::ResizeWindow(diff) => {
                let window = focus_fallback_origin(origin, state);
                self.resize_win(call_wrapper, diff, window, state)?;
            }
            Action::ResizePadding(diff) => {
                let new_width = state.window_padding + diff;
                if new_width < 0 {
                    state.window_padding = 0;
                } else {
                    state.window_padding = new_width;
                }
                for mon_ind in 0..state.monitors.len() {
                    self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                }
            }
            Action::ResizeBorders(diff) => {
                let new_width = state.window_border_width as i16 + diff;
                let actual_width = if new_width < 0 { 0 } else { new_width as u32 };
                let updated = actual_width != state.window_border_width;
                if actual_width != state.window_border_width {
                    state.window_border_width = actual_width;
                }
                if updated {
                    for mon_ind in 0..state.monitors.len() {
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                    }
                    call_wrapper.bulk_set_extents(
                        state.workspaces.get_all_managed_windows().as_slice(),
                        state.window_border_width,
                    )?;
                }
            }
            Action::ResetToDefaultSizeModifiers => {
                let window = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(window) {
                    state.workspaces.clear_size_modifiers(ws_ind);
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                    }
                }
            }
            Action::SendToFront => {
                let target = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(target) {
                    state.workspaces.send_window_to_front(ws_ind, target);
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        pgwm_core::debug!("Sent {target} to front");
                        self.focus_window(call_wrapper, mon_ind, target, state)?;
                    }
                }
            }
            Action::SendToWorkspace(num) => {
                let num = num;
                let target_window = focus_fallback_origin(origin, state);
                if let Some(ws) = state.workspaces.find_ws_containing_window(target_window) {
                    if ws == num {
                        pgwm_core::debug!("Tried to send to same workspace {}", num);
                    } else {
                        let refocus_parent = if let Some(removed_mw) = self
                            .remove_win_from_state_then_redraw_if_tiled(
                                call_wrapper,
                                target_window,
                                state,
                            )? {
                            call_wrapper.send_unmap(target_window, state)?;
                            removed_mw.focus_style
                        } else {
                            Self::find_focus_style(call_wrapper, target_window)
                                .unwrap_or(FocusStyle::Passive)
                        };
                        state.workspaces.add_child_to_ws(
                            target_window,
                            num,
                            ArrangeKind::NoFloat,
                            refocus_parent,
                        )?;
                        if let Some(target) = state.find_monitor_hosting_workspace(num) {
                            self.drawer.draw_on(call_wrapper, target, true, state)?;
                        }
                    }
                }
            }
            Action::UnFloat => {
                if let Some(input_focus) = state.input_focus {
                    if let Some(mon_ind) = state.find_monitor_index_of_window(input_focus) {
                        if state.workspaces.un_float_window(input_focus).is_some() {
                            pgwm_core::debug!("Unfloating on mon {:?}", mon_ind);
                            self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                            self.focus_window(call_wrapper, mon_ind, input_focus, state)?;
                        }
                    }
                }
            }
            Action::FocusNextWindow => {
                if let Some(cur) = state.input_focus {
                    if let Some(next) = state.workspaces.next_window(cur) {
                        pgwm_core::debug!("Focusnext from {:?} to {:?}", cur, next);
                        self.focus_window(call_wrapper, state.focused_mon, next.window, state)?;
                    }
                }
            }
            Action::FocusPreviousWindow => {
                if let Some(cur) = state.input_focus {
                    if let Some(next) = state.workspaces.prev_window(cur) {
                        self.focus_window(call_wrapper, state.focused_mon, next.window, state)?;
                    }
                }
            }
            Action::FocusNextMonitor => {
                let len = state.monitors.len();
                let next = (state.focused_mon + 1) % len;
                self.focus_mon(call_wrapper, next, state)?;
            }
            Action::FocusPreviousMonitor => {
                let len = state.monitors.len();
                let next = (state.focused_mon as i8 - 1).rem_euclid(len as i8) as usize;
                self.focus_mon(call_wrapper, next, state)?;
            }
            Action::ToggleBar => {
                let mon_ind = state.focused_mon;
                if BarManager::toggle_bar(call_wrapper, mon_ind, state)? {
                    self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                    self.bar_manager.redraw_on(call_wrapper, mon_ind, state)?;
                } else {
                    self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                }
            }
            Action::ToggleFullscreen => {
                let window = focus_fallback_origin(origin, state);
                if let Some((mon_ind, ws_ind)) = state.find_monitor_and_ws_indices_of_window(window)
                {
                    if matches!(
                        state.workspaces.get_draw_mode(ws_ind),
                        Mode::Fullscreen { .. }
                    ) {
                        if state.workspaces.unset_fullscreened(ws_ind) {
                            self.unset_fullscreen(call_wrapper, mon_ind, state)?;
                        }
                    } else {
                        self.set_fullscreen(call_wrapper, mon_ind, ws_ind, window, state)?;
                    }
                }
            }
            Action::MoveWindow => {
                if let InputSource::Mouse(x, y) = source {
                    let dimensions = call_wrapper.get_dimensions(origin)?;
                    Self::conditional_grab_pointer(call_wrapper, state)?;
                    let parent_ws_ind = state.workspaces.find_ws_containing_window(origin);
                    if let Some(parent_ws) = parent_ws_ind {
                        if !state.workspaces.is_managed_floating(origin) {
                            state.workspaces.toggle_floating(
                                origin,
                                parent_ws,
                                ArrangeKind::FloatingActive,
                            );
                            let mon = state.find_monitor_at((x, y)).unwrap_or(0);
                            self.drawer.draw_on(call_wrapper, mon, false, state)?;
                        }
                        let dimensions = dimensions.await_dimensions(call_wrapper.inner_mut())?;
                        state.drag_window =
                            Some((origin, DragPosition::new(dimensions.x, dimensions.y, x, y)));
                        pgwm_core::debug!("Dragging window {}", origin);
                    } else {
                        dimensions.inner.forget(call_wrapper.inner_mut());
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn handle_map_request(
        &self,
        call_wrapper: &mut CallWrapper,
        event: MapRequestEvent,
        state: &mut State,
    ) -> Result<()> {
        let attrs = call_wrapper.get_window_attributes(event.window)?;
        let hints = call_wrapper.get_hints(event.window)?;
        pgwm_core::debug!("MapRequest incoming for sequence {}", event.sequence);
        if let Ok(attrs) = attrs.reply(call_wrapper.inner_mut()) {
            pgwm_core::debug!("Attributes {attrs:?}");
            if attrs.override_redirect {
                pgwm_core::debug!("Override redirect, not managing");
                hints.forget(call_wrapper.inner_mut());
                return Ok(());
            }
        } else {
            pgwm_core::debug!("No attributes, not managing");
            hints.forget(call_wrapper.inner_mut());
            return Ok(());
        }
        call_wrapper.set_state(event.window, WmState::Normal)?;
        self.manage_window(call_wrapper, event.window, hints, state)
    }

    /// Add a new window that should be managed by the WM
    fn manage_window(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        hints: WmHintsCookie,
        state: &mut State,
    ) -> Result<()> {
        call_wrapper.set_base_client_event_mask(win)?;
        let dimensions_cookie = call_wrapper.get_dimensions(win)?;
        let float_indicators = call_wrapper.get_float_indicators(win)?;
        pgwm_core::debug!("Managing window {:?}", win);
        let ws_ind = if let Some(ws_ind) =
            Self::map_window_class_to_workspace(call_wrapper, win, &state.workspaces)?
        {
            ws_ind
        } else {
            state.monitors[state.focused_mon].hosted_workspace
        };
        let fi = float_indicators.await_float_indicators(call_wrapper);
        match deduce_float_status(fi, state) {
            WindowFloatDeduction::Floating { parent } => {
                let dims = dimensions_cookie.await_dimensions(call_wrapper.inner_mut())?;
                self.manage_floating(
                    call_wrapper,
                    win,
                    parent,
                    state.focused_mon,
                    ws_ind,
                    dims,
                    hints,
                    state,
                )?;
            }
            WindowFloatDeduction::Docked { parent } => {
                dimensions_cookie.inner.forget(call_wrapper.inner_mut());
                self.manage_tiled(
                    call_wrapper,
                    win,
                    parent,
                    ws_ind,
                    state.find_monitor_hosting_workspace(ws_ind),
                    hints,
                    state,
                )?;
            }
        }
        Ok(())
    }

    fn manage_tiled(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        attached_to: Option<Window>,
        ws_ind: usize,
        draw_on_mon: Option<usize>,
        hints_cookie: WmHintsCookie,
        state: &mut State,
    ) -> Result<()> {
        pgwm_core::debug!("Managing tiled {win} attached to {attached_to:?}");
        let hints_reply = hints_cookie.reply(call_wrapper.inner_mut()).ok();

        let focus_style = match Self::deduce_focus_style(call_wrapper, win, hints_reply) {
            Ok(fs) => fs,
            #[allow(unused_variables)]
            Err(e) => {
                // Only way this fails is if the window is dead.
                pgwm_core::debug!("Failed to get focus style {e}, not managing window");
                return Ok(());
            }
        };
        if let Some(attached_to) = attached_to {
            if !state.workspaces.add_attached(
                attached_to,
                win,
                ArrangeKind::NoFloat,
                focus_style,
            )? {
                pgwm_core::debug!(
                    "Parent {attached_to} for window {win} not managed, will promote"
                );
                state
                    .workspaces
                    .add_child_to_ws(win, ws_ind, ArrangeKind::NoFloat, focus_style)?;
            }
        } else {
            state
                .workspaces
                .add_child_to_ws(win, ws_ind, ArrangeKind::NoFloat, focus_style)?;
        }
        if let Some(mon_ind) = draw_on_mon {
            self.drawer.draw_on(call_wrapper, mon_ind, true, state)?;
        } else {
            call_wrapper.send_unmap(win, state)?;
        }
        call_wrapper.push_to_client_list(state.screen.root, win)?;
        self.try_focus_window(call_wrapper, win, state)?;
        Ok(())
    }

    fn find_focus_style(call_wrapper: &mut CallWrapper, win: Window) -> Result<FocusStyle> {
        let hints = call_wrapper
            .get_hints(win)?
            .reply(call_wrapper.inner_mut())
            .ok();
        Self::deduce_focus_style(call_wrapper, win, hints)
    }

    fn deduce_focus_style(
        call_wrapper: &mut CallWrapper,
        win: Window,
        hints: Option<WmHints>,
    ) -> Result<FocusStyle> {
        let take_focus = call_wrapper
            .get_protocols(win)?
            .iter()
            .any(|p| p == &Protocol::TakeFocus);

        // Explicitly set input field
        let focus = if let Some((input, group_leader)) =
            hints.and_then(|r| r.input.map(|i| (i, r.window_group)))
        {
            // If explicitly true
            if input {
                // Explicitly true and take focus present means it's locally active
                if take_focus {
                    FocusStyle::LocallyActive { group_leader }
                // Explicitly true and no take focus means Passive
                } else {
                    FocusStyle::Passive
                }
            } else {
                // Explicitly false and take focus means Globally active
                if take_focus {
                    FocusStyle::GloballyActive
                // Explicitly false and no take focus means No input
                } else {
                    FocusStyle::NoInput
                }
            }
        } else if take_focus {
            // We're interpreting no input field set as false here
            FocusStyle::GloballyActive
        } else {
            // And set as true here. Some applications doesn't bother setting input = True
            // even though they have no WM_TAKE_FOCUS, NoInput is so uncommon that we can assume
            // that this is a mistake on the application's part, I don't like it but I'm not gonna
            // make PRs for every non-conformant application.
            FocusStyle::Passive
        };
        Ok(focus)
    }

    fn manage_floating(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        attached_to: Option<Window>,
        mon_ind: usize,
        ws_ind: usize,
        dimensions: Dimensions,
        hints_cookie: WmHintsCookie,
        state: &mut State,
    ) -> Result<()> {
        pgwm_core::debug!("Managing floating {win} attached to {attached_to:?}");
        let attached_to = if attached_to == Some(state.screen.root) {
            pgwm_core::debug!("Parent was root, assigning floating to currently focused monitor");
            let mon_ind = state.focused_mon;
            let new_parent =
                if let Some(last_focus) = state.monitors[mon_ind].last_focus.map(|mw| mw.window) {
                    last_focus
                } else if let Some(first_tiled) = state
                    .workspaces
                    .find_first_tiled(state.monitors[mon_ind].hosted_workspace)?
                {
                    first_tiled.window
                } else {
                    pgwm_core::debug!("Promoting window");
                    let ws_ind = state.monitors[mon_ind].hosted_workspace;
                    self.manage_tiled(
                        call_wrapper,
                        win,
                        None,
                        ws_ind,
                        Some(mon_ind),
                        hints_cookie,
                        state,
                    )?;
                    return Ok(());
                };
            pgwm_core::debug!("Assigned to new parent {new_parent}");
            Some(new_parent)
        } else {
            attached_to
        };
        let hints_reply = hints_cookie.reply(call_wrapper.inner_mut()).ok();
        let focus_style = match Self::deduce_focus_style(call_wrapper, win, hints_reply) {
            Ok(fs) => fs,
            #[allow(unused_variables)]
            Err(e) => {
                // Only way this fails is if the window is dead.
                pgwm_core::debug!("Failed to get focus style {e}, not managing window");
                return Ok(());
            }
        };
        if let Some(attached_to) = attached_to {
            let parent_dimensions = call_wrapper.get_dimensions(attached_to)?;
            pgwm_core::debug!("Found attached {} to parent {}", win, attached_to);
            let parent_dimensions = parent_dimensions.await_dimensions(call_wrapper.inner_mut())?;
            pgwm_core::debug!(
                "Attached geometry {:?}\nParent geometry {:?}",
                dimensions,
                parent_dimensions
            );
            let dimensions = if (dimensions.x < parent_dimensions.x
                || dimensions.x + (dimensions.width) < parent_dimensions.x)
                || (dimensions.y > parent_dimensions.y
                    || dimensions.y + (dimensions.height) < parent_dimensions.y)
            {
                let parent_relative_x_offset =
                    (parent_dimensions.width - dimensions.width) as f32 / 2f32;
                let parent_relative_y_offset =
                    (parent_dimensions.height - dimensions.height) as f32 / 2f32;
                let x = parent_dimensions.x as i32 + parent_relative_x_offset as i32;
                let y = parent_dimensions.y as i32 + parent_relative_y_offset as i32;
                pgwm_core::debug!("Remapping attached to ({x}, {y})");
                call_wrapper.move_window(win, x, y, state)?;

                Dimensions::new(dimensions.width, dimensions.height, x as i16, y as i16)
            } else {
                dimensions
            };
            let (rel_x, rel_y) = calculate_relative_placement(
                state.monitors[mon_ind].dimensions,
                dimensions.x,
                dimensions.y,
            );
            state.workspaces.add_attached(
                attached_to,
                win,
                ArrangeKind::FloatingInactive(rel_x, rel_y),
                focus_style,
            )?;
        } else {
            let (rel_x, rel_y) = calculate_relative_placement(
                state.monitors[mon_ind].dimensions,
                dimensions.x,
                dimensions.y,
            );
            state.workspaces.add_child_to_ws(
                win,
                ws_ind,
                ArrangeKind::FloatingInactive(rel_x, rel_y),
                focus_style,
            )?;
        }
        call_wrapper.push_to_client_list(state.screen.root, win)?;

        Drawer::draw_floating(call_wrapper, win, dimensions, state)?;
        self.focus_window(call_wrapper, state.focused_mon, win, state)?;
        Ok(())
    }

    pub(crate) fn handle_unmap_notify(
        &self,
        call_wrapper: &mut CallWrapper,
        event: UnmapNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        // Is a managed window, manually unmapped windows are not removed
        if state
            .workspaces
            .find_ws_containing_window(event.window)
            .is_some()
        {
            self.unmanage(call_wrapper, event.window, state)?;
        }

        Ok(())
    }

    fn set_fullscreen(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        ws_ind: usize,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        state.workspaces.set_fullscreened(ws_ind, window)?;
        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
        Ok(())
    }

    fn unset_fullscreen(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
        self.bar_manager.redraw_on(call_wrapper, mon_ind, state)?;
        Ok(())
    }

    pub(crate) fn handle_destroy_notify(
        &self,
        call_wrapper: &mut CallWrapper,
        event: DestroyNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        self.unmanage(call_wrapper, event.window, state)?;
        if let Some(pos) = state
            .dying_windows
            .iter()
            .position(|marked| marked.win == event.window)
        {
            pgwm_core::util::vec_ops::remove(&mut state.dying_windows, pos);
        }
        Ok(())
    }

    pub(crate) fn handle_configure_notify(
        call_wrapper: &mut CallWrapper,
        event: ConfigureNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        // Screen resize, have to invalidate monitor state
        if event.event == state.screen.root && event.window == state.screen.root {
            for mon in 0..state.monitors.len() {
                Drawer::undraw(call_wrapper, mon, state)?;
            }
            return Err(Error::StateInvalidated);
        }
        Ok(())
    }

    pub(crate) fn handle_configure_request(
        call_wrapper: &mut CallWrapper,
        event: ConfigureRequestEvent,
        state: &mut State,
    ) -> Result<()> {
        // Don't allow reconfiguration of tiled windows
        if !state.workspaces.is_managed_tiled(event.window) {
            call_wrapper.configure_from_request(&event)?;
        }
        Ok(())
    }

    pub(crate) fn handle_button_press(
        &self,
        call_wrapper: &mut CallWrapper,
        event: ButtonPressEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        let mon_ind = state
            .find_monitor_at((event.root_x, event.root_y))
            .unwrap_or(0);
        let mon = &state.monitors[mon_ind];
        // If tabbed and clicked on tab bar win
        if let (Mode::Tabbed(_), true) = (
            state.workspaces.get_draw_mode(mon.hosted_workspace),
            event.event == mon.tab_bar_win.window.drawable,
        ) {
            let width = mon.dimensions.width;
            let stacked_children = state
                .workspaces
                .get_all_tiled_windows(mon.hosted_workspace)?
                .len();
            let bar_width = width / stacked_children as i16;
            for b in 0..stacked_children {
                if event.event_x <= bar_width as i16 * (b + 1) as i16 {
                    pgwm_core::debug!("Selected bar number {}", b);
                    if state
                        .workspaces
                        .switch_tab_focus_index(mon.hosted_workspace, b)
                    {
                        let dm = state.workspaces.get_draw_mode(mon.hosted_workspace);
                        let tiled = state
                            .workspaces
                            .get_all_tiled_windows(mon.hosted_workspace)?;
                        let focus = if let Mode::Tabbed(n) = dm {
                            Some(tiled[n])
                        } else {
                            None
                        };
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        if let Some(focus) = focus {
                            self.focus_window(call_wrapper, mon_ind, focus.window, state)?;
                        }
                    }
                    return Ok(());
                }
            }
        }
        // Priority, always accept clicks on bar
        let target = if let Some(target) = state.get_hit_bar_component(
            state
                .pointer_grabbed
                .then(|| event.child)
                .unwrap_or(event.event),
            event.root_x,
            mon_ind,
        ) {
            match target {
                MouseTarget::WorkspaceBarComponent(_)
                | MouseTarget::WindowTitle
                | MouseTarget::ShortcutComponent(_)
                | MouseTarget::StatusComponent(_) => {
                    // If we clicked on a monitor we need to focus it, other logic depends on
                    // operations happening on the focused monitor.
                    self.focus_mon(call_wrapper, mon_ind, state)?;
                    Some(target)
                }
                _ => None,
            }
        } else {
            if state.pointer_grabbed {
                // We grab pointer on root window, then the click is on event.child
                pgwm_core::debug!("Focus change from pointer grabbed {event:?}");
                self.try_focus_window(call_wrapper, event.child, state)?;
                return Ok(());
            }
            Some(MouseTarget::ClientWindow)
        };

        pgwm_core::debug!("Button press for target {:?}", target);
        if let Some(action) =
            target.and_then(|tg| state.get_mouse_action(event.detail, event.state, tg))
        {
            self.exec_action(
                call_wrapper,
                event.child,
                InputSource::Mouse(event.event_x, event.event_y),
                action.clone(),
                state,
            )?;
        }

        Ok(())
    }

    fn resize_win(
        &self,
        call_wrapper: &mut CallWrapper,
        diff_percent: i16,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        let dimensions = call_wrapper.get_dimensions(window)?;
        let diff = diff_percent as f32 / 100f32;
        if state.workspaces.update_size_modifier(window, diff)? {
            if let Some(mon_ind) = state.find_monitor_index_of_window(window) {
                self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
            }
            dimensions.inner.forget(call_wrapper.inner_mut());
        } else {
            let dimensions = dimensions.await_dimensions(call_wrapper.inner_mut())?;
            let height = (dimensions.height as f32 * (1f32 + diff)) as u32;
            let width = (dimensions.width as f32 * (1f32 + diff)) as u32;
            call_wrapper.resize_window(window, height, width, state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_button_release(
        &self,
        call_wrapper: &mut CallWrapper,
        event: ButtonReleaseEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        if let Some((win, _drag)) = state.drag_window.take() {
            let win_dims = call_wrapper.get_dimensions(win)?;
            pgwm_core::debug!("Got button release and removed drag window {win}");
            let focus_style = self
                .remove_win_from_state_then_redraw_if_tiled(call_wrapper, win, state)?
                .map_or_else(
                    || Self::find_focus_style(call_wrapper, win),
                    |mw| Ok(mw.focus_style),
                )?;
            let (x, y) = (event.root_x, event.root_y);
            let mon = state.find_monitor_at((x, y)).unwrap_or(0);
            let mon = &state.monitors[mon];
            let new_ws = mon.hosted_workspace;
            // Using different placement because one is pointer-relative and the other window-left corner relative
            let (x, y) = if let Ok(dims) = win_dims.await_dimensions(call_wrapper.inner_mut()) {
                calculate_relative_placement(mon.dimensions, dims.x, dims.y)
            } else {
                calculate_relative_placement(mon.dimensions, x, y)
            };
            state.workspaces.add_child_to_ws(
                win,
                new_ws,
                ArrangeKind::FloatingInactive(x, y),
                focus_style,
            )?;
            Self::conditional_ungrab_pointer(call_wrapper, state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_motion_notify(
        &self,
        call_wrapper: &mut CallWrapper,
        event: MotionNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        if let Some((win, drag_pos)) = &state.drag_window {
            let (x, y) = drag_pos.current_position(event.event_x, event.event_y);
            // Sigh, X11 and its mixing up i16 and i32
            let (x, y) = (x as i32, y as i32);
            call_wrapper.move_window(*win, x, y, state)?;
        } else if state.pointer_grabbed
            // Grabbed pointer on root makes the target event.child
            && event.child != state.screen.root
            && event.child != x11rb::NONE
            && state
                .input_focus
                .filter(|win| win == &event.child)
                .is_none()
        {
            if let Some(mw) = state.workspaces.get_managed_win(event.child) {
                self.try_focus_window(call_wrapper, mw.window, state)?;
                pgwm_core::debug!("Updated focus to win: {}", mw.window);
            }
            // No window targeted check which monitor we're on
        } else if event.event == state.screen.root && event.child == x11rb::NONE {
            if let Some(mon) = state.find_monitor_at((event.root_x, event.root_y)) {
                if state.focused_mon != mon {
                    self.focus_mon(call_wrapper, mon, state)?;
                    pgwm_core::debug!("Updated focus to mon: {mon}");
                }
            }
        }
        Ok(())
    }
    /**
        Only method that blindly refocuses, won't refocus on root because it feels strange as a user
        if using the mouse between windows with padding
    **/
    pub(crate) fn handle_enter(
        &self,
        call_wrapper: &mut CallWrapper,
        event: EnterNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        if event.event != state.screen.root && event.mode != NotifyMode::GRAB {
            self.try_focus_window(call_wrapper, event.event, state)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn handle_client_message(
        &self,
        call_wrapper: &mut CallWrapper,
        event: x11rb::protocol::xproto::ClientMessageEvent,
        state: &mut State,
    ) -> Result<()> {
        let cm = crate::x11::client_message::convert_message(call_wrapper, event)?;
        if let Some(message) = cm {
            match message {
                ClientMessage::RequestActiveWindow(win) => {
                    self.make_window_urgent(call_wrapper, win, state)?;
                }
                ClientMessage::StateChange((window, state_changes)) => {
                    for change in state_changes {
                        match change.state_change {
                            crate::x11::client_message::StateChange::Modal => {
                                let dimensions = call_wrapper.get_dimensions(window)?;
                                if let Some((mon_ind, ws_ind)) =
                                    state.find_monitor_and_ws_indices_of_window(window)
                                {
                                    match change.change_type {
                                        ChangeType::Add => {
                                            if !state.workspaces.is_managed_floating(window) {
                                                let dimensions = dimensions
                                                    .await_dimensions(call_wrapper.inner_mut())?;
                                                let (x, y) = calculate_relative_placement(
                                                    state.monitors[mon_ind].dimensions,
                                                    dimensions.x,
                                                    dimensions.y,
                                                );
                                                state.workspaces.toggle_floating(
                                                    window,
                                                    ws_ind,
                                                    ArrangeKind::FloatingInactive(x, y),
                                                );
                                                self.drawer.draw_on(
                                                    call_wrapper,
                                                    mon_ind,
                                                    false,
                                                    state,
                                                )?;
                                            }
                                        }
                                        ChangeType::Remove => {
                                            dimensions.inner.forget(call_wrapper.inner_mut());
                                            if state.workspaces.is_managed_floating(window) {
                                                state.workspaces.un_float_window(window);
                                                self.drawer.draw_on(
                                                    call_wrapper,
                                                    mon_ind,
                                                    false,
                                                    state,
                                                )?;
                                            }
                                        }
                                        ChangeType::Toggle => {
                                            if state.workspaces.is_managed_floating(window) {
                                                dimensions.inner.forget(call_wrapper.inner_mut());
                                                state.workspaces.un_float_window(window);
                                                self.drawer.draw_on(
                                                    call_wrapper,
                                                    mon_ind,
                                                    false,
                                                    state,
                                                )?;
                                            } else {
                                                let dimensions = dimensions
                                                    .await_dimensions(call_wrapper.inner_mut())?;
                                                let (x, y) = calculate_relative_placement(
                                                    state.monitors[mon_ind].dimensions,
                                                    dimensions.x,
                                                    dimensions.y,
                                                );
                                                state.workspaces.toggle_floating(
                                                    window,
                                                    ws_ind,
                                                    ArrangeKind::FloatingInactive(x, y),
                                                );
                                                self.drawer.draw_on(
                                                    call_wrapper,
                                                    mon_ind,
                                                    false,
                                                    state,
                                                )?;
                                            }
                                        }
                                    }
                                }
                            }
                            crate::x11::client_message::StateChange::Fullscreen => {
                                pgwm_core::debug!("Got toggle fullscreen request");
                                match change.change_type {
                                    ChangeType::Add => {
                                        if let Some((mon_ind, ws_ind)) =
                                            state.find_monitor_and_ws_indices_of_window(window)
                                        {
                                            self.set_fullscreen(
                                                call_wrapper,
                                                mon_ind,
                                                ws_ind,
                                                window,
                                                state,
                                            )?;
                                        }
                                    }
                                    ChangeType::Remove => {
                                        if let Some((mon_ind, ws_ind)) =
                                            state.find_monitor_and_ws_indices_of_window(window)
                                        {
                                            if state.workspaces.unset_fullscreened(ws_ind) {
                                                self.unset_fullscreen(
                                                    call_wrapper,
                                                    mon_ind,
                                                    state,
                                                )?;
                                            }
                                        }
                                    }
                                    ChangeType::Toggle => {
                                        if let Some((mon_ind, ws_ind)) =
                                            state.find_monitor_and_ws_indices_of_window(window)
                                        {
                                            if state.workspaces.unset_fullscreened(ws_ind) {
                                                self.unset_fullscreen(
                                                    call_wrapper,
                                                    mon_ind,
                                                    state,
                                                )?;
                                            } else {
                                                self.set_fullscreen(
                                                    call_wrapper,
                                                    mon_ind,
                                                    ws_ind,
                                                    window,
                                                    state,
                                                )?;
                                            }
                                        }
                                    }
                                }
                            }
                            crate::x11::client_message::StateChange::DemandAttention => {
                                if let Some(managed) = state.workspaces.get_managed_win(window) {
                                    match change.change_type {
                                        ChangeType::Add => {
                                            self.make_window_urgent(call_wrapper, window, state)?;
                                        }
                                        ChangeType::Remove => {
                                            self.make_window_not_urgent(
                                                call_wrapper,
                                                window,
                                                state,
                                            )?;
                                        }
                                        ChangeType::Toggle => {
                                            if managed.wants_focus {
                                                self.make_window_urgent(
                                                    call_wrapper,
                                                    window,
                                                    state,
                                                )?;
                                            } else {
                                                self.make_window_not_urgent(
                                                    call_wrapper,
                                                    window,
                                                    state,
                                                )?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ClientMessage::CloseWindow(win) => {
                    self.unmanage_and_kill(call_wrapper, win, state)?;
                }
                ClientMessage::RequestSetExtents(win) => {
                    call_wrapper.set_extents(win, state.window_border_width)?;
                }
            }
        }

        Ok(())
    }

    fn make_window_urgent(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        state: &mut State,
    ) -> Result<()> {
        if state
            .input_focus
            .filter(|focused| focused != &win)
            .is_some()
        {
            if let Some((ws_ind, changed)) = state.workspaces.set_wants_focus(win, true) {
                if changed {
                    Self::set_border_urgent(call_wrapper, win, state)?;
                    (0..state.monitors.len()).try_for_each(|mon_ind| {
                        self.bar_manager
                            .set_workspace_urgent(call_wrapper, mon_ind, ws_ind, state)
                    })?;
                    pgwm_core::debug!("Client requested focus {win:?} and it was granted");
                }
            } else {
                pgwm_core::debug!("Client requested focus {win:?} denied because it's not managed");
            }
        }
        Ok(())
    }

    fn make_window_not_urgent(
        &self,
        call_wrapper: &mut CallWrapper,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        Self::restore_normal_border(call_wrapper, window, state)?;
        if let Some((ws_ind, changed)) = state.workspaces.set_wants_focus(window, false) {
            if changed {
                let skip = if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                    if state.monitors[mon_ind]
                        .last_focus
                        .filter(|mw| mw.window == window)
                        .is_some()
                    {
                        self.bar_manager.set_workspace_focused(
                            call_wrapper,
                            mon_ind,
                            ws_ind,
                            state,
                        )?;
                        Some(mon_ind)
                    } else {
                        self.bar_manager.set_workspace_unfocused(
                            call_wrapper,
                            mon_ind,
                            ws_ind,
                            state,
                        )?;

                        Some(mon_ind)
                    }
                } else {
                    None
                };
                for mon_ind in 0..state.monitors.len() {
                    if Some(mon_ind) != skip {
                        self.bar_manager.set_workspace_unfocused(
                            call_wrapper,
                            mon_ind,
                            ws_ind,
                            state,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    // Resets borders and focuses root
    fn focus_root_on_mon(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        state.monitors[mon_ind].last_focus.take();
        if let Some(last_input_focus) = state.input_focus.take() {
            Self::restore_normal_border(call_wrapper, last_input_focus, state)?;
        }
        if let Some(old_mon_ind) = state.update_focused_mon(mon_ind) {
            self.bar_manager.set_workspace_selected_not_focused(
                call_wrapper,
                old_mon_ind,
                state.monitors[old_mon_ind].hosted_workspace,
                state,
            )?;
            self.bar_manager.set_workspace_focused(
                call_wrapper,
                mon_ind,
                state.monitors[mon_ind].hosted_workspace,
                state,
            )?;
        }
        call_wrapper.take_focus(
            state.screen.root,
            state.screen.root,
            FocusStyle::Passive,
            state,
        )?;
        Self::conditional_grab_pointer(call_wrapper, state)?;
        self.update_current_window_title_and_redraw(
            call_wrapper,
            mon_ind,
            heapless::String::from("pgwm"),
            state,
        )?;
        pgwm_core::debug!("Focused root on mon = {}", mon_ind);
        Ok(())
    }

    // Switches focus to window if exists on an monitor
    fn try_focus_window(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        state: &mut State,
    ) -> Result<bool> {
        if let Some(mon_ind) = state.find_monitor_index_of_window(win) {
            self.focus_window(call_wrapper, mon_ind, win, state)?;
            Ok(true)
        } else {
            pgwm_core::debug!("Failed to focus {win} not found on a monitor hosted workspace");
            Ok(false)
        }
    }

    fn focus_mon(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        if let Some(focus_candidate) = state.find_first_focus_candidate(mon_ind)? {
            pgwm_core::debug!("Found focus candidate {focus_candidate:?}");
            self.focus_window(call_wrapper, mon_ind, focus_candidate.window, state)
        } else {
            self.focus_root_on_mon(call_wrapper, mon_ind, state)
        }
    }

    // Switches focus to a window existing on an mon
    fn focus_window(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        win: Window,
        state: &mut State,
    ) -> Result<()> {
        let name_cookie = call_wrapper.get_name(win)?;
        if let Some(last_input_focus) = state.input_focus {
            Self::restore_normal_border(call_wrapper, last_input_focus, state)?;
            self.do_focus_window(call_wrapper, mon_ind, win, name_cookie, state)
        } else {
            self.do_focus_window(call_wrapper, mon_ind, win, name_cookie, state)
        }
    }

    fn do_focus_window(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        win: Window,
        name_cookie: FallbackNameConvertCookie,
        state: &mut State,
    ) -> Result<()> {
        if state.drag_window.is_some() {
            // Never refocus and mess with the pointer while dragging
            return Ok(());
        }
        let pointer_pos = call_wrapper.query_pointer(state)?;
        let main_focus_target = if let Some(attached) = state.workspaces.find_all_attached(win) {
            let focus = attached[0];
            Drawer::send_floating_to_top(
                call_wrapper,
                attached.iter().map(|mw| mw.window).collect(),
                state,
            )?;
            focus
        } else if let Some(mw) = state.workspaces.get_managed_win(win) {
            mw
        } else {
            // Dummy unused NoFloat
            ManagedWindow::new(win, ArrangeKind::NoFloat, FocusStyle::Passive)
        };
        self.make_window_not_urgent(call_wrapper, win, state)?;
        Self::highlight_border(call_wrapper, win, state)?; // Highlighting the base window even if a top level transient is focused
        if let Some(old_focused_mon) = state.update_focused_mon(mon_ind) {
            pgwm_core::debug!("Switched focus from {} to {}", old_focused_mon, mon_ind);
            self.bar_manager.set_workspace_selected_not_focused(
                call_wrapper,
                old_focused_mon,
                state.monitors[old_focused_mon].hosted_workspace,
                state,
            )?;
            self.bar_manager.set_workspace_focused(
                call_wrapper,
                mon_ind,
                state.monitors[mon_ind].hosted_workspace,
                state,
            )?;
        }

        self.redraw_if_tabbed(call_wrapper, mon_ind, main_focus_target.window, state)?;
        state.monitors[mon_ind]
            .last_focus
            .replace(main_focus_target);

        state.input_focus.replace(win);
        pgwm_core::debug!("Taking focus for {win}");
        call_wrapper.take_focus(state.screen.root, win, main_focus_target.focus_style, state)?;
        pgwm_core::debug!("Getting pointer position");
        let pointer_pos = pointer_pos.reply(call_wrapper.inner_mut())?;
        Self::capture_pointer_if_outside_window(
            call_wrapper,
            main_focus_target,
            pointer_pos,
            state,
        )?;
        let name = name_cookie.await_name(call_wrapper.inner_mut());
        self.update_current_window_title_and_redraw(
            call_wrapper,
            mon_ind,
            name.ok()
                .flatten()
                .unwrap_or_else(|| heapless::String::from("pgwm")),
            state,
        )?;
        pgwm_core::debug!("Focused {main_focus_target:?} on mon {mon_ind}");
        Ok(())
    }

    fn redraw_if_tabbed(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        win: Window,
        state: &mut State,
    ) -> Result<bool> {
        if state.workspaces.tab_focus_window(win) {
            // This avoids redrawing when focusing back to a tabbed window, on an intra-window
            // focus switch we still want to redraw
            if state.focused_mon != mon_ind
                || state.monitors[mon_ind]
                    .last_focus
                    .filter(|mw| mw.window == win)
                    .is_none()
            {
                pgwm_core::debug!("Redrawing tab on focus change");
                self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn highlight_border(
        call_wrapper: &mut CallWrapper,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        call_wrapper.change_border_color(window, state.colors.window_border_highlighted().pixel)?;
        Ok(())
    }

    fn restore_normal_border(
        call_wrapper: &mut CallWrapper,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        call_wrapper.change_border_color(window, state.colors.window_border().pixel)?;
        Ok(())
    }

    fn set_border_urgent(
        call_wrapper: &mut CallWrapper,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        call_wrapper.change_border_color(window, state.colors.window_border_urgent().pixel)?;
        Ok(())
    }

    fn capture_pointer_if_outside_window(
        call_wrapper: &mut CallWrapper,
        window: ManagedWindow,
        query_pointer_reply: QueryPointerReply,
        state: &mut State,
    ) -> Result<()> {
        let pointer_on_window = query_pointer_reply.child == window.window;
        pgwm_core::debug!(
            "Pointer on window {pointer_on_window} handles focus internally {:?}",
            window.focus_style
        );
        if pointer_on_window {
            Self::conditional_ungrab_pointer(call_wrapper, state)?;
        } else {
            Self::conditional_grab_pointer(call_wrapper, state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_property_notify(
        &self,
        call_wrapper: &mut CallWrapper,
        event: PropertyNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        state.last_timestamp = event.time;
        // Wm's own events
        if event.window == state.screen.root {
            return Ok(());
        }
        if let Some(property_message) =
            crate::x11::client_message::convert_property_change(call_wrapper, event)?
        {
            match property_message {
                PropertyChangeMessage::ClassName((win, cookie)) => {
                    if let Some(class_names) = cookie.await_class_names(call_wrapper.inner_mut())? {
                        self.manually_remap_win(call_wrapper, win, &class_names, state)?;
                    }
                }
                PropertyChangeMessage::Name((win, cookie)) => {
                    if let Some(focused) = state.find_monitor_focusing_window(win) {
                        if let Ok(Some(name)) = cookie.await_name(call_wrapper.inner_mut()) {
                            self.update_current_window_title_and_redraw(
                                call_wrapper,
                                focused,
                                name,
                                state,
                            )?;
                        }
                    } else {
                        cookie.ewmh_inner.forget(call_wrapper.inner_mut());
                        cookie.wm_inner.forget(call_wrapper.inner_mut());
                    }
                }
                PropertyChangeMessage::Hints((win, cookie)) => {
                    if let Ok(hints) = cookie.reply(call_wrapper.inner_mut()) {
                        if hints.urgent {
                            // Making something not urgent happens through focusing
                            pgwm_core::debug!("Got wm hint for urgency for window {win}");
                            self.make_window_urgent(call_wrapper, win, state)?;
                        }

                        let focus_style =
                            match Self::deduce_focus_style(call_wrapper, win, Some(hints)) {
                                Ok(fs) => fs,
                                #[allow(unused_variables)]
                                Err(e) => {
                                    pgwm_core::debug!("Failed to get focus style {e}");
                                    return Ok(());
                                }
                            };
                        state.workspaces.update_focus_style(focus_style, win);
                    }
                }
                PropertyChangeMessage::WmStateChange((win, wm_state)) => {
                    if wm_state == Some(WmState::Withdrawn) {
                        self.unmanage(call_wrapper, win, state)?;
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::large_types_passed_by_value)]
    fn update_current_window_title_and_redraw(
        &self,
        call_wrapper: &mut CallWrapper,
        mon_ind: usize,
        new_name: heapless::String<WM_NAME_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        state.monitors[mon_ind]
            .bar_geometry
            .window_title_section
            .display = new_name;
        self.bar_manager
            .draw_focused_window_title(call_wrapper, mon_ind, state)
    }

    fn manually_remap_win(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        class_names: &heapless::Vec<heapless::String<WM_CLASS_NAME_LIMIT>, 4>,
        state: &mut State,
    ) -> Result<()> {
        if let Some(mapped) = state.workspaces.find_ws_containing_window(win) {
            for class in class_names {
                if let Some(ind) = state.workspaces.find_ws_for_window_class_name(class) {
                    if mapped != ind {
                        pgwm_core::debug!("Remapping from {} to {} on prop change", mapped, ind);
                        let focus_style = self
                            .remove_win_from_state_then_redraw_if_tiled(call_wrapper, win, state)?
                            .map_or_else(
                                || {
                                    Self::find_focus_style(call_wrapper, win)
                                        .unwrap_or(FocusStyle::Passive)
                                },
                                |mw| mw.focus_style,
                            );
                        call_wrapper.send_unmap(win, state)?;
                        state.workspaces.add_child_to_ws(
                            win,
                            ind,
                            ArrangeKind::NoFloat,
                            focus_style,
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_visibility_change(
        &self,
        call_wrapper: &mut CallWrapper,
        event: VisibilityNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        if event.state == Visibility::UNOBSCURED {
            for mon_ind in 0..state.monitors.len() {
                if state.monitors[mon_ind].bar_win.window.drawable == event.window {
                    self.bar_manager.redraw_on(call_wrapper, mon_ind, state)?;
                }
            }
        }
        Ok(())
    }

    fn toggle_workspace(
        &self,
        call_wrapper: &mut CallWrapper,
        ws_ind: usize,
        recv_mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        let recv_prev_ws = state.monitors[recv_mon_ind].hosted_workspace;
        pgwm_core::debug!(
            "Mapping workspace {} to main window {}",
            ws_ind,
            recv_mon_ind
        );
        if recv_prev_ws == ws_ind {
            pgwm_core::debug!("Got request to replace ws with itself, skipping.");
        } else if let Some(send_mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
            self.bar_manager
                .set_workspace_unfocused(call_wrapper, send_mon_ind, ws_ind, state)?;
            self.bar_manager.set_workspace_unfocused(
                call_wrapper,
                recv_mon_ind,
                recv_prev_ws,
                state,
            )?;
            state.monitors[recv_mon_ind].hosted_workspace = ws_ind;
            state.monitors[send_mon_ind].hosted_workspace = recv_prev_ws;
            self.bar_manager
                .set_workspace_focused(call_wrapper, recv_mon_ind, ws_ind, state)?;
            self.bar_manager.set_workspace_selected_not_focused(
                call_wrapper,
                send_mon_ind,
                recv_prev_ws,
                state,
            )?;
            self.drawer
                .draw_on(call_wrapper, recv_mon_ind, false, state)?;
            self.drawer
                .draw_on(call_wrapper, send_mon_ind, false, state)?;
            let focus_to_transfer = state.monitors[send_mon_ind].last_focus.take();
            state.monitors[send_mon_ind].last_focus = state.monitors[recv_mon_ind].last_focus;
            state.monitors[recv_mon_ind].last_focus = focus_to_transfer;
            self.focus_mon(call_wrapper, recv_mon_ind, state)?;
        } else {
            Drawer::undraw(call_wrapper, recv_mon_ind, state)?;
            state.monitors[recv_mon_ind].hosted_workspace = ws_ind;
            let mon = &mut state.monitors[recv_mon_ind];
            mon.last_focus.take();
            self.drawer
                .draw_on(call_wrapper, recv_mon_ind, true, state)?;
            pgwm_core::debug!("Updating focus");
            self.bar_manager
                .set_workspace_focused(call_wrapper, recv_mon_ind, ws_ind, state)?;
            self.bar_manager.set_workspace_unfocused(
                call_wrapper,
                recv_mon_ind,
                recv_prev_ws,
                state,
            )?;
            self.focus_mon(call_wrapper, recv_mon_ind, state)?;
        }
        Ok(())
    }

    fn map_window_class_to_workspace(
        call_wrapper: &mut CallWrapper,
        win: Window,
        workspaces: &Workspaces,
    ) -> Result<Option<usize>> {
        let wm_classes = call_wrapper
            .get_class_names(win)?
            .await_class_names(call_wrapper.inner_mut())?;
        pgwm_core::debug!("WM_CLASS {:?}", wm_classes);
        if let Some(wm_classes) = wm_classes {
            for class in wm_classes {
                if let Some(ind) = workspaces.find_ws_for_window_class_name(&class) {
                    return Ok(Some(ind));
                }
            }
        }
        Ok(None)
    }

    #[cfg(feature = "status-bar")]
    pub(crate) fn draw_status(
        &self,
        call_wrapper: &mut CallWrapper,
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.bar_manager
            .update_status(call_wrapper, content, content_ind, state)
    }

    fn unmanage(
        &self,
        call_wrapper: &mut CallWrapper,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        if self
            .remove_win_from_state_then_redraw_if_tiled(call_wrapper, window, state)?
            .is_some()
        {
            let windows = state.workspaces.get_all_managed_windows();
            call_wrapper.update_client_list(windows.as_slice(), state)?;
        }
        Ok(())
    }

    fn remove_win_from_state_then_redraw_if_tiled(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        state: &mut State,
    ) -> Result<Option<ManagedWindow>> {
        if let Some(ws_ind) = state.workspaces.find_ws_containing_window(win) {
            let delete_res = state.workspaces.delete_child_from_ws(win);
            if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                return Ok(match delete_res {
                    DeleteResult::TiledTopLevel(mw) => {
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        self.switch_focus_if_last_focus_was_removed(
                            call_wrapper,
                            win,
                            mon_ind,
                            None,
                            state,
                        )?;
                        Some(mw)
                    }
                    DeleteResult::AttachedFloating(parent) => {
                        self.try_focus_window(call_wrapper, parent.window, state)?;
                        Some(parent)
                    }
                    DeleteResult::AttachedTiled(parent) => {
                        self.try_focus_window(call_wrapper, parent.window, state)?;
                        self.drawer.draw_on(call_wrapper, mon_ind, false, state)?;
                        Some(parent)
                    }
                    DeleteResult::FloatingTopLevel(mw) => {
                        self.switch_focus_if_last_focus_was_removed(
                            call_wrapper,
                            win,
                            mon_ind,
                            None,
                            state,
                        )?;
                        Some(mw)
                    }
                    DeleteResult::None => None,
                });
            }
            Ok(None)
        } else {
            Ok(None)
        }
    }

    fn switch_focus_if_last_focus_was_removed(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        mon_ind: usize,
        refocus_parent: Option<Window>,
        state: &mut State,
    ) -> Result<()> {
        if state
            .input_focus
            .filter(|focus_window| focus_window == &win)
            .is_some()
            || state.focused_mon == mon_ind
                && state.monitors[mon_ind]
                    .last_focus
                    .filter(|mw| mw.window == win)
                    .is_some()
        {
            if let Some(parent) = refocus_parent {
                self.focus_window(call_wrapper, mon_ind, parent, state)?;
            } else {
                state.monitors[mon_ind].last_focus.take();
                self.focus_mon(call_wrapper, mon_ind, state)?;
            }
        }
        Ok(())
    }

    pub(crate) fn destroy_marked(call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        while let Some(mut candidate) = state.dying_windows.first().copied() {
            if candidate.should_kill(state.kill_after) {
                call_wrapper.send_kill(candidate.win)?;
                pgwm_core::util::vec_ops::remove(&mut state.dying_windows, 0);
                pgwm_core::debug!("Sent kill for marked window {candidate:?}");
            } else if candidate.should_destroy() {
                call_wrapper.send_destroy(candidate.win)?;
                pgwm_core::debug!("Sent destroy for marked window {candidate:?}");
                candidate.sent_destroy = true;
                break;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn unmanage_and_kill(
        &self,
        call_wrapper: &mut CallWrapper,
        win: Window,
        state: &mut State,
    ) -> Result<()> {
        self.unmanage(call_wrapper, win, state)?;
        call_wrapper.send_delete(win)?;
        push_heapless!(
            state.dying_windows,
            WinMarkedForDeath::new(win, state.destroy_after)
        )?;
        pgwm_core::debug!("Marked win {win} for death");
        Ok(())
    }

    fn conditional_grab_pointer(call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        if !state.pointer_grabbed {
            call_wrapper.grab_pointer(state)?;
            state.pointer_grabbed = true;
        }
        Ok(())
    }

    fn conditional_ungrab_pointer(call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        if state.pointer_grabbed {
            call_wrapper.ungrab_pointer()?;
            state.pointer_grabbed = false;
        }
        Ok(())
    }

    fn cleanup(call_wrapper: &mut CallWrapper, state: &mut State) -> Result<()> {
        let managed = state.workspaces.get_all_managed_windows();
        for win in managed {
            call_wrapper.set_state(win, WmState::Withdrawn)?;
        }
        Self::conditional_ungrab_pointer(call_wrapper, state)?;
        call_wrapper.update_client_list(&[], state)?;
        Ok(())
    }
}

fn focus_fallback_origin(origin: Window, state: &State) -> Window {
    if let Some(focused) = state.input_focus {
        focused
    } else {
        origin
    }
}

fn deduce_float_status(indicators: FloatIndicators, state: &State) -> WindowFloatDeduction {
    let fixed_size = indicators
        .size_hints
        .and_then(|sh| {
            sh.min_size
                .and_then(|min| sh.max_size.map(|max| (min, max)))
        })
        .filter(|((min_w, min_h), (max_w, max_h))| min_w == max_w || min_h == max_h)
        .is_some();

    let should_float = fixed_size
        // No window type is set, if transient then should float
        || (indicators.no_window_type && indicators.transient_for.is_some())
        // If window type is normal, don't float, if not, if modal or dialog then float
        || (!indicators.is_normal && (indicators.is_modal || indicators.is_dialog));

    if let Some(parent) = indicators
        .transient_for
        .filter(|parent| parent != &state.screen.root)
    {
        if should_float {
            WindowFloatDeduction::Floating {
                parent: Some(parent),
            }
        } else {
            WindowFloatDeduction::Docked {
                parent: Some(parent),
            }
        }
    } else if should_float {
        WindowFloatDeduction::Floating { parent: None }
    } else {
        WindowFloatDeduction::Docked { parent: None }
    }
}

fn toggle_tabbed(mon_ind: usize, ws_ind: usize, state: &mut State) -> Result<bool> {
    state.workspaces.set_draw_mode(ws_ind, Mode::Tabbed(0));
    if let Some(should_focus) = state.find_appropriate_ws_focus(mon_ind, ws_ind) {
        return Ok(state
            .workspaces
            .switch_tab_focus_window(ws_ind, should_focus.window)?
            .filter(|b| *b)
            .is_some());
    }
    Ok(false)
}

struct ScanProperties {
    window: Window,
    attributes: Cookie<GetWindowAttributesReply>,
    transient_cookie: TransientConvertCookie,
    wm_state: Option<WmState>,
    hints: WmHintsCookie,
}

fn calculate_relative_placement(
    container_dimensions: Dimensions,
    placement_x: i16,
    placement_y: i16,
) -> (f32, f32) {
    let rel_x = (placement_x - container_dimensions.x) as f32 / container_dimensions.width as f32;
    let rel_y = (placement_y - container_dimensions.y) as f32 / container_dimensions.height as f32;
    (rel_x, rel_y)
}

enum InputSource {
    Mouse(i16, i16),
    Keyboard,
}
