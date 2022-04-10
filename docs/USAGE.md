# Getting started
A work in progress document to make usage of the WM clearer.
To make a clean install look like the default configuration examples 
Jetbrains Mono Nerd Font, Font Awesome Brands 6, and Font Awesome Free Solid 6 needs to be installed.

## Enter the WM
On startup, the WM will show an empty background and a bar on top.  
Using default configuration, `mod+shift+enter` will try to spawn [alacritty](https://github.com/alacritty/alacritty), 
if using another terminal emulator, that should be changed.
Pressing the same button again will spawn a new instance of your chosen terminal emulator on the same monitor,
by default using the left-leader tiling layout, which can also be changed.  

## Open applications
Through that terminal you could now start any other application, for example `firefox`, which will then
open on your selected monitor. To keep things separated you could move firefox to another workspace by using (default) 
`mod+shift+2` (or another number for another workspace). If firefox should always be spawned on workspace 2, 
it can be configured in that workspace's subsection in the configuration.
which could look like this:  
`mapped_class_names = ["firefox"]`  
The class_name for a window isn't always as intuitive as in firefox's case, to find a given application's class name
[xprop](https://www.x.org/releases/X11R7.5/doc/man/man1/xprop.1.html) can be used.

Another preferred way to spawn applications is using [dmenu](https://tools.suckless.org/dmenu/), to use it with pgwm 
it needs to be bound, default it's bound to `mod+d` and the configuration for that key-binding looks like this:
```toml
[[key-mapping]]
mods = ["M4"]
key =  0x0064 # d
on_click = { action = "Spawn", args = ["dmenu_run", ["-i", "-p", "Run: "]] }
```
Spawning applications like this removes the need to do things through a terminal.

## Navigate
Within a workspace the mouse can be used to select which window is focused by the WM. 
Focused is both contextual for the WM and x11, the focused window is the window that receives 
input such as that from a keyboard and mouse, as well as which window will be affected by WM-actions such as close.
Close, default `mod+shift+c` will close the focused window. By default the focused window is displayed with a white border, 
border size and color can be configured.

A window which gets mouse-movement over it becomes focused, focused can also be cycled between windows with key-bindings, 
default `mod+shift+j` and `mod+shift+k`, and between monitors with `mod+shift+.` and `mod+shift+,`.  

To change which workspace is visible on the current monitor a key-binding can be used, by default `mod+shift+<n>` where `<n>` is some digit between 
1 and 9 inclusive, which will then switch the workspace. Additionally, on the bar configurable workspace icons are displayed,
when one is clicked that workspace becomes visible on the monitor on which it was clicked.

## Change layout
To change the layout to tabbed by default `mod+<space>` can be used.
To change to another tiling layout, by default `mod+n` can be used, although both left-leader and center-leader 
looks the same on a workspace containing 2 or fewer tiled windows.

## Customization
The WM doesn't try to do much when it comes to aesthetics, it can display borders with colors depending on whether the 
window is focused or not. It can pad windows with slack-space. It draws things on the bar, and tab-bar if in the tabbed layout.
These things and some more properties can be configured with fonts, colors, sizing etc.
Functional customization comes through key-bindings and clickable shortcuts.
The WM can run binaries on key-presses, spawning a terminal uses this functionality but keys can be bound to 
spawn whatever you like. Likewise shortcuts can be configured for the bar which on-press will perform some WM-action, 
the workspace icons use this by having a ToggleWorkspace(n) on press. An example shortcut:
```toml
[[mouse-mapping]]
mods = []
button = "M1"
target = { kind = "ShortcutComponent", args = 1 }
# Lock screen
on_click = { action = "Spawn", args = ["xscreensaver-command", ["-lock"]]}
```
When pressing mouse 1 on ShortcutComponent at index 1, with no mod-buttons (for example shift) clicked, 
`xscreensaver-command -lock` will be spawned. 
A note on this is that this is not a shell command, it's invoking the binary with args, if shell functionality is required,
for example because piping is needed, running it through your shell will do the trick, as in this below example:
```toml
[[key-mapping]]
mods = []
key =  0xff61 # Print
on_click = { action = "Spawn", args = ["bash", ["-c", "maim -s -u | xclip -selection clipboard -t image/png -i"]] }
```
Which is equivalent to running:  
`maim -s -u | xclip -selection clipboard -t image/png -i`  
in your terminal if using bash. (The command let's you take a screenshot).

## Exit
Exiting is bound by default to `mod+shift+q`, the WM will try to tear down it's state and then close.