use alloc::vec::Vec;

use heapless::binary_heap::Min;
use smallmap::Map;
use xcb_rust_protocol::con::XcbState;
use xcb_rust_protocol::connection::render::{free_glyph_set, free_picture};
use xcb_rust_protocol::connection::xproto::{
    create_g_c, create_pixmap, create_window, free_pixmap, get_keyboard_mapping, grab_button,
    grab_key, map_window, ungrab_button, ungrab_key,
};
use xcb_rust_protocol::cookie::VoidCookie;
use xcb_rust_protocol::proto::xproto::{
    CapStyleEnum, CreateGCValueList, CreateWindowValueList, CursorEnum, EventMask, Gcontext,
    GrabEnum, GrabModeEnum, JoinStyleEnum, LineStyleEnum, Pixmap, Screen, Window, WindowClassEnum,
    WindowEnum,
};
use xcb_rust_protocol::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME};

use pgwm_core::colors::Colors;
use pgwm_core::config::key_map::{KeyBoardMappingKey, KeyboardMapping};
use pgwm_core::config::mouse_map::MouseActionKey;
use pgwm_core::config::workspaces::UserWorkspace;
use pgwm_core::config::{
    Action, FontCfg, BAR_SHORTCUTS, BINARY_HEAP_LIMIT, DYING_WINDOW_CACHE, KEYBOARD_MAPPINGS,
    MOUSE_MAPPINGS, STATUS_BAR_HEIGHT, TAB_BAR_HEIGHT, USER_WORKSPACES, WINDOW_BORDER_WIDTH,
    WINDOW_PADDING, WM_SHOW_BAR_INITIALLY, WORKSPACE_BAR_WINDOW_NAME_PADDING,
    WORKSPACE_SECTION_FONTS,
};
#[cfg(feature = "status-bar")]
use pgwm_core::config::{_STATUS_BAR_CHECK_SEP, _STATUS_BAR_FIRST_SEP};
use pgwm_core::geometry::{Dimensions, Line};
use pgwm_core::push_heapless;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderPicture, RenderVisualInfo};
#[cfg(feature = "status-bar")]
use pgwm_core::state::bar_geometry::StatusSection;
use pgwm_core::state::bar_geometry::{
    BarGeometry, FixedDisplayComponent, ShortcutComponent, ShortcutSection, WorkspaceSection,
};
use pgwm_core::state::workspace::Workspaces;
use pgwm_core::state::{Monitor, State, WinMarkedForDeath};
#[cfg(feature = "status-bar")]
use pgwm_core::status::checker::{Check, CheckType};

use crate::error::Result;
use crate::manager::font::{FontDrawer, LoadedFonts};
use crate::x11::call_wrapper::CallWrapper;

const COOKIE_CONTAINER_CAPACITY: usize = 64;

pub(crate) fn create_state<'a>(
    call_wrapper: &'a mut CallWrapper,
    font_manager: &'a FontDrawer<'a>,
    visual: RenderVisualInfo,
    screen: &'a Screen,
    colors: Colors,
) -> Result<State> {
    let mut cookie_container = heapless::Vec::new();
    let static_state = create_static_state(call_wrapper, screen, &colors, &mut cookie_container)?;
    do_create_state(
        call_wrapper,
        font_manager,
        visual,
        screen.clone(),
        static_state.intern_created_windows,
        heapless::Vec::new(),
        Workspaces::create_empty(&USER_WORKSPACES)?,
        colors,
        static_state.wm_check_win,
        static_state.sequences_to_ignore,
        false,
        WINDOW_BORDER_WIDTH,
        WINDOW_PADDING,
        cookie_container,
    )
}

pub(crate) fn reinit_state<'a>(
    call_wrapper: &'a mut CallWrapper,
    font_manager: &'a FontDrawer<'a>,
    visual: RenderVisualInfo,
    state: State,
) -> Result<State> {
    let cookie_container = heapless::Vec::new();
    do_create_state(
        call_wrapper,
        font_manager,
        visual,
        state.screen.clone(),
        state.intern_created_windows,
        state.dying_windows,
        state.workspaces,
        state.colors,
        state.wm_check_win,
        state.sequences_to_ignore,
        state.pointer_grabbed,
        state.window_border_width,
        state.window_padding,
        cookie_container,
    )
}

