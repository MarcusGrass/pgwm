use crate::error::Result;
use heapless::binary_heap::Min;
use heapless::{FnvIndexMap, FnvIndexSet};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::cookie::VoidCookie;

use pgwm_core::colors::Colors;
use pgwm_core::config::{
    Action, FontCfg, Fonts, Shortcut, SimpleKeyMapping, SimpleMouseMapping, TilingModifiers,
    APPLICATION_WINDOW_LIMIT, BINARY_HEAP_LIMIT, DYING_WINDOW_CACHE,
};
#[cfg(feature = "status-bar")]
use pgwm_core::config::{STATUS_BAR_CHECK_SEP, STATUS_BAR_FIRST_SEP};
use pgwm_core::geometry::{Dimensions, Line};
use pgwm_core::state::workspace::Workspaces;
use pgwm_core::state::{Monitor, State, WinMarkedForDeath};

use pgwm_core::config::key_map::{KeyBoardMappingKey, KeyboardMapping};
use pgwm_core::config::mouse_map::MouseActionKey;
use pgwm_core::config::workspaces::UserWorkspace;
use pgwm_core::push_heapless;
use pgwm_core::render::{DoubleBufferedRenderPicture, RenderPicture, RenderVisualInfo};
#[cfg(feature = "status-bar")]
use pgwm_core::state::bar_geometry::StatusSection;
use pgwm_core::state::bar_geometry::{
    BarGeometry, FixedDisplayComponent, ShortcutComponent, ShortcutSection, WorkspaceSection,
};
#[cfg(feature = "status-bar")]
use pgwm_core::status::checker::{Check, CheckType};
use x11rb::protocol::xproto::{
    ButtonIndex, CapStyle, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext,
    GrabMode, JoinStyle, LineStyle, Pixmap, Screen, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::manager::font::{FontDrawer, LoadedFonts};
use crate::x11::call_wrapper::CallWrapper;

pub(crate) fn create_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontDrawer<'a>,
    visual: RenderVisualInfo,
    fonts: &'a Fonts,
    screen: &'a Screen,
    colors: Colors,
    cursor_name: String,
    status_bar_height: i16,
    tab_bar_height: i16,
    window_padding: i16,
    window_border_width: u32,
    workspace_bar_window_name_padding: u16,
    pad_while_tabbed: bool,
    destroy_after: u64, // Millis before force-close
    kill_after: u64,    // Millis before we kill the client
    show_bar_initially: bool,
    tiling_modifiers: TilingModifiers,
    init_workspaces: &[UserWorkspace],
    shortcuts: &[Shortcut],
    key_mappings: &[SimpleKeyMapping],
    mouse_mappings: &[SimpleMouseMapping],
    #[cfg(feature = "status-bar")] checks: &[Check],
) -> Result<State> {
    let mut cookie_container = heapless::Vec::new();
    let static_state = create_static_state(
        connection,
        screen,
        &colors,
        tab_bar_height as u16,
        &mut cookie_container,
    )?;
    do_create_state(
        connection,
        call_wrapper,
        font_manager,
        fonts,
        visual,
        screen.clone(),
        static_state.intern_created_windows,
        &heapless::CopyVec::new(),
        Workspaces::create_empty(init_workspaces, tiling_modifiers)?,
        colors,
        static_state.wm_check_win,
        static_state.sequences_to_ignore,
        cursor_name,
        false,
        status_bar_height,
        shortcuts,
        tab_bar_height,
        window_padding,
        window_border_width,
        workspace_bar_window_name_padding,
        pad_while_tabbed,
        destroy_after, // Millis before force-close
        kill_after,    // Millis before we kill the client
        show_bar_initially,
        init_workspaces,
        key_mappings,
        mouse_mappings,
        #[cfg(feature = "status-bar")]
        checks,
        cookie_container,
    )
}

