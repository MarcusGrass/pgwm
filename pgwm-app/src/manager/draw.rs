use xcb_rust_protocol::proto::xproto::Window;

use pgwm_core::config::{Fonts, WM_NAME_LIMIT, WS_WINDOW_LIMIT};
use pgwm_core::geometry::draw::{Mode, OldDrawMode};
use pgwm_core::geometry::{layout::Layout, Dimensions};
use pgwm_core::push_heapless;
use pgwm_core::state::workspace::{ArrangeKind, ManagedWindow};
use pgwm_core::state::State;

use crate::error::{Error, Result};
use crate::manager::font::FontDrawer;
use crate::x11::call_wrapper::CallWrapper;

pub(crate) struct Drawer<'a> {
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
}

impl<'a> Drawer<'a> {
    pub(crate) fn send_floating_to_top(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        floating: heapless::Vec<Window, WS_WINDOW_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        for win in floating {
            call_wrapper.push_window_to_top(xcb_out_buf, win, state)?;
        }
        Ok(())
    }
    pub(crate) fn draw_floating(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        window: Window,
        dimensions: Dimensions,
        state: &mut State,
    ) -> Result<()> {
        pgwm_utils::debug!("Drawing floating {window} at {dimensions:?}");
        call_wrapper.configure_window(
            xcb_out_buf,
            window,
            dimensions,
            state.window_border_width,
            state,
        )?;
        call_wrapper.send_map(xcb_out_buf, window, state)?;
        Ok(())
    }

    fn move_floating(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        window: Window,
        x: i32,
        y: i32,
        state: &mut State,
    ) -> Result<()> {
        pgwm_utils::debug!("Drawing floating {window} at ({x}, {y})");
        call_wrapper.move_window(xcb_out_buf, window, x, y, state)?;
        call_wrapper.send_map(xcb_out_buf, window, state)?;
        Ok(())
    }

