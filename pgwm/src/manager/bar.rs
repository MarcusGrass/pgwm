use crate::error::Result;
use crate::manager::font::FontManager;
use crate::x11::call_wrapper::CallWrapper;
use pgwm_core::config::Fonts;
#[cfg(feature = "status-bar")]
use pgwm_core::config::STATUS_BAR_CHECK_CONTENT_LIMIT;
use pgwm_core::geometry::Dimensions;
use pgwm_core::state::State;
use x11rb::cookie::VoidCookie;
use x11rb::protocol::xproto::Gcontext;
use x11rb::rust_connection::RustConnection;

pub(crate) struct BarManager<'a> {
    call_wrapper: &'a CallWrapper<'a>,
    font_manager: &'a FontManager<'a>,
    fonts: &'a Fonts,
}

impl<'a> BarManager<'a> {
    #[cfg(feature = "status-bar")]
    pub(crate) fn update_window_titles(&self, state: &State) -> Result<()> {
        for mon in 0..state.monitors.len() {
            self.set_window_title(mon, state)?;
        }
        Ok(())
    }

    pub(crate) fn set_window_title(&self, mon_ind: usize, state: &State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let maybe_name = mon
            .last_focus
            .and_then(|win| self.call_wrapper.get_name(win.window).ok());
        let title_position = state
            .bar_geometry
            .calculate_window_title_position(mon.dimensions.width);

        self.call_wrapper
            .fill_rectangle(
                mon.bar_pixmap,
                state.permanent_drawables.current_workspace_gc,
                Dimensions {
                    width: title_position.length,
                    height: state.status_bar_height,
                    x: title_position.start,
                    y: 0,
                },
            )?
            .check()?;
        let mut active_window: heapless::String<256> = heapless::String::from(" ");
        active_window
            .push_str(
                maybe_name
                    .and_then(|mn| mn.await_name().ok().flatten())
                    .unwrap_or_else(|| heapless::String::from("pgwm"))
                    .as_str(),
            )
            .unwrap_or_else(|_| {
                let _ = active_window.push_str("pgwm");
            });

        self.font_manager.draw_text(
            mon.bar_pixmap,
            &active_window,
            title_position.start as u32,
            0,
            &self.fonts.window_name_display_section,
            &state.colors.workspace_bar_current_window_title_text().pixel,
            state.status_bar_height,
        )?;
        self.call_wrapper.copy_area(
            mon.bar_pixmap,
            mon.bar_win,
            state.permanent_drawables.unfocused_workspace_gc,
            title_position.start,
            0,
            title_position.start,
            0,
            title_position.length as u16,
            state.status_bar_height as u16,
        )?;
        Ok(())
    }