pub(crate) fn teardown_dynamic_state(call_wrapper: &mut CallWrapper, state: &State) -> Result<()> {
    for mon in &state.monitors {
        call_wrapper.send_destroy(mon.bar_win.window.drawable)?;
        free_picture(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            mon.bar_win.window.picture,
            true,
        )?;
        call_wrapper.send_destroy(mon.tab_bar_win.window.drawable)?;
        free_picture(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            mon.tab_bar_win.window.picture,
            true,
        )?;
    }
    Ok(())
}

pub(crate) fn teardown_full_state(
    call_wrapper: &mut CallWrapper,
    state: &State,
    loaded_fonts: &LoadedFonts,
) -> Result<()> {
    let _ = teardown_dynamic_state(call_wrapper, state);
    call_wrapper.send_destroy(state.wm_check_win)?;
    for font in loaded_fonts.fonts.values() {
        free_glyph_set(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            font.glyph_set,
            true,
        )?;
    }
    ungrab_keys(call_wrapper, &state.key_mapping, state.screen.root)?;
    for mon in &state.monitors {
        ungrab_mouse(
            call_wrapper,
            mon.bar_win.window.drawable,
            state.screen.root,
            &state.mouse_mapping,
        )?;
    }
    Ok(())
}

#[inline(always)] // definitely need this inlined, it's a monster
#[allow(
    clippy::too_many_lines,
    clippy::fn_params_excessive_bools,
    clippy::inline_always
)]
fn do_create_state<'a>(
    call_wrapper: &'a mut CallWrapper,
    font_manager: &'a FontDrawer<'a>,
    vis_info: RenderVisualInfo,
    screen: Screen,
    mut intern_created_windows: Map<Window, ()>,
    dying_windows: heapless::Vec<WinMarkedForDeath, DYING_WINDOW_CACHE>,
    workspaces: Workspaces,
    colors: Colors,
    wm_check_win: Window,
    sequences_to_ignore: heapless::BinaryHeap<u16, Min, BINARY_HEAP_LIMIT>,
    pointer_grabbed: bool,
    window_border_width: u32,
    window_padding: i16,
    mut cookie_container: heapless::Vec<VoidCookie, COOKIE_CONTAINER_CAPACITY>,
) -> Result<State> {
    let screen_dimensions = get_screen_dimensions(call_wrapper, &screen)?;

    let mut monitors = Vec::with_capacity(8);
    let mut max_bar_width = 0;
    for (i, dimensions) in screen_dimensions.into_iter().enumerate() {
        if dimensions.width > max_bar_width {
            max_bar_width = dimensions.width;
        }
        pgwm_utils::debug!("Monitor {} size = {:?}", i, dimensions);
        if i > USER_WORKSPACES.len() {
            pgwm_utils::debug!(
                "More monitors than workspaces, not using more than {}",
                i - 1
            );
            break;
        }

        let tab_bar_win = call_wrapper.generate_id()?;
        intern_created_windows.insert(tab_bar_win, ());
        push_heapless!(
            cookie_container,
            create_tab_bar_win(
                call_wrapper,
                &screen,
                tab_bar_win,
                dimensions,
                TAB_BAR_HEIGHT,
            )?
        )?;
        let bar_win = call_wrapper.generate_id()?;
        intern_created_windows.insert(bar_win, ());
        push_heapless!(
            cookie_container,
            create_workspace_bar_win(
                call_wrapper,
                &screen,
                bar_win,
                dimensions,
                STATUS_BAR_HEIGHT as u16
            )?
        )?;
        let bar_pixmap = call_wrapper.generate_id()?;
        push_heapless!(
            cookie_container,
            create_workspace_bar_pixmap(
                call_wrapper,
                &screen,
                bar_pixmap,
                dimensions,
                STATUS_BAR_HEIGHT as u16
            )?
        )?;
        if WM_SHOW_BAR_INITIALLY {
            map_window(
                &mut call_wrapper.uring,
                &mut call_wrapper.xcb_state,
                bar_win,
                true,
            )?;
        }

        let bar_win = init_xrender_double_buffered(call_wrapper, screen.root, bar_win, &vis_info)?;
        let tab_bar_win =
            init_xrender_double_buffered(call_wrapper, screen.root, tab_bar_win, &vis_info)?;
        let bar_geometry = create_bar_geometry(
            font_manager,
            dimensions.width,
            &USER_WORKSPACES,
            WORKSPACE_BAR_WINDOW_NAME_PADDING,
            WORKSPACE_BAR_WINDOW_NAME_PADDING,
            #[cfg(feature = "status-bar")]
            &pgwm_core::config::STATUS_CHECKS,
        );
        let new_mon = Monitor {
            bar_geometry,
            bar_win,
            tab_bar_win,
            dimensions,
            hosted_workspace: i,
            last_focus: None,
            show_bar: WM_SHOW_BAR_INITIALLY,
            window_title_display: heapless::String::from("pgwm"),
        };
        monitors.push(new_mon);
    }

    pgwm_utils::debug!("Initializing mouse");
    let mouse_mapping = init_mouse();
    pgwm_utils::debug!("Initializing keys");
    let key_mapping = init_keys(call_wrapper)?;
    grab_keys(call_wrapper, &key_mapping, screen.root)?;
    for bar_win in monitors.iter().map(|mon| &mon.bar_win) {
        pgwm_utils::debug!("Grabbing mouse keys on bar_win");
        grab_mouse(
            call_wrapper,
            bar_win.window.drawable,
            screen.root,
            &mouse_mapping,
        )?;
    }

    pgwm_utils::debug!("Creating status bar pixmap");
    #[cfg(feature = "status-bar")]
    let status_pixmap = call_wrapper.generate_id()?;

    #[cfg(feature = "status-bar")]
    push_heapless!(
        cookie_container,
        create_status_bar_pixmap(
            call_wrapper,
            &screen,
            status_pixmap,
            max_bar_width as u16,
            STATUS_BAR_HEIGHT as u16
        )?
    )?;

    for cookie in cookie_container {
        cookie.check(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?;
    }
    pgwm_utils::debug!("Created state");
    Ok(State {
        intern_created_windows,
        drag_window: None,
        focused_mon: 0,
        input_focus: None,
        screen: screen.clone(),
        dying_windows,
        wm_check_win,
        sequences_to_ignore,
        monitors,
        workspaces,
        colors,
        window_border_width,
        window_padding,
        pointer_grabbed,
        mouse_mapping,
        key_mapping,
        last_timestamp: CURRENT_TIME,
    })
}

fn create_static_state<'a>(
    call_wrapper: &'a mut CallWrapper,
    screen: &'a Screen,
    colors: &Colors,
    cookie_container: &mut heapless::Vec<VoidCookie, COOKIE_CONTAINER_CAPACITY>,
) -> Result<StaticState> {
    let mut intern_created_windows = Map::new();
    let gcs = create_gcs(call_wrapper, screen, colors)?;
    let tab_pixmap = call_wrapper.generate_id()?;
    push_heapless!(
        cookie_container,
        create_tab_pixmap(call_wrapper, screen, tab_pixmap, TAB_BAR_HEIGHT as u16)?
    )?;

    let sequences_to_ignore = heapless::BinaryHeap::new();
    let check_win = call_wrapper.generate_id()?;
    intern_created_windows.insert(check_win, ());
    push_heapless!(
        cookie_container,
        create_wm_check_win(call_wrapper, screen, check_win)?
    )?;
    for cookie in gcs {
        push_heapless!(cookie_container, cookie)?;
    }

    Ok(StaticState {
        wm_check_win: check_win,
        sequences_to_ignore,
        intern_created_windows,
    })
}

