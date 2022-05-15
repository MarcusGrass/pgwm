use crate::error::{Error, Result};
use crate::manager;
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::manager::font::{load_alloc_fonts, FontDrawer, LoadedFonts};
use crate::manager::Manager;
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::client_message::ClientMessageHandler;
use crate::x11::colors::alloc_colors;
use pgwm_core::config::{BarCfg, Cfg, Options, Sizing};
use pgwm_core::render::{RenderVisualInfo, VisualInfo};
use pgwm_core::state::State;
use std::collections::HashMap;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::render::{PictType, Pictformat, Pictforminfo};
use x11rb::protocol::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ClientMessageEvent, ConfigureNotifyEvent,
    ConfigureRequestEvent, DestroyNotifyEvent, EnterNotifyEvent, KeyPressEvent, MapRequestEvent,
    MotionNotifyEvent, PropertyNotifyEvent, Screen, UnmapNotifyEvent, VisibilityNotifyEvent,
    Visualid,
};
pub type XorgConnection = x11rb::rust_connection::RustConnection;

use x11rb::x11_utils::TryParse;

#[allow(clippy::too_many_lines)]
pub(crate) fn run_wm() -> Result<()> {
    #[cfg(feature = "status-bar")]
    let Cfg {
        sizing,
        options,
        tiling_modifiers,
        fonts,
        colors,
        char_remap,
        workspaces,
        mouse_mappings,
        key_mappings,
        bar,
    } = Cfg::new()?;
    #[cfg(not(feature = "status-bar"))]
    let Cfg {
        sizing,
        options,
        tiling_modifiers,
        fonts,
        colors,
        char_remap,
        workspaces,
        mouse_mappings,
        key_mappings,
        bar,
    } = Cfg::new()?;
    let Sizing {
        status_bar_height,
        tab_bar_height,
        window_padding,
        window_border_width,
        workspace_bar_window_name_padding,
    } = sizing;
    let Options {
        pad_while_tabbed,
        destroy_after,
        kill_after,
        cursor_name,
        show_bar_initially,
    } = options;
    let BarCfg {
        shortcuts,
        #[cfg(feature = "status-bar")]
        status_checks,
    } = bar;
    #[cfg(feature = "perf-test")]
    let dpy = Some(":4");
    #[cfg(not(feature = "perf-test"))]
    let dpy = None;
    let (connection, screen_num) = XorgConnection::connect(dpy)?;
    let setup = connection.setup();
    pgwm_core::debug!("Setup formats {:?}", setup.pixmap_formats);
    pgwm_core::debug!("Setup visuals {:?}", setup.roots[0].root_visual);
    let screen = &setup.roots[screen_num];
    let call_wrapper = CallWrapper::new(&connection)?;
    call_wrapper.try_become_wm(screen)?;
    connection.flush()?;
    //let resource_db = x11rb::resource_manager::new_from_resource_manager(&connection)?
    //    .ok_or(Error::X11OpenDefaultDb)?;
    let resource_db = x11rb::resource_manager::Database::new_from_resource_manager(&connection)?
        .ok_or(Error::X11OpenDefaultDb)?;
    let cursor_handle = x11rb::cursor::Handle::new(&connection, 0, &resource_db)?;
    let visual = find_render_visual_info(&connection, screen)?;
    let loaded = load_alloc_fonts(&call_wrapper, &visual, &fonts, &char_remap)?;
    let lf = LoadedFonts::new(loaded, &char_remap)?;

    let font_drawer = FontDrawer::new(&call_wrapper, &lf);
    pgwm_core::debug!("Allocating colors");
    let colors = alloc_colors(&connection, screen.default_colormap, colors)?;

    pgwm_core::debug!("Creating state");
    let mut state = crate::x11::state_lifecycle::create_state(
        &connection,
        &call_wrapper,
        &font_drawer,
        visual,
        &fonts,
        screen,
        colors,
        cursor_name,
        status_bar_height,
        tab_bar_height,
        window_padding,
        window_border_width,
        workspace_bar_window_name_padding,
        pad_while_tabbed,
        destroy_after,
        kill_after,
        show_bar_initially,
        tiling_modifiers,
        &workspaces,
        &shortcuts,
        &key_mappings,
        &mouse_mappings,
        #[cfg(feature = "status-bar")]
        &status_checks,
    )?;

    crate::debug!("Initialized mappings");
    let drawer = Drawer::new(&font_drawer, &call_wrapper, &fonts);
    crate::debug!("Initialized Drawer");
    let client_message_handler = ClientMessageHandler::new(&connection, &call_wrapper);
    crate::debug!("Initialized Client message handler");
    let bar_manager = BarManager::new(&call_wrapper, &font_drawer, &fonts);
    crate::debug!("Initialized bar sections");
    let manager = manager::Manager::new(
        &call_wrapper,
        drawer,
        bar_manager,
        client_message_handler,
        cursor_handle.reply()?,
    );
    crate::debug!("Initialized manager");
    // Extremely ugly control flow here
    #[cfg(feature = "status-bar")]
    let should_check = !status_checks.is_empty();

    #[cfg(feature = "status-bar")]
    let mut mut_checks = status_checks.clone();
    #[cfg(feature = "status-bar")]
    let mut checker = pgwm_core::status::checker::Checker::new(&mut mut_checks);
    crate::debug!("Initialized Checker");
    manager.init(&mut state)?;
    crate::debug!("Initialized manager state");
    manager.scan(&mut state)?;
    connection.flush()?;
    crate::debug!("Initialized, starting loop");
    loop {
        #[cfg(feature = "status-bar")]
        let loop_result = if should_check {
            loop_with_status(&connection, &manager, &mut checker, &mut state)
        } else {
            loop_without_status(&connection, &manager, &mut state)
        };
        #[cfg(not(feature = "status-bar"))]
        let loop_result = loop_without_status(&connection, &manager, &mut state);

        if let Err(e) = loop_result {
            match e {
                Error::StateInvalidated => {
                    crate::x11::state_lifecycle::teardown_dynamic_state(&connection, &state)?;
                    state = crate::x11::state_lifecycle::reinit_state(
                        &connection,
                        &call_wrapper,
                        &font_drawer,
                        &fonts,
                        visual,
                        state,
                        &workspaces,
                        &shortcuts,
                        &key_mappings,
                        &mouse_mappings,
                        #[cfg(feature = "status-bar")]
                        &status_checks,
                    )?;
                    manager.pick_up_state(&mut state)?;
                }
                Error::GracefulShutdown => {
                    crate::x11::state_lifecycle::teardown_full_state(&connection, &state, &lf)?;
                    call_wrapper.reset_root_focus(&state)?;
                    connection.flush()?;
                    return Ok(());
                }
                Error::FullRestart => {
                    crate::x11::state_lifecycle::teardown_full_state(&connection, &state, &lf)?;
                    call_wrapper.reset_root_focus(&state)?;
                    connection.flush()?;
                    return Err(Error::FullRestart);
                }
                _ => {
                    return Err(e);
                }
            }
        }
    }
}

