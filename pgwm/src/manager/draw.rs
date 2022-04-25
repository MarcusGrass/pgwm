use crate::error::{Error, Result};
use crate::manager::font::FontDrawer;
use crate::x11::call_wrapper::CallWrapper;
use crate::x11::cookies::FallbackNameConvertCookie;
use pgwm_core::config::{Fonts, WM_NAME_LIMIT, WS_WINDOW_LIMIT};
use pgwm_core::geometry::draw::{Mode, OldDrawMode};
use pgwm_core::geometry::{layout::Layout, Dimensions};
use pgwm_core::push_heapless;
use pgwm_core::state::workspace::{ArrangeKind, ManagedWindow};
use pgwm_core::state::State;
use x11rb::protocol::xproto::Window;

pub(crate) struct Drawer<'a> {
    font_manager: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
    call_wrapper: &'a CallWrapper<'a>,
}

impl<'a> Drawer<'a> {
    pub(crate) fn send_floating_to_top(
        &self,
        floating: heapless::CopyVec<Window, WS_WINDOW_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        for win in floating {
            self.call_wrapper.push_window_to_top(win, state)?;
        }
        Ok(())
    }
    pub(crate) fn draw_floating(
        &self,
        window: Window,
        dimensions: Dimensions,
        state: &mut State,
    ) -> Result<()> {
        pgwm_core::debug!("Drawing floating {window} at {dimensions:?}");
        self.call_wrapper
            .configure_window(window, dimensions, state.window_border_width, state)?;
        self.call_wrapper.map_window(window, state)?;
        Ok(())
    }

    fn move_floating(&self, window: Window, x: i32, y: i32, state: &mut State) -> Result<()> {
        pgwm_core::debug!("Drawing floating {window} at ({x}, {y})");
        self.call_wrapper.move_window(window, x, y, state)?;
        self.call_wrapper.map_window(window, state)?;
        Ok(())
    }

