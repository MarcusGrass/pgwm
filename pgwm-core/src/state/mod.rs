pub mod bar_geometry;
pub mod workspace;

use crate::error::Result;
use heapless::binary_heap::Min;
use std::collections::HashMap;
use std::ops::Sub;
use std::{
    ops::Add,
    time::{Duration, SystemTime},
};

use x11rb::protocol::xproto::{Screen, Window};

use crate::config::key_map::KeyBoardMappingKey;
use crate::config::mouse_map::{MouseActionKey, MouseTarget};
use crate::config::Action;
use crate::geometry::draw::Mode;
use crate::geometry::Dimensions;
use crate::render::DoubleBufferedRenderPicture;
use crate::state::bar_geometry::BarGeometry;
use crate::{
    colors::Colors,
    config::{APPLICATION_WINDOW_LIMIT, BINARY_HEAP_LIMIT, DYING_WINDOW_CACHE},
    state::workspace::{ManagedWindow, Workspaces},
};

#[allow(clippy::struct_excessive_bools)]
pub struct State {
    pub wm_check_win: Window,
    pub intern_created_windows: heapless::FnvIndexSet<Window, APPLICATION_WINDOW_LIMIT>,
    pub dying_windows: heapless::Vec<WinMarkedForDeath, DYING_WINDOW_CACHE>,
    pub drag_window: Option<(Window, DragPosition)>,
    pub focused_mon: usize,
    pub input_focus: Option<Window>,
    pub screen: Screen,
    pub sequences_to_ignore: heapless::BinaryHeap<u16, Min, BINARY_HEAP_LIMIT>,
    pub monitors: Vec<Monitor>,
    pub workspaces: Workspaces,
    pub colors: Colors,
    pub window_border_width: u32,
    pub status_bar_height: i16,
    pub tab_bar_height: i16,
    pub window_padding: i16,
    pub pad_while_tabbed: bool,
    pub workspace_bar_window_name_padding: u16,
    pub cursor_name: String,
    pub pointer_grabbed: bool,
    pub destroy_after: u64,
    pub kill_after: u64,
    pub show_bar_initially: bool,
    pub mouse_mapping: HashMap<MouseActionKey, Action>,
    pub key_mapping: HashMap<KeyBoardMappingKey, Action>,
}

impl State {
    pub fn push_sequence(&mut self, sequence: u16) {
        let _ = self.sequences_to_ignore.push(sequence);
    }

    /// In libX11 you can drain response-events to some sent events, such as a `MapNotify` after a `MapRequest`
    /// As far as I know this isn't an option so we have to "blacklist" events that are caused by
    /// events we produce. It's a coarse way of doing it and can produce bugs.
    pub fn should_ignore_sequence(&mut self, sequence: u16) -> bool {
        let mut should_ignore = false;
        while let Some(to_ignore) = self.sequences_to_ignore.peek() {
            // Sequence numbers can wrap around, so we cannot simply check for
            // "to_ignore <= seqno". This is equivalent to "to_ignore - seqno <= 0", which is what we
            // check instead. Since sequence numbers are unsigned, we need a trick: We decide
            // that values from [MAX/2, MAX] count as "<= 0" and the rest doesn't.
            if (*to_ignore).wrapping_sub(sequence) <= u16::MAX / 2 {
                // If the two sequence numbers are equal, this event should be ignored.
                should_ignore = *to_ignore == sequence;
                break;
            }
            self.sequences_to_ignore.pop();
        }
        should_ignore
    }

    /// Unless you're using a mad amount of monitors this will be fast
    #[must_use]
    pub fn find_monitor_focusing_window(&self, window: Window) -> Option<usize> {
        for (i, mon) in self.monitors.iter().enumerate() {
            if mon.last_focus.filter(|mw| mw.window == window).is_some() {
                return Some(i);
            }
        }
        None
    }

    #[must_use]
    pub fn find_monitor_hosting_workspace(&self, ws_ind: usize) -> Option<usize> {
        for (i, mon) in self.monitors.iter().enumerate() {
            if mon.hosted_workspace == ws_ind {
                return Some(i);
            }
        }
        None
    }

    #[must_use]
    pub fn find_monitor_index_of_window(&self, window: Window) -> Option<usize> {
        self.workspaces
            .find_ws_containing_window(window)
            .and_then(|ws_ind| self.find_monitor_hosting_workspace(ws_ind))
    }

