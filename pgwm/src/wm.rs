use crate::error::{Error, Result};
use crate::manager;
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::manager::font::FontManager;
use crate::manager::Manager;
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::client_message::ClientMessageHandler;
use crate::x11::colors::alloc_colors;
use pgwm_core::config::{BarCfg, Cfg, Options, Sizing};
use pgwm_core::state::State;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::resource_manager::Database;
use x11rb::rust_connection::RustConnection;

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
        mut status_checks,
    } = bar;
    let (connection, screen_num) = x11rb::connect(None)?;
    let setup = connection.setup();
    let screen = &setup.roots[screen_num];
    let call_wrapper = CallWrapper::new(&connection)?;
    call_wrapper.try_become_wm(screen)?;
    connection.flush()?;
    let resource_db = Database::new_from_default(&connection)?;
    let cursor_handle = x11rb::cursor::Handle::new(&connection, 0, &resource_db)?;

    let colors = alloc_colors(&connection, screen.default_colormap, colors)?;

    let font_manager = FontManager::new(&colors, &fonts, &char_remap)?;

    let mut state = crate::x11::state_lifecycle::create_state(
        &connection,
        &call_wrapper,
        &font_manager,
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
        status_checks.len(),
    )?;

    crate::debug!("Initialized mappings");
    let drawer = Drawer::new(&font_manager, &call_wrapper, &fonts);
    crate::debug!("Initialized Drawer");
    let client_message_handler = ClientMessageHandler::new(&connection, &call_wrapper);
    crate::debug!("Initialized Client message handler");

    let bar_manager = BarManager::new(&call_wrapper, &font_manager, &fonts);
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
    let mut checker = pgwm_core::status::checker::Checker::new(&mut status_checks);
    crate::debug!("Initialized Checker");
    manager.init(&mut state)?;
    crate::debug!("Initialized manager state");
    connection.flush()?;
    manager.scan(&mut state)?;
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
                        &font_manager,
                        &fonts,
                        state,
                        &workspaces,
                        &shortcuts,
                        &key_mappings,
                        &mouse_mappings,
                    )?;
                    manager.pick_up_state(&mut state)?;
                }
                Error::GracefulShutdown => {
                    crate::x11::state_lifecycle::teardown_full_state(&connection, &state)?;
                    connection.flush()?;
                    call_wrapper.reset_root_focus(&state)?;
                    connection.flush()?;
                    return Ok(());
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
    connection: &RustConnection,
    manager: &Manager<'a>,
    checker: &mut pgwm_core::status::checker::Checker,
    state: &mut State,
) -> Result<()> {
    let mut next_check = std::time::Instant::now();
    // Extremely hot place in the code, should bench the checker
    loop {
        // Flush events
        connection.flush()?;
        // Check any events in queue
        while let Some(event) = connection.poll_for_event()? {
            // Handle all
            manager.handle_event(event, state)?;
        }
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
    connection: &'a RustConnection,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    // Arbitrarily chosen
    const DEADLINE: Duration = Duration::from_millis(1000);
    loop {
        // Cleanup
        connection.flush()?;
        while let Some(event) = connection.poll_for_event()? {
            manager.handle_event(event, state)?;
        }
        manager.destroy_marked(state)?;
        // Blocking with a time-out to allow destroying marked even if there are no events
        new_event_within_deadline(connection, std::time::Instant::now(), DEADLINE)?;
    }
}

fn new_event_within_deadline(
    connection: &RustConnection,
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