pub(crate) fn reinit_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
    visual: RenderVisualInfo,
    state: State,
    init_workspaces: &[UserWorkspace],
    shortcuts: &[Shortcut],
    key_mappings: &[SimpleKeyMapping],
    mouse_mappings: &[SimpleMouseMapping],
    #[cfg(feature = "status-bar")] checks: &[Check],
) -> Result<State> {
    let cookie_container = heapless::Vec::new();
    do_create_state(
        connection,
        call_wrapper,
        font_manager,
        fonts,
        visual,
        state.screen.clone(),
        state.intern_created_windows,
        &state.dying_windows,
        state.workspaces,
        state.colors,
        state.wm_check_win,
        state.sequences_to_ignore,
        state.cursor_name,
        state.pointer_grabbed,
        state.status_bar_height,
        shortcuts,
        state.tab_bar_height,
        state.window_padding,
        state.window_border_width,
        state.workspace_bar_window_name_padding,
        state.pad_while_tabbed,
        state.destroy_after,
        state.kill_after,
        state.show_bar_initially,
        init_workspaces,
        key_mappings,
        mouse_mappings,
        #[cfg(feature = "status-bar")]
        checks,
        cookie_container,
    )
}

pub(crate) fn teardown_dynamic_state(connection: &RustConnection, state: &State) -> Result<()> {
    for mon in &state.monitors {
        connection.destroy_window(mon.bar_win.window.drawable)?;
        x11rb::protocol::render::free_picture(connection, mon.bar_win.window.picture)?;
        connection.destroy_window(mon.tab_bar_win.window.drawable)?;
        x11rb::protocol::render::free_picture(connection, mon.tab_bar_win.window.picture)?;
    }
    Ok(())
}

pub(crate) fn teardown_full_state(
    connection: &RustConnection,
    state: &State,
    loaded_fonts: &LoadedFonts,
) -> Result<()> {
    let _ = teardown_dynamic_state(connection, state);
    connection.destroy_window(state.wm_check_win)?;
    for font in loaded_fonts.fonts.values() {
        x11rb::protocol::render::free_glyph_set(connection, font.glyph_set)?;
    }
    Ok(())
}

