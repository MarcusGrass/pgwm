
# Wanted behaviour
Behaviour that's expected/wanted from this wm
- [x] Crashes on errors
- [x] It can tile windows in at least one pattern
- [x] It can display windows tabbed
- [x] Windows are configured into workspaces
- [x] Workspaces can be moved between monitors
- [x] Workspace-layouts can be cycled
- [x] Windows withing workspaces can be cycled
- [x] Windows can be moved between workspaces
- [x] When a window is moved from a workspace it ends up at the first tab-order
- [x] Windows can be manually floated
- [x] Floating windows can be dragged
- [x] Windows can be manually un-floated
- [x] Floated windows are always on-top of un-floated windows
- [x] A status bar can be put at the top of each desired monitor
- [x] Workspaces can be toggled by clicking the status bar
- [x] The statusbar can be hidden
- [x] The statusbar can display workspace information
- [x] Gets ewmh names
- [x] Resize floating windows
- [x] Arbitrary looks such as bar-height should be customizable
- [x] Visible cursor without window supplying it
- [x] Resizeable tiles
- [x] Window flashes border / ws icon when urgent

# Focus Wanted behaviour
- [x] Monitors can be focused without having children on them
- [x] Spawned windows take focus, windows are spawned on the currently focused monitor
- [x] If a window is destroyed or moved from the currently focused monitor, the first tiled window
  on that monitor is focused, if no windows are on the monitor, that monitor is focused
- [x] If a window is moused over, it's focused
- [x] If a workspace is toggled, if that workspace existed on another monitor, that monitor's
  focus is inherited by the new one, otherwise the first tiled window on that monitor is focused,
  if no windows exist the monitor itself is focused

# Bugs
- [x] All mouse events are captured, should grab pointer only when special buttons are pressent and also replay those press events to the client
- [x] Without refocus weirdness happens when windows are rescaled (clion)
- [x] If focused window is destroyed, revert to last focus (probably connected to above)
- [x] Second clion window doesn't load properly (probably connected to above)
- [x] Popups which should float dont (clion)
- [x] Popups get sent to the back of the stack on unfocus (clion)
- [x] Iconstates become their own windows
- [x] Unfloating a transient requires a redraw when it closes clion
- [x] Unfloating windows could be more reasonable, mouse position would be better than origin position
- [x] No tab on single monitor (startup misconfigured xrandr, probably x-offset related)
- [x] Mouse enter empty window should switch focus
- [x] Tab text poorly formatted (maybe idgaf, could dynamically get height from XFT or let the user configure size and height)
- [x] Reconfig on root screen resize (nice to have, xrandr changes etc, restarting the wm takes care of it but still)
- [x] Colors as rgb instead of the x11 names, could alloc with normal connection
- [x] Weird window remnants on hot-reload, only encountered on Gentoo, something with window discovery on startup
- [x] Focus changes when using mouse not working correctly with window padding, probably because
  of passing through the root window
- [x] Fix focus changes when sending to or toggling another workspace
- [x] Toggle status bar seemingly not working when using padding - turns out I missed it in
  refactoring dimension calculations
- [x] Next window not working on the first press when tabbed
- [x] No border on transients, redraws on transient unmap
- [x] Borders remain even when transients are unmapped
- [x] Left display bar remains covered when spawning a window
- [x] An unfloated window doesn't properly get input focus
- [x] dock floating seems not to work anymore - Cause is ignoring downstream configures from move client
- [x] Allow padding on tabbed layout
- [x] Some floated windows are not un-floated (probably unmanaged from popups, clion merge fixing f.e.)
- [x] Windows sometimes reconfigure themselves placing them outside their set position until re-tiling happens
- [x] toggle bar on cursor monitor, not focused monitor
- [x] Mouse is not initially spawned (typo in cursor name)

# Polish
- [x] Make transients follow parent
- [x] Undraw border when unfocusing transient
- [x] Undraw transients when switching ws
- [x] Pass references instead of copying, benchmark first (Copying is faster for small values, break-even seems to be at about 16 bytes)
- [x] Can completely remove borders Can configure border width to 0
- [x] Can conditionally remove borders at runtime
- [x] Different window layouts
- [x] Highlight focused workspace on bar in some way
- [x] respond to urgency by highlighting the workspace/window
- [x] Rewrite stupid stuff as macros, e.g. intern atoms and colors
- [x] Make Xinerama optional
- [x] Supports ewmh and normal wm hinting correctly
- [x] Kill if destroy doesn't cut it
- [x] Make status bar an uring dependency a feature
- [x] Allow pointer motions (or only button clicks) to set focus to an unfocused window, this seems to require grabbing the pointer when it's on a window that's not focused
- [x] Make calculating font width/height less stupid (It's still stupid, but less...)
- [x] Validate configuration to avoid unnecessary crashes
- [x] Don't run status checks if status bar isn't visible.
- [x] Get local offset at start, makes each date check go about 100 times faster ~5.4 mus vs ~38ns 

