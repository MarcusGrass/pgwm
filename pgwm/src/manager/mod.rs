pub(crate) mod bar;
pub(crate) mod draw;
pub(crate) mod font;

use crate::error::{Error, Result};
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::x11::call_wrapper::{CallWrapper, FloatIndicators, WindowFloatDeduction, WmState};
use crate::x11::client_message::{
    ChangeType, ClientMessage, ClientMessageHandler, PropertyChangeMessage,
};
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
use x11rb::properties::WmHintsCookie;
use x11rb::protocol::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ConfigureNotifyEvent, ConfigureRequestEvent,
    DestroyNotifyEvent, EnterNotifyEvent, GetWindowAttributesReply, KeyPressEvent, MapRequestEvent,
    MapState, MotionNotifyEvent, NotifyMode, PropertyNotifyEvent, QueryPointerReply,
    UnmapNotifyEvent, Visibility, VisibilityNotifyEvent, Window,
};
use x11rb::rust_connection::SingleThreadedRustConnection;

pub(crate) struct Manager<'a> {
    call_wrapper: &'a CallWrapper<'a>,
    drawer: Drawer<'a>,
    bar_manager: BarManager<'a>,
    client_message_handler: ClientMessageHandler<'a>,
    cursor_handle: x11rb::cursor::Handle,
}

impl<'a> Manager<'a> {
    pub(crate) fn new(
        call_wrapper: &'a CallWrapper,
        drawer: Drawer<'a>,
        bar_manager: BarManager<'a>,
        client_message_handler: ClientMessageHandler<'a>,
        cursor_handle: x11rb::cursor::Handle,
    ) -> Self {
        Self {
            call_wrapper,
            drawer,
            bar_manager,
            client_message_handler,
            cursor_handle,
        }
    }

    pub(crate) fn init(&self, state: &mut State) -> Result<()> {
        let ch_wa = self
            .call_wrapper
            .set_root_event_mask(&self.cursor_handle, state)?;
        ch_wa.check()?;
        pgwm_core::debug!("Set root event mask");
        self.bar_manager.draw_static(state)?;
        pgwm_core::debug!("Drew workspace sections");
        self.call_wrapper.set_default_manager_props(state)?;
        pgwm_core::debug!("Drew default manager properties");
        Ok(())
    }

