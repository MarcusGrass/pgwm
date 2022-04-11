use crate::config::workspaces::UserWorkspace;
use crate::config::{DefaultDraw, TilingModifiers, APPLICATION_WINDOW_LIMIT, WS_WINDOW_LIMIT};
use crate::error::Result;
use crate::geometry::draw::{Mode, OldDrawMode};
use crate::geometry::layout::Layout;
use crate::push_heapless;
use crate::util::vec_ops::push_to_front;
use heapless::FnvIndexMap;
use std::collections::HashMap;
use x11rb::protocol::xproto::Window;

#[derive(Debug, PartialEq)]
pub struct Workspaces {
    // Hot read
    spaces: Vec<Workspace>,
    // Hot on read/write
    win_to_ws: FnvIndexMap<Window, usize, APPLICATION_WINDOW_LIMIT>,
    // Hot read
    name_to_ws: HashMap<String, usize>,
    base_tiling_modifiers: TilingModifiers,
}

impl Workspaces {
    pub fn create_empty(
        init_workspaces: &[UserWorkspace],
        tiling_modifiers: TilingModifiers,
    ) -> Result<Self> {
        let mut v = Vec::<Workspace>::new();
        let mut name_to_ws = HashMap::new();
        for (i, ws) in init_workspaces.iter().enumerate() {
            v.push(Workspace {
                draw_mode: match ws.default_draw {
                    DefaultDraw::LeftLeader => Mode::Tiled(Layout::LeftLeader),
                    DefaultDraw::CenterLeader => Mode::Tiled(Layout::CenterLeader),
                    DefaultDraw::Tabbed => Mode::Tabbed(0),
                },
                name: ws.name.clone(),
                children: heapless::CopyVec::new(), // Realloc is what's going to take time here
                tiling_modifiers: tiling_modifiers.clone(),
            });
            for mapped in &ws.mapped_class_names {
                name_to_ws.insert(mapped.clone(), i);
            }
        }
        Ok(Workspaces {
            spaces: v,
            win_to_ws: FnvIndexMap::new(),
            name_to_ws,
            base_tiling_modifiers: tiling_modifiers,
        })
    }

    #[must_use]
    pub fn get_all_managed_windows(&self) -> heapless::CopyVec<Window, APPLICATION_WINDOW_LIMIT> {
        self.win_to_ws.keys().copied().collect()
    }

