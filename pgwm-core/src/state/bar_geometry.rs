use crate::config::mouse_map::MouseTarget;
#[cfg(feature = "status-bar")]
use crate::config::{
    STATUS_BAR_CHECK_CONTENT_LIMIT, STATUS_BAR_CHECK_SEP, STATUS_BAR_FIRST_SEP,
    STATUS_BAR_TOTAL_LENGTH_LIMIT, STATUS_BAR_UNIQUE_CHECK_LIMIT,
};
use crate::geometry::Line;

pub struct BarGeometry {
    pub workspace: WorkspaceSection,
    pub shortcuts: ShortcutSection,
    #[cfg(feature = "status-bar")]
    pub status: StatusSection,
    pub window_title_start: i16,
}

impl BarGeometry {
    #[must_use]
    pub fn hit_on_click(&self, mon_width: i16, x: i16) -> Option<MouseTarget> {
        let hit = self
            .workspace
            .hit_component(x)
            .or_else(|| self.shortcuts.hit_component(mon_width, x));
        #[cfg(feature = "status-bar")]
        {
            hit.or_else(|| self.status.hit_component(mon_width, x))
                .or_else(|| {
                    (x >= self.window_title_start
                        && x <= self.window_title_start
                            + (mon_width
                                - self.status.width
                                - self.window_title_start
                                - self.shortcuts.width))
                        .then(|| MouseTarget::WindowTitle)
                })
        }
        #[cfg(not(feature = "status-bar"))]
        {
            hit.or_else(|| {
                (x >= self.window_title_start
                    && x <= self.window_title_start
                        + (mon_width - self.window_title_start - self.shortcuts.width))
                    .then(|| MouseTarget::WindowTitle)
            })
        }
    }

    #[must_use]
    pub fn calculate_window_title_position(&self, mon_width: i16) -> Line {
        #[cfg(feature = "status-bar")]
        let length = mon_width - self.status.width - self.shortcuts.width - self.window_title_start;
        #[cfg(not(feature = "status-bar"))]
        let length = mon_width - self.shortcuts.width - self.window_title_start;
        Line::new(self.window_title_start, length)
    }

    #[must_use]
    #[cfg(feature = "status-bar")]
    pub fn calculate_status_position(&self, mon_width: i16) -> Line {
        Line::new(
            mon_width - self.status.width - self.shortcuts.width,
            self.status.width,
        )
    }

    #[must_use]
    pub fn calculate_shortcuts_position(&self, mon_width: i16) -> Line {
        let line = Line::new(mon_width - self.shortcuts.width, self.shortcuts.width);
        crate::debug!(
            "Calculated line at {line:?}, {:?}",
            self.shortcuts.components
        );
        line
    }

    #[must_use]
    pub fn new(
        workspace: WorkspaceSection,
        shortcuts: ShortcutSection,
        #[cfg(feature = "status-bar")] status: StatusSection,
    ) -> Self {
        Self {
            window_title_start: workspace.position.start + workspace.position.length,
            workspace,
            shortcuts,
            #[cfg(feature = "status-bar")]
            status,
        }
    }
}

pub struct ShortcutSection {
    pub width: i16,
    pub components: Vec<ShortcutComponent>,
}

#[derive(Debug)]
pub struct ShortcutComponent {
    pub width: i16,
    pub write_offset: i16,
    pub text: String,
}

impl ShortcutSection {
    fn hit_component(&self, mon_width: i16, x: i16) -> Option<MouseTarget> {
        let start = mon_width - self.width;
        if x >= start && x <= start + self.width {
            let mut offset = start;
            for (ind, component) in self.components.iter().enumerate() {
                if x >= offset && x <= offset + component.width {
                    return Some(MouseTarget::ShortcutComponent(ind));
                }
                offset += component.width;
            }
        }
        None
    }
}

#[cfg(feature = "status-bar")]
pub struct StatusSection {
    pub width: i16,
    pub first_sep_len: i16,
    pub sep_len: i16,
    pub components: heapless::CopyVec<StatusComponent, STATUS_BAR_UNIQUE_CHECK_LIMIT>,
}

#[cfg(feature = "status-bar")]
impl StatusSection {
    #[must_use]
    pub fn new(num_components: usize, sep_len: i16, first_sep_len: i16) -> Self {
        Self {
            width: 0,
            sep_len,
            first_sep_len,
            components: std::iter::repeat(StatusComponent {
                width: 0,
                display: heapless::String::from(""),
            })
            .take(num_components)
            .collect(),
        }
    }
    pub fn update_section_widths(
        &mut self,
        new_content: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT>,
        new_component_width: i16,
        new_component_ind: usize,
    ) -> bool {
        if self.components[new_component_ind].display == new_content {
            return false;
        }
        let last = self.components.len() - 1;
        let component = &mut self.components[new_component_ind];
        let old_width = component.width;
        component.display = new_content;
        if old_width == new_component_width {
            false
        } else {
            if new_component_ind == 0 {
                component.width = new_component_width + self.first_sep_len;
            } else if new_component_ind == last {
                component.width = new_component_width + self.sep_len + self.first_sep_len;
            } else {
                component.width = new_component_width + self.sep_len;
            }
            self.width = self.components.iter().map(|cmp| cmp.width).sum::<i16>();
            true
        }
    }

    #[must_use]
    pub fn get_content_as_str(&self) -> heapless::String<STATUS_BAR_TOTAL_LENGTH_LIMIT> {
        let mut s = heapless::String::from(STATUS_BAR_FIRST_SEP);
        let last = self.components.len() - 1;

        self.components
            .iter()
            .filter_map(|cnt| (!cnt.display.is_empty()).then(|| &cnt.display))
            .enumerate()
            .for_each(|(ind, content)| {
                if ind == 0 {
                    let _ = s.push_str(content);
                } else if ind == last {
                    let formatted: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT> =
                        crate::format_heapless!("{STATUS_BAR_CHECK_SEP}{content}{STATUS_BAR_FIRST_SEP}  ");
                    let _ = s.push_str(&formatted);
                } else {
                    let formatted: heapless::String<STATUS_BAR_CHECK_CONTENT_LIMIT> =
                        crate::format_heapless!("{STATUS_BAR_CHECK_SEP}{content}");
                    let _ = s.push_str(&formatted);
                }
            });

        s
    }

    #[must_use]
    fn hit_component(&self, mon_width: i16, x: i16) -> Option<MouseTarget> {
        crate::debug!(
            "click on mon with width {mon_width} at {x}\n{:?}",
            self.components
        );
        let start_x = mon_width - self.width;
        (x >= start_x && x <= start_x + self.width)
            .then(|| {
                let mut offset = start_x;
                for (ind, component) in self.components.iter().enumerate() {
                    if x >= offset && x <= offset + component.width {
                        return Some(MouseTarget::StatusComponent(ind));
                    }
                    offset += component.width;
                }
                None
            })
            .flatten()
    }
}

#[cfg(feature = "status-bar")]
#[derive(Debug, Copy, Clone)]
pub struct StatusComponent {
    pub width: i16,
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
