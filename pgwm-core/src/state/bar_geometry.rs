use crate::config::mouse_map::MouseTarget;
#[cfg(feature = "status-bar")]
use crate::config::{
    STATUS_BAR_CHECK_CONTENT_LIMIT, STATUS_BAR_CHECK_SEP, STATUS_BAR_FIRST_SEP,
    STATUS_BAR_TOTAL_LENGTH_LIMIT, STATUS_BAR_UNIQUE_CHECK_LIMIT,
};
use crate::format_heapless;
use crate::geometry::Line;

pub struct BarGeometry {
    pub workspace: WorkspaceSection,
    pub shortcuts: ShortcutSection,
    #[cfg(feature = "status-bar")]
    pub status: StatusSection,
    pub window_title_section: Line,
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
                    .contains(x)
                    .then(|| MouseTarget::WindowTitle)
            })
        }
        #[cfg(not(feature = "status-bar"))]
        {
            hit.or_else(|| {
                self.window_title_section
                    .contains(x)
                    .then(|| MouseTarget::WindowTitle)
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
            window_title_section: Line::new(
                workspace.position.start + workspace.position.length,
                title_width,
            ),
            workspace,
            shortcuts,
            #[cfg(feature = "status-bar")]
            status,
        }
    }
}

pub struct ShortcutSection {
    pub position: Line,
    pub components: Vec<ShortcutComponent>,
}

#[derive(Debug)]
pub struct ShortcutComponent {
    pub position: Line,
    pub write_offset: i16,
    pub text: String,
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
                            .then(|| MouseTarget::ShortcutComponent(ind))
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
    pub components: heapless::CopyVec<StatusComponent, STATUS_BAR_UNIQUE_CHECK_LIMIT>,
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
        let mut corrected_lengths: heapless::CopyVec<i16, STATUS_BAR_UNIQUE_CHECK_LIMIT> =
            heapless::CopyVec::new();
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
        let mut components = heapless::CopyVec::new();
        let start = mon_width - right_offset - total_length;
        let mut offset = 0;
        for length in corrected_lengths {
            let _ = components.push(StatusComponent {
                position: Line {
                    start: start + offset,
                    length,
                },
                display: Default::default(),
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
        new_content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        new_component_ind: usize,
    ) -> (heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>, Line) {
        let content = if new_component_ind == 0 {
            format_heapless!("{STATUS_BAR_FIRST_SEP}{new_content}")
        } else if new_component_ind == self.components.len() - 1 {
            format_heapless!("{STATUS_BAR_CHECK_SEP}{new_content}{STATUS_BAR_FIRST_SEP}  ")
        } else {
            format_heapless!("{STATUS_BAR_CHECK_SEP}{new_content}")
        };
        let component = &mut self.components[new_component_ind];
        component.display = content;
        (content, component.position)
    }

    #[must_use]
    pub fn get_full_content(&self) -> heapless::String<STATUS_BAR_TOTAL_LENGTH_LIMIT> {
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
                            .then(|| MouseTarget::StatusComponent(ind))
                    })
            })
            .flatten()
    }
}

#[cfg(feature = "status-bar")]
#[derive(Debug, Copy, Clone)]
pub struct StatusComponent {
    pub position: Line,
    pub display: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
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
                            .then(|| MouseTarget::WorkspaceBarComponent(ind))
                    })
            })
            .flatten()
    }
}
pub struct FixedDisplayComponent {
    pub position: Line,
    pub write_offset: i16,
    pub text: String,
}