    #[must_use]
    pub fn get_all_windows_in_ws(
        &self,
        ws_ind: usize,
    ) -> heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT> {
        self.spaces[ws_ind]
            .get_all_windows()
            .iter()
            .copied()
            .copied()
            .collect()
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
    pub fn get_managed_win(&self, window: Window) -> Option<ManagedWindow> {
        self.win_to_ws
            .get(&window)
            .and_then(|ws_ind| self.spaces[*ws_ind].find_managed_window(window))
    }

    pub fn update_size_modifier(&mut self, window: Window, resize: f32) -> Result<bool> {
        self.win_to_ws
            .get(&window)
            .map(|ws_ind| {
                let ws = &mut self.spaces[*ws_ind];
                let tiled = ws.get_all_tiled()?;
                if let Some(index) = tiled.iter().position(|ch| ch.window == window) {
                    match ws.draw_mode {
                        Mode::Tiled(Layout::LeftLeader) => {
                            if index == 0 {
                                ws.tiling_modifiers.left_leader =
                                    resize_safe(ws.tiling_modifiers.left_leader, resize);
                                Ok(true)
                            } else {
                                ws.tiling_modifiers.vertically_tiled[index - 1] = resize_safe(
                                    ws.tiling_modifiers.vertically_tiled[index - 1],
                                    resize,
                                );
                                Ok(true)
                            }
                        }
                        Mode::Tiled(Layout::CenterLeader) => {
                            if index == 0 {
                                ws.tiling_modifiers.center_leader =
                                    resize_safe(ws.tiling_modifiers.center_leader, resize);
                                Ok(true)
                            } else {
                                ws.tiling_modifiers.vertically_tiled[index - 1] = resize_safe(
                                    ws.tiling_modifiers.vertically_tiled[index - 1],
                                    resize,
                                );
                                Ok(true)
                            }
                        }
                        _ => Ok(false),
                    }
                } else {
                    Ok(false)
                }
            })
            .unwrap_or(Ok(false))
    }

    pub fn clear_size_modifiers(&mut self, ws_ind: usize) {
        self.spaces[ws_ind].tiling_modifiers = self.base_tiling_modifiers.clone();
    }

    pub fn unset_fullscreened(&mut self, ws_ind: usize) -> bool {
        let ws = &mut self.spaces[ws_ind];
        let dm = ws.draw_mode;
        if let Mode::Fullscreen { last_draw_mode, .. } = dm {
            ws.draw_mode = last_draw_mode.to_draw_mode();
            return true;
        }
        false
    }

    pub fn set_fullscreened(&mut self, ws_ind: usize, window: Window) -> Result<()> {
        let ws = &mut self.spaces[ws_ind];
        let dm = ws.draw_mode;
        if !matches!(dm, Mode::Fullscreen { .. }) {
            self.spaces[ws_ind].draw_mode = Mode::Fullscreen {
                window,
                last_draw_mode: OldDrawMode::from_draw_mode(dm)?,
            };
        }
        Ok(())
    }

    #[must_use]
    pub fn get_wants_focus_workspaces(&self) -> Vec<bool> {
        self.spaces.iter().map(Workspace::any_wants_focus).collect()
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
            let children = ws.get_all_tiled()?;
            if let Some(pos) = children.iter().position(|ch| ch.window == window) {
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
        self.spaces[num].toggle_float(window, floating)
    }

    pub fn add_child_to_ws(
        &mut self,
        window: Window,
        num: usize,
        arrange: ArrangeKind,
        refocus_parent: bool,
    ) -> Result<()> {
        self.win_to_ws
            .insert(window, num)
            .expect("Map capacity overflow, increase configured capacity to have more windows");
        self.spaces[num].add_child(window, arrange, refocus_parent)
    }

    pub fn add_attached(
        &mut self,
        parent: Window,
        attached: Window,
        arrange: ArrangeKind,
        internal_focus_handling: bool,
    ) -> Result<bool> {
        if let Some(ws) = self.win_to_ws.get(&parent).copied() {
            self.win_to_ws
                .insert(attached, ws)
                .expect("Map capacity overflow, increase configured capacity to have more windows");
            self.spaces[ws].add_attached(parent, attached, arrange, internal_focus_handling)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    #[must_use]
    pub fn find_all_attached(
        &self,
        parent: Window,
    ) -> Option<&heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>> {
        self.win_to_ws
            .get(&parent)
            .and_then(|ws| self.spaces[*ws].find_all_attached(parent))
    }

    #[must_use]
    pub fn find_managed_parent(&self, child: Window) -> Option<ManagedWindow> {
        self.win_to_ws.get(&child).and_then(|ws_ind| {
            self.spaces[*ws_ind]
                .children
                .iter()
                .find(|ch| ch.attached.iter().any(|mw| mw.window == child))
                .map(|ch| ch.managed)
        })
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
                self.spaces[ind].delete_child(window)
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

    pub fn find_first_tiled(&self, num: usize) -> Result<Option<ManagedWindow>> {
        self.spaces[num].get_all_tiled().map(|children| {
            children
                .iter()
                .find_map(|ch| (ch.arrange == ArrangeKind::NoFloat).then(|| *ch))
        })
    }

    pub fn get_all_tiled_windows(
        &self,
        num: usize,
    ) -> Result<heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>> {
        self.spaces[num].get_all_tiled()
    }

    pub fn tab_focus_window(&mut self, window: Window) -> bool {
        self.win_to_ws
            .get(&window)
            .map_or(false, |ind| self.spaces[*ind].tab_focus_window(window))
    }

    #[must_use]
    pub fn next_window(&self, cur: Window) -> Option<ManagedWindow> {
        self.win_to_ws
            .get(&cur)
            .copied()
            .and_then(|ws| self.spaces[ws].find_next(cur))
    }

    #[must_use]
    pub fn prev_window(&self, cur: Window) -> Option<ManagedWindow> {
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
}

#[derive(PartialEq, Debug)]
pub enum DeleteResult {
    TiledTopLevel(ManagedWindow),
    FloatingTopLevel(ManagedWindow),
    AttachedFloating(ManagedWindow),
    AttachedTiled(ManagedWindow),
    None,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub draw_mode: Mode,
    pub name: String,
    // Actually fine, searching small vectors is extremely efficient, as long as we don't need to
    // realloc
    pub children: heapless::CopyVec<Child, WS_WINDOW_LIMIT>,
    pub tiling_modifiers: TilingModifiers,
}

impl Workspace {
    fn add_child(
        &mut self,
        window: Window,
        arrange: ArrangeKind,
        refocus_parent: bool,
    ) -> Result<()> {
        crate::debug!("Adding child to ws: win = {} {:?}", window, arrange,);
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
                    internal_focus_handling: refocus_parent,
                },
                attached: heapless::CopyVec::new(),
            },
        )
    }

    fn get_all_windows(&self) -> heapless::CopyVec<&ManagedWindow, { WS_WINDOW_LIMIT }> {
        self.children
            .iter()
            .flat_map(|ch| std::iter::once(&ch.managed).chain(ch.attached.iter()))
            .collect()
    }

    fn get_all_windows_mut(&mut self) -> heapless::Vec<&mut ManagedWindow, { WS_WINDOW_LIMIT }> {
        self.children
            .iter_mut()
            .flat_map(|ch| std::iter::once(&mut ch.managed).chain(ch.attached.iter_mut()))
            .collect()
    }

    fn get_all_tiled(&self) -> Result<heapless::CopyVec<ManagedWindow, { WS_WINDOW_LIMIT }>> {
        let mut smol = heapless::CopyVec::new();
        for child in self.get_all_windows() {
            if child.arrange == ArrangeKind::NoFloat {
                push_heapless!(smol, *child)?;
            }
        }
        Ok(smol)
    }

    fn add_attached(
        &mut self,
        parent: Window,
        attached: Window,
        arrange: ArrangeKind,
        refocus_parent: bool,
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
                    internal_focus_handling: refocus_parent,
                },
            )?;
        }
        Ok(())
    }