    /// Scan for already existing windows and manage them
    pub(crate) fn scan(&self, state: &mut State) -> Result<()> {
        let subwindows = self
            .call_wrapper
            .query_subwindows(state.screen.root)?
            .await_children()?;
        let mut children_with_properties: heapless::Vec<ScanProperties, APPLICATION_WINDOW_LIMIT> =
            heapless::Vec::new();
        for win in subwindows {
            let state = self.call_wrapper.get_state(win)?;
            let attr_cookie = self.call_wrapper.get_window_attributes(win)?;
            let is_transient = self.call_wrapper.get_is_transient_for(win)?;
            let hints = self.call_wrapper.get_hints(win)?;
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
        let mut transients: heapless::Vec<
            (Window, WmHintsCookie<'a, SingleThreadedRustConnection>),
            APPLICATION_WINDOW_LIMIT,
        > = heapless::Vec::new();
        let mut non_transients: heapless::Vec<
            (Window, WmHintsCookie<'a, SingleThreadedRustConnection>),
            APPLICATION_WINDOW_LIMIT,
        > = heapless::Vec::new();
        for ScanProperties {
            window,
            attributes,
            transient_cookie,
            wm_state,
            hints,
        } in children_with_properties
        {
            if let Ok(attr) = attributes.reply() {
                if !attr.override_redirect
                    // If the window is a viewable top level -> manage
                    // Additionally, when the WM starts up, if a WM state is set that's a pretty good
                    // heuristic for whether or not to manage.
                    && (attr.map_state == MapState::VIEWABLE || wm_state.is_some())
                    && !state.intern_created_windows.contains(&window)
                {
                    if transient_cookie.await_is_transient_for()?.is_some() {
                        push_heapless!(transients, (window, hints))?;
                    } else {
                        push_heapless!(non_transients, (window, hints))?;
                    }
                } else {
                    continue;
                }
            }
        }

        for (win, hints) in non_transients {
            self.manage_window(win, hints, state)?;
        }
        for (win, hints) in transients {
            self.manage_window(win, hints, state)?;
        }
        Ok(())
    }

    pub(crate) fn pick_up_state(&self, state: &mut State) -> Result<()> {
        for mon_ind in 0..state.monitors.len() {
            self.drawer.draw_on(mon_ind, true, state)?;
        }
        self.bar_manager.draw_static(state)?;
        Ok(())
    }

    pub(crate) fn handle_key_press(&self, event: KeyPressEvent, state: &mut State) -> Result<()> {
        if let Some(action) = state.get_key_action(event.detail, event.state) {
            self.exec_action(event.event, InputSource::Keyboard, action.clone(), state)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn exec_action(
        &self,
        origin: Window,
        source: InputSource,
        action: Action,
        state: &mut State,
    ) -> Result<()> {
        match action {
            Action::Restart => {
                self.cleanup(state)?;
                return Err(Error::FullRestart);
            }
            Action::Quit => {
                self.cleanup(state)?;
                return Err(Error::GracefulShutdown);
            }
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
                self.unmanage_and_kill(win, state)?;
            }
            Action::ToggleWorkspace(num) => {
                self.toggle_workspace(num, state.focused_mon, state)?;
            }
            Action::NextTilingMode => {
                let window = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(window) {
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        state.workspaces.cycle_tiling_mode(ws_ind);
                        self.drawer.draw_on(mon_ind, false, state)?;
                        self.focus_mon(mon_ind, state)?;
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
                        self.drawer.draw_on(mon_ind, false, state)?;
                        self.focus_mon(mon_ind, state)?;
                    }
                }
            }
            Action::ResizeWindow(diff) => {
                let window = focus_fallback_origin(origin, state);
                self.resize_win(diff, window, state)?;
            }
            Action::ResizePadding(diff) => {
                let new_width = state.window_padding + diff;
                if new_width < 0 {
                    state.window_padding = 0;
                } else {
                    state.window_padding = new_width;
                }
                for mon_ind in 0..state.monitors.len() {
                    self.drawer.draw_on(mon_ind, false, state)?;
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
                        self.drawer.draw_on(mon_ind, false, state)?;
                    }
                    self.call_wrapper.bulk_set_extents(
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
                        self.drawer.draw_on(mon_ind, false, state)?;
                    }
                }
            }
            Action::SendToFront => {
                let target = focus_fallback_origin(origin, state);
                if let Some(ws_ind) = state.workspaces.find_ws_containing_window(target) {
                    state.workspaces.send_window_to_front(ws_ind, target);
                    if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                        self.drawer.draw_on(mon_ind, false, state)?;
                        pgwm_core::debug!("Sent {target} to front");
                        self.focus_window(mon_ind, target, state)?;
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
                        let refocus_parent = if let Some(removed_mw) =
                            self.remove_win_from_state_then_redraw_if_tiled(target_window, state)?
                        {
                            self.call_wrapper.unmap_window(target_window, state)?;
                            removed_mw.focus_style
                        } else {
                            FocusStyle::Pull
                        };
                        state.workspaces.add_child_to_ws(
                            target_window,
                            num,
                            ArrangeKind::NoFloat,
                            refocus_parent,
                        )?;
                        if let Some(target) = state.find_monitor_hosting_workspace(num) {
                            self.drawer.draw_on(target, true, state)?;
                        }
                    }
                }
            }
            Action::UnFloat => {
                if let Some(input_focus) = state.input_focus {
                    if let Some(mon_ind) = state.find_monitor_index_of_window(input_focus) {
                        if state.workspaces.un_float_window(input_focus).is_some() {
                            pgwm_core::debug!("Unfloating on mon {:?}", mon_ind);
                            self.drawer.draw_on(mon_ind, false, state)?;
                            self.focus_window(mon_ind, input_focus, state)?;
                        }
                    }
                }
            }
            Action::FocusNextWindow => {
                if let Some(cur) = state.input_focus {
                    if let Some(next) = state.workspaces.next_window(cur) {
                        pgwm_core::debug!("Focusnext from {:?} to {:?}", cur, next);
                        self.focus_window(state.focused_mon, next.window, state)?;
                    }
                }
            }
            Action::FocusPreviousWindow => {
                if let Some(cur) = state.input_focus {
                    if let Some(next) = state.workspaces.prev_window(cur) {
                        self.focus_window(state.focused_mon, next.window, state)?;
                    }
                }
            }
            Action::FocusNextMonitor => {
                let len = state.monitors.len();
                let next = (state.focused_mon + 1) % len;
                self.focus_mon(next, state)?;
            }
            Action::FocusPreviousMonitor => {
                let len = state.monitors.len();
                let next = (state.focused_mon as i8 - 1).rem_euclid(len as i8) as usize;
                self.focus_mon(next, state)?;
            }
            Action::ToggleBar => {
                let mon_ind = state.focused_mon;
                if let Some(remap_cookie) = self.bar_manager.toggle_bar(mon_ind, state)? {
                    // Do some other stuff before trying to draw on remapped window, if drawn before full remap it'll look weird
                    self.drawer.draw_on(mon_ind, false, state)?;
                    // Check is fine, this path is far from hot
                    remap_cookie.check()?;
                    self.bar_manager.redraw_on(mon_ind, state)?;
                } else {
                    self.drawer.draw_on(mon_ind, false, state)?;
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
                            self.unset_fullscreen(mon_ind, state)?;
                        }
                    } else {
                        self.set_fullscreen(mon_ind, ws_ind, window, state)?;
                    }
                }
            }
            Action::MoveWindow => {
                if let InputSource::Mouse(x, y) = source {
                    let dimensions = self.call_wrapper.get_dimensions(origin)?;
                    self.conditional_grab_pointer(state)?;
                    let parent_ws_ind = state.workspaces.find_ws_containing_window(origin);
                    if let Some(parent_ws) = parent_ws_ind {
                        if !state.workspaces.is_managed_floating(origin) {
                            state.workspaces.toggle_floating(
                                origin,
                                parent_ws,
                                ArrangeKind::FloatingActive,
                            );
                            let mon = state.find_monitor_at((x, y)).unwrap_or(0);
                            self.drawer.draw_on(mon, false, state)?;
                        }
                        let dimensions = dimensions.await_dimensions()?;
                        state.drag_window =
                            Some((origin, DragPosition::new(dimensions.x, dimensions.y, x, y)));
                        pgwm_core::debug!("Dragging window {}", origin);
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn handle_map_request(
        &self,
        event: MapRequestEvent,
        state: &mut State,
    ) -> Result<()> {
        let attrs = self.call_wrapper.get_window_attributes(event.window)?;
        let hints = self.call_wrapper.get_hints(event.window)?;
        pgwm_core::debug!("Maprequest incoming for sequence {}", event.sequence);
        if let Ok(attrs) = attrs.reply() {
            pgwm_core::debug!("Attributes {attrs:?}");
            if attrs.override_redirect {
                pgwm_core::debug!("Override redirect, not managing");
                return Ok(());
            }
        } else {
            pgwm_core::debug!("No attributes, not managing");
            return Ok(());
        }
        self.call_wrapper.set_state(event.window, WmState::Normal)?;
        self.manage_window(event.window, hints, state)
    }

    /// Add a new window that should be managed by the WM
    fn manage_window(
        &self,
        win: Window,
        hints: WmHintsCookie<'a, SingleThreadedRustConnection>,
        state: &mut State,
    ) -> Result<()> {
        self.call_wrapper.set_base_client_event_mask(win)?;
        let dimensions_cookie = self.call_wrapper.get_dimensions(win)?;
        let float_indicators = self.call_wrapper.get_float_indicators(win)?;
        pgwm_core::debug!("Managing window {:?}", win);
        let ws_ind =
            if let Some(ws_ind) = self.map_window_class_to_workspace(win, &state.workspaces)? {
                ws_ind
            } else {
                state.monitors[state.focused_mon].hosted_workspace
            };
        match deduce_float_status(float_indicators.await_float_indicators(), state) {
            WindowFloatDeduction::Floating { parent } => {
                self.manage_floating(
                    win,
                    parent,
                    state.focused_mon,
                    ws_ind,
                    dimensions_cookie.await_dimensions()?,
                    hints,
                    state,
                )?;
            }
            WindowFloatDeduction::Docked { parent } => {
                self.manage_tiled(
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
        win: Window,
        attached_to: Option<Window>,
        ws_ind: usize,
        draw_on_mon: Option<usize>,
        hints_cookie: WmHintsCookie<'a, SingleThreadedRustConnection>,
        state: &mut State,
    ) -> Result<()> {
        pgwm_core::debug!("Managing tiled {win} attached to {attached_to:?}");
        let hints = hints_cookie.reply();
        let focus_style = if let Ok(hints) = hints {
            if hints.input.filter(|b| *b).is_some() {
                FocusStyle::Push {
                    group_leader: hints.window_group,
                }
            } else {
                FocusStyle::Pull
            }
        } else {
            FocusStyle::Pull
        };
        if let Some(attached_to) = attached_to {
            // Should probably look into this more, happens with gpg pop-up auth.
            // It sends duplicate map-requests on the same sequence and uses WindowTypeNormal when
            // imo it makes more sense being a float, the input-hint is True as well so it's not pull-based weirdness
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
            self.drawer.draw_on(mon_ind, true, state)?;
        } else {
            self.call_wrapper.unmap_window(win, state)?;
        }
        self.call_wrapper
            .push_to_client_list(state.screen.root, win)?;
        self.try_focus_window(win, state)?;
        Ok(())
    }

    fn manage_floating(
        &self,
        win: Window,
        attached_to: Option<Window>,
        mon_ind: usize,
        ws_ind: usize,
        dimensions: Dimensions,
        hints_cookie: WmHintsCookie<'a, SingleThreadedRustConnection>,
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
                    self.manage_tiled(win, None, ws_ind, Some(mon_ind), hints_cookie, state)?;
                    return Ok(());
                };
            pgwm_core::debug!("Assigned to new parent {new_parent}");
            Some(new_parent)
        } else {
            attached_to
        };
        let focus_style = if let Ok(hints) = hints_cookie.reply() {
            if hints.input.filter(|b| *b).is_some() {
                FocusStyle::Push {
                    group_leader: hints.window_group,
                }
            } else {
                FocusStyle::Pull
            }
        } else {
            FocusStyle::Pull
        };
        if let Some(attached_to) = attached_to {
            let parent_dimensions = self.call_wrapper.get_dimensions(attached_to)?;
            pgwm_core::debug!("Found attached {} to parent {}", win, attached_to);
            let parent_dimensions = parent_dimensions.await_dimensions()?;
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
                self.call_wrapper.move_window(win, x, y, state)?;

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
        self.call_wrapper
            .push_to_client_list(state.screen.root, win)?;

        self.drawer.draw_floating(win, dimensions, state)?;
        self.focus_window(state.focused_mon, win, state)?;
        Ok(())
    }

    pub(crate) fn handle_unmap_notify(
        &self,
        event: UnmapNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        // Is a managed window, manually unmapped windows are not removed
        if state
            .workspaces
            .find_ws_containing_window(event.window)
            .is_some()
        {
            self.unmanage(event.window, state)?;
        }

        Ok(())
    }

    fn set_fullscreen(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        window: Window,
        state: &mut State,
    ) -> Result<()> {
        state.workspaces.set_fullscreened(ws_ind, window)?;
        self.drawer.draw_on(mon_ind, false, state)?;
        Ok(())
    }

    fn unset_fullscreen(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        self.drawer.draw_on(mon_ind, false, state)?;
        self.bar_manager.redraw_on(mon_ind, state)?;
        Ok(())
    }

    pub(crate) fn handle_destroy_notify(
        &self,
        event: DestroyNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        self.unmanage(event.window, state)?;
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
        &self,
        event: ConfigureNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        // Screen resize, have to invalidate monitor state
        if event.event == state.screen.root && event.window == state.screen.root {
            for mon in 0..state.monitors.len() {
                self.drawer.undraw(mon, state)?;
            }
            return Err(Error::StateInvalidated);
        }
        Ok(())
    }

    pub(crate) fn handle_configure_request(
        &self,
        event: ConfigureRequestEvent,
        state: &mut State,
    ) -> Result<()> {
        // Don't allow reconfiguration of tiled windows
        if !state.workspaces.is_managed_tiled(event.window) {
            self.call_wrapper.configure_from_request(&event)?;
        }
        Ok(())
    }

    pub(crate) fn handle_button_press(
        &self,
        event: ButtonPressEvent,
        state: &mut State,
    ) -> Result<()> {
        let mon_ind = state
            .find_monitor_at((event.root_x, event.root_y))
            .unwrap_or(0);
        let mon = &state.monitors[mon_ind];
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
                        self.drawer.draw_on(mon_ind, false, state)?;
                        if let Some(focus) = focus {
                            self.focus_window(mon_ind, focus.window, state)?;
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
                | MouseTarget::StatusComponent(_) => Some(target),
                _ => None,
            }
        } else {
            if state.pointer_grabbed {
                // We grab pointer on root window, then the click is on event.child
                pgwm_core::debug!("Focus change from pointer grabbed {event:?}");
                self.try_focus_window(event.child, state)?;
                return Ok(());
            }
            Some(MouseTarget::ClientWindow)
        };

        pgwm_core::debug!("Button press for target {:?}", target);
        if let Some(action) =
            target.and_then(|tg| state.get_mouse_action(event.detail, event.state, tg))
        {
            self.exec_action(
                event.child,
                InputSource::Mouse(event.event_x, event.event_y),
                action.clone(),
                state,
            )?;
        }

        Ok(())
    }

    fn resize_win(&self, diff_percent: i16, window: Window, state: &mut State) -> Result<()> {
        let dimensions = self.call_wrapper.get_dimensions(window)?;
        let diff = diff_percent as f32 / 100f32;
        if state.workspaces.update_size_modifier(window, diff)? {
            if let Some(mon_ind) = state.find_monitor_index_of_window(window) {
                self.drawer.draw_on(mon_ind, false, state)?;
            }
        } else {
            let dimensions = dimensions.await_dimensions()?;
            let height = (dimensions.height as f32 * (1f32 + diff)) as u32;
            let width = (dimensions.width as f32 * (1f32 + diff)) as u32;
            self.call_wrapper
                .resize_window(window, height, width, state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_button_release(
        &self,
        event: ButtonReleaseEvent,
        state: &mut State,
    ) -> Result<()> {
        if let Some((win, _drag)) = state.drag_window.take() {
            let win_dims = self.call_wrapper.get_dimensions(win)?;
            pgwm_core::debug!("Got button release and removed drag window {win}");
            let focus_style = self
                .remove_win_from_state_then_redraw_if_tiled(win, state)?
                .map_or(FocusStyle::Pull, |mw| mw.focus_style);
            let (x, y) = (event.root_x, event.root_y);
            let mon = state.find_monitor_at((x, y)).unwrap_or(0);
            let mon = &state.monitors[mon];
            let new_ws = mon.hosted_workspace;
            // Using different placement because one is pointer-relative and the other window-left corner relative
            let (x, y) = if let Ok(dims) = win_dims.await_dimensions() {
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
            self.conditional_ungrab_pointer(state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_motion_notify(
        &self,
        event: MotionNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        if let Some((win, drag_pos)) = &state.drag_window {
            let (x, y) = drag_pos.current_position(event.event_x, event.event_y);
            // Sigh, X11 and its mixing up i16 and i32
            let (x, y) = (x as i32, y as i32);
            self.call_wrapper.move_window(*win, x, y, state)?;
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
                self.try_focus_window(mw.window, state)?;
                pgwm_core::debug!("Updated focus to win: {}", mw.window);
            }
            // No window targeted check which monitor we're on
        } else if event.event == state.screen.root && event.child == x11rb::NONE {
            if let Some(mon) = state.find_monitor_at((event.root_x, event.root_y)) {
                if state.focused_mon != mon {
                    self.focus_mon(mon, state)?;
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
    pub(crate) fn handle_enter(&self, event: EnterNotifyEvent, state: &mut State) -> Result<()> {
        if event.event != state.screen.root && event.mode != NotifyMode::GRAB {
            self.try_focus_window(event.event, state)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn handle_client_message(
        &self,
        event: x11rb::protocol::xproto::ClientMessageEvent,
        state: &mut State,
    ) -> Result<()> {
        let cm = self.client_message_handler.convert_message(event)?;
        if let Some(message) = cm {
            match message {
                ClientMessage::RequestActiveWindow(win) => {
                    self.make_window_urgent(win, state)?;
                }
                ClientMessage::StateChange((window, state_changes)) => {
                    for change in state_changes {
                        match change.state_change {
                            crate::x11::client_message::StateChange::Modal => {
                                let dimensions = self.call_wrapper.get_dimensions(window)?;
                                if let Some((mon_ind, ws_ind)) =
                                    state.find_monitor_and_ws_indices_of_window(window)
                                {
                                    match change.change_type {
                                        ChangeType::Add => {
                                            if !state.workspaces.is_managed_floating(window) {
                                                let dimensions = dimensions.await_dimensions()?;
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
                                                self.drawer.draw_on(mon_ind, false, state)?;
                                            }
                                        }
                                        ChangeType::Remove => {
                                            if state.workspaces.is_managed_floating(window) {
                                                state.workspaces.un_float_window(window);
                                                self.drawer.draw_on(mon_ind, false, state)?;
                                            }
                                        }
                                        ChangeType::Toggle => {
                                            if state.workspaces.is_managed_floating(window) {
                                                state.workspaces.un_float_window(window);
                                                self.drawer.draw_on(mon_ind, false, state)?;
                                            } else {
                                                let dimensions = dimensions.await_dimensions()?;
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
                                                self.drawer.draw_on(mon_ind, false, state)?;
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
                                            self.set_fullscreen(mon_ind, ws_ind, window, state)?;
                                        }
                                    }
                                    ChangeType::Remove => {
                                        if let Some((mon_ind, ws_ind)) =
                                            state.find_monitor_and_ws_indices_of_window(window)
                                        {
                                            if state.workspaces.unset_fullscreened(ws_ind) {
                                                self.unset_fullscreen(mon_ind, state)?;
                                            }
                                        }
                                    }
                                    ChangeType::Toggle => {
                                        if let Some((mon_ind, ws_ind)) =
                                            state.find_monitor_and_ws_indices_of_window(window)
                                        {
                                            if state.workspaces.unset_fullscreened(ws_ind) {
                                                self.unset_fullscreen(mon_ind, state)?;
                                            } else {
                                                self.set_fullscreen(
                                                    mon_ind, ws_ind, window, state,
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
                                            self.make_window_urgent(window, state)?;
                                        }
                                        ChangeType::Remove => {
                                            self.make_window_not_urgent(window, state)?;
                                        }
                                        ChangeType::Toggle => {
                                            if managed.wants_focus {
                                                self.make_window_urgent(window, state)?;
                                            } else {
                                                self.make_window_not_urgent(window, state)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ClientMessage::CloseWindow(win) => {
                    self.unmanage_and_kill(win, state)?;
                }
                ClientMessage::RequestSetExtents(win) => {
                    self.call_wrapper
                        .set_extents(win, state.window_border_width)?;
                }
            }
        }

        Ok(())
    }

    fn make_window_urgent(&self, win: Window, state: &mut State) -> Result<()> {
        if state
            .input_focus
            .filter(|focused| focused != &win)
            .is_some()
        {
            if let Some((ws_ind, changed)) = state.workspaces.set_wants_focus(win, true) {
                if changed {
                    self.set_border_urgent(win, state)?;
                    (0..state.monitors.len()).try_for_each(|mon_ind| {
                        self.bar_manager
                            .set_workspace_urgent(mon_ind, ws_ind, state)
                    })?;
                    pgwm_core::debug!("Client requested focus {win:?} and it was granted");
                }
            } else {
                pgwm_core::debug!("Client requested focus {win:?} denied because it's not managed");
            }
        }
        Ok(())
    }

    fn make_window_not_urgent(&self, window: Window, state: &mut State) -> Result<()> {
        self.restore_normal_border(window, state)?;
        if let Some((ws_ind, changed)) = state.workspaces.set_wants_focus(window, false) {
            if changed {
                let skip = if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                    if state.monitors[mon_ind]
                        .last_focus
                        .filter(|mw| mw.window == window)
                        .is_some()
                    {
                        self.bar_manager
                            .set_workspace_focused(mon_ind, ws_ind, state)?;
                        Some(mon_ind)
                    } else {
                        self.bar_manager
                            .set_workspace_unfocused(mon_ind, ws_ind, state)?;

                        Some(mon_ind)
                    }
                } else {
                    None
                };
                for mon_ind in 0..state.monitors.len() {
                    if Some(mon_ind) != skip {
                        self.bar_manager
                            .set_workspace_unfocused(mon_ind, ws_ind, state)?;
                    }
                }
            }
        }
        Ok(())
    }

    // Resets borders and focuses root
    fn focus_root_on_mon(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        state.monitors[mon_ind].last_focus.take();
        if let Some(last_input_focus) = state.input_focus.take() {
            self.restore_normal_border(last_input_focus, state)?;
        }
        if let Some(old_mon_ind) = state.update_focused_mon(mon_ind) {
            self.bar_manager.set_workspace_selected_not_focused(
                old_mon_ind,
                state.monitors[old_mon_ind].hosted_workspace,
                state,
            )?;
            self.bar_manager.set_workspace_focused(
                mon_ind,
                state.monitors[mon_ind].hosted_workspace,
                state,
            )?;
        }
        self.call_wrapper
            .take_focus(state.screen.root, state.screen.root, FocusStyle::Pull)?;
        self.conditional_grab_pointer(state)?;
        self.update_current_window_title_and_redraw(
            mon_ind,
            heapless::String::from("pgwm"),
            state,
        )?;
        pgwm_core::debug!("Focused root on mon = {}", mon_ind);
        Ok(())
    }

    // Switches focus to window if exists on an monitor
    fn try_focus_window(&self, win: Window, state: &mut State) -> Result<bool> {
        if let Some(mon_ind) = state.find_monitor_index_of_window(win) {
            self.focus_window(mon_ind, win, state)?;
            Ok(true)
        } else {
            pgwm_core::debug!("Failed to focus {win} not found on a monitor hosted workspace");
            Ok(false)
        }
    }

    fn focus_mon(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        if let Some(focus_candidate) = state.find_first_focus_candidate(mon_ind)? {
            pgwm_core::debug!("Found focus candidate {focus_candidate:?}");
            self.focus_window(mon_ind, focus_candidate.window, state)
        } else {
            self.focus_root_on_mon(mon_ind, state)
        }
    }

    // Switches focus to a window existing on an mon
    fn focus_window(&self, mon_ind: usize, win: Window, state: &mut State) -> Result<()> {
        let name_cookie = self.call_wrapper.get_name(win)?;
        if let Some(last_input_focus) = state.input_focus {
            self.restore_normal_border(last_input_focus, state)?;
            self.do_focus_window(mon_ind, win, name_cookie, state)
        } else {
            self.do_focus_window(mon_ind, win, name_cookie, state)
        }
    }

    fn do_focus_window(
        &self,
        mon_ind: usize,
        win: Window,
        name_cookie: FallbackNameConvertCookie,
        state: &mut State,
    ) -> Result<()> {
        if state.drag_window.is_some() {
            // Never refocus and mess with the pointer while dragging
            return Ok(());
        }
        let pointer_pos = self.call_wrapper.query_pointer(state)?;
        let main_focus_target = if let Some(attached) = state.workspaces.find_all_attached(win) {
            let focus = attached[0];
            self.drawer
                .send_floating_to_top(attached.iter().map(|mw| mw.window).collect(), state)?;
            focus
        } else if let Some(mw) = state.workspaces.get_managed_win(win) {
            mw
        } else {
            // Dummy unused NoFloat
            ManagedWindow::new(win, ArrangeKind::NoFloat, FocusStyle::Pull)
        };
        self.make_window_not_urgent(win, state)?;
        self.highlight_border(win, state)?; // Highlighting the base window even if a top level transient is focused
        if let Some(old_focused_mon) = state.update_focused_mon(mon_ind) {
            pgwm_core::debug!("Switched focus from {} to {}", old_focused_mon, mon_ind);
            self.bar_manager.set_workspace_selected_not_focused(
                old_focused_mon,
                state.monitors[old_focused_mon].hosted_workspace,
                state,
            )?;
            self.bar_manager.set_workspace_focused(
                mon_ind,
                state.monitors[mon_ind].hosted_workspace,
                state,
            )?;
        }

        self.redraw_if_tabbed(mon_ind, main_focus_target.window, state)?;
        state.monitors[mon_ind]
            .last_focus
            .replace(main_focus_target);

        state.input_focus.replace(win);
        pgwm_core::debug!("Taking focus for {win}");
        self.call_wrapper
            .take_focus(state.screen.root, win, main_focus_target.focus_style)?;
        pgwm_core::debug!("Getting pointer position");
        self.capture_pointer_if_outside_window(main_focus_target, pointer_pos.reply()?, state)?;
        self.update_current_window_title_and_redraw(
            mon_ind,
            name_cookie
                .await_name()
                .ok()
                .flatten()
                .unwrap_or_else(|| heapless::String::from("pgwm")),
            state,
        )?;
        pgwm_core::debug!("Focused {main_focus_target:?} on mon {mon_ind}");
        Ok(())
    }

    fn redraw_if_tabbed(&self, mon_ind: usize, win: Window, state: &mut State) -> Result<bool> {
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
                self.drawer.draw_on(mon_ind, false, state)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn highlight_border(&self, window: Window, state: &mut State) -> Result<()> {
        self.call_wrapper
            .change_border_color(window, state.colors.window_border_highlighted().pixel)?;
        Ok(())
    }

    fn restore_normal_border(&self, window: Window, state: &mut State) -> Result<()> {
        self.call_wrapper
            .change_border_color(window, state.colors.window_border().pixel)?;
        Ok(())
    }

    fn set_border_urgent(&self, window: Window, state: &mut State) -> Result<()> {
        self.call_wrapper
            .change_border_color(window, state.colors.window_border_urgent().pixel)?;
        Ok(())
    }

    fn capture_pointer_if_outside_window(
        &self,
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
            self.conditional_ungrab_pointer(state)?;
        } else {
            self.conditional_grab_pointer(state)?;
        }
        Ok(())
    }

    pub(crate) fn handle_property_notify(
        &self,
        event: PropertyNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        // Wm's own events
        if event.window == state.screen.root {
            return Ok(());
        }
        if let Some(property_message) =
            self.client_message_handler.convert_property_change(event)?
        {
            match property_message {
                PropertyChangeMessage::ClassName((win, cookie)) => {
                    if let Some(class_names) = cookie.await_class_names()? {
                        self.manually_remap_win(win, &class_names, state)?;
                    }
                }
                PropertyChangeMessage::Name((win, cookie)) => {
                    if let Some(focused) = state.find_monitor_focusing_window(win) {
                        if let Ok(Some(name)) = cookie.await_name() {
                            self.update_current_window_title_and_redraw(focused, name, state)?;
                        }
                    }
                }
                PropertyChangeMessage::Hints((win, cookie)) => {
                    if let Ok(hints) = cookie.reply() {
                        if hints.urgent {
                            // Making something not urgent happens through focusing
                            pgwm_core::debug!("Got wm hint for urgency for window {win}");
                            self.make_window_urgent(win, state)?;
                        }

                        let focus_style = if hints.input.filter(|b| *b).is_some() {
                            FocusStyle::Push {
                                group_leader: hints.window_group,
                            }
                        } else {
                            FocusStyle::Pull
                        };
                        state.workspaces.update_focus_style(focus_style, win);
                    }
                }
                PropertyChangeMessage::WmStateChange((win, wm_state)) => {
                    if wm_state == Some(WmState::Withdrawn) {
                        self.unmanage(win, state)?;
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::large_types_passed_by_value)]
    fn update_current_window_title_and_redraw(
        &self,
        mon_ind: usize,
        new_name: heapless::String<WM_NAME_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        state.monitors[mon_ind]
            .bar_geometry
            .window_title_section
            .display = new_name;
        self.bar_manager.draw_focused_window_title(mon_ind, state)
    }

    fn manually_remap_win(
        &self,
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
                            .remove_win_from_state_then_redraw_if_tiled(win, state)?
                            .map_or(FocusStyle::Pull, |mw| mw.focus_style);
                        self.call_wrapper.unmap_window(win, state)?;
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
        event: VisibilityNotifyEvent,
        state: &mut State,
    ) -> Result<()> {
        if event.state == Visibility::UNOBSCURED {
            for mon_ind in 0..state.monitors.len() {
                if state.monitors[mon_ind].bar_win.window.drawable == event.window {
                    self.bar_manager.redraw_on(mon_ind, state)?;
                }
            }
        }
        Ok(())
    }

    fn toggle_workspace(
        &self,
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
                .set_workspace_unfocused(send_mon_ind, ws_ind, state)?;
            self.bar_manager
                .set_workspace_unfocused(recv_mon_ind, recv_prev_ws, state)?;
            state.monitors[recv_mon_ind].hosted_workspace = ws_ind;
            state.monitors[send_mon_ind].hosted_workspace = recv_prev_ws;
            self.bar_manager
                .set_workspace_focused(recv_mon_ind, ws_ind, state)?;
            self.bar_manager.set_workspace_selected_not_focused(
                send_mon_ind,
                recv_prev_ws,
                state,
            )?;
            self.drawer.draw_on(recv_mon_ind, false, state)?;
            self.drawer.draw_on(send_mon_ind, false, state)?;
            let focus_to_transfer = state.monitors[send_mon_ind].last_focus.take();
            state.monitors[send_mon_ind].last_focus = state.monitors[recv_mon_ind].last_focus;
            state.monitors[recv_mon_ind].last_focus = focus_to_transfer;
            self.focus_mon(recv_mon_ind, state)?;
        } else {
            self.drawer.undraw(recv_mon_ind, state)?;
            state.monitors[recv_mon_ind].hosted_workspace = ws_ind;
            let mon = &mut state.monitors[recv_mon_ind];
            mon.last_focus.take();
            self.drawer.draw_on(recv_mon_ind, true, state)?;
            pgwm_core::debug!("Updating focus");
            self.bar_manager
                .set_workspace_focused(recv_mon_ind, ws_ind, state)?;
            self.bar_manager
                .set_workspace_unfocused(recv_mon_ind, recv_prev_ws, state)?;
            self.focus_mon(recv_mon_ind, state)?;
        }
        Ok(())
    }

    fn map_window_class_to_workspace(
        &self,
        win: Window,
        workspaces: &Workspaces,
    ) -> Result<Option<usize>> {
        let wm_classes = self
            .call_wrapper
            .get_class_names(win)?
            .await_class_names()?;
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
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.bar_manager.update_status(content, content_ind, state)
    }

    fn unmanage(&self, window: Window, state: &mut State) -> Result<()> {
        if self
            .remove_win_from_state_then_redraw_if_tiled(window, state)?
            .is_some()
        {
            let windows = state.workspaces.get_all_managed_windows();
            self.call_wrapper
                .update_client_list(windows.as_slice(), state)?;
        }
        Ok(())
    }

    fn remove_win_from_state_then_redraw_if_tiled(
        &self,
        win: Window,
        state: &mut State,
    ) -> Result<Option<ManagedWindow>> {
        if let Some(ws_ind) = state.workspaces.find_ws_containing_window(win) {
            let delete_res = state.workspaces.delete_child_from_ws(win);
            if let Some(mon_ind) = state.find_monitor_hosting_workspace(ws_ind) {
                return Ok(match delete_res {
                    DeleteResult::TiledTopLevel(mw) => {
                        self.drawer.draw_on(mon_ind, false, state)?;
                        self.switch_focus_if_last_focus_was_removed(win, mon_ind, None, state)?;
                        Some(mw)
                    }
                    DeleteResult::AttachedFloating(parent) => {
                        self.try_focus_window(parent.window, state)?;
                        Some(parent)
                    }
                    DeleteResult::AttachedTiled(parent) => {
                        self.try_focus_window(parent.window, state)?;
                        self.drawer.draw_on(mon_ind, false, state)?;
                        Some(parent)
                    }
                    DeleteResult::FloatingTopLevel(mw) => {
                        self.switch_focus_if_last_focus_was_removed(win, mon_ind, None, state)?;
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
                self.focus_window(mon_ind, parent, state)?;
            } else {
                state.monitors[mon_ind].last_focus.take();
                self.focus_mon(mon_ind, state)?;
            }
        }
        Ok(())
    }

    pub(crate) fn destroy_marked(&self, state: &mut State) -> Result<()> {
        while let Some(mut candidate) = state.dying_windows.first().copied() {
            if candidate.should_kill(state.kill_after) {
                self.call_wrapper.kill_client(candidate.win)?;
                pgwm_core::util::vec_ops::remove(&mut state.dying_windows, 0);
                pgwm_core::debug!("Sent kill for marked window {candidate:?}");
            } else if candidate.should_destroy() {
                self.call_wrapper.destroy_window(candidate.win)?;
                pgwm_core::debug!("Sent destroy for marked window {candidate:?}");
                candidate.sent_destroy = true;
                break;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn unmanage_and_kill(&self, win: Window, state: &mut State) -> Result<()> {
        self.unmanage(win, state)?;
        self.call_wrapper.send_delete(win)?;
        push_heapless!(
            state.dying_windows,
            WinMarkedForDeath::new(win, state.destroy_after)
        )?;
        pgwm_core::debug!("Marked win {win} for death");
        Ok(())
    }

    fn conditional_grab_pointer(&self, state: &mut State) -> Result<()> {
        if !state.pointer_grabbed {
            self.call_wrapper.grab_pointer(state)?;
            state.pointer_grabbed = true;
        }
        Ok(())
    }

    fn conditional_ungrab_pointer(&self, state: &mut State) -> Result<()> {
        if state.pointer_grabbed {
            self.call_wrapper.ungrab_pointer()?;
            state.pointer_grabbed = false;
        }
        Ok(())
    }

    fn cleanup(&self, state: &mut State) -> Result<()> {
        let managed = state.workspaces.get_all_managed_windows();
        for win in managed {
            self.call_wrapper.set_state(win, WmState::Withdrawn)?;
        }
        self.conditional_ungrab_pointer(state)?;
        self.call_wrapper.update_client_list(&[], state)?;
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

struct ScanProperties<'a> {
    window: Window,
    attributes: Cookie<'a, SingleThreadedRustConnection, GetWindowAttributesReply>,
    transient_cookie: TransientConvertCookie<'a>,
    wm_state: Option<WmState>,
    hints: WmHintsCookie<'a, SingleThreadedRustConnection>,
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