#[allow(clippy::fn_params_excessive_bools)]
#[allow(clippy::too_many_lines)]
fn do_create_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
    vis_info: RenderVisualInfo,
    screen: Screen,
    mut intern_created_windows: heapless::FnvIndexSet<Window, APPLICATION_WINDOW_LIMIT>,
    dying_windows: &heapless::CopyVec<WinMarkedForDeath, DYING_WINDOW_CACHE>,
    workspaces: Workspaces,
    colors: Colors,
    wm_check_win: Window,
    sequences_to_ignore: heapless::BinaryHeap<u16, Min, BINARY_HEAP_LIMIT>,
    cursor_name: String,
    pointer_grabbed: bool,
    status_bar_height: i16,
    shortcuts: &[Shortcut],
    tab_bar_height: i16,
    window_padding: i16,
    window_border_width: u32,
    workspace_bar_window_name_padding: u16,
    pad_while_tabbed: bool,
    destroy_after: u64, // Millis before force-close
    kill_after: u64,    // Millis before we kill the client
    show_bar_initially: bool,
    init_workspaces: &[UserWorkspace],
    key_mappings: &[SimpleKeyMapping],
    mouse_mappings: &[SimpleMouseMapping],
    #[cfg(feature = "status-bar")] checks: &[Check],
    mut cookie_container: heapless::Vec<VoidCookie<'a, RustConnection>, 32>,
) -> Result<State> {
    let screen_dimensions = get_screen_dimensions(connection, &screen)?;
    let mut monitors = Vec::with_capacity(8);
    let mut max_bar_width = 0;
    for (i, dimensions) in screen_dimensions.into_iter().enumerate() {
        if dimensions.width > max_bar_width {
            max_bar_width = dimensions.width;
        }
        pgwm_core::debug!("Monitor {} size = {:?}", i, dimensions);
        if i > init_workspaces.len() {
            pgwm_core::debug!(
                "More monitors than workspaces, not using more than {}",
                i - 1
            );
            break;
        }

        let tab_bar_win = connection.generate_id()?;
        intern_created_windows.insert(tab_bar_win).unwrap();
        push_heapless!(
            cookie_container,
            create_tab_bar_win(connection, &screen, tab_bar_win, dimensions, tab_bar_height)?
        )?;
        let bar_win = connection.generate_id()?;
        intern_created_windows.insert(bar_win).unwrap();
        push_heapless!(
            cookie_container,
            create_workspace_bar_win(
                connection,
                &screen,
                bar_win,
                dimensions,
                status_bar_height as u16
            )?
        )?;
        let bar_pixmap = connection.generate_id()?;
        push_heapless!(
            cookie_container,
            create_workspace_bar_pixmap(
                connection,
                &screen,
                bar_pixmap,
                dimensions,
                status_bar_height as u16
            )?
        )?;
        if show_bar_initially {
            connection.map_window(bar_win)?;
        }

        let bar_win = init_xrender_double_buffered(
            connection,
            call_wrapper,
            screen.root,
            bar_win,
            &vis_info,
        )?;
        let tab_bar_win = init_xrender_double_buffered(
            connection,
            call_wrapper,
            screen.root,
            tab_bar_win,
            &vis_info,
        )?;
        let bar_geometry = create_bar_geometry(
            font_manager,
            fonts,
            dimensions.width,
            init_workspaces,
            workspace_bar_window_name_padding,
            shortcuts,
            workspace_bar_window_name_padding,
            #[cfg(feature = "status-bar")]
            checks,
        );
        let new_mon = Monitor {
            bar_geometry,
            bar_win,
            tab_bar_win,
            dimensions,
            hosted_workspace: i,
            last_focus: None,
            show_bar: show_bar_initially,
            window_title_display: heapless::String::from("pgwm"),
        };
        monitors.push(new_mon);
    }
    let mouse_mapping = init_mouse(mouse_mappings);
    let key_mapping = init_keys(connection, key_mappings)?;
    grab_keys(connection, &key_mapping, screen.root)?;
    for bar_win in monitors.iter().map(|mon| &mon.bar_win) {
        grab_mouse(
            connection,
            bar_win.window.drawable,
            screen.root,
            &mouse_mapping,
        )?;
    }

    #[cfg(feature = "status-bar")]
    let status_pixmap = connection.generate_id()?;

    #[cfg(feature = "status-bar")]
    push_heapless!(
        cookie_container,
        create_status_bar_pixmap(
            connection,
            &screen,
            status_pixmap,
            max_bar_width as u16,
            status_bar_height as u16
        )?
    )?;

    for cookie in cookie_container {
        cookie.check()?;
    }
    pgwm_core::debug!("Created state");
    Ok(State {
        intern_created_windows,
        drag_window: None,
        focused_mon: 0,
        input_focus: None,
        screen: screen.clone(),
        dying_windows: *dying_windows,
        wm_check_win,
        sequences_to_ignore,
        monitors,
        workspaces,
        window_border_width,
        colors,
        pointer_grabbed,
        status_bar_height,
        tab_bar_height,
        window_padding,
        pad_while_tabbed,
        workspace_bar_window_name_padding,
        cursor_name,
        destroy_after,
        kill_after,
        show_bar_initially,
        mouse_mapping,
        key_mapping,
    })
}