struct StaticState {
    wm_check_win: Window,
    sequences_to_ignore: heapless::BinaryHeap<u16, Min, BINARY_HEAP_LIMIT>,
    intern_created_windows: Map<Window, ()>,
}

fn create_tab_bar_win(
    call_wrapper: &mut CallWrapper,
    screen: &Screen,
    tab_bar_win: Window,
    dimensions: Dimensions,
    tab_bar_height: i16,
) -> Result<VoidCookie> {
    let create_win = CreateWindowValueList::default()
        .event_mask(EventMask::BUTTON_PRESS)
        .background_pixel(0);
    Ok(create_window(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        COPY_DEPTH_FROM_PARENT,
        tab_bar_win,
        screen.root,
        dimensions.x,
        dimensions.y,
        dimensions.width as u16,
        tab_bar_height as u16,
        0,
        WindowClassEnum::INPUT_OUTPUT,
        0,
        create_win,
        false,
    )?)
}

fn create_workspace_bar_win(
    call_wrapper: &mut CallWrapper,
    screen: &Screen,
    ws_bar_win: Window,
    dimensions: Dimensions,
    status_bar_height: u16,
) -> Result<VoidCookie> {
    let cw = CreateWindowValueList::default()
        .background_pixel(screen.black_pixel)
        .event_mask(
            EventMask::ENTER_WINDOW
                | EventMask::FOCUS_CHANGE
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::VISIBILITY_CHANGE
                | EventMask::LEAVE_WINDOW,
        );
    Ok(create_window(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        COPY_DEPTH_FROM_PARENT,
        ws_bar_win,
        screen.root,
        dimensions.x,
        dimensions.y,
        dimensions.width as u16,
        status_bar_height,
        0,
        WindowClassEnum::INPUT_OUTPUT,
        0,
        cw,
        false,
    )?)
}