    pub(crate) fn draw_on(
        &self,
        mon_ind: usize,
        map_windows: bool,
        state: &mut State,
    ) -> Result<()> {
        let ws_ind = state.monitors[mon_ind].hosted_workspace;
        let windows_in_ws = state.workspaces.get_all_windows_in_ws(ws_ind);
        let mut tiled = heapless::CopyVec::<ManagedWindow, WS_WINDOW_LIMIT>::new();
        let mut floating = heapless::CopyVec::<ManagedWindow, WS_WINDOW_LIMIT>::new();
        for mw in windows_in_ws {
            if mw.arrange == ArrangeKind::NoFloat {
                push_heapless!(tiled, mw)?;
            } else {
                push_heapless!(floating, mw)?;
            }
        }
        self.draw(
            mon_ind,
            tiled
                .iter()
                .map(|win| Drawtarget {
                    window: win.window,
                    map: map_windows,
                })
                .collect(),
            &tiled,
            state,
        )?;

        pgwm_core::debug!("Drawing {} floating on mon = {mon_ind}", floating.len());
        for mw in floating {
            if let ArrangeKind::FloatingInactive(rel_x, rel_y) = mw.arrange {
                let dimensions = state.monitors[mon_ind].dimensions;
                let x = (dimensions.x as f32 + dimensions.width as f32 * rel_x) as i32;
                let y = (dimensions.y as f32
                    + state.status_bar_height as f32
                    + dimensions.height as f32 * rel_y) as i32;
                self.move_floating(mw.window, x, y, state)?;
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        mon_ind: usize,
        targets: heapless::CopyVec<Drawtarget, WS_WINDOW_LIMIT>,
        windows: &heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>,
        state: &mut State,
    ) -> Result<()> {
        if targets.is_empty() {
            self.call_wrapper
                .unmap_window(state.monitors[mon_ind].tab_bar_win.window.drawable, state)?;
            return Ok(());
        }
        let ws_ind = state.monitors[mon_ind].hosted_workspace;
        let draw_mode = state.workspaces.get_draw_mode(ws_ind);
        match draw_mode {
            Mode::Tiled(layout) => {
                self.draw_tiled(mon_ind, ws_ind, windows, targets, layout, state)?;
            }
            Mode::Tabbed(target) => self.draw_tabbed(mon_ind, targets, target, state)?,
            Mode::Fullscreen {
                window,
                last_draw_mode,
            } => {
                // Making sure that we can de-toggle fullscreen without missing mapped windows etc,
                // pretty inefficient to draw everything below but whatever
                match last_draw_mode {
                    OldDrawMode::Tiled(layout) => {
                        self.draw_tiled(mon_ind, ws_ind, windows, targets, layout, state)?;
                    }
                    OldDrawMode::Tabbed(target) => {
                        self.draw_tabbed(mon_ind, targets, target, state)?;
                    }
                }
                self.call_wrapper.configure_window(
                    window,
                    state.monitors[mon_ind].dimensions,
                    0,
                    state,
                )?;
                self.call_wrapper.map_window(window, state)?;
            }
        }
        Ok(())
    }
    fn draw_tiled(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        windows: &heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>,
        targets: heapless::CopyVec<Drawtarget, WS_WINDOW_LIMIT>,
        layout: Layout,
        state: &mut State,
    ) -> Result<()> {
        pgwm_core::debug!("Drawing tiled {windows:?} on mon = {mon_ind}");
        self.call_wrapper
            .unmap_window(state.monitors[mon_ind].tab_bar_win.window.drawable, state)?;
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
            windows.len(),
            tiling_modifiers.vertically_tiled.as_slice(),
            tiling_modifiers.left_leader,
            tiling_modifiers.center_leader,
        )?;
        if dimensions.len() != windows.len() {
            return Err(Error::Tiling);
        }
        let mon_x = state.monitors[mon_ind].dimensions.x;
        for (ind, target) in targets.iter().enumerate() {
            let dim = dimensions[ind];
            let new_dimensions = Dimensions {
                width: dim.width,
                height: dim.height,
                x: dim.x + mon_x,
                y: dim.y,
            };
            let win = target.window;

            self.call_wrapper.configure_window(
                win,
                new_dimensions,
                state.window_border_width,
                state,
            )?;
            if target.map {
                self.call_wrapper.map_window(win, state)?;
            }
        }
        Ok(())
    }

    fn draw_tabbed(
        &self,
        mon_ind: usize,
        targets: heapless::CopyVec<Drawtarget, WS_WINDOW_LIMIT>,
        target: usize,
        state: &mut State,
    ) -> Result<()> {
        let names: heapless::Vec<Result<FallbackNameConvertCookie<'a>>, WS_WINDOW_LIMIT> = targets
            .iter()
            .map(|win| self.call_wrapper.get_name(win.window))
            .collect();

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
                self.call_wrapper.map_window(dt.window, state)?;
            }
        }
        self.call_wrapper
            .configure_window(win, new_win_dims, state.window_border_width, state)?;
        let found_names = names
            .into_iter()
            .map(|maybe_name_cookie| {
                maybe_name_cookie
                    .ok()
                    .and_then(|cookie| cookie.await_name().ok().flatten())
                    .unwrap_or_else(|| heapless::String::from("Unknown name"))
            })
            .collect::<Vec<heapless::String<WM_NAME_LIMIT>>>();
        self.draw_tab_bar(mon_ind, found_names.as_slice(), target, padding, state)
    }

    pub(crate) fn undraw(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        self.call_wrapper
            .unmap_window(state.monitors[mon_ind].tab_bar_win.window.drawable, state)?;
        state
            .workspaces
            .get_all_windows_in_ws(state.monitors[mon_ind].hosted_workspace)
            .iter()
            .try_for_each(|mw| {
                self.call_wrapper.unmap_window(mw.window, state)?;
                Ok::<_, crate::error::Error>(())
            })?;
        Ok(())
    }

    fn draw_tab_bar(
        &self,
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
        self.call_wrapper.configure_window(
            win,
            Dimensions::new(
                dimensions.width - 2 * padding,
                state.tab_bar_height,
                dimensions.x + padding,
                state.status_bar_height + padding,
            ),
            0,
            state,
        )?;
        self.call_wrapper.map_window(win, state)?;
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
            let mut text_width = text_dimensions.0 as usize;
            let text = if text_width > split_width as usize {
                let ratio = split_width as f32 / (text_width as f32 * 1.25f32); // Add some paddig for safety
                let text = name;
                let max_write = (text.len() as f32 * ratio) as usize;
                text_width = (text_width as f32 * ratio) as usize; // Ugly fix will make it a bit off-center probably
                &text[..max_write]
            } else {
                name
            };
            let center_offset = (split_width as usize - text_width) / 2;

            self.font_manager.draw(
                dbw,
                text,
                &self.fonts.tab_bar_section,
                Dimensions::new(split_width, state.tab_bar_height, split_width * i as i16, 0),
                center_offset as i16,
                0,
                bg,
                state.colors.tab_bar_text,
            )?;
        }
        Ok(())
    }

    pub fn new(
        font_manager: &'a FontDrawer<'a>,
        call_wrapper: &'a CallWrapper,
        fonts: &'a Fonts,
    ) -> Self {
        Drawer {
            font_manager,
            fonts,
            call_wrapper,
        }
    }
}

#[derive(Copy, Clone)]
struct Drawtarget {
    window: Window,
    map: bool,
}
