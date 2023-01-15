use alloc::vec;
use alloc::vec::Vec;
use core::time::Duration;
use rusl::io_uring::{io_uring_register_buffers, io_uring_register_files, setup_io_uring};
use rusl::platform::{IoSliceMut, IoUringParamFlags, OpenFlags};
use rusl::unistd::open;
use smallmap::Map;
use tiny_std::signal::{CatchSignal, SaSignalaction};
use xcb_rust_protocol::con::XcbBuffers;
use xcb_rust_protocol::connection::render::RenderConnection;
use xcb_rust_protocol::proto::render::{PictTypeEnum, Pictformat, Pictforminfo};
use xcb_rust_protocol::proto::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ClientMessageEvent, ConfigureNotifyEvent,
    ConfigureRequestEvent, DestroyNotifyEvent, EnterNotifyEvent, KeyPressEvent, MapRequestEvent,
    MotionNotifyEvent, PropertyNotifyEvent, Screen, UnmapNotifyEvent, VisibilityNotifyEvent,
    Visualid,
};
use xcb_rust_protocol::util::FixedLengthFromBytes;
use xcb_rust_protocol::{XcbConnection, XcbEnv};

use pgwm_core::config::{BarCfg, Cfg, Options, Sizing};
use pgwm_core::render::{RenderVisualInfo, VisualInfo};
use pgwm_core::state::State;
use pgwm_core::status::sys::bat::BAT_FILE;
use pgwm_core::status::sys::cpu::CPU_LOAD_FILE;
use pgwm_core::status::sys::mem::MEM_LOAD_FILE;
use pgwm_core::status::sys::net::NET_STAT_FILE;

use crate::error::{Error, Result};
use crate::manager;
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::manager::font::{load_alloc_fonts, FontDrawer, LoadedFonts};
use crate::manager::Manager;
use crate::uring::UringWrapper;
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::colors::alloc_colors;

pub type XorgConnection = xcb_rust_connection::connection::SocketConnection;