fn create_workspace_bar_pixmap(
    call_wrapper: &mut CallWrapper,
    screen: &Screen,
    bar_pixmap: Pixmap,
    dimensions: Dimensions,
    status_bar_height: u16,
) -> Result<VoidCookie> {
    Ok(create_pixmap(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        screen.root_depth,
        bar_pixmap,
        screen.root,
        dimensions.width as u16,
        status_bar_height,
        false,
    )?)
}

fn create_wm_check_win<'a>(
    call_wrapper: &'a mut CallWrapper,
    screen: &'a Screen,
    check_win: Window,
) -> Result<VoidCookie> {
    let cw = CreateWindowValueList::default()
        .event_mask(EventMask::NO_EVENT)
        .background_pixel(0);
    Ok(create_window(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        COPY_DEPTH_FROM_PARENT,
        check_win,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClassEnum::INPUT_OUTPUT,
        0,
        cw,
        false,
    )?)
}

fn create_gcs<'a>(
    call_wrapper: &'a mut CallWrapper,
    screen: &'a Screen,
    colors: &Colors,
) -> Result<heapless::Vec<VoidCookie, 16>> {
    let colors_needing_gcs = [
        colors.tab_bar_focused_tab_background().pixel,
        colors.tab_bar_unfocused_tab_background().pixel,
        colors.workspace_bar_urgent_workspace_background().pixel,
        colors.workspace_bar_current_window_title_background().pixel,
        colors.workspace_bar_focused_workspace_background().pixel,
        colors.workspace_bar_unfocused_workspace_background().pixel,
        colors
            .workspace_bar_selected_unfocused_workspace_background()
            .pixel,
        colors.status_bar_background().pixel,
        colors.shortcut_background().pixel,
    ];
    let mut v = heapless::Vec::new();
    for pix in colors_needing_gcs {
        push_heapless!(v, create_background_gc(call_wrapper, screen.root, pix)?.1)?;
    }

    Ok(v)
}