    fn find_all_attached(
        &self,
        parent_window: Window,
    ) -> Option<&heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>> {
        self.children
            .iter()
            .find(|ch| ch.managed.window == parent_window && !ch.attached.is_empty())
            .map(|ch| &ch.attached)
    }

    fn un_float(&mut self, window: Window) -> Option<bool> {
        self.get_all_windows_mut().iter_mut().find_map(|mw| {
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

    fn find_managed_window(&self, window: Window) -> Option<ManagedWindow> {
        self.children.iter().find_map(|ch| {
            if ch.managed.window == window {
                Some(ch.managed)
            } else {
                ch.attached
                    .iter()
                    .filter(|tr| tr.window == window)
                    .copied()
                    .next()
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
                    crate::debug!("Removed attached from ws {:?}", child);
                    return if mw.arrange == ArrangeKind::NoFloat {
                        DeleteResult::AttachedTiled(child.managed)
                    } else {
                        DeleteResult::AttachedFloating(child.managed)
                    };
                }
            }
            DeleteResult::None
        }
    }

    fn is_floating(&self, window: Window) -> bool {
        self.get_all_windows()
            .iter()
            .any(|ch| ch.window == window && ch.arrange != ArrangeKind::NoFloat)
    }

    fn toggle_float(&mut self, window: Window, floating: ArrangeKind) -> bool {
        let mut all_children = self.get_all_windows_mut();
        if let Some(ind) = all_children.iter().position(|ch| ch.window == window) {
            if all_children[ind].arrange == floating {
                false
            } else {
                crate::debug!("Floating {}", ind);
                all_children[ind].arrange = floating;
                true
            }
        } else {
            false
        }
    }

    fn any_wants_focus(&self) -> bool {
        self.get_all_windows().iter().any(|win| win.wants_focus)
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

    fn find_next(&self, cur: Window) -> Option<ManagedWindow> {
        let all = self.get_all_windows();
        let len = all.len();
        if len == 1 {
            return None;
        }
        all.iter()
            .position(|ch| ch.window == cur)
            .and_then(|ind| all.get((ind + 1) % len))
            .copied()
            .copied()
    }

    fn find_prev(&self, cur: Window) -> Option<ManagedWindow> {
        let all = self.get_all_windows();
        let len = all.len();
        if len == 1 {
            return None;
        }
        all.iter()
            .position(|ch| ch.window == cur)
            .and_then(|ind| all.get((ind as i16 - 1).rem_euclid(len as i16) as usize))
            .copied()
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
        self.get_all_windows()
            .iter()
            .filter(|ch| ch.arrange == ArrangeKind::NoFloat)
            .count()
    }
}

fn resize_safe(old: f32, diff: f32) -> f32 {
    let new = old + diff;
    if new <= 0.0 {
        old
    } else {
        new
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Child {
    pub managed: ManagedWindow,
    // Attached for some reason, window group, transient for, etc
    pub attached: heapless::CopyVec<ManagedWindow, WS_WINDOW_LIMIT>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ArrangeKind {
    NoFloat,
    // This state is kind of error prone, just used for knowing whether or not to draw tiled
    FloatingActive,
    FloatingInactive(f32, f32),
    FloatOnTop(Window),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManagedWindow {
    pub window: Window,
    pub wants_focus: bool,
    pub arrange: ArrangeKind,
    pub internal_focus_handling: bool,
}

impl ManagedWindow {
    #[must_use]
    pub fn new(window: Window, arrange: ArrangeKind, refocus_parent: bool) -> Self {
        ManagedWindow {
            window,
            wants_focus: false,
            arrange,
            internal_focus_handling: refocus_parent,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Cfg;
    use crate::geometry::draw::Mode;
    use crate::geometry::layout::Layout;
    use crate::state::workspace::{ArrangeKind, DeleteResult, Workspaces};

    fn empty_workspaces() -> Workspaces {
        let cfg = Cfg::default();
        Workspaces::create_empty(&cfg.workspaces, cfg.tiling_modifiers).unwrap()
    }

    #[test]
    fn init_empty() {
        let workspaces = empty_workspaces();
        for i in 0..9 {
            assert_eq!(0, workspaces.get_all_managed_windows().len());
            assert_eq!(0, workspaces.get_all_windows_in_ws(0).len());
            assert_eq!(0, workspaces.get_all_tiled_windows(i).unwrap().len());
        }
    }

    #[test]
    fn map_doesnt_leak() {
        let mut workspaces = empty_workspaces();
        workspaces
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(1, workspaces.get_all_managed_windows().len());
        assert_eq!(1, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).unwrap().len());
        assert!(workspaces.find_all_attached(0).is_none());
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::FloatingActive, false)
            .unwrap();
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(2, workspaces.get_all_managed_windows().len());
        assert_eq!(2, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).unwrap().len());
        assert!(workspaces.find_all_attached(0).is_none());

        assert!(workspaces
            .add_attached(0, 2, ArrangeKind::FloatingInactive(0.0, 0.0), false)
            .unwrap());
        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_some());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(3, workspaces.get_all_managed_windows().len());
        assert_eq!(3, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).unwrap().len());
        assert_eq!(1, workspaces.find_all_attached(0).unwrap().len());

        workspaces
            .add_child_to_ws(3, 1, ArrangeKind::NoFloat, false)
            .unwrap();

        assert!(workspaces.get_managed_win(0).is_some());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_some());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(4, workspaces.get_all_managed_windows().len());
        assert_eq!(3, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_windows_in_ws(1).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).unwrap().len());
        assert_eq!(1, workspaces.get_all_tiled_windows(0).unwrap().len());
        assert_eq!(1, workspaces.find_all_attached(0).unwrap().len());

        assert!(matches!(
            workspaces.delete_child_from_ws(0),
            DeleteResult::TiledTopLevel(_)
        ));

        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_some());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(2, workspaces.get_all_managed_windows().len());
        assert_eq!(1, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_windows_in_ws(1).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).unwrap().len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).unwrap().len());

        assert!(matches!(
            workspaces.delete_child_from_ws(1),
            DeleteResult::FloatingTopLevel(_)
        ));
        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_some());
        assert_eq!(1, workspaces.get_all_managed_windows().len());
        assert_eq!(0, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(1, workspaces.get_all_windows_in_ws(1).len());
        assert_eq!(1, workspaces.get_all_tiled_windows(1).unwrap().len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).unwrap().len());

