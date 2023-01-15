use pgwm_core::colors::Color;
use pgwm_core::config::Fonts;
#[cfg(feature = "status-bar")]
use pgwm_core::config::STATUS_BAR_CHECK_CONTENT_LIMIT;
use pgwm_core::geometry::Dimensions;
use pgwm_core::state::State;

use crate::error::Result;
use crate::manager::font::FontDrawer;
use crate::uring::UringWrapper;
use crate::x11::call_wrapper::CallWrapper;

pub(crate) struct BarManager<'a> {
    font_drawer: &'a FontDrawer<'a>,
    fonts: &'a Fonts,
}

impl<'a> BarManager<'a> {
    pub(crate) fn draw_focused_window_title(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        let mon = &state.monitors[mon_ind];
        let section = &mon.bar_geometry.window_title_section;
        let title_position = section.position;
        pgwm_utils::debug!("Starting window title draw");
        let draw_width = self.font_drawer.draw(
            call_wrapper,
            xcb_out_buf,
            &mon.bar_win,
            &section.display,
            &self.fonts.workspace_section,
            Dimensions::new(
                section.last_draw_width,
                state.status_bar_height,
                title_position.start,
                0,
            ),
            title_position.length,
            state.workspace_bar_window_name_padding as i16,
            0,
            state.colors.workspace_bar_current_window_title_background,
            state.colors.workspace_bar_current_window_title_text,
        )?;
        state.monitors[mon_ind]
            .bar_geometry
            .window_title_section
            .last_draw_width = draw_width + state.workspace_bar_window_name_padding as i16;
        Ok(())
    }