fn create_background_gc(
    call_wrapper: &mut CallWrapper,
    win: Window,
    pixel: u32,
) -> Result<(Gcontext, VoidCookie)> {
    let gc = call_wrapper.generate_id()?;
    let gc_aux = CreateGCValueList::default()
        .graphics_exposures(0)
        .line_style(LineStyleEnum::SOLID)
        .cap_style(CapStyleEnum::BUTT)
        .join_style(JoinStyleEnum::MITER)
        .foreground(pixel)
        .background(pixel);

    let cookie = create_g_c(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        gc,
        win,
        gc_aux,
        false,
    )?;
    Ok((gc, cookie))
}

#[cfg(not(feature = "xinerama"))]
#[allow(clippy::unnecessary_wraps)]
fn get_screen_dimensions(
    _connection: &mut CallWrapper,
    screen: &Screen,
) -> Result<Vec<Dimensions>> {
    Ok(alloc::vec![Dimensions::new(
        screen.width_in_pixels as i16,
        screen.height_in_pixels as i16,
        0,
        0,
    )])
}

#[cfg(feature = "xinerama")]
fn get_screen_dimensions(
    call_wrapper: &mut CallWrapper,
    _screen: &Screen,
) -> Result<Vec<Dimensions>> {
    Ok(xcb_rust_protocol::connection::xinerama::query_screens(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        false,
    )?
    .reply(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?
    .screen_info
    .iter()
    .map(|screen_info| {
        Dimensions::new(
            screen_info.width as i16,
            screen_info.height as i16,
            screen_info.x_org,
            screen_info.y_org,
        )
    })
    .collect())
}

fn create_tab_pixmap<'a>(
    call_wrapper: &'a mut CallWrapper,
    screen: &'a Screen,
    pixmap: Pixmap,
    tab_bar_height: u16,
) -> Result<VoidCookie> {
    Ok(create_pixmap(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        screen.root_depth,
        pixmap,
        screen.root,
        screen.width_in_pixels,
        tab_bar_height,
        false,
    )?)
}

#[cfg(feature = "status-bar")]
fn create_status_bar_pixmap(
    call_wrapper: &mut CallWrapper,
    screen: &Screen,
    pixmap: Pixmap,
    max_bar_width: u16,
    status_bar_height: u16,
) -> Result<VoidCookie> {
    Ok(create_pixmap(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        screen.root_depth,
        pixmap,
        screen.root,
        max_bar_width,
        status_bar_height,
        false,
    )?)
}

fn create_bar_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    mon_width: i16,
    workspaces: &[UserWorkspace],
    workspace_bar_window_name_padding: u16,
    shortcut_padding: u16,
    #[cfg(feature = "status-bar")] checks: &[Check],
) -> BarGeometry {
    let workspace_section = create_workspace_section_geometry(
        font_manager,
        workspaces,
        workspace_bar_window_name_padding,
    );
    let shortcut_section = create_shortcut_geometry(font_manager, mon_width, shortcut_padding);
    #[cfg(feature = "status-bar")]
    let status_section = create_status_section_geometry(
        font_manager,
        mon_width,
        shortcut_section.position.length,
        checks,
    );

    BarGeometry::new(
        mon_width,
        workspace_section,
        shortcut_section,
        #[cfg(feature = "status-bar")]
        status_section,
    )
}

#[cfg(feature = "status-bar")]
fn create_status_section_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    mon_width: i16,
    shortcut_width: i16,
    checks: &[Check],
) -> StatusSection {
    use pgwm_core::config::STATUS_SECTION;
    let mut check_lengths: heapless::Vec<i16, { pgwm_core::config::STATUS_CHECKS.len() }> =
        heapless::Vec::new();
    for check in checks {
        let length = match &check.check_type {
            CheckType::Battery(bc) => bc
                .get_checks()
                .iter()
                .map(|bc| {
                    font_manager
                        .text_geometry(&bc.max_length_content(), STATUS_SECTION)
                        .0
                })
                .max()
                .unwrap_or(0),
            CheckType::Cpu(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), STATUS_SECTION)
                    .0
            }
            CheckType::Net(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), STATUS_SECTION)
                    .0
            }
            CheckType::Mem(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), STATUS_SECTION)
                    .0
            }
            CheckType::Date(fmt) => {
                font_manager
                    .text_geometry(&fmt.format_date(), STATUS_SECTION)
                    .0
            }
        };
        let _ = check_lengths.push(length);
    }
    let sep_len = font_manager
        .text_geometry(_STATUS_BAR_CHECK_SEP, STATUS_SECTION)
        .0;
    let first_sep = font_manager
        .text_geometry(_STATUS_BAR_FIRST_SEP, STATUS_SECTION)
        .0;
    StatusSection::new(
        mon_width,
        shortcut_width,
        &check_lengths,
        sep_len,
        first_sep,
    )
}