        assert!(matches!(
            workspaces.delete_child_from_ws(3),
            DeleteResult::TiledTopLevel(_)
        ));
        assert!(workspaces.get_managed_win(0).is_none());
        assert!(workspaces.get_managed_win(1).is_none());
        assert!(workspaces.get_managed_win(2).is_none());
        assert!(workspaces.get_managed_win(3).is_none());
        assert_eq!(0, workspaces.get_all_managed_windows().len());
        assert_eq!(0, workspaces.get_all_windows_in_ws(0).len());
        assert_eq!(0, workspaces.get_all_windows_in_ws(1).len());
        assert_eq!(0, workspaces.get_all_tiled_windows(1).unwrap().len());
        assert_eq!(0, workspaces.get_all_tiled_windows(0).unwrap().len());
        assert_eq!(workspaces, empty_workspaces());
    }

    #[test]
    fn finds_ws_containing_window() {
        let mut workspaces = empty_workspaces();
        assert!(workspaces.find_ws_containing_window(0).is_none());
        workspaces
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
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
        assert!(workspaces.find_first_tiled(0).unwrap().is_none());
        assert_eq!(workspaces, empty_workspaces());
        workspaces
            .add_child_to_ws(0, 0, ArrangeKind::FloatingInactive(0.0, 0.0), false)
            .unwrap();
        assert!(workspaces.find_first_tiled(0).unwrap().is_none());
        assert_ne!(workspaces, empty_workspaces());
        workspaces
            .add_attached(0, 1, ArrangeKind::NoFloat, false)
            .unwrap();
        assert!(workspaces.find_first_tiled(0).unwrap().is_some());
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
        assert!(!workspaces.update_size_modifier(1, 0.1).unwrap());
        workspaces
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        assert!(workspaces.update_size_modifier(0, 0.1).unwrap());
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
        assert!(!workspaces.update_size_modifier(1, 0.1).unwrap());
        workspaces
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        assert!(workspaces.update_size_modifier(0, 0.1).unwrap());
        assert_eq!(
            base + 0.1,
            workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0]
        );
        let base = workspaces.get_ws(0).tiling_modifiers.vertically_tiled[0];
        // Would go past 0
        assert!(workspaces.update_size_modifier(0, -10.0).unwrap());
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
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
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
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        assert!(workspaces.next_window(0).is_none());
        assert!(workspaces.prev_window(0).is_none());
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::FloatingActive, false)
            .unwrap();
        assert_eq!(Some(0), workspaces.next_window(1).map(|mw| mw.window));
        assert_eq!(Some(0), workspaces.prev_window(1).map(|mw| mw.window));
        workspaces
            .add_child_to_ws(2, 0, ArrangeKind::NoFloat, false)
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
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        workspaces.set_draw_mode(0, Mode::Tabbed(5));
        // Will return a draw_mode of 0 if OOB
        assert_eq!(Mode::Tabbed(0), workspaces.get_draw_mode(0));
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::NoFloat, false)
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
            .add_child_to_ws(0, 0, ArrangeKind::NoFloat, false)
            .unwrap();
        workspaces
            .add_child_to_ws(1, 0, ArrangeKind::NoFloat, false)
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
            .add_child_to_ws(0, 0, ArrangeKind::FloatingInactive(0.0, 0.0), false)
            .unwrap();
        assert!(workspaces.is_managed_floating(0));
        workspaces.delete_child_from_ws(0);
        assert!(!workspaces.is_managed_floating(0));
        assert_eq!(workspaces, empty_workspaces());
    }
}
