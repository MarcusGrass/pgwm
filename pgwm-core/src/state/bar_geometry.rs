use alloc::vec::Vec;

use crate::config::mouse_map::MouseTarget;
use crate::config::_WM_NAME_LIMIT;
#[cfg(feature = "status-bar")]
use crate::config::{
    STATUS_CHECKS, _STATUS_BAR_CHECK_CONTENT_LIMIT, _STATUS_BAR_CHECK_SEP, _STATUS_BAR_FIRST_SEP,
    _STATUS_BAR_TOTAL_LENGTH_LIMIT,
};
use crate::geometry::Line;

pub struct BarGeometry {
    pub workspace: WorkspaceSection,
    pub shortcuts: ShortcutSection,
    #[cfg(feature = "status-bar")]
    pub status: StatusSection,
    pub window_title_section: WindowTitleSection,
}

impl BarGeometry {
    #[must_use]
    pub fn hit_on_click(&self, x: i16) -> Option<MouseTarget> {
        let hit = self
            .workspace
            .hit_component(x)
            .or_else(|| self.shortcuts.hit_component(x));
        #[cfg(feature = "status-bar")]
        {
            hit.or_else(|| self.status.hit_component(x)).or_else(|| {
                self.window_title_section
                    .position
                    .contains(x)
                    .then_some(MouseTarget::WindowTitle)
            })
        }
        #[cfg(not(feature = "status-bar"))]
        {
            hit.or_else(|| {
                self.window_title_section
                    .position
                    .contains(x)
                    .then_some(MouseTarget::WindowTitle)
            })
        }
    }

    #[must_use]
    pub fn new(
        mon_width: i16,
        workspace: WorkspaceSection,
        shortcuts: ShortcutSection,
        #[cfg(feature = "status-bar")] status: StatusSection,
    ) -> Self {
        #[cfg(feature = "status-bar")]
        let title_width = mon_width
            - workspace.position.length
            - shortcuts.position.length
            - status.position.length;
        #[cfg(not(feature = "status-bar"))]
        let title_width = mon_width - workspace.position.length - shortcuts.position.length;

        Self {
            window_title_section: WindowTitleSection {
                position: Line::new(
                    workspace.position.start + workspace.position.length,
                    title_width,
                ),
                display: heapless::String::try_from("pgwm").unwrap(),
                last_draw_width: title_width, // Set last draw to full with so initial draw, paints the entire section
            },
            workspace,
            shortcuts,
            #[cfg(feature = "status-bar")]
            status,
        }
    }
}

#[derive(Clone)]
pub struct WindowTitleSection {
    pub position: Line,
    pub display: heapless::String<_WM_NAME_LIMIT>,
    pub last_draw_width: i16,
}

pub struct ShortcutSection {
    pub position: Line,
    pub components: Vec<ShortcutComponent>,
}

#[derive(Debug)]
pub struct ShortcutComponent {
    pub position: Line,
    pub write_offset: i16,
    pub text: &'static str,
}

impl ShortcutSection {
    fn hit_component(&self, x: i16) -> Option<MouseTarget> {
        (x >= self.position.start && x <= self.position.start + self.position.length)
            .then(|| {
                self.components
                    .iter()
                    .enumerate()
                    .find_map(|(ind, component)| {
                        (x >= component.position.start
                            && x <= component.position.start + component.position.length)
                            .then_some(MouseTarget::ShortcutComponent(ind))
                    })
            })
            .flatten()
    }
}

#[cfg(feature = "status-bar")]
pub struct StatusSection {
    pub position: Line,
    pub first_sep_len: i16,
    pub sep_len: i16,
    pub components: heapless::Vec<StatusComponent, { STATUS_CHECKS.len() }>,
}

#[cfg(feature = "status-bar")]
impl StatusSection {
    #[must_use]
    pub fn new(
        mon_width: i16,
        right_offset: i16,
        check_lengths: &[i16],
        sep_len: i16,
        first_sep_len: i16,
    ) -> Self {
        let mut total_length = 0;
        let mut corrected_lengths: heapless::Vec<i16, { STATUS_CHECKS.len() }> =
            heapless::Vec::new();
        for (ind, check) in check_lengths.iter().enumerate() {
            let mut cur_length = 0;
            if ind == 0 {
                cur_length += check + first_sep_len;
            } else if ind == check_lengths.len() - 1 {
                cur_length += check + sep_len + first_sep_len;
            } else {
                cur_length += check + sep_len;
            }
            let _ = corrected_lengths.push(cur_length);
            total_length += cur_length;
        }
        let mut components = heapless::Vec::new();
        let start = mon_width - right_offset - total_length;
        let mut offset = 0;
        for length in corrected_lengths {
            let _ = components.push(StatusComponent {
                position: Line {
                    start: start + offset,
                    length,
                },
                display: heapless::String::default(),
            });
            offset += length;
        }

        Self {
            position: Line {
                start,
                length: total_length,
            },
            sep_len,
            first_sep_len,
            components,
        }
    }

    pub fn update_and_get_section_line(
        &mut self,
        new_content: heapless::String<_STATUS_BAR_CHECK_CONTENT_LIMIT>,
        new_component_ind: usize,
    ) -> (heapless::String<_STATUS_BAR_CHECK_CONTENT_LIMIT>, Line) {
        let content = if new_component_ind == 0 {
            crate::format_heapless!("{_STATUS_BAR_FIRST_SEP}{new_content}")
        } else if new_component_ind == self.components.len() - 1 {
            crate::format_heapless!("{_STATUS_BAR_CHECK_SEP}{new_content}{_STATUS_BAR_FIRST_SEP}  ")
        } else {
            crate::format_heapless!("{_STATUS_BAR_CHECK_SEP}{new_content}")
        };
        let component = &mut self.components[new_component_ind];
        component.display = content.clone();
        (content, component.position)
    }

    #[must_use]
    pub fn get_full_content(&self) -> heapless::String<_STATUS_BAR_TOTAL_LENGTH_LIMIT> {
        let mut s = heapless::String::new();
        for component in &self.components {
            let _ = s.push_str(&component.display);
        }

        s
    }

    #[must_use]
    fn hit_component(&self, x: i16) -> Option<MouseTarget> {
        (x >= self.position.start && x <= self.position.start + self.position.length)
            .then(|| {
                self.components
                    .iter()
                    .enumerate()
                    .find_map(|(ind, component)| {
                        (x >= component.position.start
                            && x <= component.position.start + component.position.length)
                            .then_some(MouseTarget::StatusComponent(ind))
                    })
            })
            .flatten()
    }
}

#[cfg(feature = "status-bar")]
#[derive(Debug, Clone)]
pub struct StatusComponent {
    pub position: Line,
    pub display: heapless::String<_STATUS_BAR_CHECK_CONTENT_LIMIT>,
}

pub struct WorkspaceSection {
    pub position: Line,
    pub components: Vec<FixedDisplayComponent>,
}

impl WorkspaceSection {
    fn hit_component(&self, x: i16) -> Option<MouseTarget> {
        (x >= self.position.start && x <= self.position.start + self.position.length)
            .then(|| {
                self.components
                    .iter()
                    .enumerate()
                    .find_map(|(ind, component)| {
                        (x >= component.position.start
                            && x <= component.position.start + component.position.length)
                            .then_some(MouseTarget::WorkspaceBarComponent(ind))
                    })
            })
            .flatten()
    }
}

pub struct FixedDisplayComponent {
    pub position: Line,
    pub write_offset: i16,
    pub text: &'static str,
}
