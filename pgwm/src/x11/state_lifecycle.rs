use crate::error::Result;
use heapless::binary_heap::Min;
use heapless::{FnvIndexMap, FnvIndexSet};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::cookie::VoidCookie;

use pgwm_core::colors::Colors;
use pgwm_core::config::{
    Action, Fonts, Shortcut, SimpleKeyMapping, SimpleMouseMapping, TilingModifiers,
    APPLICATION_WINDOW_LIMIT, BINARY_HEAP_LIMIT, DYING_WINDOW_CACHE,
};
#[cfg(feature = "status-bar")]
use pgwm_core::config::{STATUS_BAR_CHECK_SEP, STATUS_BAR_FIRST_SEP};
use pgwm_core::geometry::{Dimensions, Line};
use pgwm_core::state::workspace::Workspaces;
use pgwm_core::state::{Monitor, PermanentDrawables, State, WinMarkedForDeath};

use pgwm_core::config::key_map::{KeyBoardMappingKey, KeyboardMapping};
use pgwm_core::config::mouse_map::MouseActionKey;
use pgwm_core::config::workspaces::UserWorkspace;
use pgwm_core::push_heapless;
#[cfg(feature = "status-bar")]
use pgwm_core::state::bar_geometry::StatusSection;
use pgwm_core::state::bar_geometry::{
    BarGeometry, FixedDisplayComponent, ShortcutComponent, ShortcutSection, WorkspaceSection,
};
use x11rb::protocol::xproto::{
    ButtonIndex, CapStyle, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask, Gcontext,
    GrabMode, JoinStyle, LineStyle, Pixmap, Screen, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::manager::font::FontManager;
use crate::x11::call_wrapper::CallWrapper;

pub(crate) fn create_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontManager<'a>,
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
    #[cfg(feature = "status-bar")] num_checks: usize,
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
        screen.clone(),
        static_state.intern_created_windows,
        &heapless::CopyVec::new(),
        Workspaces::create_empty(init_workspaces, tiling_modifiers)?,
        colors,
        static_state.permanent_drawables,
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
        num_checks,
        cookie_container,
    )
}

pub(crate) fn reinit_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontManager<'a>,
    fonts: &'a Fonts,
    state: State,
    init_workspaces: &[UserWorkspace],
    shortcuts: &[Shortcut],
    key_mappings: &[SimpleKeyMapping],
    mouse_mappings: &[SimpleMouseMapping],
) -> Result<State> {
    let cookie_container = heapless::Vec::new();
    do_create_state(
        connection,
        call_wrapper,
        font_manager,
        fonts,
        state.screen.clone(),
        state.intern_created_windows,
        &state.dying_windows,
        state.workspaces,
        state.colors,
        state.permanent_drawables,
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
        state.bar_geometry.status.components.len(),
        cookie_container,
    )
}

pub(crate) fn teardown_dynamic_state(connection: &RustConnection, state: &State) -> Result<()> {
    for mon in &state.monitors {
        connection.destroy_window(mon.tab_bar_win)?;
        connection.destroy_window(mon.bar_win)?;
        connection.free_pixmap(mon.bar_pixmap)?;
    }
    #[cfg(feature = "status-bar")]
    connection.free_pixmap(state.status_pixmap)?;
    Ok(())
}

pub(crate) fn teardown_full_state(connection: &RustConnection, state: &State) -> Result<()> {
    let _ = teardown_dynamic_state(connection, state);
    connection.destroy_window(state.permanent_drawables.wm_check_win)?;
    connection.free_pixmap(state.permanent_drawables.tab_bar_pixmap)?;
    connection.free_gc(state.permanent_drawables.shortcut_gc)?;
    connection.free_gc(state.permanent_drawables.status_bar_gc)?;
    connection.free_gc(state.permanent_drawables.tab_bar_text_gc)?;
    connection.free_gc(state.permanent_drawables.tab_bar_selected_gc)?;
    connection.free_gc(state.permanent_drawables.tab_bar_deselected_gc)?;
    connection.free_gc(state.permanent_drawables.urgent_workspace_gc)?;
    connection.free_gc(state.permanent_drawables.focused_workspace_gc)?;
    connection.free_gc(state.permanent_drawables.current_workspace_gc)?;
    connection.free_gc(state.permanent_drawables.selected_unfocused_workspace_gc)?;
    connection.free_gc(state.permanent_drawables.unfocused_workspace_gc)?;
    Ok(())
}