fn create_workspace_section_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    workspaces: &[UserWorkspace],
    workspace_bar_window_name_padding: u16,
) -> WorkspaceSection {
    let (components, position) = create_fixed_components(
        workspaces.iter().map(|s| s.name),
        0,
        workspace_bar_window_name_padding,
        font_manager,
        WORKSPACE_SECTION_FONTS,
    );
    WorkspaceSection {
        position,
        components,
    }
}

fn create_shortcut_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    mon_width: i16,
    shortcut_padding: u16,
) -> ShortcutSection {
    let (components, position) = create_fixed_components(
        BAR_SHORTCUTS.into_iter(),
        0,
        shortcut_padding,
        font_manager,
        WORKSPACE_SECTION_FONTS,
    );
    let position = Line::new(mon_width - position.length, position.length);
    let mut shifted_components = Vec::new();
    let component_offset = 0;
    for component in components {
        shifted_components.push(ShortcutComponent {
            position: Line::new(position.start + component_offset, component.position.length),
            write_offset: component.write_offset,
            text: component.text,
        });
    }
    ShortcutSection {
        position,
        components: shifted_components,
    }
}

fn create_fixed_components<It: Iterator<Item = &'static str>>(
    it: It,
    x: i16,
    padding: u16,
    font_manager: &FontDrawer,
    fonts: &[FontCfg],
) -> (Vec<FixedDisplayComponent>, Line) {
    let mut widths = Vec::new();
    // Equal spacing
    let mut max_width = 0;
    for (i, text) in it.enumerate() {
        widths.push((font_manager.text_geometry(text, fonts).0, text));
        if widths[i].0 > max_width {
            max_width = widths[i].0;
        }
    }
    let box_width = max_width as u16 + padding;
    let mut components = Vec::with_capacity(16);
    let mut component_offset = x;
    let num_widths = widths.len();
    for (width, text) in widths {
        let write_offset = (box_width - width as u16) as f32 / 2f32;
        components.push(FixedDisplayComponent {
            position: Line::new(component_offset, box_width as i16),
            write_offset: write_offset as i16,
            text,
        });
        component_offset += box_width as i16;
    }
    let total_width = num_widths * box_width as usize;
    (components, Line::new(x, total_width as i16))
}

