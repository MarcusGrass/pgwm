use crate::error::{Error, Result};
use crate::manager;
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::manager::font::{load_alloc_fonts, FontDrawer, LoadedFonts};
use crate::manager::Manager;
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::colors::alloc_colors;
use pgwm_core::config::{BarCfg, Cfg, Options, Sizing};
use pgwm_core::render::{RenderVisualInfo, VisualInfo};
use pgwm_core::state::State;
use std::collections::HashMap;
use std::time::Duration;
use x11rb::protocol::render::{PictType, Pictformat, Pictforminfo};
use x11rb::protocol::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ClientMessageEvent, ConfigureNotifyEvent,
    ConfigureRequestEvent, DestroyNotifyEvent, EnterNotifyEvent, KeyPressEvent, MapRequestEvent,
    MotionNotifyEvent, PropertyNotifyEvent, Screen, UnmapNotifyEvent, VisibilityNotifyEvent,
    Visualid,
};
pub type XorgConnection = x11rb::SocketConnection;

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
    let setup = connection.setup().clone();

    let screen = &setup.roots[screen_num];
    let mut call_wrapper = CallWrapper::new(connection)?;

    call_wrapper.try_become_wm(screen)?;
    pgwm_core::debug!("Became wm");
    //let resource_db = x11rb::resource_manager::new_from_resource_manager(&connection)?
    //    .ok_or(Error::X11OpenDefaultDb)?;
    let rdb = x11rb::resource_manager::protocol::Database::GET_RESOURCE_DATABASE;
    let get_prop = x11rb::xcb::xproto::get_property(
        call_wrapper.inner_mut(),
        rdb.delete,
        screen.root,
        rdb.property,
        rdb.type_,
        rdb.long_offset,
        rdb.long_length,
        false,
    )?
    .reply(call_wrapper.inner_mut())?;
    pgwm_core::debug!("Got resource database properties");
    let resource_db =
        x11rb::resource_manager::protocol::Database::new_from_get_property_reply(&get_prop)
            .ok_or(Error::X11OpenDefaultDb)?;
    let cursor_handle = x11rb::cursor::Handle::new(call_wrapper.inner_mut(), 0, &resource_db)?;
    let visual = find_render_visual_info(call_wrapper.inner_mut(), screen)?;
    let loaded = load_alloc_fonts(&mut call_wrapper, &visual, &fonts, &char_remap)?;
    let lf = LoadedFonts::new(loaded, &char_remap)?;

    let font_drawer = FontDrawer::new(&lf);
    let colors = alloc_colors(call_wrapper.inner_mut(), screen.default_colormap, colors)?;

    pgwm_core::debug!("Creating state");
    let mut state = crate::x11::state_lifecycle::create_state(
        &mut call_wrapper,
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
    let drawer = Drawer::new(&font_drawer, &fonts);
    crate::debug!("Initialized Drawer");
    let bar_manager = BarManager::new(&font_drawer, &fonts);
    crate::debug!("Initialized bar sections");
    let manager = manager::Manager::new(drawer, bar_manager, cursor_handle);
    crate::debug!("Initialized manager");
    // Extremely ugly control flow here
    #[cfg(feature = "status-bar")]
    let should_check = !status_checks.is_empty();

    #[cfg(feature = "status-bar")]
    let mut mut_checks = status_checks.clone();
    #[cfg(feature = "status-bar")]
    let mut checker = pgwm_core::status::checker::Checker::new(&mut mut_checks);
    crate::debug!("Initialized Checker");
    manager.init(&mut call_wrapper, &mut state)?;
    crate::debug!("Initialized manager state");
    manager.scan(&mut call_wrapper, &mut state)?;
    crate::debug!("Initialized, starting loop");
    call_wrapper.inner_mut().sync()?;
    loop {
        #[cfg(feature = "status-bar")]
        let loop_result = if should_check {
            loop_with_status(&mut call_wrapper, &manager, &mut checker, &mut state)
        } else {
            loop_without_status(&mut call_wrapper, &manager, &mut state)
        };
        #[cfg(not(feature = "status-bar"))]
        let loop_result = loop_without_status(&mut call_wrapper, &manager, &mut state);

        if let Err(e) = loop_result {
            match e {
                Error::StateInvalidated => {
                    crate::x11::state_lifecycle::teardown_dynamic_state(&mut call_wrapper, &state)?;
                    state = crate::x11::state_lifecycle::reinit_state(
                        &mut call_wrapper,
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
                    manager.pick_up_state(&mut call_wrapper, &mut state)?;
                }
                Error::GracefulShutdown => {
                    crate::x11::state_lifecycle::teardown_full_state(
                        &mut call_wrapper,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_focus(&state)?;
                    return Ok(());
                }
                Error::FullRestart => {
                    crate::x11::state_lifecycle::teardown_full_state(
                        &mut call_wrapper,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_focus(&state)?;
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
    call_wrapper: &mut CallWrapper,
    manager: &Manager<'a>,
    checker: &mut pgwm_core::status::checker::Checker,
    state: &mut State,
) -> Result<()> {
    let mut next_check = std::time::Instant::now();
    // Extremely hot place in the code, should bench the checker
    loop {
        // This looks dumb... Anyway, avoiding an unnecessary poll and going straight to status update
        // if no new events or duration is now.
        while let Some(event) = call_wrapper
            .inner_mut()
            .read_next_event(next_check.duration_since(std::time::Instant::now()))?
        {
            handle_event(event, call_wrapper, manager, state)?;
        }
        // Status wants redraw and no new events.
        let dry_run = !state.any_monitors_showing_status();
        let next = checker.run_next(dry_run);
        if let Some(content) = next.content {
            manager.draw_status(call_wrapper, content, next.position, state)?;
        }
        next_check = next.next_check;
        // Check destroyed, not that important so moved from event handling flow
        Manager::destroy_marked(call_wrapper, state)?;
        call_wrapper.inner_mut().sync()?;
    }
}

fn loop_without_status<'a>(
    call_wrapper: &mut CallWrapper,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    // Arbitrarily chosen
    const DEADLINE: Duration = Duration::from_millis(1000);
    loop {
        while let Some(event) = call_wrapper.inner_mut().read_next_event(DEADLINE)? {
            handle_event(event, call_wrapper, manager, state)?;
        }
        Manager::destroy_marked(call_wrapper, state)?;
        call_wrapper.inner_mut().sync()?;
    }
}

#[inline]
fn handle_event<'a>(
    raw: Vec<u8>,
    call_wrapper: &mut CallWrapper,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    // Ripped from xcb_connection_protocol, non-public small parsing functions
    let response_type = raw.get(0).map(|x| x & 0x7f).ok_or(Error::X11EventParse)?;
    let seq = raw
        .get(2..4)
        .map(|b| u16::from_ne_bytes(b.try_into().unwrap()))
        .ok_or(Error::X11EventParse)?;

    #[cfg(feature = "debug")]
    dbg_event(&raw, &call_wrapper.inner_mut().extensions);
    if state.should_ignore_sequence(seq)
        && (response_type == x11rb::protocol::xproto::ENTER_NOTIFY_EVENT
            || response_type == x11rb::protocol::xproto::UNMAP_NOTIFY_EVENT)
    {
        pgwm_core::debug!("[Ignored]");
        return Ok(());
    }

    match response_type {
        x11rb::protocol::xproto::KEY_PRESS_EVENT => {
            manager.handle_key_press(
                call_wrapper,
                KeyPressEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::MAP_REQUEST_EVENT => {
            manager.handle_map_request(
                call_wrapper,
                MapRequestEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::UNMAP_NOTIFY_EVENT => {
            let evt = UnmapNotifyEvent::try_parse(&raw).unwrap().0;
            manager.handle_unmap_notify(call_wrapper, evt, state)?;
        }
        x11rb::protocol::xproto::DESTROY_NOTIFY_EVENT => {
            manager.handle_destroy_notify(
                call_wrapper,
                DestroyNotifyEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::CONFIGURE_NOTIFY_EVENT => {
            Manager::handle_configure_notify(
                call_wrapper,
                ConfigureNotifyEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::CONFIGURE_REQUEST_EVENT => {
            Manager::handle_configure_request(
                call_wrapper,
                ConfigureRequestEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::BUTTON_PRESS_EVENT => {
            manager.handle_button_press(
                call_wrapper,
                ButtonPressEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::BUTTON_RELEASE_EVENT => {
            manager.handle_button_release(
                call_wrapper,
                ButtonReleaseEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::MOTION_NOTIFY_EVENT => {
            manager.handle_motion_notify(
                call_wrapper,
                MotionNotifyEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::ENTER_NOTIFY_EVENT => {
            let evt = EnterNotifyEvent::try_parse(&raw).unwrap().0;
            manager.handle_enter(call_wrapper, evt, state)?;
        }
        x11rb::protocol::xproto::CLIENT_MESSAGE_EVENT => {
            manager.handle_client_message(
                call_wrapper,
                ClientMessageEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::PROPERTY_NOTIFY_EVENT => {
            manager.handle_property_notify(
                call_wrapper,
                PropertyNotifyEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        x11rb::protocol::xproto::VISIBILITY_NOTIFY_EVENT => {
            manager.handle_visibility_change(
                call_wrapper,
                VisibilityNotifyEvent::try_parse(&raw).unwrap().0,
                state,
            )?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "debug")]
fn dbg_event(raw: &[u8], ext_info_provider: &x11rb::x11_utils::ExtensionInfoProvider) {
    match x11rb::xcb::Event::parse(raw, ext_info_provider) {
        Ok(evt) => {
            eprintln!("Got event {evt:?}");
        }
        Err(e) => {
            eprintln!("Failed to parse event {e}");
        }
    }
}

fn find_render_visual_info(
    connection: &mut XorgConnection,
    screen: &Screen,
) -> Result<RenderVisualInfo> {
    Ok(RenderVisualInfo {
        root: find_appropriate_visual(connection, screen.root_depth, Some(screen.root_visual))?,
        render: find_appropriate_visual(connection, 32, None)?,
    })
}

fn find_appropriate_visual(
    connection: &mut XorgConnection,
    depth: u8,
    match_visual_id: Option<Visualid>,
) -> Result<VisualInfo> {
    let formats = x11rb::xcb::render::query_pict_formats(connection, false)?.reply(connection)?;
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
