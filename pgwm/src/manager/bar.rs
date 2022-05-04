use crate::error::Result;
use crate::manager::font::FontDrawer;
use crate::x11::call_wrapper::CallWrapper;
use pgwm_core::colors::Color;
use pgwm_core::config::Fonts;
#[cfg(feature = "status-bar")]
use pgwm_core::config::STATUS_BAR_CHECK_CONTENT_LIMIT;
use pgwm_core::geometry::Dimensions;
use pgwm_core::state::State;
use x11rb::cookie::VoidCookie;
use x11rb::rust_connection::RustConnection;

pub(crate) struct BarManager<'a> {
    call_wrapper: &'a CallWrapper<'a>,
    font_drawer: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
}

impl<'a> BarManager<'a> {
    #[cfg(feature = "status-bar")]
    pub(crate) fn update_window_titles(&self, state: &mut State) -> Result<()> {
        for mon in 0..state.monitors.len() {
            self.set_window_title(mon, state)?;
        }
        Ok(())
    }

    pub(crate) fn set_window_title(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let maybe_name = mon
            .last_focus
            .and_then(|win| self.call_wrapper.get_name(win.window).ok());
        let title_position = state
            .bar_geometry
            .calculate_window_title_position(mon.dimensions.width);
        let name = maybe_name
            .and_then(|r| r.await_name().ok())
            .flatten()
            .unwrap_or_else(|| heapless::String::from("pgwm"));

        let mon = &mut state.monitors[mon_ind];
        self.font_drawer.draw(
            &mon.bar_win,
            &name,
            &self.fonts.workspace_section,
            Dimensions::new(
                title_position.length,
                state.status_bar_height,
                title_position.start,
                0,
            ),
            state.workspace_bar_window_name_padding as i16,
            0,
            state.colors.workspace_bar_current_window_title_background,
            state.colors.workspace_bar_current_window_title_text,
        )?;
        Ok(())
    }

    pub(crate) fn set_workspace_focused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_focused_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_unfocused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_unfocused_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_urgent(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_urgent_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_selected_not_focused(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            mon_ind,
            ws_ind,
            state
                .colors
                .workspace_bar_selected_unfocused_workspace_background,
            state,
        )
    }

    fn draw_ws(
        &self,
        mon_ind: usize,
        ws_ind: usize,
        bg_color: Color,
        state: &mut State,
    ) -> Result<()> {
        let mon = &mut state.monitors[mon_ind];
        let component = &state.bar_geometry.workspace.components[ws_ind];
        let name = &state.workspaces.get_ws(ws_ind).name;
        self.font_drawer.draw(
            &mon.bar_win,
            name,
            &self.fonts.workspace_section,
            Dimensions::new(
                component.position.length,
                state.status_bar_height,
                component.position.start,
                0,
            ),
            component.write_offset,
            0,
            bg_color,
            state.colors.workspace_bar_workspace_section_text,
        )?;
        Ok(())
    }

    fn init_workspace(&self, mon_ind: usize, ws_ind: usize, state: &mut State) -> Result<()> {
        let mon = &mut state.monitors[mon_ind];
        let is_mon_focus = state.focused_mon == mon_ind;
        let wants_focus = state.workspaces.get_wants_focus_workspaces();
        for (ind, ws) in state.bar_geometry.workspace.components.iter().enumerate() {
            let name = &ws.text;
            let bg = if name.contains(state.workspaces.get_ws(ws_ind).name.as_str()) {
                if is_mon_focus {
                    state.colors.workspace_bar_focused_workspace_background
                } else {
                    state
                        .colors
                        .workspace_bar_selected_unfocused_workspace_background
                }
            } else if wants_focus[ind] {
                state.colors.workspace_bar_urgent_workspace_background
            } else {
                state.colors.workspace_bar_unfocused_workspace_background
            };
            self.font_drawer.draw(
                &mon.bar_win,
                name,
                &self.fonts.workspace_section,
                Dimensions::new(
                    ws.position.length,
                    state.status_bar_height,
                    ws.position.start,
                    0,
                ),
                ws.write_offset,
                0,
                bg,
                state.colors.workspace_bar_workspace_section_text,
            )?;
        }
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
            .font_drawer
            .text_geometry(&content, &self.fonts.status_section)
            .0;
        let new_width = state.bar_geometry.status.update_section_widths(
            content,
            content_slice_width,
            content_ind,
        );
        self.draw_status_with_internal_data(state)?;
        Ok(new_width)
    }

    #[cfg(feature = "status-bar")]
    fn draw_status_with_internal_data(&self, state: &State) -> Result<()> {
        let content = state.bar_geometry.status.get_content_as_str();
        let bg = state.colors.status_bar_background();
        let text_col = state.colors.status_bar_text();

        for i in 0..state.monitors.len() {
            let status_position = state
                .bar_geometry
                .calculate_status_position(state.monitors[i].dimensions.width);
            let src_y = state.monitors[i].dimensions.y;
            self.font_drawer.draw(
                &state.monitors[i].bar_win,
                &content,
                &self.fonts.status_section,
                Dimensions::new(
                    status_position.length,
                    state.status_bar_height,
                    status_position.start,
                    src_y,
                ),
                0,
                0,
                *bg,
                *text_col,
            )?;
        }
        Ok(())
    }

    pub(crate) fn draw_shortcuts(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        let mon = &mut state.monitors[mon_ind];
        let pos = state
            .bar_geometry
            .calculate_shortcuts_position(mon.dimensions.width);
        let mut offset = pos.start;
        let bg = state.colors.shortcut_background();
        let text = state.colors.shortcut_text();
        for shortcut in &state.bar_geometry.shortcuts.components {
            let name = &shortcut.text;
            self.font_drawer.draw(
                &mon.bar_win,
                name,
                &self.fonts.shortcut_section,
                Dimensions::new(shortcut.width, state.status_bar_height, offset, 0),
                shortcut.write_offset,
                0,
                *bg,
                *text,
            )?;
            offset += shortcut.width;
        }
        Ok(())
    }

    pub(crate) fn redraw_on(&self, mon_ind: usize, state: &mut State) -> Result<()> {
        self.init_workspace(mon_ind, state.monitors[mon_ind].hosted_workspace, state)?;
        #[cfg(feature = "status-bar")]
        self.draw_status_with_internal_data(state)?;
        self.set_window_title(mon_ind, state)?;
        self.draw_shortcuts(mon_ind, state)?;
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
                .unmap_window(state.monitors[mon_ind].bar_win.window.drawable, state)?;
            Ok(None)
        } else {
            let cookie = self
                .call_wrapper
                .map_window(state.monitors[mon_ind].bar_win.window.drawable, state)?;
            state.monitors[mon_ind].show_bar = true;
            Ok(Some(cookie))
        }
    }

    pub fn new(
        call_wrapper: &'a CallWrapper<'a>,
        font_drawer: &'a FontDrawer<'a>,
        fonts: &'a Fonts,
    ) -> Self {
        Self {
            call_wrapper,
            font_drawer,
            fonts,
        }
    }
}