    pub(crate) fn set_workspace_focused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.permanent_drawables.focused_workspace_gc,
            state,
        )
    }

    pub(crate) fn set_workspace_unfocused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.permanent_drawables.unfocused_workspace_gc,
            state,
        )
    }

    pub(crate) fn set_workspace_urgent(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.permanent_drawables.urgent_workspace_gc,
            state,
        )
    }

    pub(crate) fn set_workspace_selected_not_focused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.permanent_drawables.selected_unfocused_workspace_gc,
            state,
        )
    }

    fn draw_ws(&self, mon_ind: usize, ws_ind: usize, gc: Gcontext, state: &State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let component = &state.bar_geometry.workspace.components[ws_ind];
        let fill = self.call_wrapper.fill_rectangle(
            mon.bar_pixmap,
            gc,
            Dimensions {
                x: component.position.start,
                y: 0,
                width: component.position.length,
                height: state.status_bar_height,
            },
        )?;
        let name = &state.workspaces.get_ws(ws_ind).name;
        fill.check()?;
        self.font_manager.draw_text(
            mon.bar_pixmap,
            name,
            (component.position.start + component.write_offset) as u32,
            0,
            &self.fonts.workspace_section,
            &state.colors.workspace_bar_workspace_section_text().pixel,
            state.status_bar_height,
        )?;
        self.call_wrapper.copy_area(
            mon.bar_pixmap,
            mon.bar_win,
            state.permanent_drawables.unfocused_workspace_gc,
            component.position.start,
            0,
            component.position.start,
            0,
            component.position.length as u16,
            state.status_bar_height as u16,
        )?;
        Ok(())
    }

    fn init_workspace(&self, mon_ind: usize, ws_ind: usize, state: &State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let width = state.bar_geometry.workspace.position.length;
        let fill = self.call_wrapper.fill_rectangle(
            mon.bar_pixmap,
            state.permanent_drawables.unfocused_workspace_gc,
            Dimensions {
                x: state.bar_geometry.workspace.position.start,
                y: 0,
                width,
                height: state.status_bar_height,
            },
        )?;

        fill.check()?;
        let is_mon_focus = state.focused_mon == mon_ind;
        let wants_focus = state.workspaces.get_wants_focus_workspaces();
        for (ind, ws) in state.bar_geometry.workspace.components.iter().enumerate() {
            let name = &ws.text;
            if name.contains(state.workspaces.get_ws(ws_ind).name.as_str()) {
                let gc = if is_mon_focus {
                    state.permanent_drawables.focused_workspace_gc
                } else {
                    state.permanent_drawables.selected_unfocused_workspace_gc
                };
                self.call_wrapper
                    .fill_rectangle(
                        mon.bar_pixmap,
                        gc,
                        Dimensions {
                            width: ws.position.length as i16,
                            height: state.status_bar_height,
                            x: ws.position.start,
                            y: 0,
                        },
                    )?
                    .check()?;
            } else if wants_focus[ind] {
                self.call_wrapper
                    .fill_rectangle(
                        mon.bar_pixmap,
                        state.permanent_drawables.urgent_workspace_gc,
                        Dimensions {
                            x: ws.position.start,
                            y: 0,
                            width: ws.position.length as i16,
                            height: state.status_bar_height,
                        },
                    )?
                    .check()?;
            }
            self.font_manager.draw_text(
                mon.bar_pixmap,
                name,
                (ws.position.start + ws.write_offset) as u32,
                0,
                &self.fonts.workspace_section,
                &state.colors.workspace_bar_workspace_section_text().pixel,
                state.status_bar_height,
            )?;
        }
        self.call_wrapper.copy_area(
            mon.bar_pixmap,
            mon.bar_win,
            state.permanent_drawables.unfocused_workspace_gc,
            state.bar_geometry.workspace.position.start,
            0,
            state.bar_geometry.workspace.position.start,
            0,
            state.bar_geometry.workspace.position.length as u16,
            state.status_bar_height as u16,
        )?;
        Ok(())
    }

    pub(crate) fn draw_static(&self, state: &mut State) -> Result<()> {
        for mon_ind in 0..state.monitors.len() {
            self.init_workspace(mon_ind, state.monitors[mon_ind].hosted_workspace, state)?;
            self.draw_shortcuts(mon_ind, state)?;
        }
        Ok(())
    }

    #[cfg(feature = "status-bar")]
    pub(crate) fn update_status(
        &self,
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        //pgwm_core::debug!("Updating status, with {content} at {content_ind}");
        if self.draw_status(content, content_ind, state)? {
            //pgwm_core::debug!("Updated status");
            self.update_window_titles(state)?;
        }
        Ok(())
    }

    #[cfg(feature = "status-bar")]
    fn draw_status(
        &self,
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<bool> {
        let content_slice_width = self
            .font_manager
            .get_width_and_height(&content, &self.fonts.status_section)?
            .0 as i16;
        let new_width = state.bar_geometry.status.update_section_widths(
            content,
            content_slice_width,
            content_ind,
        );
        let new_content = state.bar_geometry.status.get_content_as_str();
        // Could skip this entirely and just get width on init but that would mess with monospaced fonts
        let new_content_width = if new_width {
            self.font_manager
                .get_width_and_height(&new_content, &self.fonts.status_section)?
                .0 as i16
        } else {
            content_slice_width
        };
        self.call_wrapper
            .fill_rectangle(
                state.status_pixmap,
                state.permanent_drawables.status_bar_gc,
                Dimensions {
                    x: 0,
                    y: 0,
                    width: new_content_width,
                    height: state.status_bar_height,
                },
            )?
            .check()?;
        self.font_manager.draw_text(
            state.status_pixmap,
            &new_content,
            0,
            0,
            &self.fonts.status_section,
            &state.colors.status_bar_text().pixel,
            state.status_bar_height,
        )?;

        for mon in &state.monitors {
            let status_position = state
                .bar_geometry
                .calculate_status_position(mon.dimensions.width);

            self.call_wrapper.copy_area(
                state.status_pixmap,
                mon.bar_win,
                state.permanent_drawables.status_bar_gc,
                0,
                0,
                status_position.start,
                0,
                status_position.length as u16,
                state.status_bar_height as u16,
            )?;
        }
        Ok(new_width)
    }

    pub(crate) fn draw_shortcuts(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let pos = state
            .bar_geometry
            .calculate_shortcuts_position(mon.dimensions.width);
        let fill = self.call_wrapper.fill_rectangle(
            mon.bar_pixmap,
            state.permanent_drawables.shortcut_gc,
            Dimensions {
                x: pos.start,
                y: 0,
                width: pos.length,
                height: state.status_bar_height,
            },
        )?;
        fill.check()?;
        let mut offset = pos.start;
        for shortcut in &state.bar_geometry.shortcuts.components {
            let name = &shortcut.text;
            self.font_manager.draw_text(
                mon.bar_pixmap,
                name,
                (offset + shortcut.write_offset) as u32,
                0,
                &self.fonts.shortcut_section,
                &state.colors.shortcut_text().pixel,
                state.status_bar_height,
            )?;
            offset += shortcut.width;
        }
        self.call_wrapper.copy_area(
            mon.bar_pixmap,
            mon.bar_win,
            state.permanent_drawables.shortcut_gc,
            pos.start,
            0,
            pos.start,
            0,
            pos.length as u16,
            state.status_bar_height as u16,
        )?;
        Ok(())
    }

    pub(crate) fn redraw_on(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        self.call_wrapper.copy_area(
            mon.bar_pixmap,
            mon.bar_win,
            state.permanent_drawables.unfocused_workspace_gc,
            0,
            0,
            0,
            0,
            mon.dimensions.width as u16,
            state.status_bar_height as u16,
        )?;
        // Shared status pixmap creates some issues, either we copy the pixmap to the bar pixmap
        // on each draw, or we do this double copy on a full redraw, this is further from the hot-path.
        #[cfg(feature = "status-bar")]
        {
            let status_position = state
                .bar_geometry
                .calculate_status_position(mon.dimensions.width);

            self.call_wrapper.copy_area(
                state.status_pixmap,
                mon.bar_win,
                state.permanent_drawables.status_bar_gc,
                0,
                0,
                status_position.start,
                0,
                status_position.length as u16,
                state.status_bar_height as u16,
            )?;
        }

        Ok(())
    }

    pub(crate) fn toggle_bar(
        &self,
        mon_ind: usize,
        state: &mut State,
    ) -> Result<Option<VoidCookie<RustConnection>>> {
        if state.monitors[mon_ind].show_bar {
            state.monitors[mon_ind].show_bar = false;
            self.call_wrapper
                .unmap_window(state.monitors[mon_ind].bar_win, state)?;
            Ok(None)
        } else {
            let cookie = self
                .call_wrapper
                .map_window(state.monitors[mon_ind].bar_win, state)?;
            state.monitors[mon_ind].show_bar = true;
            Ok(Some(cookie))
        }
    }

    pub fn new(
        call_wrapper: &'a CallWrapper<'a>,
        font_manager: &'a FontManager<'a>,
        fonts: &'a Fonts,
    ) -> Self {
        Self {
            call_wrapper,
            font_manager,
            fonts,
        }
    }
}