    pub(crate) fn set_workspace_focused(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            call_wrapper,
            xcb_out_buf,
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_focused_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_unfocused(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            call_wrapper,
            xcb_out_buf,
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_unfocused_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_urgent(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            call_wrapper,
            xcb_out_buf,
            mon_ind,
            ws_ind,
            state.colors.workspace_bar_urgent_workspace_background,
            state,
        )
    }

    pub(crate) fn set_workspace_selected_not_focused(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_ws(
            call_wrapper,
            xcb_out_buf,
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
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        bg_color: Color,
        state: &mut State,
    ) -> Result<()> {
        let mon = &mut state.monitors[mon_ind];
        let component = &mon.bar_geometry.workspace.components[ws_ind];
        let name = &state.workspaces.get_ws(ws_ind).name;
        pgwm_utils::debug!("Starting workspace draw");
        self.font_drawer.draw(
            call_wrapper,
            xcb_out_buf,
            &mon.bar_win,
            name,
            &self.fonts.workspace_section,
            Dimensions::new(
                component.position.length,
                state.status_bar_height,
                component.position.start,
                0,
            ),
            component.position.length,
            component.write_offset,
            0,
            bg_color,
            state.colors.workspace_bar_workspace_section_text,
        )?;
        Ok(())
    }

    fn init_workspace(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        ws_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        let mon = &mut state.monitors[mon_ind];
        let is_mon_focus = state.focused_mon == mon_ind;
        let wants_focus = state.workspaces.get_wants_focus_workspaces();
        pgwm_utils::debug!("Running clean workspace redraw on mon {mon_ind}");
        for (ind, ws) in mon.bar_geometry.workspace.components.iter().enumerate() {
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
                call_wrapper,
                xcb_out_buf,
                &mon.bar_win,
                name,
                &self.fonts.workspace_section,
                Dimensions::new(
                    ws.position.length,
                    state.status_bar_height,
                    ws.position.start,
                    0,
                ),
                ws.position.length,
                ws.write_offset,
                0,
                bg,
                state.colors.workspace_bar_workspace_section_text,
            )?;
        }
        Ok(())
    }

    pub(crate) fn draw_static(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        state: &mut State,
    ) -> Result<()> {
        for mon_ind in 0..state.monitors.len() {
            self.init_workspace(
                call_wrapper,
                xcb_out_buf,
                mon_ind,
                state.monitors[mon_ind].hosted_workspace,
                state,
            )?;
            self.draw_shortcuts(call_wrapper, xcb_out_buf, mon_ind, state)?;
        }
        Ok(())
    }

    #[cfg(feature = "status-bar")]
    pub(crate) fn update_status(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.draw_status(call_wrapper, xcb_out_buf, content, content_ind, state)
    }

    #[cfg(feature = "status-bar")]
    fn draw_status(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        content_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        let bg = state.colors.status_bar_background();
        let text_col = state.colors.status_bar_text();
        for mon_ind in 0..state.monitors.len() {
            let (content, pos) = state.monitors[mon_ind]
                .bar_geometry
                .status
                .update_and_get_section_line(content.clone(), content_ind);
            let src_y = state.monitors[mon_ind].dimensions.y;
            self.font_drawer.draw(
                call_wrapper,
                xcb_out_buf,
                &state.monitors[mon_ind].bar_win,
                &content,
                &self.fonts.status_section,
                Dimensions::new(pos.length, state.status_bar_height, pos.start, src_y),
                pos.length,
                0,
                0,
                *bg,
                *text_col,
            )?;
        }
        Ok(())
    }

    #[cfg(feature = "status-bar")]
    fn draw_status_with_internal_data(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        state: &State,
    ) -> Result<()> {
        let bg = state.colors.status_bar_background();
        let text_col = state.colors.status_bar_text();

        for i in 0..state.monitors.len() {
            for section in &state.monitors[i].bar_geometry.status.components {
                let status_position = section.position;
                let src_y = state.monitors[i].dimensions.y;
                self.font_drawer.draw(
                    call_wrapper,
                    xcb_out_buf,
                    &state.monitors[i].bar_win,
                    &section.display,
                    &self.fonts.status_section,
                    Dimensions::new(
                        status_position.length,
                        state.status_bar_height,
                        status_position.start,
                        src_y,
                    ),
                    status_position.length,
                    0,
                    0,
                    *bg,
                    *text_col,
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn draw_shortcuts(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        pgwm_utils::debug!("Starting shortcuts draw");
        let mon = &mut state.monitors[mon_ind];
        let pos = mon.bar_geometry.shortcuts.position;
        let mut offset = pos.start;
        let bg = state.colors.shortcut_background();
        let text = state.colors.shortcut_text();
        for shortcut in &mon.bar_geometry.shortcuts.components {
            let name = &shortcut.text;
            self.font_drawer.draw(
                call_wrapper,
                xcb_out_buf,
                &mon.bar_win,
                name,
                &self.fonts.shortcut_section,
                Dimensions::new(shortcut.position.length, state.status_bar_height, offset, 0),
                shortcut.position.length,
                shortcut.write_offset,
                0,
                *bg,
                *text,
            )?;
            offset += shortcut.position.length;
        }
        Ok(())
    }

    pub(crate) fn redraw_on(
        &self,
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        state: &mut State,
    ) -> Result<()> {
        self.init_workspace(
            call_wrapper,
            xcb_out_buf,
            mon_ind,
            state.monitors[mon_ind].hosted_workspace,
            state,
        )?;
        #[cfg(feature = "status-bar")]
        self.draw_status_with_internal_data(call_wrapper, xcb_out_buf, state)?;
        // Make sure to cover the entire bar with a background again
        state.monitors[mon_ind]
            .bar_geometry
            .window_title_section
            .last_draw_width = state.monitors[mon_ind]
            .bar_geometry
            .window_title_section
            .position
            .length;
        self.draw_focused_window_title(call_wrapper, xcb_out_buf, mon_ind, state)?;
        self.draw_shortcuts(call_wrapper, xcb_out_buf, mon_ind, state)?;
        Ok(())
    }

    pub(crate) fn toggle_bar(
        call_wrapper: &mut CallWrapper,
        xcb_out_buf: &mut [u8],
        mon_ind: usize,
        state: &mut State,
    ) -> Result<bool> {
        if state.monitors[mon_ind].show_bar {
            state.monitors[mon_ind].show_bar = false;
            call_wrapper.send_unmap(
                xcb_out_buf,
                state.monitors[mon_ind].bar_win.window.drawable,
                state,
            )?;
            Ok(false)
        } else {
            call_wrapper.send_map(
                xcb_out_buf,
                state.monitors[mon_ind].bar_win.window.drawable,
                state,
            )?;
            state.monitors[mon_ind].show_bar = true;
            Ok(true)
        }
    }

    pub fn new(font_drawer: &'a FontDrawer<'a>, fonts: &'a Fonts) -> Self {
        Self { font_drawer, fonts }
    }
}