fn create_static_state<'a>(
    connection: &'a RustConnection,
    screen: &'a Screen,
    colors: &Colors,
    tab_bar_height: u16,
    cookie_container: &mut heapless::Vec<VoidCookie<'a, RustConnection>, 32>,
) -> Result<StaticState> {
    let mut intern_created_windows = FnvIndexSet::new();
    let mut gcs = create_gcs(connection, screen, colors)?;
    let tab_pixmap = connection.generate_id()?;
    push_heapless!(
        cookie_container,
        create_tab_pixmap(connection, screen, tab_pixmap, tab_bar_height)?
    )?;

    let sequences_to_ignore = heapless::BinaryHeap::new();
    let check_win = connection.generate_id()?;
    intern_created_windows.insert(check_win).unwrap();
    push_heapless!(
        cookie_container,
        create_wm_check_win(connection, screen, check_win)?
    )?;
    let keys = gcs.keys().copied().collect::<heapless::CopyVec<u32, 8>>();
    for key in keys {
        let (_, cookie) = gcs.remove(&key).unwrap();
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
    intern_created_windows: heapless::FnvIndexSet<Window, APPLICATION_WINDOW_LIMIT>,
}

fn create_tab_bar_win<'a>(
    connection: &'a RustConnection,
    screen: &Screen,
    tab_bar_win: Window,
    dimensions: Dimensions,
    tab_bar_height: i16,
) -> Result<VoidCookie<'a, RustConnection>> {
    let create_win = CreateWindowAux::new()
        .event_mask(EventMask::BUTTON_PRESS)
        .background_pixel(0);
    Ok(connection.create_window(
        COPY_DEPTH_FROM_PARENT,
        tab_bar_win,
        screen.root,
        dimensions.x,
        dimensions.y,
        dimensions.width as u16,
        tab_bar_height as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &create_win,
    )?)
}

fn create_workspace_bar_win<'a>(
    connection: &'a RustConnection,
    screen: &Screen,
    ws_bar_win: Window,
    dimensions: Dimensions,
    status_bar_height: u16,
) -> Result<VoidCookie<'a, RustConnection>> {
    let cw = CreateWindowAux::new()
        .background_pixel(screen.black_pixel)
        .event_mask(
            EventMask::ENTER_WINDOW
                | EventMask::FOCUS_CHANGE
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::VISIBILITY_CHANGE
                | EventMask::LEAVE_WINDOW,
        );
    Ok(connection.create_window(
        COPY_DEPTH_FROM_PARENT,
        ws_bar_win,
        screen.root,
        dimensions.x,
        dimensions.y,
        dimensions.width as u16,
        status_bar_height,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &cw,
    )?)
}

fn create_workspace_bar_pixmap<'a>(
    connection: &'a RustConnection,
    screen: &Screen,
    bar_pixmap: Pixmap,
    dimensions: Dimensions,
    status_bar_height: u16,
) -> Result<VoidCookie<'a, RustConnection>> {
    Ok(connection.create_pixmap(
        screen.root_depth,
        bar_pixmap,
        screen.root,
        dimensions.width as u16,
        status_bar_height,
    )?)
}

fn create_wm_check_win<'a>(
    connection: &'a RustConnection,
    screen: &'a Screen,
    check_win: Window,
) -> Result<VoidCookie<'a, RustConnection>> {
    let cw = CreateWindowAux::new()
        .event_mask(EventMask::NO_EVENT)
        .background_pixel(0);
    Ok(connection.create_window(
        COPY_DEPTH_FROM_PARENT,
        check_win,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &cw,
    )?)
}

fn create_gcs<'a>(
    connection: &'a RustConnection,
    screen: &'a Screen,
    colors: &Colors,
) -> Result<FnvIndexMap<u32, (Gcontext, VoidCookie<'a, RustConnection>), 8>> {
    let mut map = FnvIndexMap::new();
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
    for pix in colors_needing_gcs {
        let res = create_background_gc(connection, screen.root, pix)?;
        map.insert(pix, res).expect("Undersize gc map");
    }

    Ok(map)
}
fn create_background_gc(
    connection: &RustConnection,
    win: Window,
    pixel: u32,
) -> Result<(Gcontext, VoidCookie<RustConnection>)> {
    let gc = connection.generate_id()?;
    let gc_aux = CreateGCAux::new()
        .graphics_exposures(0)
        .line_style(LineStyle::SOLID)
        .cap_style(CapStyle::BUTT)
        .join_style(JoinStyle::MITER)
        .foreground(pixel)
        .background(pixel);
    let cookie = connection.create_gc(gc, win, &gc_aux)?;
    Ok((gc, cookie))
}

