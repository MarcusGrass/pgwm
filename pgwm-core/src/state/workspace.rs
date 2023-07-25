use alloc::vec::Vec;

use smallmap::Map;
use xcb_rust_protocol::proto::xproto::Window;

use crate::config::workspaces::UserWorkspace;
use crate::config::{DefaultDraw, APPLICATION_WINDOW_LIMIT, WS_WINDOW_LIMIT, TilingModifiers, TILING_MODIFIERS};
use crate::error::Result;
use crate::geometry::draw::{Mode, OldDrawMode};
use crate::geometry::layout::Layout;
use crate::state::properties::WindowProperties;
use crate::util::vec_ops::push_to_front;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Workspaces {
    // Hot read
    spaces: Vec<Workspace>,
    // Hot on read/write
    win_to_ws: Map<Window, usize>,
    // Hot read
    name_to_ws: Map<&'static str, usize>,
}

impl Workspaces {
    pub fn create_empty(
        init_workspaces: &[UserWorkspace],
    ) -> Result<Self> {
        let mut v = Vec::<Workspace>::new();
        let mut name_to_ws = Map::new();
        for (i, ws) in init_workspaces.iter().enumerate() {
            v.push(Workspace {
                draw_mode: match ws.default_draw {
                    DefaultDraw::LeftLeader => Mode::Tiled(Layout::LeftLeader),
                    DefaultDraw::CenterLeader => Mode::Tiled(Layout::CenterLeader),
                    DefaultDraw::Tabbed => Mode::Tabbed(0),
                },
                name: ws.name,
                children: heapless::Vec::new(), // Realloc is what's going to take time here
                tiling_modifiers: TILING_MODIFIERS,
            });
            for mapped in ws.mapped_class_names {
                name_to_ws.insert(*mapped, i);
            }
        }
        Ok(Workspaces {
            spaces: v,
            win_to_ws: Map::new(),
            name_to_ws,
        })
    }

    #[must_use]
    pub fn get_all_managed_windows(&self) -> heapless::Vec<Window, APPLICATION_WINDOW_LIMIT> {
        self.win_to_ws.keys().copied().collect()
    }

    pub fn iter_all_managed_windows_in_ws(
        &self,
        ws_ind: usize,
    ) -> impl Iterator<Item = &ManagedWindow> {
        self.spaces[ws_ind].iter_all_windows()
    }

    #[must_use]
    pub fn find_ws_for_window_class_name(&self, wm_class: &str) -> Option<usize> {
        self.name_to_ws.get(wm_class).copied()
    }

    #[must_use]
    pub fn get_ws(&self, num: usize) -> &Workspace {
        &self.spaces[num]
    }

    #[must_use]
    pub fn get_managed_win(&self, window: Window) -> Option<&ManagedWindow> {
        self.win_to_ws
            .get(&window)
            .and_then(|ws_ind| self.spaces[*ws_ind].find_managed_window(window))
    }

    #[must_use]
    pub fn get_managed_win_mut(&mut self, window: Window) -> Option<&mut ManagedWindow> {
        self.win_to_ws
            .get_mut(&window)
            .and_then(|ws_ind| self.spaces[*ws_ind].find_managed_window_mut(window))
    }

    pub fn update_size_modifier(&mut self, window: Window, resize: f32) -> bool {
        self.win_to_ws.get(&window).map_or(false, |ws_ind| {
            let ws = &mut self.spaces[*ws_ind];
            ws.resize_children(window, resize)
        })
    }

    pub fn clear_size_modifiers(&mut self, ws_ind: usize) {
        self.spaces[ws_ind].tiling_modifiers = TILING_MODIFIERS;
    }

    pub fn unset_fullscreened(&mut self, ws_ind: usize) -> Option<Window> {
        let ws = &mut self.spaces[ws_ind];
        let dm = ws.draw_mode;
        if let Mode::Fullscreen {
            last_draw_mode,
            window,
        } = dm
        {
            ws.draw_mode = last_draw_mode.to_draw_mode();
            // If it's not managed we want to remove it from the ws mapping to avoid a memory leak
            if ws.find_managed_window(window).is_none() {
                self.win_to_ws.remove(&window);
            }
            return Some(window);
        }
        None
    }

