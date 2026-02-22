use alloc::vec;
use alloc::vec::Vec;
use rusl::platform::{AddressFamily, SocketAddressUnix, SocketFlags, SocketOptions, SocketType};
use rusl::process::{CatchSignal, SaSignalaction};
use rusl::string::unix_str::UnixStr;
use smallmap::Map;
use tiny_std::unix::fd::RawFd;
use xcb_rust_protocol::XcbEnv;
use xcb_rust_protocol::con::XcbState;
use xcb_rust_protocol::connection::render::query_pict_formats;
use xcb_rust_protocol::proto::render::{PictTypeEnum, Pictformat, Pictforminfo};
use xcb_rust_protocol::proto::xproto::{
    ButtonPressEvent, ButtonReleaseEvent, ClientMessageEvent, ConfigureNotifyEvent,
    ConfigureRequestEvent, DestroyNotifyEvent, EnterNotifyEvent, KeyPressEvent, MapRequestEvent,
    MotionNotifyEvent, PropertyNotifyEvent, Screen, UnmapNotifyEvent, VisibilityNotifyEvent,
    Visualid,
};
use xcb_rust_protocol::util::FixedLengthFromBytes;

use pgwm_core::render::{RenderVisualInfo, VisualInfo};
use pgwm_core::state::State;

use crate::error::{Error, Result};
use crate::manager;
use crate::manager::Manager;
use crate::manager::bar::BarManager;
use crate::manager::draw::Drawer;
use crate::manager::font::{FontDrawer, LoadedFonts, load_alloc_fonts};
use crate::uring::{UringReadEvent, UringWrapper};
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::colors::alloc_colors;

const HOME: &UnixStr = UnixStr::from_str_checked("HOME\0");
const XENVIRONMENT: &UnixStr = UnixStr::from_str_checked("XENVIRONMENT\0");
const XAUTHORITY: &UnixStr = UnixStr::from_str_checked("XAUTHORITY\0");
const DISPLAY: &UnixStr = UnixStr::from_str_checked("DISPLAY\0");
const XCURSOR_SIZE: &UnixStr = UnixStr::from_str_checked("XCURSOR_SIZE\0");