#[allow(clippy::fn_params_excessive_bools)]
#[allow(clippy::too_many_lines)]
fn do_create_state<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontManager<'a>,
    fonts: &'a Fonts,
    screen: Screen,
    mut intern_created_windows: heapless::FnvIndexSet<Window, APPLICATION_WINDOW_LIMIT>,
    dying_windows: &heapless::CopyVec<WinMarkedForDeath, DYING_WINDOW_CACHE>,
    workspaces: Workspaces,
    colors: Colors,
    permanent_drawables: PermanentDrawables,
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
    #[cfg(feature = "status-bar")] num_checks: usize,
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
            create_tab_bar_win(connection, &screen, tab_bar_win, dimensions)?
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
        init_ws_bar_drawable(
            connection,
            call_wrapper,
            dimensions,
            bar_win,
            permanent_drawables.unfocused_workspace_gc,
            status_bar_height,
            show_bar_initially,
        )?;

        let new_mon = Monitor {
            bar_win,
            bar_pixmap,
            tab_bar_win,
            dimensions,
            hosted_workspace: i,
            last_focus: None,
            show_bar: show_bar_initially,
            window_title_display: heapless::String::from("pgwm"),
        };
        monitors.push(new_mon);
    }
    let workspace_section = create_bar_geometry(
        font_manager,
        fonts,
        init_workspaces,
        workspace_bar_window_name_padding,
    )?;

    let shortcuts = create_shortcut_geometry(
        font_manager,
        fonts,
        workspace_section.position.length,
        shortcuts,
        workspace_bar_window_name_padding,
    )?;
    let mouse_mapping = init_mouse(mouse_mappings);
    let key_mapping = init_keys(connection, key_mappings)?;
    grab_keys(connection, &key_mapping, screen.root)?;
    for bar_win in monitors.iter().map(|mon| mon.bar_win) {
        init_shortcut_section(
            call_wrapper,
            workspace_section.position.start + workspace_section.position.length,
            bar_win,
            permanent_drawables.unfocused_workspace_gc,
            status_bar_height,
            &shortcuts,
        )?;
        grab_mouse(connection, bar_win, screen.root, &mouse_mapping)?;
    }
    #[cfg(feature = "status-bar")]
    let bar_geometry = {
        let sep_len = font_manager
            .get_width_and_height(STATUS_BAR_CHECK_SEP, &fonts.status_section)?
            .0;
        let first_sep = font_manager
            .get_width_and_height(STATUS_BAR_FIRST_SEP, &fonts.status_section)?
            .0;
        BarGeometry::new(
            workspace_section,
            shortcuts,
            StatusSection::new(num_checks, sep_len as i16, first_sep as i16),
        )
    };
    #[cfg(not(feature = "status-bar"))]
    let bar_geometry = BarGeometry::new(workspace_section, shortcuts);

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
        permanent_drawables,
        sequences_to_ignore,
        monitors,
        workspaces,
        #[cfg(feature = "status-bar")]
        status_pixmap,
        window_border_width,
        colors,
        pointer_grabbed,
        status_bar_height,
        tab_bar_height,
        window_padding,
        pad_while_tabbed,
        workspace_bar_window_name_padding,
        cursor_name,
        bar_geometry,
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
    let (text_gc, cookie) = create_background_gc(connection, screen.root, screen.black_pixel)?;
    push_heapless!(cookie_container, cookie)?;
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
    let permanent_drawables = PermanentDrawables {
        tab_bar_pixmap: tab_pixmap,
        tab_bar_selected_gc: gcs[&colors.tab_bar_focused_tab_background().pixel].0,
        tab_bar_deselected_gc: gcs[&colors.tab_bar_unfocused_tab_background().pixel].0,
        tab_bar_text_gc: text_gc,
        wm_check_win: check_win,
        unfocused_workspace_gc: gcs[&colors.workspace_bar_unfocused_workspace_background().pixel].0,
        selected_unfocused_workspace_gc: gcs[&colors
            .workspace_bar_selected_unfocused_workspace_background()
            .pixel]
            .0,
        focused_workspace_gc: gcs[&colors.workspace_bar_focused_workspace_background().pixel].0,
        current_workspace_gc: gcs[&colors.workspace_bar_current_window_title_background().pixel].0,
        urgent_workspace_gc: gcs[&colors.workspace_bar_urgent_workspace_background().pixel].0,
        status_bar_gc: gcs[&colors.status_bar_background().pixel].0,
        shortcut_gc: gcs[&colors.shortcut_background().pixel].0,
    };
    let keys = gcs.keys().copied().collect::<heapless::CopyVec<u32, 8>>();
    for key in keys {
        let (_, cookie) = gcs.remove(&key).unwrap();
        push_heapless!(cookie_container, cookie)?;
    }

    Ok(StaticState {
        permanent_drawables,
        sequences_to_ignore,
        intern_created_windows,
    })
}