    pub fn set_fullscreened(&mut self, ws_ind: usize, window: Window) -> Result<Option<Window>> {
        // We want to be able to track if a ws owns a fullscreened window even if it's not managed
        self.win_to_ws.insert(window, ws_ind);
        let ws = &mut self.spaces[ws_ind];
        let dm = ws.draw_mode;
        let (new_mode, old_fullscreen) = match dm {
            Mode::Tiled(_) | Mode::Tabbed(_) => (
                Mode::Fullscreen {
                    window,
                    last_draw_mode: OldDrawMode::from_draw_mode(dm)?,
                },
                None,
            ),
            Mode::Fullscreen {
                window: old_win,
                last_draw_mode,
            } => {
                if old_win == window {
                    (dm, None)
                } else {
                    (
                        Mode::Fullscreen {
                            window,
                            last_draw_mode,
                        },
                        Some(old_win),
                    )
                }
            }
        };
        self.spaces[ws_ind].draw_mode = new_mode;
        Ok(old_fullscreen)
    }

    #[must_use]
    pub fn get_wants_focus_workspaces(&self) -> Vec<bool> {
        self.spaces
            .iter()
            .map(|ws| ws.iter_all_windows().any(|ch| ch.wants_focus))
            .collect()
    }

    pub fn set_wants_focus(&mut self, window: Window, wants_focus: bool) -> Option<(usize, bool)> {
        self.win_to_ws.get(&window).and_then(|ind| {
            self.spaces[*ind]
                .set_wants_focus(window, wants_focus)
                .map(|b| (*ind, b))
        })
    }

    pub fn set_draw_mode(&mut self, num: usize, draw_mode: Mode) -> bool {
        let ws = &mut self.spaces[num];
        if ws.draw_mode == draw_mode {
            false
        } else {
            ws.draw_mode = draw_mode;
            true
        }
    }

    pub fn cycle_tiling_mode(&mut self, num: usize) {
        let ws = &mut self.spaces[num];
        if let Mode::Tiled(layout) = ws.draw_mode {
            ws.draw_mode = Mode::Tiled(layout.next());
        }
    }

    pub fn switch_tab_focus_index(&mut self, num: usize, focus: usize) -> bool {
        let ws = &mut self.spaces[num];
        if let Mode::Tabbed(n) = ws.draw_mode {
            ws.draw_mode = Mode::Tabbed(focus);
            n != focus
        } else {
            panic!("Switching tab-focus on untabbed child");
        }
    }