#[cfg(not(feature = "xinerama"))]
#[allow(clippy::unnecessary_wraps)]
fn get_screen_dimensions<'a>(
    _connection: &'a RustConnection,
    screen: &'a Screen,
) -> Result<Vec<Dimensions>> {
    Ok(vec![Dimensions::new(
        screen.width_in_pixels as i16,
        screen.height_in_pixels as i16,
        0,
        0,
    )])
}

#[cfg(feature = "xinerama")]
fn get_screen_dimensions<'a>(
    connection: &'a RustConnection,
    _screen: &'a Screen,
) -> Result<Vec<Dimensions>> {
    Ok(x11rb::protocol::xinerama::query_screens(connection)?
        .reply()?
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
    connection: &'a RustConnection,
    screen: &'a Screen,
    pixmap: Pixmap,
    tab_bar_height: u16,
) -> Result<VoidCookie<'a, RustConnection>> {
    Ok(connection.create_pixmap(
        screen.root_depth,
        pixmap,
        screen.root,
        screen.width_in_pixels,
        tab_bar_height,
    )?)
}
#[cfg(feature = "status-bar")]
fn create_status_bar_pixmap<'a>(
    connection: &'a RustConnection,
    screen: &Screen,
    pixmap: Pixmap,
    max_bar_width: u16,
    status_bar_height: u16,
) -> Result<VoidCookie<'a, RustConnection>> {
    Ok(connection.create_pixmap(
        screen.root_depth,
        pixmap,
        screen.root,
        max_bar_width,
        status_bar_height,
    )?)
}

fn create_bar_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
    mon_width: i16,
    workspaces: &[UserWorkspace],
    workspace_bar_window_name_padding: u16,
    shortcuts: &[Shortcut],
    shortcut_padding: u16,
    #[cfg(feature = "status-bar")] checks: &[Check],
) -> BarGeometry {
    let workspace_section = create_workspace_section_geometry(
        font_manager,
        fonts,
        workspaces,
        workspace_bar_window_name_padding,
    );
    let shortcut_section =
        create_shortcut_geometry(font_manager, fonts, mon_width, shortcuts, shortcut_padding);
    #[cfg(feature = "status-bar")]
    let status_section = create_status_section_geometry(
        font_manager,
        fonts,
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
    fonts: &'a Fonts,
    mon_width: i16,
    shortcut_width: i16,
    checks: &[Check],
) -> StatusSection {
    let mut check_lengths: heapless::CopyVec<
        i16,
        { pgwm_core::config::STATUS_BAR_UNIQUE_CHECK_LIMIT },
    > = heapless::CopyVec::new();
    for check in checks {
        let length = match check.check_type {
            CheckType::Battery(bc) => bc
                .iter()
                .map(|bc| {
                    font_manager
                        .text_geometry(&bc.max_length_content(), &fonts.status_section)
                        .0
                })
                .max()
                .unwrap_or(0),
            CheckType::Cpu(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), &fonts.status_section)
                    .0
            }
            CheckType::Net(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), &fonts.status_section)
                    .0
            }
            CheckType::Mem(fmt) => {
                font_manager
                    .text_geometry(&fmt.max_length_content(), &fonts.status_section)
                    .0
            }
            CheckType::Date(fmt) => {
                let tokens = time::format_description::parse(&fmt.pattern).unwrap_or_default();
                font_manager
                    .text_geometry(&fmt.format_date(&tokens), &fonts.status_section)
                    .0
            }
        };
        let _ = check_lengths.push(length);
    }
    let sep_len = font_manager
        .text_geometry(STATUS_BAR_CHECK_SEP, &fonts.status_section)
        .0;
    let first_sep = font_manager
        .text_geometry(STATUS_BAR_FIRST_SEP, &fonts.status_section)
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
    fonts: &'a Fonts,
    workspaces: &[UserWorkspace],
    workspace_bar_window_name_padding: u16,
) -> WorkspaceSection {
    let (components, position) = create_fixed_components(
        workspaces.iter().map(|s| s.name.clone()),
        0,
        workspace_bar_window_name_padding,
        font_manager,
        &fonts.workspace_section,
    );
    WorkspaceSection {
        position,
        components,
    }
}