const SOCK_IN_INDEX: usize = 0;
const SOCK_OUT_INDEX: usize = 1;
const BAT_INDEX: usize = 2;
const NET_INDEX: usize = 3;
const MEM_INDEX: usize = 4;
const CPU_INDEX: usize = 5;

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
    } = Cfg::new(
        tiny_std::env::var("XDG_CONFIG_HOME").ok(),
        tiny_std::env::var("HOME").ok(),
    )?;
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
    } = Cfg::new(
        tiny_std::env::var("XDG_CONFIG_HOME").ok(),
        tiny_std::env::var("HOME").ok(),
    )?;
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
    // We just spawn user stuff, we don't care when they terminate, could signalfd -> poll if we did
    // without the raw unsafety of setting up a signal handler
    unsafe {
        tiny_std::signal::add_signal_action(CatchSignal::SIGCHLD, SaSignalaction::Ign)?;
    }
    let xcb_env = env_to_xcb_env();
    let mut xcb_socket_in_buffer = vec![0u8; 65536];
    let mut xcb_socket_out_buffer = vec![0u8; 65536];
    let mut buffers = XcbBuffers {
        in_buffer: xcb_socket_in_buffer.as_mut_slice(),
        out_buffer: xcb_socket_out_buffer.as_mut_slice(),
    };
    let (mut connection, screen_num, socket_fd) =
        XorgConnection::connect(dpy, &mut buffers, xcb_env)?;
    let mut bat_buf = [0u8; 64];
    let mut net_buf = vec![0u8; 4096];
    let mut mem_buf = vec![0u8; 4096];
    let mut cpu_buf = vec![0u8; 4096];
    let bat_fd = open(BAT_FILE, OpenFlags::O_RDONLY)?;
    let net_fd = open(NET_STAT_FILE, OpenFlags::O_RDONLY)?;
    let mem_fd = open(MEM_LOAD_FILE, OpenFlags::O_RDONLY)?;
    let cpu_fd = open(CPU_LOAD_FILE, OpenFlags::O_RDONLY)?;
    let mut uring_wrapper = UringWrapper::new(
        buffers,
        &mut bat_buf,
        &mut net_buf,
        &mut mem_buf,
        &mut cpu_buf,
        socket_fd,
        bat_fd,
        net_fd,
        mem_fd,
        cpu_fd,
    )?;
    let setup = connection.setup().clone();
    uring_wrapper.await_write_completions()?;
    connection.reset_out_offset();
    pgwm_utils::debug!("Connected");
    let screen = &setup.roots[screen_num];
    let mut call_wrapper = CallWrapper::new(connection)?;
    pgwm_utils::debug!("Set up call wrapper");
    call_wrapper
        .inner_mut()
        .sync(uring_wrapper.xcb_buffers_mut())?;
    call_wrapper.try_become_wm(
        &mut xcb_socket_in_buffer,
        &mut xcb_socket_out_buffer,
        screen,
    )?;
    pgwm_utils::debug!("Became wm");
    pgwm_utils::debug!("Got resource database properties");
    let resource_db = xcb_rust_protocol::helpers::resource_manager::new_from_default(
        call_wrapper.inner_mut(),
        uring_wrapper.xcb_buffers_mut(),
        tiny_std::env::var("HOME").ok(),
        tiny_std::env::var("XENVIRONMENT").ok(),
    )?;
    let cursor_handle = xcb_rust_protocol::helpers::cursor::Handle::new(
        call_wrapper.inner_mut(),
        uring_wrapper.xcb_buffers_mut(),
        screen_num,
        &resource_db,
        xcb_env,
    )?;
    let visual = find_render_visual_info(
        call_wrapper.inner_mut(),
        uring_wrapper.xcb_buffers_mut(),
        screen,
    )?;
    let loaded = load_alloc_fonts(&mut call_wrapper, &visual, &fonts, &char_remap)?;
    crate::debug!("Loaded {} fonts", loaded.len());
    let lf = LoadedFonts::new(loaded, &char_remap)?;
    let font_drawer = FontDrawer::new(&lf);
    call_wrapper.inner_mut().flush(&mut xcb_socket_out_buffer)?;
    crate::debug!("Font drawer initialized");
    let colors = alloc_colors(call_wrapper.inner_mut(), screen.default_colormap, colors)?;
    crate::debug!("Allocated colors");

    pgwm_utils::debug!("Creating state");
    let mut state = crate::x11::state_lifecycle::create_state(
        &mut call_wrapper,
        &mut xcb_socket_in_buffer,
        &mut xcb_socket_out_buffer,
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
    let mut uring = setup_io_uring(8, IoUringParamFlags::IORING_SETUP_SINGLE_ISSUER)?;
    let mut bat_buf = [0u8; 64];
    let mut net_buf = vec![0u8; 4096];
    let mut mem_buf = vec![0u8; 4096];
    let mut cpu_buf = vec![0u8; 4096];
    unsafe {
        io_uring_register_buffers(
            uring.fd,
            &[
                IoSliceMut::new(&mut bat_buf),
                IoSliceMut::new(&mut net_buf),
                IoSliceMut::new(&mut mem_buf),
                IoSliceMut::new(&mut cpu_buf),
            ],
        )?;
    }
    let bat_fd = open(BAT_FILE, OpenFlags::O_RDONLY)?;
    let net_fd = open(NET_STAT_FILE, OpenFlags::O_RDONLY)?;
    let mem_fd = open(MEM_LOAD_FILE, OpenFlags::O_RDONLY)?;
    let cpu_fd = open(CPU_LOAD_FILE, OpenFlags::O_RDONLY)?;
    io_uring_register_files(uring.fd, &[bat_fd, net_fd, mem_fd, cpu_fd])?;

    loop {
        #[cfg(feature = "status-bar")]
        let loop_result = if should_check {
            loop_with_status(
                &mut call_wrapper,
                &mut xcb_socket_in_buffer,
                &mut xcb_socket_out_buffer,
                &manager,
                &mut checker,
                &mut state,
            )
        } else {
            loop_without_status(
                &mut call_wrapper,
                &mut xcb_socket_in_buffer,
                &mut xcb_socket_out_buffer,
                &manager,
                &mut state,
            )
        };
        #[cfg(not(feature = "status-bar"))]
        let loop_result = loop_without_status(&mut call_wrapper, &manager, &mut state);

        if let Err(e) = loop_result {
            match e {
                Error::StateInvalidated => {
                    crate::x11::state_lifecycle::teardown_dynamic_state(
                        &mut call_wrapper,
                        &mut xcb_socket_out_buffer,
                        &state,
                    )?;
                    call_wrapper
                        .inner_mut()
                        .sync(&mut xcb_socket_in_buffer, &mut xcb_socket_out_buffer)?;
                    state = crate::x11::state_lifecycle::reinit_state(
                        &mut call_wrapper,
                        &mut xcb_socket_in_buffer,
                        &mut xcb_socket_out_buffer,
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
                        &mut xcb_socket_in_buffer,
                        &mut xcb_socket_out_buffer,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_window(&mut xcb_socket_out_buffer, &state)?;
                    call_wrapper
                        .inner_mut()
                        .sync(&mut xcb_socket_in_buffer, &mut xcb_socket_out_buffer)?;
                    drop(call_wrapper);
                    return Ok(());
                }
                Error::FullRestart => {
                    crate::x11::state_lifecycle::teardown_full_state(
                        &mut call_wrapper,
                        &mut xcb_socket_in_buffer,
                        &mut xcb_socket_out_buffer,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_window(&mut xcb_socket_out_buffer, &state)?;
                    call_wrapper
                        .inner_mut()
                        .sync(&mut xcb_socket_in_buffer, &mut xcb_socket_out_buffer)?;
                    drop(call_wrapper);
                    return Err(Error::FullRestart);
                }
                _ => {
                    return Err(e);
                }
            }
        }
    }
}

fn env_to_xcb_env() -> XcbEnv<'static> {
    XcbEnv {
        home_dir: tiny_std::env::var("HOME").ok(),
        x_environment: tiny_std::env::var("XENVIRONMENT").ok(),
        x_authority: tiny_std::env::var("XAUTHORITY").ok(),
        display: tiny_std::env::var("DISPLAY").ok(),
        x_cursor_size: tiny_std::env::var("XCURSOR_SIZE").ok(),
    }
}

#[cfg(feature = "status-bar")]
fn loop_with_status(
    call_wrapper: &mut CallWrapper,
    xcb_buffers: &mut XcbBuffers,
    manager: &Manager,
    checker: &mut pgwm_core::status::checker::Checker,
    state: &mut State,
) -> Result<()> {
    let mut next_check = tiny_std::time::Instant::now();
    // Extremely hot place in the code, should bench the checker
    loop {
        // This looks dumb... Anyway, avoiding an unnecessary poll and going straight to status update
        // if no new events or duration is now.
        while let Some(event) = call_wrapper.inner_mut().read_next_event(
            xcb_buffers.in_buffer,
            next_check
                .duration_since(tiny_std::time::Instant::now())
                .unwrap_or_default(),
        )? {
            handle_event(event, call_wrapper, manager, state)?;
            call_wrapper.inner_mut().flush(xcb_buffers.in_buffer)?;
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
        call_wrapper.inner_mut().flush(xcb_socket_out_buffer)?;
        #[cfg(feature = "debug")]
        call_wrapper
            .inner_mut()
            .clear_cache(xcb_socket_in_buffer, xcb_socket_out_buffer)?;
    }
}

fn loop_without_status<'a>(
    call_wrapper: &mut CallWrapper,
    xcb_socket_in_buffer: &mut [u8],
    xcb_socket_out_buffer: &mut [u8],
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    // Arbitrarily chosen
    const DEADLINE: Duration = Duration::from_secs(10_000);
    loop {
        while let Some(event) = call_wrapper
            .inner_mut()
            .read_next_event(xcb_socket_in_buffer, DEADLINE)?
        {
            handle_event(event, call_wrapper, manager, state)?;
            call_wrapper.inner_mut().flush(xcb_socket_out_buffer)?;
        }
        Manager::destroy_marked(call_wrapper, state)?;
        call_wrapper.inner_mut().flush(xcb_socket_out_buffer)?;
        #[cfg(feature = "debug")]
        call_wrapper
            .inner_mut()
            .clear_cache(xcb_socket_in_buffer, xcb_socket_out_buffer)?;
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
    let response_type = raw.first().map(|x| x & 0x7f).ok_or(Error::X11EventParse)?;
    let seq = raw
        .get(2..4)
        .map(|b| u16::from_ne_bytes(b.try_into().unwrap()))
        .ok_or(Error::X11EventParse)?;

    #[cfg(feature = "debug")]
    dbg_event(&raw, &call_wrapper.inner_mut().extensions);
    // Unmap and enter are caused by upstream actions, causing unwanted focusing behaviour etc.
    if state.should_ignore_sequence(seq)
        && (response_type == xcb_rust_protocol::proto::xproto::ENTER_NOTIFY_EVENT
            || response_type == xcb_rust_protocol::proto::xproto::UNMAP_NOTIFY_EVENT)
    {
        pgwm_utils::debug!("[Ignored]");
        return Ok(());
    }

    match response_type {
        xcb_rust_protocol::proto::xproto::KEY_PRESS_EVENT => {
            manager.handle_key_press(
                call_wrapper,
                KeyPressEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::MAP_REQUEST_EVENT => {
            manager.handle_map_request(
                call_wrapper,
                MapRequestEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::UNMAP_NOTIFY_EVENT => {
            let evt = UnmapNotifyEvent::from_bytes(&raw).unwrap();
            manager.handle_unmap_notify(call_wrapper, evt, state)?;
        }
        xcb_rust_protocol::proto::xproto::DESTROY_NOTIFY_EVENT => {
            manager.handle_destroy_notify(
                call_wrapper,
                DestroyNotifyEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::CONFIGURE_NOTIFY_EVENT => {
            Manager::handle_configure_notify(
                call_wrapper,
                ConfigureNotifyEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::CONFIGURE_REQUEST_EVENT => {
            Manager::handle_configure_request(
                call_wrapper,
                ConfigureRequestEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::BUTTON_PRESS_EVENT => {
            manager.handle_button_press(
                call_wrapper,
                ButtonPressEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::BUTTON_RELEASE_EVENT => {
            manager.handle_button_release(
                call_wrapper,
                ButtonReleaseEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::MOTION_NOTIFY_EVENT => {
            manager.handle_motion_notify(
                call_wrapper,
                MotionNotifyEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::ENTER_NOTIFY_EVENT => {
            let evt = EnterNotifyEvent::from_bytes(&raw).unwrap();
            manager.handle_enter(call_wrapper, evt, state)?;
        }
        xcb_rust_protocol::proto::xproto::CLIENT_MESSAGE_EVENT => {
            manager.handle_client_message(
                call_wrapper,
                ClientMessageEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::PROPERTY_NOTIFY_EVENT => {
            manager.handle_property_notify(
                call_wrapper,
                PropertyNotifyEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        xcb_rust_protocol::proto::xproto::VISIBILITY_NOTIFY_EVENT => {
            manager.handle_visibility_change(
                call_wrapper,
                VisibilityNotifyEvent::from_bytes(&raw).unwrap(),
                state,
            )?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(feature = "debug")]
fn dbg_event(
    raw: &[u8],
    ext_info_provider: &xcb_rust_connection::helpers::basic_info_provider::BasicExtensionInfoProvider,
) {
    match xcb_rust_protocol::proto::Event::from_bytes(raw, ext_info_provider) {
        Ok(evt) => {
            crate::debug!("Got event: {evt:?}");
        }
        Err(e) => {
            crate::debug!("Failed to parse event {e}");
        }
    }
}

fn find_render_visual_info(
    connection: &mut XorgConnection,
    xcb_buffers: &mut XcbBuffers,
    screen: &Screen,
) -> Result<RenderVisualInfo> {
    Ok(RenderVisualInfo {
        root: find_appropriate_visual(
            connection,
            xcb_buffers,
            screen.root_depth,
            Some(screen.root_visual),
        )?,
        render: find_appropriate_visual(connection, xcb_buffers, 32, None)?,
    })
}

fn find_appropriate_visual(
    connection: &mut XorgConnection,
    xcb_buffers: &mut XcbBuffers,
    depth: u8,
    match_visual_id: Option<Visualid>,
) -> Result<VisualInfo> {
    let formats = connection
        .query_pict_formats(xcb_socket_out_buffer, false)?
        .reply(connection, xcb_socket_in_buffer, xcb_socket_out_buffer)?;
    let candidates = formats
        .formats
        .into_iter()
        // Need a 32 bit depth visual
        .filter_map(|pfi| {
            (pfi.r#type == PictTypeEnum::DIRECT && pfi.depth == depth).then_some((pfi.id, pfi))
        })
        .collect::<Map<Pictformat, Pictforminfo>>();
    // Should only be one
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