#[cfg(feature = "status-bar")]
fn loop_with_status<'a>(
    connection: &XorgConnection,
    manager: &Manager<'a>,
    checker: &mut pgwm_core::status::checker::Checker,
    state: &mut State,
) -> Result<()> {
    let mut next_check = std::time::Instant::now();
    // Extremely hot place in the code, should bench the checker
    loop {
        // Handle any events currently in queue
        drain_events(connection, manager, state)?;
        // Flush events
        connection.flush()?;
        // This looks dumb... Anyway, avoiding an unnecessary poll and going straight to status update
        // if no new events or duration is now.
        let now = std::time::Instant::now();
        if match next_check.checked_duration_since(now) {
            Some(dur) => !new_event_within_deadline(connection, now, dur)?,
            None => true,
        } {
            // Status wants redraw and no new events.
            let dry_run = !state.any_monitors_showing_status();
            let next = checker.run_next(dry_run);
            if let Some(content) = next.content {
                manager.draw_status(content, next.position, state)?;
            }
            next_check = next.next_check;
            // Check destroyed, not that important so moved from event handling flow
            manager.destroy_marked(state)?;
        }
    }
}

fn loop_without_status<'a>(
    connection: &'a XorgConnection,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    // Arbitrarily chosen
    const DEADLINE: Duration = Duration::from_millis(1000);
    loop {
        drain_events(connection, manager, state)?;
        manager.destroy_marked(state)?;
        // Cleanup
        connection.flush()?;
        // Blocking with a time-out to allow destroying marked even if there are no events
        new_event_within_deadline(connection, std::time::Instant::now(), DEADLINE)?;
    }
}