#[allow(clippy::too_many_lines)]
pub(crate) fn run_wm() -> Result<()> {
    #[cfg(feature = "perf-test")]
    let dpy = Some(":4");
    #[cfg(not(feature = "perf-test"))]
    let dpy = None;
    // We just spawn user stuff, we don't care when they terminate, could signalfd -> poll if we did
    // without the raw unsafety of setting up a signal handler
    unsafe {
        rusl::process::add_signal_action(CatchSignal::Chld, SaSignalaction::Ign)?;
    }
    crate::debug!("Set sigignore for children");
    let xcb_env = env_to_xcb_env();
    let xcb_socket_in_buffer = vec![0u8; 65536];
    let xcb_socket_out_buffer = vec![0u8; 65536];
    crate::debug!("Looking for socket path");
    let (path, dpy_info) = xcb_rust_connection::connection::find_socket_path(dpy)?;
    let socket_fd = rusl::network::socket(
        AddressFamily::AF_UNIX,
        SocketOptions::new(SocketType::SOCK_STREAM, SocketFlags::empty()),
        0,
    )?;

    //let socket_fd = tiny_std::net::UnixStream::connect(path, true)?;
    let addr = SocketAddressUnix::try_from_unix(&path)?;
    rusl::network::connect_unix(socket_fd, &addr)?;

    let mut uring_wrapper = instantiate_uring(
        xcb_socket_in_buffer,
        xcb_socket_out_buffer,
        socket_fd,
        #[cfg(feature = "status-bar")]
        &pgwm_core::config::STATUS_CHECKS,
    )?;
    // On connect we'll start the listening loop
    uring_wrapper.submit_sock_read()?;
    let screen_num = dpy_info.screen;
    let evt_state = xcb_rust_connection::connection::setup(&mut uring_wrapper, xcb_env, dpy_info)?;
    let setup = evt_state.setup().clone();
    pgwm_utils::debug!("Connected");
    let screen = &setup.roots[screen_num as usize];
    let mut call_wrapper = CallWrapper::new(evt_state, uring_wrapper)?;
    pgwm_utils::debug!("Set up call wrapper");
    call_wrapper.try_become_wm(screen)?;
    pgwm_utils::debug!("Became wm");
    pgwm_utils::debug!("Got resource database properties");
    let resource_db = xcb_rust_protocol::helpers::resource_manager::new_from_default(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        tiny_std::env::var_unix(HOME).ok(),
        tiny_std::env::var_unix(XENVIRONMENT).ok(),
    )?;
    let cursor_handle = xcb_rust_protocol::helpers::cursor::Handle::new(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        screen_num as usize,
        &resource_db,
        xcb_env,
    )?;
    let visual = find_render_visual_info(&mut call_wrapper, screen)?;
    let loaded = load_alloc_fonts(&mut call_wrapper, &visual)?;
    call_wrapper.uring.await_write_completions()?;

    crate::debug!("Loaded {} fonts", loaded.len());
    let lf = LoadedFonts::new(loaded)?;
    let font_drawer = FontDrawer::new(&lf);
    crate::debug!("Font drawer initialized");
    let colors = alloc_colors(&mut call_wrapper, screen.default_colormap)?;
    crate::debug!("Allocated colors");

    pgwm_utils::debug!("Creating state");
    let mut state = crate::x11::state_lifecycle::create_state(
        &mut call_wrapper,
        &font_drawer,
        visual,
        screen,
        colors,
    )?;

    crate::debug!("Initialized mappings");
    let drawer = Drawer::new(&font_drawer);
    crate::debug!("Initialized Drawer");
    let bar_manager = BarManager::new(&font_drawer);
    crate::debug!("Initialized bar sections");
    let manager = manager::Manager::new(drawer, bar_manager, cursor_handle);
    crate::debug!("Initialized manager");
    // Extremely ugly control flow here
    #[cfg(feature = "status-bar")]
    let should_check = !pgwm_core::config::STATUS_CHECKS.is_empty();

    #[cfg(feature = "status-bar")]
    let mut mut_checks = pgwm_core::config::STATUS_CHECKS;
    #[cfg(feature = "status-bar")]
    let mut checker = pgwm_core::status::checker::Checker::new(&mut mut_checks);
    crate::debug!("Initialized Checker");
    manager.init(&mut call_wrapper, &mut state)?;
    crate::debug!("Initialized manager state");
    manager.scan(&mut call_wrapper, &mut state)?;
    crate::debug!("Initialized, starting loop");
    loop {
        #[cfg(feature = "status-bar")]
        let loop_result = if should_check {
            loop_with_status(&mut call_wrapper, &manager, &mut checker, &mut state)
        } else {
            loop_without_status(&mut call_wrapper, &mut checker, &manager, &mut state)
        };
        #[cfg(not(feature = "status-bar"))]
        let loop_result = loop_without_status(&mut call_wrapper, &manager, &mut state);

        if let Err(e) = loop_result {
            match e {
                Error::StateInvalidated => {
                    crate::debug!("Invalidated state, reloading");
                    crate::x11::state_lifecycle::teardown_dynamic_state(&mut call_wrapper, &state)?;
                    call_wrapper.uring.await_write_completions()?;
                    state = crate::x11::state_lifecycle::reinit_state(
                        &mut call_wrapper,
                        &font_drawer,
                        visual,
                        state,
                    )?;
                    manager.pick_up_state(&mut call_wrapper, &mut state)?;
                }
                Error::GracefulShutdown => {
                    crate::x11::state_lifecycle::teardown_full_state(
                        &mut call_wrapper,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_window(&state)?;
                    // TODO: sync
                    drop(call_wrapper);
                    return Ok(());
                }
                Error::FullRestart => {
                    crate::debug!("Got full restart");
                    crate::x11::state_lifecycle::teardown_full_state(
                        &mut call_wrapper,
                        &state,
                        &lf,
                    )?;
                    call_wrapper.reset_root_window(&state)?;
                    // TODO: Sync
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
        home_dir: tiny_std::env::var_unix(HOME).ok(),
        x_environment: tiny_std::env::var_unix(XENVIRONMENT).ok(),
        x_authority: tiny_std::env::var_unix(XAUTHORITY).ok(),
        display: tiny_std::env::var_unix(DISPLAY).ok(),
        x_cursor_size: tiny_std::env::var_unix(XCURSOR_SIZE).ok(),
    }
}

fn instantiate_uring(
    xcb_socket_in_buffer: Vec<u8>,
    xcb_socket_out_buffer: Vec<u8>,
    socket_fd: RawFd,
    #[cfg(feature = "status-bar")] checks: &[pgwm_core::status::checker::Check],
) -> Result<UringWrapper> {
    // We're doing the alloc here regardless of if the check is used for simplicity
    #[cfg(feature = "status-bar")]
    let bat_buf = vec![0u8; 64];
    #[cfg(feature = "status-bar")]
    let net_buf = vec![0u8; 4096];
    #[cfg(feature = "status-bar")]
    let mem_buf = vec![0u8; 4096];
    #[cfg(feature = "status-bar")]
    let cpu_buf = vec![0u8; 4096];
    #[cfg(feature = "status-bar")]
    let mut bat_fd = None;
    #[cfg(feature = "status-bar")]
    let mut net_fd = None;
    #[cfg(feature = "status-bar")]
    let mut mem_fd = None;
    #[cfg(feature = "status-bar")]
    let mut cpu_fd = None;
    #[cfg(feature = "status-bar")]
    for check in checks {
        match check.check_type {
            pgwm_core::status::checker::CheckType::Battery(_) => {
                bat_fd = Some(try_open_fd(pgwm_core::status::sys::bat::BAT_FILE)?);
            }
            pgwm_core::status::checker::CheckType::Cpu(_) => {
                cpu_fd = Some(try_open_fd(pgwm_core::status::sys::cpu::CPU_LOAD_FILE)?);
            }
            pgwm_core::status::checker::CheckType::Net(_) => {
                net_fd = Some(try_open_fd(pgwm_core::status::sys::net::NET_STAT_FILE)?);
            }
            pgwm_core::status::checker::CheckType::Mem(_) => {
                mem_fd = Some(try_open_fd(pgwm_core::status::sys::mem::MEM_LOAD_FILE)?);
            }
            pgwm_core::status::checker::CheckType::Date(_) => {}
        }
    }

    let uring_wrapper = UringWrapper::new(
        xcb_socket_in_buffer,
        xcb_socket_out_buffer,
        socket_fd,
        #[cfg(feature = "status-bar")]
        bat_buf,
        #[cfg(feature = "status-bar")]
        net_buf,
        #[cfg(feature = "status-bar")]
        mem_buf,
        #[cfg(feature = "status-bar")]
        cpu_buf,
        #[cfg(feature = "status-bar")]
        bat_fd.unwrap_or_default(),
        #[cfg(feature = "status-bar")]
        net_fd.unwrap_or_default(),
        #[cfg(feature = "status-bar")]
        mem_fd.unwrap_or_default(),
        #[cfg(feature = "status-bar")]
        cpu_fd.unwrap_or_default(),
    )?;
    Ok(uring_wrapper)
}

#[cfg(feature = "status-bar")]
fn try_open_fd(file: &UnixStr) -> Result<RawFd> {
    match rusl::unistd::open(file, rusl::platform::OpenFlags::O_RDONLY) {
        Ok(f) => Ok(f),
        Err(e) => {
            tiny_std::eprintln!("Failed to open check file {file:?} {e}");
            Err(e.into())
        }
    }
}

#[cfg(feature = "status-bar")]
fn loop_with_status(
    call_wrapper: &mut CallWrapper,
    manager: &Manager,
    checker: &mut pgwm_core::status::checker::Checker,
    state: &mut State,
) -> Result<()> {
    for (next, when) in checker.get_all_check_submits() {
        match next {
            pgwm_core::status::checker::NextCheck::BAT => {
                call_wrapper.uring.submit_bat_read(&when)?;
            }
            pgwm_core::status::checker::NextCheck::CPU => {
                call_wrapper.uring.submit_cpu_read(&when)?;
            }
            pgwm_core::status::checker::NextCheck::NET => {
                call_wrapper.uring.submit_net_read(&when)?;
            }
            pgwm_core::status::checker::NextCheck::MEM => {
                call_wrapper.uring.submit_mem_read(&when)?;
            }
            pgwm_core::status::checker::NextCheck::Date => {
                call_wrapper.uring.submit_date_timeout(&when);
            }
        }
    }
    crate::debug!("Starting wm loop");
    // Extremely hot place in the code, should bench the checker
    loop {
        for evt in call_wrapper.uring.check_ready_cached() {
            handle_read_event(evt, call_wrapper, checker, manager, state)?;
        }
        let next = call_wrapper.uring.await_next_completion()?;
        handle_read_event(next, call_wrapper, checker, manager, state)?;
        Manager::destroy_marked(call_wrapper, state)?;
        #[cfg(feature = "debug")]
        call_wrapper
            .xcb_state
            .clear_cache(&mut call_wrapper.uring)?;

        // Need to flush to prevent busting the out buffer
        call_wrapper.uring.await_write_completions()?;
    }
}

#[inline]
fn handle_read_event(
    next: UringReadEvent,
    call_wrapper: &mut CallWrapper,
    #[cfg(feature = "status-bar")] checker: &mut pgwm_core::status::checker::Checker,
    manager: &Manager,
    state: &mut State,
) -> Result<()> {
    match next {
        UringReadEvent::SockIn => {
            for event in xcb_rust_connection::connection::try_drain(
                &mut call_wrapper.uring,
                &mut call_wrapper.xcb_state,
            )? {
                handle_event(event, call_wrapper, manager, state)?;
            }
        }
        #[cfg(feature = "status-bar")]
        UringReadEvent::Bat => {
            crate::debug!("Got bat event");
            if let Some(next) = checker.handle_completed(
                pgwm_core::status::checker::NextCheck::BAT,
                call_wrapper.uring.read_bat().unwrap(),
            ) {
                if let Some(content) = next.content {
                    manager.draw_status(call_wrapper, content, next.position, state)?;
                }
                call_wrapper.uring.submit_bat_read(&next.next_check)?;
            }
        }
        #[cfg(feature = "status-bar")]
        UringReadEvent::Net => {
            crate::debug!("Got net event");
            if let Some(next) = checker.handle_completed(
                pgwm_core::status::checker::NextCheck::NET,
                call_wrapper.uring.read_net().unwrap(),
            ) {
                if let Some(content) = next.content {
                    manager.draw_status(call_wrapper, content, next.position, state)?;
                }
                call_wrapper.uring.submit_net_read(&next.next_check)?;
            }
        }
        #[cfg(feature = "status-bar")]
        UringReadEvent::Mem => {
            crate::debug!("Got mem event");
            if let Some(next) = checker.handle_completed(
                pgwm_core::status::checker::NextCheck::MEM,
                call_wrapper.uring.read_mem().unwrap(),
            ) {
                if let Some(content) = next.content {
                    manager.draw_status(call_wrapper, content, next.position, state)?;
                }
                call_wrapper.uring.submit_mem_read(&next.next_check)?;
            }
        }
        #[cfg(feature = "status-bar")]
        UringReadEvent::Cpu => {
            crate::debug!("Got cpu event");
            if let Some(next) = checker.handle_completed(
                pgwm_core::status::checker::NextCheck::CPU,
                call_wrapper.uring.read_cpu().unwrap(),
            ) {
                if let Some(content) = next.content {
                    manager.draw_status(call_wrapper, content, next.position, state)?;
                }
                call_wrapper.uring.submit_cpu_read(&next.next_check)?;
            }
        }
        #[cfg(feature = "status-bar")]
        UringReadEvent::DateTimeout => {
            crate::debug!("Got date event");
            if let Some(next) =
                checker.handle_completed(pgwm_core::status::checker::NextCheck::Date, &[])
            {
                call_wrapper.uring.read_date();
                if let Some(content) = next.content {
                    manager.draw_status(call_wrapper, content, next.position, state)?;
                }
                call_wrapper.uring.submit_date_timeout(&next.next_check);
            }
        }
    }
    Ok(())
}

fn loop_without_status<'a>(
    call_wrapper: &mut CallWrapper,
    #[cfg(feature = "status-bar")] checker: &mut pgwm_core::status::checker::Checker,
    manager: &'a Manager<'a>,
    state: &mut State,
) -> Result<()> {
    crate::debug!(
        "Counter after initial sock read submit {:?}",
        call_wrapper.uring.counter
    );
    crate::debug!("Starting wm loop");
    // Extremely hot place in the code, should bench the checker
    loop {
        crate::debug!("Checking cached");
        for evt in call_wrapper.uring.check_ready_cached() {
            #[cfg(feature = "status-bar")]
            handle_read_event(evt, call_wrapper, checker, manager, state)?;
            #[cfg(not(feature = "status-bar"))]
            handle_read_event(evt, call_wrapper, manager, state)?;
        }
        crate::debug!("Checked cached, awaiting next completion");
        let next = call_wrapper.uring.await_next_completion()?;
        crate::debug!("Got next completion");
        #[cfg(feature = "status-bar")]
        handle_read_event(next, call_wrapper, checker, manager, state)?;
        #[cfg(not(feature = "status-bar"))]
        handle_read_event(next, call_wrapper, manager, state)?;
        crate::debug!("Handled next completion");
        Manager::destroy_marked(call_wrapper, state)?;
        #[cfg(feature = "debug")]
        call_wrapper
            .xcb_state
            .clear_cache(&mut call_wrapper.uring)?;

        // Need to flush to prevent busting the out buffer
        call_wrapper.uring.await_write_completions()?;
        crate::debug!("Loop done {:?}", call_wrapper.uring.counter);
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
    dbg_event(&raw, &call_wrapper.xcb_state.extensions);
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
    call_wrapper: &mut CallWrapper,
    screen: &Screen,
) -> Result<RenderVisualInfo> {
    Ok(RenderVisualInfo {
        root: find_appropriate_visual(call_wrapper, screen.root_depth, Some(screen.root_visual))?,
        render: find_appropriate_visual(call_wrapper, 32, None)?,
    })
}

fn find_appropriate_visual(
    call_wrapper: &mut CallWrapper,
    depth: u8,
    match_visual_id: Option<Visualid>,
) -> Result<VisualInfo> {
    let formats = query_pict_formats(&mut call_wrapper.uring, &mut call_wrapper.xcb_state, false)?
        .reply(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?;
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