fn create_shortcut_geometry<'a>(
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
    mon_width: i16,
    shortcuts: &[Shortcut],
    shortcut_padding: u16,
) -> ShortcutSection {
    let (components, position) = create_fixed_components(
        shortcuts.iter().map(|s| s.name.clone()),
        0,
        shortcut_padding,
        font_manager,
        &fonts.workspace_section,
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

fn create_fixed_components<It: Iterator<Item = String>>(
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
        widths.push((
            font_manager.text_geometry(text.as_str(), fonts).0,
            text.clone(),
        ));
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

fn init_keys(
    connection: &RustConnection,
    simple_key_mappings: &[SimpleKeyMapping],
) -> Result<HashMap<KeyBoardMappingKey, Action>> {
    let setup = connection.setup();
    let lo = setup.min_keycode;
    let hi = setup.max_keycode;
    let capacity = hi - lo + 1;
    let mapping = connection.get_keyboard_mapping(lo, capacity)?.reply()?;
    let syms = mapping.keysyms;
    let mut map = HashMap::new();

    let mut converted: Vec<KeyboardMapping> = Vec::new();
    for simple_key_mapping in simple_key_mappings {
        converted.push(simple_key_mapping.clone().to_key_mapping());
    }
    for (keysym_ind, sym) in syms.iter().enumerate() {
        while let Some(keymap_ind) = converted.iter().position(|k| &k.keysym == sym) {
            let key_def = converted.swap_remove(keymap_ind);
            let mods = u16::from(key_def.modmask);
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
    connection: &RustConnection,
    key_map: &HashMap<KeyBoardMappingKey, Action>,
    root_win: Window,
) -> Result<()> {
    for key in key_map.keys() {
        connection
            .grab_key(
                true,
                root_win,
                key.mods,
                key.code,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .check()?;
    }
    Ok(())
}

fn init_mouse(simple_mouse_mappings: &[SimpleMouseMapping]) -> HashMap<MouseActionKey, Action> {
    let mut action_map = HashMap::new();
    for mapping in simple_mouse_mappings
        .iter()
        .map(|smm| smm.clone().to_mouse_mapping())
    {
        action_map.insert(
            MouseActionKey {
                detail: u8::from(mapping.button),
                state: u16::from(mapping.mods),
                target: mapping.target,
            },
            mapping.action,
        );
    }
    action_map
}

fn grab_mouse(
    connection: &RustConnection,
    bar_win: Window,
    root_win: Window,
    mouse_map: &HashMap<MouseActionKey, Action>,
) -> Result<()> {
    for key in mouse_map.keys() {
        connection.grab_button(
            true,
            key.target.on_bar().then(|| bar_win).unwrap_or(root_win),
            u32::from(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE) as u16,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            0u16,
            0u16,
            ButtonIndex::from(key.detail),
            key.state,
        )?;
    }
    Ok(())
}

fn init_xrender_double_buffered(
    connection: &RustConnection,
    call_wrapper: &CallWrapper,
    root: Window,
    window: Window,
    vis_info: &RenderVisualInfo,
) -> Result<DoubleBufferedRenderPicture> {
    let direct = call_wrapper.window_mapped_picture(window, vis_info)?;
    let write_buf_pixmap = connection.generate_id()?;
    connection.create_pixmap(vis_info.render.depth, write_buf_pixmap, root, 1, 1)?;
    let write_buf_picture = call_wrapper.pixmap_mapped_picture(write_buf_pixmap, vis_info)?;
    call_wrapper.fill_xrender_rectangle(
        write_buf_picture,
        x11rb::protocol::render::Color {
            red: 0xffff,
            green: 0xffff,
            blue: 0xffff,
            alpha: 0xffff,
        },
        Dimensions::new(1, 1, 0, 0),
    )?;
    connection.free_pixmap(write_buf_pixmap)?;
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