#[inline]
fn drain_events<'a>(
    connection: &'a XorgConnection,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    while let Some(raw) = connection.poll_for_raw_event()? {
        // Ripped from x11rb_protocol, non-public small parsing functions
        let response_type = raw.get(0).map(|x| x & 0x7f).ok_or(Error::X11EventParse)?;
        let seq = raw
            .get(2..4)
            .map(|b| u16::from_ne_bytes(b.try_into().unwrap()))
            .ok_or(Error::X11EventParse)?;

        if state.should_ignore_sequence(seq)
            && (response_type == x11rb::protocol::xproto::ENTER_NOTIFY_EVENT
                || response_type == x11rb::protocol::xproto::UNMAP_NOTIFY_EVENT)
        {
            continue;
        }

        match response_type {
            x11rb::protocol::xproto::KEY_PRESS_EVENT => {
                manager.handle_key_press(KeyPressEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::MAP_REQUEST_EVENT => {
                manager.handle_map_request(MapRequestEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::UNMAP_NOTIFY_EVENT => {
                let evt = UnmapNotifyEvent::try_parse(&raw).unwrap().0;
                manager.handle_unmap_notify(evt, state)?;
            }
            x11rb::protocol::xproto::DESTROY_NOTIFY_EVENT => {
                manager
                    .handle_destroy_notify(DestroyNotifyEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::CONFIGURE_NOTIFY_EVENT => {
                manager.handle_configure_notify(
                    ConfigureNotifyEvent::try_parse(&raw).unwrap().0,
                    state,
                )?;
            }
            x11rb::protocol::xproto::CONFIGURE_REQUEST_EVENT => {
                manager.handle_configure_request(
                    ConfigureRequestEvent::try_parse(&raw).unwrap().0,
                    state,
                )?;
            }
            x11rb::protocol::xproto::BUTTON_PRESS_EVENT => {
                manager.handle_button_press(ButtonPressEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::BUTTON_RELEASE_EVENT => {
                manager
                    .handle_button_release(ButtonReleaseEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::MOTION_NOTIFY_EVENT => {
                manager
                    .handle_motion_notify(MotionNotifyEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::ENTER_NOTIFY_EVENT => {
                let evt = EnterNotifyEvent::try_parse(&raw).unwrap().0;
                manager.handle_enter(evt, state)?;
            }
            x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT => {
                manager
                    .handle_client_message(ClientMessageEvent::try_parse(&raw).unwrap().0, state)?;
            }
            x11rb::protocol::xproto::PROPERTY_NOTIFY_EVENT => {
                manager.handle_property_notify(
                    PropertyNotifyEvent::try_parse(&raw).unwrap().0,
                    state,
                )?;
            }
            x11rb::protocol::xproto::VISIBILITY_NOTIFY_EVENT => {
                manager.handle_visibility_change(
                    VisibilityNotifyEvent::try_parse(&raw).unwrap().0,
                    state,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn new_event_within_deadline(
    connection: &XorgConnection,
    start_instant: std::time::Instant,
    deadline: Duration,
) -> Result<bool> {
    use std::os::raw::c_int;
    use std::os::unix::io::AsRawFd;

    use nix::poll::{poll, PollFd, PollFlags};

    let fd = connection.stream().as_raw_fd();
    let mut poll_fds = [PollFd::new(fd, PollFlags::POLLIN)];
    loop {
        if let Some(timeout_millis) = deadline
            .checked_sub(start_instant.elapsed())
            .map(|remaining| c_int::try_from(remaining.as_millis()).unwrap_or(c_int::MAX))
        {
            match poll(&mut poll_fds, timeout_millis) {
                Ok(_) => {
                    if poll_fds[0]
                        .revents()
                        .unwrap_or_else(PollFlags::empty)
                        .contains(PollFlags::POLLIN)
                    {
                        return Ok(true);
                    }
                }
                // try again
                Err(nix::Error::EINTR) => {}
                Err(e) => return Err(e.into()),
            }
        } else {
            return Ok(false);
        }
        if start_instant.elapsed() >= deadline {
            return Ok(false);
        }
    }
}

fn find_render_visual_info(
    connection: &XorgConnection,
    screen: &Screen,
) -> Result<RenderVisualInfo> {
    Ok(RenderVisualInfo {
        root: find_appropriate_visual(connection, screen.root_depth, Some(screen.root_visual))?,
        render: find_appropriate_visual(connection, 32, None)?,
    })
}

fn find_appropriate_visual(
    connection: &XorgConnection,
    depth: u8,
    match_visual_id: Option<Visualid>,
) -> Result<VisualInfo> {
    let formats = x11rb::protocol::render::query_pict_formats(connection)?.reply()?;
    let candidates = formats
        .formats
        .into_iter()
        // Need a 32 bit depth visual
        .filter_map(|pfi| {
            (pfi.type_ == PictType::DIRECT && pfi.depth == depth).then(|| (pfi.id, pfi))
        })
        .collect::<HashMap<Pictformat, Pictforminfo>>();
    // Should only be one
    pgwm_core::debug!("{candidates:?}");
    for screen in formats.screens {
        let candidate = screen.depths.into_iter().find_map(|pd| {
            (pd.depth == depth)
                .then(|| {
                    pd.visuals.into_iter().find(|pv| {
                        if let Some(match_vid) = match_visual_id {
                            pv.visual == match_vid && candidates.contains_key(&pv.format)
                        } else {
                            candidates.contains_key(&pv.format)
                        }
                    })
                })
                .flatten()
        });
        if let Some(candidate) = candidate {
            pgwm_core::debug!("{candidate:?}");
            return Ok(VisualInfo {
                visual_id: candidate.visual,
                pict_format: candidate.format,
                direct_format: candidates[&candidate.format].direct,
                depth,
            });
        }
    }
    Err(Error::NoAppropriateVisual)
}