    #[must_use]
    pub fn find_monitor_and_ws_indices_of_window(&self, window: Window) -> Option<(usize, usize)> {
        if let Some(ws_ind) = self.workspaces.find_ws_containing_window(window) {
            if let Some(mon_ind) = self.find_monitor_hosting_workspace(ws_ind) {
                return Some((mon_ind, ws_ind));
            }
        }
        None
    }

    #[must_use]
    pub fn find_monitor_at(&self, origin: (i16, i16)) -> Option<usize> {
        for i in 0..self.monitors.len() {
            let dimensions = &self.monitors[i].dimensions;
            if origin.0 >= dimensions.x && origin.0 <= dimensions.width + dimensions.x {
                return Some(i);
            }
        }
        None
    }

    pub fn find_first_focus_candidate(&self, mon_ind: usize) -> Result<Option<ManagedWindow>> {
        let mon = &self.monitors[mon_ind];
        if let Some(win) = mon.last_focus {
            Ok(Some(win))
        } else {
            let tiled = self
                .workspaces
                .get_all_tiled_windows(mon.hosted_workspace)?;
            if tiled.is_empty() {
                Ok(None)
            } else {
                Ok(match self.workspaces.get_draw_mode(mon.hosted_workspace) {
                    Mode::Tiled(_) => Some(tiled[0]),
                    Mode::Tabbed(u) => Some(tiled[u]),
                    Mode::Fullscreen { window, .. } => self.workspaces.get_managed_win(window),
                })
            }
        }
    }

    #[must_use]
    pub fn find_appropriate_ws_focus(
        &self,
        mon_ind: usize,
        ws_ind: usize,
    ) -> Option<ManagedWindow> {
        if let Some(currently_focused_window) = self.input_focus {
            if let Some(ws_ind_with_focus) = self
                .workspaces
                .find_ws_containing_window(currently_focused_window)
            {
                if ws_ind_with_focus == ws_ind {
                    return self.workspaces.get_managed_win(currently_focused_window);
                }
            }
        } else if self.monitors[mon_ind].hosted_workspace == ws_ind {
            if let Some(focused_on_mon) = self.monitors[mon_ind].last_focus {
                if let Some(ws_ind_with_focus) = self
                    .workspaces
                    .find_ws_containing_window(focused_on_mon.window)
                {
                    if ws_ind_with_focus == ws_ind {
                        return self.workspaces.get_managed_win(focused_on_mon.window);
                    }
                }
            }
        }
        None
    }

    #[must_use]
    pub fn any_monitors_showing_status(&self) -> bool {
        self.monitors.iter().any(|mon| mon.show_bar)
    }

    #[must_use]
    pub fn get_hit_bar_component(
        &self,
        clicked_win: Window,
        x: i16,
        mon_ind: usize,
    ) -> Option<MouseTarget> {
        let mon = &self.monitors[mon_ind];
        (clicked_win == mon.bar_win.window.drawable)
            .then(|| {
                let rel_x = x - mon.dimensions.x;
                mon.bar_geometry.hit_on_click(rel_x)
            })
            .flatten()
    }

    #[must_use]
    pub fn get_key_action(&self, code: u8, mods: u16) -> Option<&Action> {
        self.key_mapping.get(&KeyBoardMappingKey::new(code, mods))
    }

    #[must_use]
    pub fn get_mouse_action(&self, detail: u8, state: u16, target: MouseTarget) -> Option<&Action> {
        self.mouse_mapping
            .get(&MouseActionKey::new(detail, state, target))
    }

    pub fn update_focused_mon(&mut self, new_focus: usize) -> Option<usize> {
        if self.focused_mon == new_focus {
            None
        } else {
            Some(std::mem::replace(&mut self.focused_mon, new_focus))
        }
    }
}

pub struct Monitor {
    pub bar_win: DoubleBufferedRenderPicture,
    pub tab_bar_win: DoubleBufferedRenderPicture,
    pub bar_geometry: BarGeometry,
    pub dimensions: Dimensions,
    pub hosted_workspace: usize,
    pub last_focus: Option<ManagedWindow>,
    pub show_bar: bool,
    pub window_title_display: heapless::String<256>,
}

#[derive(Copy, Clone)]
pub struct DrawArea {
    pub width: i16,
    pub window: Window,
}
pub struct DragPosition {
    origin_x: i16,
    origin_y: i16,
    event_origin_x: i16,
    event_origin_y: i16,
}