    pub fn switch_tab_focus_window(&mut self, num: usize, window: Window) -> Result<Option<bool>> {
        let ws = &mut self.spaces[num];
        if let Mode::Tabbed(_) = ws.draw_mode {
            if let Some(pos) = ws.tiling_index_of(window) {
                let new = Mode::Tabbed(pos);
                let changed = ws.draw_mode != new;
                ws.draw_mode = new;
                Ok(Some(changed))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    pub fn send_window_to_front(&mut self, num: usize, win: Window) {
        self.spaces[num].send_to_front(win);
    }

    pub fn toggle_floating(&mut self, window: Window, num: usize, floating: ArrangeKind) -> bool {
        if let Some(mw) = self.spaces[num]
            .iter_all_windows_mut()
            .find(|mw| mw.window == window)
        {
            if mw.arrange == floating {
                false
            } else {
                pgwm_utils::debug!("Floating {}", mw.window);
                mw.arrange = floating;
                true
            }
        } else {
            false
        }
    }

    pub fn add_child_to_ws(
        &mut self,
        window: Window,
        num: usize,
        arrange: ArrangeKind,
        focus_style: FocusStyle,
        properties: &WindowProperties,
    ) -> Result<()> {
        self.win_to_ws.insert(window, num);
        self.spaces[num].add_child(window, arrange, focus_style, properties.clone())
    }

    pub fn add_attached(
        &mut self,
        parent: Window,
        attached: Window,
        arrange: ArrangeKind,
        focus_style: FocusStyle,
        properties: &WindowProperties,
    ) -> Result<bool> {
        if let Some(ws) = self.win_to_ws.get(&parent).copied() {
            self.win_to_ws.insert(attached, ws);
            self.spaces[ws].add_attached(
                parent,
                attached,
                arrange,
                focus_style,
                properties.clone(),
            )?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[must_use]
    pub fn find_all_attached_managed(
        &self,
        parent: Window,
    ) -> Option<&heapless::Vec<ManagedWindow, WS_WINDOW_LIMIT>> {
        self.win_to_ws
            .get(&parent)
            .and_then(|ws| self.spaces[*ws].find_all_attached_managed(parent))
    }

    pub fn un_float_window(&mut self, window: Window) -> Option<bool> {
        if let Some(ws) = self.win_to_ws.get(&window).copied() {
            self.spaces[ws].un_float(window)
        } else {
            None
        }
    }

    pub fn delete_child_from_ws(&mut self, window: Window) -> DeleteResult {
        self.win_to_ws
            .remove(&window)
            .map_or(DeleteResult::None, |ind| {
                if let Some(ws_child) = self.spaces[ind]
                    .children
                    .iter()
                    .find(|ch| ch.managed.window == window)
                {
                    for child in &ws_child.attached {
                        self.win_to_ws.remove(&child.window);
                    }
                }
                let dr = self.spaces[ind].delete_child(window);
                // We need to remove fullscreen status If the window was fullscreened
                // or else we'll have a bug
                if let Mode::Fullscreen {
                    window: fs_window,
                    last_draw_mode,
                } = self.spaces[ind].draw_mode
                {
                    if fs_window == window {
                        self.spaces[ind].draw_mode = last_draw_mode.to_draw_mode();
                    }
                };
                dr
            })
    }

    #[must_use]
    pub fn find_ws_containing_window(&self, window: Window) -> Option<usize> {
        self.win_to_ws.get(&window).copied()
    }

    #[must_use]
    pub fn is_managed_floating(&self, win: Window) -> bool {
        if let Some(ind) = self.win_to_ws.get(&win) {
            self.spaces[*ind].is_floating(win)
        } else {
            false
        }
    }

    #[must_use]
    pub fn is_managed_tiled(&self, win: Window) -> bool {
        if let Some(ind) = self.win_to_ws.get(&win) {
            !self.spaces[*ind].is_floating(win)
        } else {
            false
        }
    }

    #[must_use]
    pub fn find_first_tiled(&self, num: usize) -> Option<Window> {
        self.spaces[num]
            .iter_all_windows()
            .find_map(|ch| (ch.arrange == ArrangeKind::NoFloat).then_some(ch.window))
    }

    #[must_use]
    pub fn get_all_tiled_windows(
        &self,
        num: usize,
    ) -> heapless::Vec<&ManagedWindow, WS_WINDOW_LIMIT> {
        self.spaces[num]
            .iter_all_windows()
            .filter(|mw| mw.arrange == ArrangeKind::NoFloat)
            .collect()
    }

    pub fn tab_focus_window(&mut self, window: Window) -> bool {
        self.win_to_ws
            .get(&window)
            .map_or(false, |ind| self.spaces[*ind].tab_focus_window(window))
    }

    #[must_use]
    pub fn next_window(&self, cur: Window) -> Option<&ManagedWindow> {
        self.win_to_ws
            .get(&cur)
            .copied()
            .and_then(|ws| self.spaces[ws].find_next(cur))
    }

    #[must_use]
    pub fn prev_window(&self, cur: Window) -> Option<&ManagedWindow> {
        self.win_to_ws
            .get(&cur)
            .copied()
            .and_then(|ws| self.spaces[ws].find_prev(cur))
    }

    #[must_use]
    pub fn get_draw_mode(&self, num: usize) -> Mode {
        let ws = &self.spaces[num];
        if let Mode::Tabbed(n) = ws.draw_mode {
            let visible_children = ws.num_tiled();
            if visible_children <= n {
                Mode::Tabbed(0)
            } else {
                Mode::Tabbed(n)
            }
        } else {
            ws.draw_mode
        }
    }

    pub fn update_focus_style(&mut self, focus_style: FocusStyle, win: Window) {
        if let Some(mw) = self
            .win_to_ws
            .get(&win)
            .and_then(|ind| self.spaces[*ind].find_managed_window_mut(win))
        {
            mw.focus_style = focus_style;
        }
    }
}

#[derive(Debug)]
pub enum DeleteResult {
    TiledTopLevel(ManagedWindow),
    FloatingTopLevel(ManagedWindow),
    AttachedFloating((Window, ManagedWindow)),
    AttachedTiled((Window, ManagedWindow)),
    None,
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Workspace {
    pub draw_mode: Mode,
    pub name: &'static str,
    // Actually fine, searching small vectors is extremely efficient, as long as we don't need to
    // realloc
    pub children: heapless::Vec<Child, WS_WINDOW_LIMIT>,
    pub tiling_modifiers: TilingModifiers,
}

impl Workspace {
    fn add_child(
        &mut self,
        window: Window,
        arrange: ArrangeKind,
        focus_style: FocusStyle,
        properties: WindowProperties,
    ) -> Result<()> {
        pgwm_utils::debug!("Adding child to ws: win = {} {:?}", window, arrange,);
        for child in &mut self.children {
            if child.managed.window == window {
                child.managed.arrange = arrange;
                return Ok(());
            }
        }
        push_to_front(
            &mut self.children,
            Child {
                managed: ManagedWindow {
                    window,
                    wants_focus: false,
                    arrange,
                    focus_style,
                    properties,
                },
                attached: heapless::Vec::new(),
            },
        )
    }

    fn iter_all_windows(&self) -> impl Iterator<Item = &ManagedWindow> {
        self.children
            .iter()
            .flat_map(|ch| core::iter::once(&ch.managed).chain(ch.attached.iter()))
    }

    fn iter_all_windows_mut(&mut self) -> impl Iterator<Item = &mut ManagedWindow> {
        self.children
            .iter_mut()
            .flat_map(|ch| core::iter::once(&mut ch.managed).chain(ch.attached.iter_mut()))
    }

    fn resize_children(&mut self, window: Window, resize: f32) -> bool {
        let ind = self.tiling_index_of(window);
        if let Some(index) = ind {
            match self.draw_mode {
                Mode::Tiled(Layout::LeftLeader) => {
                    if index == 0 {
                        self.tiling_modifiers.left_leader =
                            resize_safe(self.tiling_modifiers.left_leader, resize);
                        true
                    } else {
                        self.tiling_modifiers.vertically_tiled[index - 1] =
                            resize_safe(self.tiling_modifiers.vertically_tiled[index - 1], resize);
                        true
                    }
                }
                Mode::Tiled(Layout::CenterLeader) => {
                    if index == 0 {
                        self.tiling_modifiers.center_leader =
                            resize_safe(self.tiling_modifiers.center_leader, resize);
                        true
                    } else {
                        self.tiling_modifiers.vertically_tiled[index - 1] =
                            resize_safe(self.tiling_modifiers.vertically_tiled[index - 1], resize);
                        true
                    }
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn tiling_index_of(&self, window: Window) -> Option<usize> {
        self.iter_all_windows()
            .filter(|mw| mw.arrange == ArrangeKind::NoFloat)
            .position(|w| w.window == window)
    }

    fn add_attached(
        &mut self,
        parent: Window,
        attached: Window,
        arrange: ArrangeKind,
        focus_style: FocusStyle,
        properties: WindowProperties,
    ) -> Result<()> {
        if let Some(ind) = self
            .children
            .iter()
            .position(|ch| ch.managed.window == parent)
        {
            let ws_child = &mut self.children[ind];
            for att in &mut ws_child.attached {
                if att.window == attached {
                    att.arrange = arrange;
                    return Ok(());
                }
            }
            push_to_front(
                &mut ws_child.attached,
                ManagedWindow {
                    window: attached,
                    wants_focus: false,
                    arrange,
                    focus_style,
                    properties,
                },
            )?;
        }
        Ok(())
    }

    fn find_all_attached_managed(
        &self,
        parent_window: Window,
    ) -> Option<&heapless::Vec<ManagedWindow, WS_WINDOW_LIMIT>> {
        self.children
            .iter()
            .find(|ch| ch.managed.window == parent_window && !ch.attached.is_empty())
            .map(|ch| &ch.attached)
    }

    fn un_float(&mut self, window: Window) -> Option<bool> {
        self.iter_all_windows_mut().find_map(|mw| {
            (mw.window == window).then(|| {
                if mw.arrange == ArrangeKind::NoFloat {
                    false
                } else {
                    mw.arrange = ArrangeKind::NoFloat;
                    true
                }
            })
        })
    }

    fn find_managed_window(&self, window: Window) -> Option<&ManagedWindow> {
        self.children.iter().find_map(|ch| {
            if ch.managed.window == window {
                Some(&ch.managed)
            } else {
                ch.attached.iter().find(|tr| tr.window == window)
            }
        })
    }

    fn find_managed_window_mut(&mut self, window: Window) -> Option<&mut ManagedWindow> {
        self.children.iter_mut().find_map(|ch| {
            if ch.managed.window == window {
                Some(&mut ch.managed)
            } else {
                ch.attached.iter_mut().find(|tr| tr.window == window)
            }
        })
    }

    fn delete_child(&mut self, window: Window) -> DeleteResult {
        if let Some(ind) = self
            .children
            .iter()
            .position(|ch| ch.managed.window == window)
        {
            let ch = crate::util::vec_ops::remove(&mut self.children, ind);
            if ch.managed.arrange == ArrangeKind::NoFloat {
                DeleteResult::TiledTopLevel(ch.managed)
            } else {
                DeleteResult::FloatingTopLevel(ch.managed)
            }
        } else {
            for child in &mut self.children {
                if let Some(ind) = child.attached.iter().position(|tr| tr.window == window) {
                    let mw = crate::util::vec_ops::remove(&mut child.attached, ind);
                    pgwm_utils::debug!("Removed attached from ws {:?}", child);
                    return if mw.arrange == ArrangeKind::NoFloat {
                        DeleteResult::AttachedTiled((child.managed.window, mw))
                    } else {
                        DeleteResult::AttachedFloating((child.managed.window, mw))
                    };
                }
            }
            DeleteResult::None
        }
    }

    fn is_floating(&self, window: Window) -> bool {
        self.iter_all_windows()
            .any(|ch| ch.window == window && ch.arrange != ArrangeKind::NoFloat)
    }

    fn set_wants_focus(&mut self, window: Window, wants_focus: bool) -> Option<bool> {
        self.children.iter_mut().find_map(|ch| {
            if ch.managed.window == window {
                let changed = Some(ch.managed.wants_focus != wants_focus);
                ch.managed.wants_focus = wants_focus;
                changed
            } else {
                ch.attached.iter_mut().find_map(|tr| {
                    if tr.window == window {
                        let changed = Some(tr.wants_focus != wants_focus);
                        tr.wants_focus = wants_focus;
                        changed
                    } else {
                        Some(false)
                    }
                })
            }
        })
    }

    fn send_to_front(&mut self, window: Window) {
        if let Some(old_ind) = self.children.iter().position(|ch| {
            ch.managed.window == window && matches!(ch.managed.arrange, ArrangeKind::NoFloat)
        }) {
            self.children.swap(old_ind, 0);
        }
    }

    fn find_next(&self, cur: Window) -> Option<&ManagedWindow> {
        let all = self
            .iter_all_windows()
            .collect::<heapless::Vec<&ManagedWindow, WS_WINDOW_LIMIT>>();
        let len = all.len();
        if len == 1 {
            return None;
        }
        all.iter()
            .position(|ch| ch.window == cur)
            .and_then(|ind| all.get((ind + 1) % len))
            .copied()
    }

    fn find_prev(&self, cur: Window) -> Option<&ManagedWindow> {
        let all = self
            .iter_all_windows()
            .collect::<heapless::Vec<&ManagedWindow, WS_WINDOW_LIMIT>>();
        let len = all.len();
        if len == 1 {
            return None;
        }
        all.iter()
            .position(|ch| ch.window == cur)
            .and_then(|ind| all.get((ind as i16 - 1).rem_euclid(len as i16) as usize))
            .copied()
    }

    fn tab_focus_window(&mut self, focus: Window) -> bool {
        if matches!(self.draw_mode, Mode::Tabbed(_)) {
            if let Some(ind) = self
                .children
                .iter()
                .position(|ch| ch.managed.window == focus)
            {
                self.draw_mode = Mode::Tabbed(ind);
                return true;
            }
        }
        false
    }

    fn num_tiled(&self) -> usize {
        self.iter_all_windows()
            .filter(|ch| ch.arrange == ArrangeKind::NoFloat)
            .count()
    }
}

#[inline]
fn resize_safe(old: f32, diff: f32) -> f32 {
    let new = old + diff;
    if new <= 0.0 {
        old
    } else {
        new
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Child {
    pub managed: ManagedWindow,
    // Attached for some reason, window group, transient for, etc
    pub attached: heapless::Vec<ManagedWindow, WS_WINDOW_LIMIT>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ArrangeKind {
    NoFloat,
    // This state is kind of error prone, just used for knowing whether or not to draw tiled
    FloatingActive,
    FloatingInactive(f32, f32),
}

#[derive(Clone, Debug)]
pub struct ManagedWindow {
    pub window: Window,
    pub wants_focus: bool,
    pub arrange: ArrangeKind,
    pub focus_style: FocusStyle,
    pub properties: WindowProperties,
}

#[cfg(test)]
impl PartialEq for ManagedWindow {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
            && self.wants_focus == other.wants_focus
            && self.arrange == other.arrange
            && self.focus_style == other.focus_style
    }
}

/// [docs of different input focus styles ](https://tronche.com/gui/x/icccm/sec-4.html#s-4.1.7)
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FocusStyle {
    NoInput,
    Passive,
    LocallyActive,
    GloballyActive,
}

impl ManagedWindow {
    #[must_use]
    pub fn new(
        window: Window,
        arrange: ArrangeKind,
        focus_style: FocusStyle,
        properties: WindowProperties,
    ) -> Self {
        ManagedWindow {
            window,
            wants_focus: false,
            arrange,
            focus_style,
            properties,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use crate::config::{USER_WORKSPACES};
    use crate::geometry::draw::Mode;
    use crate::geometry::layout::Layout;
    use crate::state::properties::{WindowProperties, WmName};
    use crate::state::workspace::{ArrangeKind, DeleteResult, FocusStyle, Workspaces};

    fn default_properties() -> WindowProperties {
        WindowProperties {
            wm_state: None,
            net_wm_state: crate::state::properties::NetWmState::default(),
            hints: None,
            size_hints: None,
            window_types: heapless::Vec::default(),
            leader: None,
            pid: None,
            class: heapless::Vec::default(),
            protocols: heapless::Vec::default(),
            name: WmName::NetWmName(heapless::String::default()),
            transient_for: None,
        }
    }

    fn empty_workspaces() -> Workspaces {
        Workspaces::create_empty(&USER_WORKSPACES).unwrap()
    }

    #[test]
    fn init_empty() {
        let workspaces = empty_workspaces();
        for i in 0..9 {
            assert_eq!(0, workspaces.get_all_managed_windows().len());
            assert_eq!(0, workspaces.iter_all_managed_windows_in_ws(0).count());
            assert_eq!(0, workspaces.get_all_tiled_windows(i).len());
        }
    }

    #[test]
    fn map_doesnt_leak() {
        let mut workspaces = empty_workspaces();
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(1, workspaces.get_all_managed_windows().len());
        assert_eq!(1, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).len());
        assert!(workspaces.find_all_attached_managed(0).is_none());
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::FloatingActive,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(2, workspaces.get_all_managed_windows().len());
        assert_eq!(2, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).len());
        assert!(workspaces.find_all_attached_managed(0).is_none());

        assert!(workspaces
            .add_attached(
                0,
                2,
                ArrangeKind::FloatingInactive(0.0, 0.0),
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap());
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_some());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(3, workspaces.get_all_managed_windows().len());
        assert_eq!(3, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).len());
        assert_eq!(1, workspaces.find_all_attached_managed(0).unwrap().len());

        workspaces
            .add_child_to_ws(
                3,
                1,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();

        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_some());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(4, workspaces.get_all_managed_windows().len());
        assert_eq!(3, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.iter_all_managed_windows_in_ws(1).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).len());
        assert_eq!(1, workspaces.find_all_attached_managed(0).unwrap().len());

        assert!(matches!(
            workspaces.delete_child_from_ws(0),
            DeleteResult::TiledTopLevel(_)
        ));

        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(2, workspaces.get_all_managed_windows().len());
        assert_eq!(1, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.iter_all_managed_windows_in_ws(1).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).len());

        assert!(matches!(
            workspaces.delete_child_from_ws(1),
            DeleteResult::FloatingTopLevel(_)
        ));
        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(1, workspaces.get_all_managed_windows().len());
        assert_eq!(0, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(1, workspaces.iter_all_managed_windows_in_ws(1).count());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).len());

        assert!(matches!(
            workspaces.delete_child_from_ws(3),
            DeleteResult::TiledTopLevel(_)
        ));
        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(0, workspaces.get_all_managed_windows().len());
        assert_eq!(0, workspaces.iter_all_managed_windows_in_ws(0).count());
        assert_eq!(0, workspaces.iter_all_managed_windows_in_ws(1).count());
        assert_eq!(0, workspaces.get_all_tiled_windows(1).len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).len());
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn finds_ws_containing_window() {
        let mut workspaces = empty_workspaces();
        assert!(workspaces.find_ws_containing_window(0).is_none());
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert_eq!(0, workspaces.find_ws_containing_window(0).unwrap());
    }

    #[test]
    fn will_cycle_tiling() {
        let mut workspaces = empty_workspaces();
        if let Mode::Tiled(layout) = workspaces.get_draw_mode(0) {
            assert_eq!(Layout::LeftLeader, layout);
        } else {
            panic!("Test doesn't start in DrawMode tiled");
        }
        workspaces.cycle_tiling_mode(0);
        if let Mode::Tiled(layout) = workspaces.get_draw_mode(0) {
            assert_eq!(Layout::CenterLeader, layout);
        } else {
            panic!("Test doesn't start in tiled drawmode");
        }
        assert_ne!(workspaces, empty_workspaces());
        workspaces.cycle_tiling_mode(0);
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_toggle_fullscreening() {
        let mut workspaces = empty_workspaces();
        workspaces.set_fullscreened(0, 0).unwrap();
        // Can set an unmanaged window to fullscreen, if that window is then unmapped without this state changing it'll cause a crash though.
        assert_ne!(workspaces, empty_workspaces());
        workspaces.unset_fullscreened(0);
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_toggle_draw_mode() {
        let mut workspaces = empty_workspaces();
        assert!(!workspaces.set_draw_mode(0, Mode::Tiled(Layout::LeftLeader)));
        assert_eq!(workspaces, empty_workspaces());
        assert!(workspaces.set_draw_mode(0, Mode::Tiled(Layout::CenterLeader)));
        assert_ne!(workspaces, empty_workspaces());
        assert!(workspaces.set_draw_mode(0, Mode::Tiled(Layout::LeftLeader)));
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_find_first_tiled() {
        let mut workspaces = empty_workspaces();
        assert!(workspaces.find_first_tiled(0).is_none());
        assert_eq!(workspaces, empty_workspaces());
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::FloatingInactive(0.0, 0.0),
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.find_first_tiled(0).is_none());
        assert_ne!(workspaces, empty_workspaces());
        workspaces
            .add_attached(
                0,
                1,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.find_first_tiled(0).is_some());
        assert!(matches!(
            workspaces.delete_child_from_ws(0),
            DeleteResult::FloatingTopLevel(_)
        ));
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn can_update_and_clear_size_modifiers() {
        let mut workspaces = empty_workspaces();

        workspaces.clear_size_modifiers(0);
        assert_eq!(workspaces, empty_workspaces());
        assert!(!workspaces.update_size_modifier(1, 0.1));
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        assert!(workspaces.update_size_modifier(0, 0.1));
        assert_eq!(
            base + 0.1,
            workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0]
        );
        assert_ne!(workspaces, empty_workspaces());
        workspaces.clear_size_modifiers(0);
        workspaces.delete_child_from_ws(0);
        workspaces.delete_child_from_ws(1);
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn wont_allow_resizing_past_zero() {
        let mut workspaces = empty_workspaces();

        workspaces.clear_size_modifiers(0);
        assert_eq!(workspaces, empty_workspaces());
        assert!(!workspaces.update_size_modifier(1, 0.1));
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        assert!(workspaces.update_size_modifier(0, 0.1));
        assert_eq!(
            base + 0.1,
            workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0]
        );
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        // Would go past 0
        assert!(workspaces.update_size_modifier(0, -10.0));
        // No change
        assert_eq!(
            base,
            workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0]
        );
    }

    #[test]
    fn can_get_set_wants_focus() {
        let mut workspaces = empty_workspaces();
        let base: Vec<bool> = vec![false; 9];
        assert_eq!(base, workspaces.get_wants_focus_workspaces());
        assert!(workspaces
            .set_wants_focus(0, true)
            .filter(|(_, ch)| *ch)
            .is_none());
        assert_eq!(workspaces, empty_workspaces());
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces
            .set_wants_focus(0, true)
            .filter(|(_, ch)| *ch)
            .is_some());
        let mut modified = base.clone();
        modified[0] = true;
        assert_eq!(modified, workspaces.get_wants_focus_workspaces());
        workspaces.set_wants_focus(0, false);
        assert_eq!(base, workspaces.get_wants_focus_workspaces());
        workspaces.set_wants_focus(0, true);
        assert!(matches!(
            workspaces.delete_child_from_ws(0),
            DeleteResult::TiledTopLevel(_)
        ));
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_get_next_and_prev_win() {
        let mut workspaces = empty_workspaces();
        assert!(workspaces.next_window(0).is_none());
        assert!(workspaces.prev_window(0).is_none());
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.next_window(0).is_none());
        assert!(workspaces.prev_window(0).is_none());
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::FloatingActive,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert_eq!(Some(0), workspaces.next_window(1).map(|mw| mw.window));
        assert_eq!(Some(0), workspaces.prev_window(1).map(|mw| mw.window));
        workspaces
            .add_child_to_ws(
                2,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        // Reverse insertion order
        assert_eq!(Some(1), workspaces.next_window(2).map(|mw| mw.window));
        assert_eq!(Some(0), workspaces.next_window(1).map(|mw| mw.window));
        assert_eq!(Some(0), workspaces.prev_window(2).map(|mw| mw.window));
        workspaces.delete_child_from_ws(0);
        workspaces.delete_child_from_ws(1);
        workspaces.delete_child_from_ws(2);
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_get_set_tab_focus_index() {
        let mut workspaces = empty_workspaces();
        workspaces.set_draw_mode(0, Mode::Tabbed(5));
        assert_ne!(workspaces, empty_workspaces());
        // No change when no visible windows, kinda weird behaviour.
        assert_eq!(Mode::Tabbed(0), workspaces.get_draw_mode(0));

        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        workspaces.set_draw_mode(0, Mode::Tabbed(5));
        // Will return a draw_mode of 0 if OOB
        assert_eq!(Mode::Tabbed(0), workspaces.get_draw_mode(0));
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        // still oob...
        assert_eq!(Mode::Tabbed(0), workspaces.get_draw_mode(0));
        workspaces.set_draw_mode(0, Mode::Tabbed(1));
        assert_eq!(Mode::Tabbed(1), workspaces.get_draw_mode(0));
        workspaces.delete_child_from_ws(0);
        workspaces.delete_child_from_ws(1);
        workspaces.set_draw_mode(0, Mode::Tiled(Layout::LeftLeader));
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_switch_tab_focus_window() {
        let mut workspaces = empty_workspaces();
        assert!(workspaces.switch_tab_focus_window(0, 0).unwrap().is_none());
        workspaces.set_draw_mode(0, Mode::Tabbed(0));
        assert!(workspaces.switch_tab_focus_window(0, 0).unwrap().is_none());
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        workspaces
            .add_child_to_ws(
                1,
                0,
                ArrangeKind::NoFloat,
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.switch_tab_focus_window(0, 5).unwrap().is_none());
        assert_eq!(Mode::Tabbed(0), workspaces.get_draw_mode(0));
        assert!(workspaces.switch_tab_focus_window(0, 0).unwrap().unwrap());
        assert!(!workspaces.switch_tab_focus_window(0, 0).unwrap().unwrap());
        assert!(workspaces.switch_tab_focus_window(0, 1).unwrap().unwrap());
        workspaces.delete_child_from_ws(0);
        workspaces.delete_child_from_ws(1);
        workspaces.set_draw_mode(0, Mode::Tiled(Layout::LeftLeader));
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn can_check_if_managed_floating() {
        let mut workspaces = empty_workspaces();
        assert!(!workspaces.is_managed_floating(0));
        workspaces
            .add_child_to_ws(
                0,
                0,
                ArrangeKind::FloatingInactive(0.0, 0.0),
                FocusStyle::Passive,
                &default_properties(),
            )
            .unwrap();
        assert!(workspaces.is_managed_floating(0));
        workspaces.delete_child_from_ws(0);
        assert!(!workspaces.is_managed_floating(0));
        assert_eq!(workspaces, empty_workspaces());
    }
}