fn init_keys(call_wrapper: &mut CallWrapper) -> Result<Map<KeyBoardMappingKey, Action>> {
    let setup = call_wrapper.xcb_state.setup();
    let lo = setup.min_keycode;
    let hi = setup.max_keycode;
    let capacity = hi - lo + 1;

    let mapping = get_keyboard_mapping(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        lo,
        capacity,
        false,
    )?
    .reply(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?;
    pgwm_utils::debug!("Got key mapping");
    let syms = mapping.keysyms;
    let mut map = Map::new();

    let mut converted: Vec<KeyboardMapping> = KEYBOARD_MAPPINGS.to_vec();
    for (keysym_ind, sym) in syms.iter().enumerate() {
        while let Some(keymap_ind) = converted.iter().position(|k| &k.keysym == sym) {
            let key_def = converted.swap_remove(keymap_ind);
            let mods = key_def.modmask.0;
            let modded_ind = keysym_ind + mods as usize;
            let code =
                (modded_ind - mods as usize) / mapping.keysyms_per_keycode as usize + lo as usize;
            let key = KeyBoardMappingKey::new(code as u8, mods);
            map.insert(key, key_def.action);
        }
    }
    Ok(map)
}

fn grab_keys(
    call_wrapper: &mut CallWrapper,
    key_map: &Map<KeyBoardMappingKey, Action>,
    root_win: Window,
) -> Result<()> {
    for key in key_map.keys() {
        grab_key(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            0,
            root_win,
            key.mods.into(),
            key.code.into(),
            GrabModeEnum::ASYNC,
            GrabModeEnum::ASYNC,
            false,
        )?
        .check(&mut call_wrapper.uring, &mut call_wrapper.xcb_state)?;
    }
    Ok(())
}

fn ungrab_keys(
    call_wrapper: &mut CallWrapper,
    key_map: &Map<KeyBoardMappingKey, Action>,
    root_win: Window,
) -> Result<()> {
    for key in key_map.keys() {
        ungrab_key(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            GrabEnum(key.code),
            root_win,
            key.mods.into(),
            true,
        )?;
    }
    Ok(())
}

fn init_mouse() -> Map<MouseActionKey, Action> {
    let mut action_map = Map::new();
    for mapping in MOUSE_MAPPINGS {
        action_map.insert(
            MouseActionKey {
                detail: mapping.button.0,
                state: mapping.mods.0,
                target: mapping.target,
            },
            mapping.action,
        );
    }
    action_map
}

fn grab_mouse(
    call_wrapper: &mut CallWrapper,
    bar_win: Window,
    root_win: Window,
    mouse_map: &Map<MouseActionKey, Action>,
) -> Result<()> {
    for key in mouse_map.keys() {
        grab_button(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            0,
            if key.target.on_bar() {
                bar_win
            } else {
                root_win
            },
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE,
            GrabModeEnum::ASYNC,
            GrabModeEnum::ASYNC,
            WindowEnum::NONE,
            CursorEnum::NONE,
            key.detail.into(),
            key.state.into(),
            true,
        )?;
    }
    Ok(())
}

fn ungrab_mouse(
    call_wrapper: &mut CallWrapper,
    bar_win: Window,
    root_win: Window,
    mouse_map: &Map<MouseActionKey, Action>,
) -> Result<()> {
    for key in mouse_map.keys() {
        ungrab_button(
            &mut call_wrapper.uring,
            &mut call_wrapper.xcb_state,
            key.detail.into(),
            if key.target.on_bar() {
                bar_win
            } else {
                root_win
            },
            key.state.into(),
            true,
        )?;
    }
    Ok(())
}

fn init_xrender_double_buffered(
    call_wrapper: &mut CallWrapper,
    root: Window,
    window: Window,
    vis_info: &RenderVisualInfo,
) -> Result<DoubleBufferedRenderPicture> {
    let direct = call_wrapper.window_mapped_picture(window, vis_info)?;
    let write_buf_pixmap = call_wrapper.generate_id()?;
    create_pixmap(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        vis_info.render.depth,
        write_buf_pixmap,
        root,
        1,
        1,
        true,
    )?;
    let write_buf_picture = call_wrapper.pixmap_mapped_picture(write_buf_pixmap, vis_info)?;
    call_wrapper.fill_xrender_rectangle(
        write_buf_picture,
        xcb_rust_protocol::proto::render::Color {
            red: 0xffff,
            green: 0xffff,
            blue: 0xffff,
            alpha: 0xffff,
        },
        Dimensions::new(1, 1, 0, 0),
    )?;
    free_pixmap(
        &mut call_wrapper.uring,
        &mut call_wrapper.xcb_state,
        write_buf_pixmap,
        true,
    )?;
    Ok(DoubleBufferedRenderPicture {
        window: RenderPicture {
            drawable: window,
            picture: direct,
            format: vis_info.root.pict_format,
        },
        pixmap: RenderPicture {
            drawable: write_buf_pixmap,
            picture: write_buf_picture,
            format: vis_info.render.pict_format,
        },
    })
}