struct StaticState {
    permanent_drawables: PermanentDrawables,
    sequences_to_ignore: heapless::BinaryHeap<u16, Min, BINARY_HEAP_LIMIT>,
    intern_created_windows: heapless::FnvIndexSet<Window, APPLICATION_WINDOW_LIMIT>,
}

fn create_tab_bar_win<'a>(
    connection: &'a RustConnection,
    screen: &Screen,
    tab_bar_win: Window,
    dimensions: Dimensions,
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
        dimensions.height as u16,
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
    font_manager: &'a FontManager<'a>,
    fonts: &'a Fonts,
    workspaces: &[UserWorkspace],
    workspace_bar_window_name_padding: u16,
) -> Result<WorkspaceSection> {
    let (components, position) = create_fixed_components(
        workspaces.iter().map(|s| s.name.clone()),
        0,
        workspace_bar_window_name_padding,
        font_manager,
        &fonts.workspace_section,
    )?;
    Ok(WorkspaceSection {
        position,
        components,
    })
}
fn create_shortcut_geometry<'a>(
    font_manager: &'a FontManager<'a>,
    fonts: &'a Fonts,
    offset: i16,
    shortcuts: &[Shortcut],
    shortcut_padding: u16,
) -> Result<ShortcutSection> {
    let (components, position) = create_fixed_components(
        shortcuts.iter().map(|s| s.name.clone()),
        offset,
        shortcut_padding,
        font_manager,
        &fonts.workspace_section,
    )?;
    Ok(ShortcutSection {
        width: position.length,
        components: components
            .into_iter()
            .map(|cmp| ShortcutComponent {
                width: cmp.position.length,
                write_offset: cmp.write_offset,
                text: cmp.text,
            })
            .collect(),
    })
}

fn create_fixed_components<It: Iterator<Item = String>>(
    it: It,
    x: i16,
    padding: u16,
    font_manager: &FontManager,
    fonts: &[String],
) -> Result<(Vec<FixedDisplayComponent>, Line)> {
    let mut widths = Vec::new();
    // Equal spacing
    let mut max_width = 0;
    for (i, text) in it.enumerate() {
        widths.push((
            font_manager.get_width_and_height(text.as_str(), fonts)?.0,
            text.clone(),
        ));
        if widths[i].0 > max_width {
            max_width = widths[i].0;
        }
    }
    let box_width = max_width + padding;
    let mut components = Vec::with_capacity(16);
    let mut component_offset = x;
    let num_widths = widths.len();
    for (width, text) in widths {
        let write_offset = (box_width - width) as f32 / 2f32;
        components.push(FixedDisplayComponent {
            position: Line::new(component_offset, box_width as i16),
            write_offset: write_offset as i16,
            text,
        });
        component_offset += box_width as i16;
    }
    let total_width = num_widths * box_width as usize;
    Ok((components, Line::new(x, total_width as i16)))
}

fn init_ws_bar_drawable<'a>(
    connection: &'a RustConnection,
    call_wrapper: &'a CallWrapper<'a>,
    monitor_dimensions: Dimensions,
    bar_win: Window,
    deselected_workspace_gc: Gcontext,
    status_bar_height: i16,
    show_initially: bool,
) -> Result<()> {
    call_wrapper.fill_rectangle(
        bar_win,
        deselected_workspace_gc,
        Dimensions {
            x: 0,
            y: 0,
            width: monitor_dimensions.width,
            height: status_bar_height,
        },
    )?;
    if show_initially {
        connection.map_window(bar_win)?;
    }
    Ok(())
}

fn init_shortcut_section<'a>(
    call_wrapper: &'a CallWrapper<'a>,
    offset: i16,
    bar_win: Window,
    shortcut_bg: Gcontext,
    status_bar_height: i16,
    shortcuts: &ShortcutSection,
) -> Result<()> {
    call_wrapper.fill_rectangle(
        bar_win,
        shortcut_bg,
        Dimensions {
            x: offset,
            y: 0,
            width: shortcuts.width,
            height: status_bar_height,
        },
    )?;
    Ok(())
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