impl DragPosition {
    #[must_use]
    pub fn new(origin_x: i16, origin_y: i16, event_origin_x: i16, event_origin_y: i16) -> Self {
        DragPosition {
            origin_x,
            origin_y,
            event_origin_x,
            event_origin_y,
        }
    }

    #[must_use]
    pub fn current_position(&self, cursor_x: i16, cursor_y: i16) -> (i16, i16) {
        let x = self.origin_x + cursor_x - self.event_origin_x;
        let y = self.origin_y + cursor_y - self.event_origin_y;
        (x, y)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WinMarkedForDeath {
    pub win: Window,
    die_at: SystemTime,
    pub sent_destroy: bool,
}

impl WinMarkedForDeath {
    #[must_use]
    pub fn new(win: Window, destroy_after: u64) -> Self {
        Self {
            win,
            die_at: SystemTime::now().add(Duration::from_millis(destroy_after)),
            sent_destroy: false,
        }
    }
    #[must_use]
    pub fn should_kill(&self, kill_after: u64) -> bool {
        self.sent_destroy && self.die_at <= SystemTime::now().sub(Duration::from_millis(kill_after))
    }
    #[must_use]
    pub fn should_destroy(&self) -> bool {
        self.die_at <= SystemTime::now()
    }
}

#[cfg(test)]
mod tests {
    use crate::colors::{Color, Colors};
    use crate::config::{Cfg, USED_DIFFERENT_COLOR_SEGMENTS};
    use crate::geometry::{Dimensions, Line};
    use crate::render::{DoubleBufferedRenderPicture, RenderPicture};
    use crate::state::bar_geometry::{
        BarGeometry, ShortcutSection, WindowTitleSection, WorkspaceSection,
    };
    use crate::state::workspace::{ArrangeKind, FocusStyle, ManagedWindow, Workspaces};
    use crate::state::{Monitor, State};
    use x11rb::protocol::xproto::{BackingStore, Screen};

    fn create_base_state() -> State {
        let cfg = Cfg::default();
        let monitor0 = Monitor {
            bar_win: DoubleBufferedRenderPicture {
                window: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
                pixmap: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
            },
            tab_bar_win: DoubleBufferedRenderPicture {
                window: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
                pixmap: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
            },
            bar_geometry: BarGeometry {
                workspace: WorkspaceSection {
                    position: Line::new(0, 0),
                    components: vec![],
                },
                shortcuts: ShortcutSection {
                    position: Line::new(0, 0),
                    components: vec![],
                },
                #[cfg(feature = "status-bar")]
                status: crate::state::bar_geometry::StatusSection {
                    position: Line::new(0, 0),
                    first_sep_len: 0,
                    sep_len: 0,
                    components: heapless::Vec::default(),
                },
                window_title_section: WindowTitleSection {
                    position: Line::new(0, 0),
                    display: heapless::String::default(),
                    last_draw_width: 0,
                },
            },
            dimensions: Dimensions::new(1000, 1000, 0, 0),
            hosted_workspace: 0,
            last_focus: None,
            show_bar: false,
            window_title_display: heapless::String::default(),
        };
        let monitor1 = Monitor {
            bar_geometry: BarGeometry {
                workspace: WorkspaceSection {
                    position: Line::new(0, 0),
                    components: vec![],
                },
                shortcuts: ShortcutSection {
                    position: Line::new(0, 0),
                    components: vec![],
                },
                #[cfg(feature = "status-bar")]
                status: crate::state::bar_geometry::StatusSection {
                    position: Line::new(0, 0),
                    first_sep_len: 0,
                    sep_len: 0,
                    components: heapless::Vec::default(),
                },
                window_title_section: WindowTitleSection {
                    position: Line::new(0, 0),
                    display: heapless::String::default(),
                    last_draw_width: 0,
                },
            },
            bar_win: DoubleBufferedRenderPicture {
                window: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
                pixmap: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
            },
            tab_bar_win: DoubleBufferedRenderPicture {
                window: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
                pixmap: RenderPicture {
                    drawable: 0,
                    picture: 0,
                    format: 0,
                },
            },
            dimensions: Dimensions::new(1000, 1000, 1000, 0),
            hosted_workspace: 1,
            last_focus: None,
            show_bar: false,
            window_title_display: heapless::String::default(),
        };
        let pixels: heapless::Vec<Color, USED_DIFFERENT_COLOR_SEGMENTS> =
            std::iter::repeat_with(|| Color {
                pixel: 0,
                bgra8: [0, 0, 0, 0],
            })
            .take(USED_DIFFERENT_COLOR_SEGMENTS)
            .collect();
        State {
            wm_check_win: 0,
            intern_created_windows: heapless::IndexSet::default(),
            dying_windows: heapless::Vec::default(),
            drag_window: None,
            focused_mon: 0,
            input_focus: None,
            screen: Screen {
                root: 0,
                default_colormap: 0,
                white_pixel: 0,
                black_pixel: 0,
                current_input_masks: 0,
                width_in_pixels: 0,
                height_in_pixels: 0,
                width_in_millimeters: 0,
                height_in_millimeters: 0,
                min_installed_maps: 0,
                max_installed_maps: 0,
                root_visual: 0,
                backing_stores: BackingStore::NOT_USEFUL,
                save_unders: false,
                root_depth: 0,
                allowed_depths: vec![],
            },
            sequences_to_ignore: heapless::BinaryHeap::default(),
            monitors: vec![monitor0, monitor1],
            workspaces: Workspaces::create_empty(&cfg.workspaces, cfg.tiling_modifiers).unwrap(),
            colors: Colors::from_vec(pixels),
            window_border_width: 0,
            status_bar_height: 0,
            tab_bar_height: 0,
            window_padding: 0,
            pad_while_tabbed: false,
            workspace_bar_window_name_padding: 0,
            cursor_name: String::new(),
            pointer_grabbed: false,
            destroy_after: 0,
            kill_after: 0,
            show_bar_initially: true,
            mouse_mapping: std::collections::HashMap::default(),
            key_mapping: std::collections::HashMap::default(),
        }
    }

    #[test]
    fn can_find_monitor_from_different_sources() {
        let mut state = create_base_state();
        state.focused_mon = 0;
        assert!(state.find_monitor_index_of_window(0).is_none());
        assert!(state.find_monitor_and_ws_indices_of_window(0).is_none());
        assert!(state.find_monitor_hosting_workspace(2).is_none());
        assert!(state.find_monitor_focusing_window(0).is_none());
        assert_eq!(0, state.find_monitor_hosting_workspace(0).unwrap());
        assert_eq!(1, state.find_monitor_hosting_workspace(1).unwrap());
        state
            .workspaces
            .add_child_to_ws(15, 0, ArrangeKind::NoFloat, FocusStyle::Pull)
            .unwrap();
        assert!(state.find_monitor_focusing_window(15).is_none());
        state.monitors[0].last_focus = Some(ManagedWindow::new(
            15,
            ArrangeKind::NoFloat,
            FocusStyle::Pull,
        ));
        assert_eq!(0, state.find_monitor_focusing_window(15).unwrap());
        assert_eq!(0, state.find_monitor_index_of_window(15).unwrap());
        assert_eq!(
            Some((0, 0)),
            state.find_monitor_and_ws_indices_of_window(15)
        );
        assert_eq!(0, state.find_monitor_at((0, 0)).unwrap());
        // Defaults to 0
        assert!(state.find_monitor_at((9999, 9999)).is_none());
        assert_eq!(0, state.find_monitor_at((1000, 0)).unwrap());
        assert_eq!(0, state.find_monitor_at((1000, 1000)).unwrap());
        assert_eq!(1, state.find_monitor_at((1001, 0)).unwrap());
        assert_eq!(1, state.find_monitor_at((2000, 0)).unwrap());
        assert!(state.find_monitor_at((2001, 0)).is_none());
    }

    #[test]
    fn will_ignore_sequences() {
        // Wrapping ignores sequences which always increase linearly (not considering wrapping)
        let mut state = create_base_state();
        state.push_sequence(55);
        // Only ignores specific sequence
        assert!(!state.should_ignore_sequence(54));
        assert!(state.should_ignore_sequence(55));
        // Sequence numbers can be shared so it should keep ignoring
        assert!(state.should_ignore_sequence(55));
        assert!(!state.should_ignore_sequence(56));
        // When processing a sequence with a higher number we dropped the lower one the prevent leakage
        assert!(!state.should_ignore_sequence(55));
    }
}