    pub(crate) fn draw_on(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        map_windows: bool,
        state: &mut State,
    ) -> Result<()> {
        let ws_ind = state.monitors[mon_ind].hosted_workspace;
        let mut tiled = heapless::Vec::<&ManagedWindow, WS_WINDOW_LIMIT>::new();
        let mut floating = heapless::Vec::<(Window, ArrangeKind), WS_WINDOW_LIMIT>::new();
        for mw in state.workspaces.iter_all_managed_windows_in_ws(ws_ind) {
            if mw.arrange == ArrangeKind::NoFloat {
                push_heapless!(tiled, mw)?;
            } else {
                push_heapless!(floating, (mw.window, mw.arrange))?;
            }
        }
        let targets = tiled
            .iter()
            .map(|win| Drawtarget {
                window: win.window,
                map: map_windows,
                name: win.properties.name.get_cloned(),
            })
            .collect();
        drop(tiled);
        self.draw(call_wrapper, xcb_out_buf, mon_ind, targets, state)?;

        pgwm_utils::debug!("Drawing {} floating on mon = {mon_ind}", floating.len());
        for (win, arrange) in floating {
            if let ArrangeKind::FloatingInactive(rel_x, rel_y) = arrange {
                let dimensions = state.monitors[mon_ind].dimensions;
                let x = (dimensions.x as f32 + dimensions.width as f32 * rel_x) as i32;
                let y = (dimensions.y as f32
                    + state.status_bar_height as f32
                    + dimensions.height as f32 * rel_y) as i32;
                Self::move_floating(call_wrapper, xcb_out_buf, win, x, y, state)?;
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        targets: heapless::Vec<Drawtarget, WS_WINDOW_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        if targets.is_empty() {
            call_wrapper.send_unmap(
                xcb_out_buf,
                state.monitors[mon_ind].tab_bar_win.window.drawable,
                state,
            )?;
            return Ok(());
        }
        let ws_ind = state.monitors[mon_ind].hosted_workspace;
        let draw_mode = state.workspaces.get_draw_mode(ws_ind);
        match draw_mode {
            Mode::Tiled(layout) => {
                Self::draw_tiled(
                    call_wrapper,
                    xcb_out_buf,
                    mon_ind,
                    ws_ind,
                    targets,
                    layout,
                    state,
                )?;
            }
            Mode::Tabbed(target) => {
                self.draw_tabbed(call_wrapper, xcb_out_buf, mon_ind, targets, target, state)?;
            }
            Mode::Fullscreen {
                window,
                last_draw_mode,
            } => {
                // Making sure that we can de-toggle fullscreen without missing mapped windows etc,
                // pretty inefficient to draw everything below but whatever
                match last_draw_mode {
                    OldDrawMode::Tiled(layout) => {
                        Self::draw_tiled(
                            call_wrapper,
                            xcb_out_buf,
                            mon_ind,
                            ws_ind,
                            targets,
                            layout,
                            state,
                        )?;
                    }
                    OldDrawMode::Tabbed(target) => {
                        self.draw_tabbed(
                            call_wrapper,
                            xcb_out_buf,
                            mon_ind,
                            targets,
                            target,
                            state,
                        )?;
                    }
                }
                call_wrapper.configure_window(
                    xcb_out_buf,
                    window,
                    state.monitors[mon_ind].dimensions,
                    0,
                    state,
                )?;
                call_wrapper.send_map(xcb_out_buf, window, state)?;
            }
        }
        Ok(())
    }

    fn draw_tiled(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        targets: heapless::Vec<Drawtarget, WS_WINDOW_LIMIT>,
        layout: Layout,
        state: &mut State,
    ) -> Result<()> {
        pgwm_utils::debug!("Drawing tiled {targets:?} on mon = {mon_ind}");
        call_wrapper.send_unmap(
            xcb_out_buf,
            state.monitors[mon_ind].tab_bar_win.window.drawable,
            state,
        )?;
        let mon_dimensions = state.monitors[mon_ind].dimensions;
        let tiling_modifiers = &state.workspaces.get_ws(ws_ind).tiling_modifiers;
        let dimensions = layout.calculate_dimensions(
            mon_dimensions.width as u32,
            mon_dimensions.height as u32,
            state.window_padding,
            state.window_border_width,
            if state.monitors[mon_ind].show_bar {
                state.status_bar_height
            } else {
                0
            },
            true,
            targets.len(),
            tiling_modifiers.vertically_tiled.as_slice(),
            tiling_modifiers.left_leader,
            tiling_modifiers.center_leader,
        )?;
        if dimensions.len() != targets.len() {
            return Err(Error::Tiling);
        }
        let mon_x = state.monitors[mon_ind].dimensions.x;
        let mon_y = state.monitors[mon_ind].dimensions.y;
        for (ind, target) in targets.iter().enumerate() {
            let dim = dimensions[ind];
            let new_dimensions = Dimensions {
                width: dim.width,
                height: dim.height,
                x: dim.x + mon_x,
                y: dim.y + mon_y,
            };
            let win = target.window;

            call_wrapper.configure_window(
                xcb_out_buf,
                win,
                new_dimensions,
                state.window_border_width,
                state,
            )?;
            if target.map {
                call_wrapper.send_map(xcb_out_buf, win, state)?;
            }
        }
        Ok(())
    }

    fn draw_tabbed(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        targets: heapless::Vec<Drawtarget, WS_WINDOW_LIMIT>,
        target: usize,
        state: &mut State,
    ) -> Result<()> {
        let dt = &targets[target];
        let win = dt.window;
        let mon = &state.monitors[mon_ind];

        let padding = if state.pad_while_tabbed {
            state.window_padding
        } else {
            0
        };
        let x = mon.dimensions.x + padding;
        let y = mon.dimensions.y + state.status_bar_height + state.tab_bar_height + padding;
        let new_win_dims = Dimensions {
            height: mon.dimensions.height
                - state.status_bar_height
                - state.tab_bar_height
                - padding * 2
                - state.window_border_width as i16 * 2,
            width: mon.dimensions.width - state.window_border_width as i16 * 2 - padding * 2,
            x,
            y,
        };
        for dt in targets.iter() {
            if dt.map {
                call_wrapper.send_map(xcb_out_buf, dt.window, state)?;
            }
        }
        call_wrapper.configure_window(
            xcb_out_buf,
            win,
            new_win_dims,
            state.window_border_width,
            state,
        )?;
        let found_names = targets
            .into_iter()
            .map(|mw| mw.name)
            .collect::<heapless::Vec<heapless::String<WM_NAME_LIMIT>, WS_WINDOW_LIMIT>>();
        self.draw_tab_bar(
            call_wrapper,
            xcb_out_buf,
            mon_ind,
            found_names.as_slice(),
            target,
            padding,
            state,
        )
    }

    pub(crate) fn undraw(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        call_wrapper.send_unmap(
            xcb_out_buf,
            state.monitors[mon_ind].tab_bar_win.window.drawable,
            state,
        )?;
        for win in state
            .workspaces
            .iter_all_managed_windows_in_ws(state.monitors[mon_ind].hosted_workspace)
            .map(|mw| mw.window)
            // Annoying having to collect after move but whatever
            .collect::<heapless::Vec<Window, WS_WINDOW_LIMIT>>()
        {
            call_wrapper.send_unmap(xcb_out_buf, win, state)?;
        }
        Ok(())
    }

    fn draw_tab_bar(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_names: &[heapless::String<WM_NAME_LIMIT>],
        selected: usize,
        padding: i16,
        state: &mut State,
    ) -> Result<()> {
        let dimensions = state.monitors[mon_ind].dimensions;
        let split = (dimensions.width - 2 * padding) as usize / ws_names.len();
        let mut rounding_err =
            dimensions.width as usize - 2 * padding as usize - ws_names.len() * split;
        let win = state.monitors[mon_ind].tab_bar_win.window.drawable;
        call_wrapper.configure_window(
            xcb_out_buf,
            win,
            Dimensions::new(
                dimensions.width - 2 * padding,
                state.tab_bar_height,
                dimensions.x + padding,
                state.status_bar_height + padding + dimensions.y,
            ),
            0,
            state,
        )?;
        call_wrapper.send_map(xcb_out_buf, win, state)?;
        let dbw = &state.monitors[mon_ind].tab_bar_win;
        for (i, name) in ws_names.iter().enumerate() {
            let split_width = if rounding_err > 0 {
                rounding_err -= 1;
                split as i16 + 1
            } else {
                split as i16
            };
            let bg = if i == selected {
                state.colors.tab_bar_focused_tab_background
            } else {
                state.colors.tab_bar_unfocused_tab_background
            };
            let text_dimensions = self
                .font_manager
                .text_geometry(name, &self.fonts.tab_bar_section);
            let text_width = text_dimensions.0;
            let draw_name = if split_width >= text_width { name } else { "" };
            let center_offset = (split_width - text_width) / 2;

            self.font_manager.draw(
                call_wrapper,
                xcb_out_buf,
                dbw,
                draw_name,
                &self.fonts.tab_bar_section,
                Dimensions::new(split_width, state.tab_bar_height, split_width * i as i16, 0),
                split_width,
                center_offset,
                0,
                bg,
                state.colors.tab_bar_text,
            )?;
        }
        Ok(())
    }

    pub fn new(font_manager: &'a FontDrawer<'a>, fonts: &'a Fonts) -> Self {
        Drawer {
            font_manager,
            fonts,
        }
    }
}

#[derive(Clone, Debug)]
struct Drawtarget {
    window: Window,
    map: bool,
    name: heapless::String<WM_NAME_LIMIT>,
}
