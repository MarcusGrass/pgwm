use crate::colors::RGBA;
use tiny_std::UnixStr;
use x11_keysyms::{
    XK_Print, XK_Return, XK_b, XK_c, XK_comma, XK_d, XK_f, XK_h, XK_j, XK_k, XK_l, XK_n, XK_period,
    XK_q, XK_r, XK_space, XK_t, XK_1, XK_2, XK_3, XK_4, XK_5, XK_6, XK_7, XK_8, XK_9,
};
use xcb_rust_protocol::proto::xproto::{ButtonIndexEnum, ModMask};

use crate::config::key_map::KeyboardMapping;
use crate::config::mouse_map::{MouseMapping, MouseTarget};
use crate::config::workspaces::UserWorkspace;

pub mod key_map;
pub mod mouse_map;
pub mod workspaces;

/// Internal
#[cfg(feature = "status-bar")]
pub const _STATUS_BAR_CHECK_CONTENT_LIMIT: usize = 32;

/// Internal
#[cfg(feature = "status-bar")]
pub const _STATUS_BAR_BAT_SEGMENT_LIMIT: usize = 5;

/// Internal
#[cfg(feature = "status-bar")]
pub const _STATUS_BAR_TOTAL_LENGTH_LIMIT: usize = 256;

/// Internal
#[cfg(feature = "status-bar")]
pub const _STATUS_BAR_CHECK_SEP: &str = " | ";

/// Internal
#[cfg(feature = "status-bar")]
pub const _STATUS_BAR_FIRST_SEP: &str = " ";

/// Internal
pub const _WM_NAME_LIMIT: usize = 256;

/// Internal
pub const _WM_CLASS_NAME_LIMIT: usize = 128;

/// The name that the window manager will broadcast itself as.
pub const WINDOW_MANAGER_NAME: &str = "pgwm";

/// Should not be changed, internally used.
pub const _WINDOW_MANAGER_NAME_BUF_SIZE: usize = WINDOW_MANAGER_NAME.len() * 2;

/// How many windows can reside in a workspace, loosely used but if tiling into really small windows
/// is desired, this can be raised an arbitrary amount.
/// Not too harsh on stack space.
pub const WS_WINDOW_LIMIT: usize = 16;

/// Size of the binary which stores events to ignore. Since it's flushed on every incoming event above
/// the given ignored sequence its max required size could be statically determined, but that's a pain,
/// 64 should be enough.
pub const BINARY_HEAP_LIMIT: usize = 64;

/// Cache size of windows that have been closed but not destroyed yet. These will be destroyed
/// and later killed if no destroy-notify is received. Can be arbitrarily chosen but will cause
/// a crash if too low.
/// Only triggered in the event that for some reason a lot of windows that are misbehaving are manually
/// closed at the same time and refuse to die within timeout.
pub const DYING_WINDOW_CACHE: usize = 16;

/// Convenience constant, internal
pub const _NUM_TILING_MODIFIERS: usize = WS_WINDOW_LIMIT - 1;

/// Height in pixels of the status bar
/// Cannot be 0 or larger than any monitor's height
/// Instead of setting this to zero, to hide the bar either bind and use [`Action::ToggleBar`],
/// or set it to hidden by default with [`WM_SHOW_BAR_INITIALLY`].
pub const STATUS_BAR_HEIGHT: i16 = 20;

/// Height in pixels of the tab bar showing which tabs are open (if in tabbed mode)
pub const TAB_BAR_HEIGHT: i16 = 20;

/// Space between windows that are not decorated with a border, neighbouring windows share this space ie. 2 windows tiled
/// horizontally `[a, b]` will have a total length of 3 * `window_padding`, one left of a, one in the middle, and one right of b
pub const WINDOW_PADDING: i16 = 8;

/// Decorated space around windows, neighbouring windows do not share this space ie. 2 windows tiled horizontally
/// `[a, b] `will have a total length of 4 * `window_border_width`, , one left of a, one right of a, one left of b, and one right of b
pub const WINDOW_BORDER_WIDTH: u32 = 3;

/// Padding to the left of where in the workspace bar the window's `WM_NAME` or `_NET_WM_NAME` property is displayed
pub const WORKSPACE_BAR_WINDOW_NAME_PADDING: u16 = 8;

/// Whether or not to have window padding in the tabbed layout
pub const PAD_WHILE_TABBED: bool = true;

/// When a window is signalled to be killed a delete request is sent to the client this is a timeout in milliseconds
/// starting from when that request is sent to when a destroy-window for that client is sent to x11
pub const CLIENT_WINDOW_DESTROY_AFTER: u64 = 2000;

/// Millis before force-close
/// If a window is not destroyed after sending a destroy-window, a kill request will be sent after this timeout in milliseconds
pub const CLIENT_WINDOW_KILL_AFTER: u64 = 5000;

/// X11 cursor name, can be found online somewhere, currently unknown where.
/// Millis before we kill the client
pub const X11_CURSOR_NAME: &str = "left_ptr";

/// Show bar on start
pub const WM_SHOW_BAR_INITIALLY: bool = true;

/// The leader window's relative horizontal size in comparison with its tiling neighbours.
/// In the left-leader-layout there are 2 windows tiled horizontally.
/// With this value set to 2.0 this gives a relative left window size of 2.0/(2.0+1.0) = 2/3
pub const WM_TILING_MODIFIER_LEFT_LEADER: f32 = 2.0;
/// In the center-leader-layout there are 3 windows tiled horizontally.
/// With this value set to 2.0 this gives a relative center window size of 2.0/(2.0+1.0+1.0) = 2/4,
/// The center window takes up half the available monitor horizontal space.
pub const WM_TILING_MODIFIER_CENTER_LEADER: f32 = 2.0;
/// Similar to above this modifier affects windows that are vertically tiled
/// With the same relative sizing as the above this property determines a window's relative size in its tiling direction.
/// In the left-leader-scenario the second and following windows tile vertically, meaning if you have 3 windows tiled and increase
/// the modifier at index 0, the top-right window will increase in height while its below neighbour will be displaced.
/// With only two windows, the modifier on index 1 does not change any window's claim on the monitor real estate, as
/// that window will then tile vertically while already taking up maximum height.
pub const WM_TILING_MODIFIER_VERTICALLY_TILED: [f32; _NUM_TILING_MODIFIERS] =
    [1.0; _NUM_TILING_MODIFIERS];

/// Internal
pub const WM_TILING_MODIFIERS: TilingModifiers = TilingModifiers {
    left_leader: WM_TILING_MODIFIER_LEFT_LEADER,
    center_leader: WM_TILING_MODIFIER_CENTER_LEADER,
    vertically_tiled: WM_TILING_MODIFIER_VERTICALLY_TILED,
};

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TilingModifiers {
    pub left_leader: f32,
    pub center_leader: f32,
    pub vertically_tiled: [f32; _NUM_TILING_MODIFIERS],
}

/// Colors, RGBA color values
pub const COLORS: [RGBA; 17] = [
    WINDOW_BORDER,
    WINDOW_BORDER_HIGHLIGHTED,
    WINDOW_BORDER_URGENT,
    WORKSPACE_BAR_SELECTED_UNFOCUSED_WORKSPACE_BACKGROUND,
    WORKSPACE_BAR_UNFOCUSED_WORKSPACE_BACKGROUND,
    WORKSPACE_BAR_FOCUSED_WORKSPACE_BACKGROUND,
    WORKSPACE_BAR_URGENT_WORKSPACE_BACKGROUND,
    WORKSPACE_BAR_WORKSPACE_SECTION_TEXT,
    WORKSPACE_BAR_CURRENT_WINDOW_TITLE_TEXT,
    WORKSPACE_BAR_CURRENT_WINDOW_TITLE_BACKGROUND,
    STATUS_BAR_TEXT,
    STATUS_BAR_BACKGROUND,
    TAB_BAR_TEXT,
    TAB_BAR_FOCUSED_TAB_BACKGROUND,
    TAB_BAR_UNFOCUSED_TAB_BACKGROUND,
    SHORTCUT_TEXT,
    SHORTCUT_BACKGROUND,
];

/// Window border color when not focused
pub const WINDOW_BORDER: RGBA = default_black();
/// Window border color when focused
pub const WINDOW_BORDER_HIGHLIGHTED: RGBA = default_white();
/// Window border color when signaled to be urgent
pub const WINDOW_BORDER_URGENT: RGBA = default_orange();
/// Workspace text box background color for a workspace that is show but not focused (multiple monitors)
pub const WORKSPACE_BAR_SELECTED_UNFOCUSED_WORKSPACE_BACKGROUND: RGBA = default_light_gray();
/// Workspace text box background color for unfocused workspaces
pub const WORKSPACE_BAR_UNFOCUSED_WORKSPACE_BACKGROUND: RGBA = default_black();
/// Workspace text box background color for the focused workspace
pub const WORKSPACE_BAR_FOCUSED_WORKSPACE_BACKGROUND: RGBA = default_blue();
/// Workspace text box background color for a workspace containing an urgent window
pub const WORKSPACE_BAR_URGENT_WORKSPACE_BACKGROUND: RGBA = default_orange();
/// Text color for the workspace names
pub const WORKSPACE_BAR_WORKSPACE_SECTION_TEXT: RGBA = default_white();
/// Text color for the active window's `WM_NAME`/`_NET_WM_NAME`
pub const WORKSPACE_BAR_CURRENT_WINDOW_TITLE_TEXT: RGBA = default_white();
/// Background color for the portion of the bar that displays the above name
pub const WORKSPACE_BAR_CURRENT_WINDOW_TITLE_BACKGROUND: RGBA = default_dark_gray();
/// Text for the status bar section
pub const STATUS_BAR_TEXT: RGBA = default_white();
/// Background color for the above
pub const STATUS_BAR_BACKGROUND: RGBA = default_light_gray();
/// Tab bar background for the focused tab
pub const TAB_BAR_TEXT: RGBA = default_white();
/// Tab bar background for unfocused tabs
pub const TAB_BAR_FOCUSED_TAB_BACKGROUND: RGBA = default_light_gray();
/// Tab bar text color
pub const TAB_BAR_UNFOCUSED_TAB_BACKGROUND: RGBA = default_black();
/// Shortcut background color
pub const SHORTCUT_TEXT: RGBA = default_white();
/// Shortcut text color
pub const SHORTCUT_BACKGROUND: RGBA = default_black();

/// Just some default colors
const fn default_white() -> RGBA {
    (223, 223, 223, 0)
}

const fn default_dark_gray() -> RGBA {
    (40, 44, 52, 1)
}

const fn default_light_gray() -> RGBA {
    (56, 66, 82, 0)
}

const fn default_black() -> RGBA {
    (28, 31, 36, 0)
}

const fn default_blue() -> RGBA {
    (48, 53, 168, 0)
}

const fn default_orange() -> RGBA {
    (224, 44, 16, 0)
}

const DEFAULT_FONT: FontCfg<'static> = FontCfg::new(
    UnixStr::from_str_checked(
        "/usr/share/fonts/jetbrains-mono/JetBrainsMonoNerdFontMono-Regular.ttf\0",
    ),
    "14.0",
);
/// This is a mapping of fonts to be drawn at different sections.
/// It will use fonts left-to-right and draw single characters with the backup render if
/// the previous render does not provide them. There can at most be `FALLBACK_FONTS_LIMIT`
/// per target segment.
///
/// Fonts to use when drawing workspace names (top left of status-bar)
pub const WORKSPACE_SECTION_FONTS: &[FontCfg<'static>] = &[DEFAULT_FONT];

/// Fonts to use when the currentl focused window's name
pub const WINDOW_NAME_DISPLAY_SECTION: &[FontCfg<'static>] = &[DEFAULT_FONT];

/// Fonts to use when drawing the status section
#[cfg(feature = "status-bar")]
pub const STATUS_SECTION: &[FontCfg<'static>] = &[DEFAULT_FONT];

/// Fonts to use when drawing the name of tabbed windows
pub const TAB_BAR_SECTION: &[FontCfg<'static>] = &[DEFAULT_FONT];

/// Fonts to use when drawing the shortcut section
pub const SHORTCUT_SECTION: &[FontCfg<'static>] = &[DEFAULT_FONT];

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FontCfg<'a> {
    pub path: &'a UnixStr,
    // Can't have an f32 as a map key.. sigh
    pub size: &'a str,
}

impl<'a> FontCfg<'a> {
    #[must_use]
    pub const fn new(path: &'a UnixStr, size: &'a str) -> Self {
        Self { path, size }
    }
}

/// Shortcuts, placed to the right-most part of the tab bar.
pub const BAR_SHORTCUTS: [&str; 2] = ["\u{f304}", "\u{f502}"];

/// Status checks, put at the top-right of the tab bar.
#[cfg(feature = "status-bar")]
pub const STATUS_CHECKS: [crate::status::checker::Check; 4] = [
    crate::status::checker::Check {
        check_type: crate::status::checker::CheckType::Cpu(crate::status::checker::CpuFormat::new(
            "\u{f2db}", 1,
        )),
        interval: 1000,
    },
    crate::status::checker::Check {
        check_type: crate::status::checker::CheckType::Mem(crate::status::checker::MemFormat::new(
            "\u{f538}", 1,
        )),
        interval: 1000,
    },
    crate::status::checker::Check {
        check_type: crate::status::checker::CheckType::Net(crate::status::checker::NetFormat::new(
            "\u{f093}", "\u{f019}", 1,
        )),
        interval: 1000,
    },
    crate::status::checker::Check {
        check_type: crate::status::checker::CheckType::Date(
            crate::status::checker::DateFormat::new(
                "\u{f073}",
                crate::status::time::ClockFormatter::new(
                    crate::status::time::Format::new(&[
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Year),
                        crate::status::time::FormatChunk::Value(" "),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Month),
                        crate::status::time::FormatChunk::Value(" "),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Day),
                        crate::status::time::FormatChunk::Value(" v"),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Week),
                        crate::status::time::FormatChunk::Value(" "),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Hour),
                        crate::status::time::FormatChunk::Value(":"),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Minute),
                        crate::status::time::FormatChunk::Value(":"),
                        crate::status::time::FormatChunk::Token(crate::status::time::Token::Second),
                    ]),
                    offset(),
                ),
            ),
        ),
        interval: 1000,
    },
];

#[must_use]
#[cfg(feature = "status-bar")]
#[allow(clippy::match_wild_err_arm)]
pub const fn offset() -> time::UtcOffset {
    let offset_res = time::UtcOffset::from_hms(1, 0, 0);
    match offset_res {
        Ok(offset) => offset,
        Err(_err) => {
            panic!("Invalid utc offset provided!")
        }
    }
}

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Default, Debug, Copy, Clone)]
pub enum DefaultDraw {
    #[default]
    LeftLeader,
    CenterLeader,
    Tabbed,
}

/// Available workspaces and their names and respective `class_name` mappings
/// The `mapped_class_names` is an array of wm class names
/// If a window is spawned with a mapped class name it will be remapped to the specified workspace
/// Finding a windows `WM_CLASS_NAME` property can be done with fe. [xprop](https://www.x.org/releases/X11R7.5/doc/man/man1/xprop.1.html)
pub const USER_WORKSPACES: [UserWorkspace; 9] = [
    UserWorkspace::new(
        "\u{f121}",
        &[
            "jetbrains-rustrover",
            "jetbrains-clion",
            "jetbrains-idea",
            "lapce",
        ],
        DefaultDraw::LeftLeader,
    ),
    UserWorkspace::new("\u{f120}", &[], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{e007}", &["Navigator"], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f086}", &["Slack", "discord"], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f1bc}", &["spotify"], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f11b}", &[], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f7d9}", &["Pavucontrol"], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f02b}", &[], DefaultDraw::LeftLeader),
    UserWorkspace::new("\u{f02c}", &[], DefaultDraw::LeftLeader),
];

/** Which mouse-keys will be grabbed and what actions will be executed if they are pressed.
Actions:
`MoveClient`: Will float a client if tiled and until the pressed button is released the window
will be moved along with the client
Resize: Will resize the client along its tiling axis if tiled, or both axes if floating, by the contained number.
eg. Resize(i16) means that when that button is pressed the window size will increase by 2,
while Resize(-2) means that the window size will decrease by 2
The unit of 2 is undefined, it's some implementation specific modifier
Available modifiers can be found in `ButtonIndex` imported at the top of this file (although it's M1 through M5).
`MouseTarget` should likely always be `MouseTarget::ClientWindow`
 **/
pub const MOUSE_MAPPINGS: [MouseMapping; 16] = [
    MouseMapping {
        target: MouseTarget::ClientWindow,
        mods: MOD_KEY,
        button: ButtonIndexEnum::ONE,
        action: Action::MoveWindow,
    },
    MouseMapping {
        target: MouseTarget::ClientWindow,
        mods: MOD_KEY,
        button: ButtonIndexEnum::FOUR,
        action: Action::ResizeWindow(4),
    },
    MouseMapping {
        target: MouseTarget::ClientWindow,
        mods: MOD_KEY,
        button: ButtonIndexEnum::FIVE,
        action: Action::ResizeWindow(-4),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(0),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(0),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(1),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(1),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(2),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(2),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(3),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(3),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(4),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(4),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(5),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(5),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(6),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(6),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(7),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(7),
    },
    MouseMapping {
        target: MouseTarget::WorkspaceBarComponent(8),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::ToggleWorkspace(8),
    },
    MouseMapping {
        target: MouseTarget::StatusComponent(0),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::Spawn(
            UnixStr::from_str_checked("/home/gramar/.local/bin/alacritty\0"),
            &[
                UnixStr::from_str_checked("-e\0"),
                UnixStr::from_str_checked("htop\0"),
            ],
        ),
    },
    MouseMapping {
        target: MouseTarget::StatusComponent(3),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::Spawn(
            UnixStr::from_str_checked("/usr/bin/firefox\0"),
            &[
                UnixStr::from_str_checked("-new-tab\0"),
                UnixStr::from_str_checked("https://calendar.google.com\0"),
            ],
        ),
    },
    MouseMapping {
        target: MouseTarget::ShortcutComponent(0),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::Spawn(
            UnixStr::from_str_checked("/home/gramar/.local/bin/alacritty\0"),
            &[
                UnixStr::from_str_checked("-e\0"),
                // Using bash to access '~' as home
                UnixStr::from_str_checked("/bin/bash\0"),
                UnixStr::from_str_checked("-c\0"),
                // Pop some configuration files in a new terminal
                UnixStr::from_str_checked("nvim ~/.bashrc ~/.xinitrc ~/.config/pgwm/pgwm.toml\0"),
            ],
        ),
    },
    MouseMapping {
        target: MouseTarget::ShortcutComponent(1),
        mods: ModMask(0u16),
        button: ButtonIndexEnum::ONE,
        action: Action::Spawn(
            UnixStr::from_str_checked("/usr/bin/xscreensaver-command\0"),
            &[UnixStr::from_str_checked("-lock\0")],
        ),
    },
];

/// The mod key, maps to super on my machine's keyboard, can be changed to any of the available
/// `ModMasks`, check the `ModMask` struct.
const MOD_KEY: ModMask = ModMask::FOUR;

/// Keyboard mapping.
/// The first argument is a bitwise or of all applied masks or `ModMask::from(0u16)` denoting none.
/// The second argument is the x11 Keysyms, [found here](https://cgit.freedesktop.org/xorg/proto/x11proto/tree/keysymdef.h)
/// if more are needed they can be qualified as `x11::keysym::XK_b` or imported at the top of the file with the
/// others and used more concisely as `XK_b`.
/// The third parameter is the action that should be taken when the mods and key gets pressed.
/// It's an enum of which all values are exemplified in the below default configuration.
pub const KEYBOARD_MAPPINGS: [KeyboardMapping; 41] = [
    // Shows or hides the top bar
    KeyboardMapping::new(MOD_KEY, XK_b, Action::ToggleBar),
    // Focuses the (logically) previous window of the focused workspace (if any)
    KeyboardMapping::new(MOD_KEY, XK_k, Action::FocusPreviousWindow),
    // Focuses the (logically) next window of the focused workspace (if any)
    KeyboardMapping::new(MOD_KEY, XK_j, Action::FocusNextWindow),
    // Focuses the (logically) previous monitor of the focused monitor (if any)
    KeyboardMapping::new(MOD_KEY, XK_comma, Action::FocusPreviousMonitor),
    // Focuses the (logically) next monitor of the focused monitor (if any)
    KeyboardMapping::new(MOD_KEY, XK_period, Action::FocusNextMonitor),
    // Cycles the DrawMode from tiled to tabbed
    KeyboardMapping::new(MOD_KEY, XK_space, Action::CycleDrawMode),
    // Cycles the Tiling layout from left-leader to center-leader to left-leader to ... etc.
    KeyboardMapping::new(MOD_KEY, XK_n, Action::NextTilingMode),
    // Updates the window size, if positive increases size, negative decreases.
    // If tiled the window will expand in its tiling direction.
    // If floating it expands equally in size and width (percentage wise, although this isn't a percentage)
    KeyboardMapping::new(MOD_KEY, XK_l, Action::ResizeWindow(4)),
    KeyboardMapping::new(MOD_KEY, XK_h, Action::ResizeWindow(-4)),
    // Updates the window border, same as above.
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_l,
        Action::ResizeBorders(1),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_h,
        Action::ResizeBorders(-1),
    ),
    // Updates the window padding, same as above.
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::CONTROL.0 | ModMask::SHIFT.0),
        XK_l,
        Action::ResizePadding(1),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::CONTROL.0 | ModMask::SHIFT.0),
        XK_h,
        Action::ResizePadding(-1),
    ),
    // Reset runtime window resizing to configured defaults.
    KeyboardMapping::new(MOD_KEY, XK_r, Action::ResetToDefaultSizeModifiers),
    // Restart the wm.
    KeyboardMapping::new(ModMask(MOD_KEY.0 | ModMask::SHIFT.0), XK_r, Action::Restart),
    // Send a window to logically 0th position of the tiling stack
    KeyboardMapping::new(MOD_KEY, XK_Return, Action::SendToFront),
    // Close a window
    KeyboardMapping::new(ModMask(MOD_KEY.0 | ModMask::SHIFT.0), XK_c, Action::Close),
    // Gracefully exit the WM
    KeyboardMapping::new(ModMask(MOD_KEY.0 | ModMask::SHIFT.0), XK_q, Action::Quit),
    // Unfloat a tiling window, placing it at the 0th position of the tile-set
    KeyboardMapping::new(MOD_KEY, XK_t, Action::UnFloat),
    // Toggle fullscreen on the currently focused workspace
    KeyboardMapping::new(MOD_KEY, XK_f, Action::ToggleFullscreen),
    // Toggle a workspace on the currently focused monitor.
    // The number is an index, and if that index does not match an existing workspace
    // the WM will immediately crash.
    KeyboardMapping::new(MOD_KEY, XK_1, Action::ToggleWorkspace(0)),
    KeyboardMapping::new(MOD_KEY, XK_2, Action::ToggleWorkspace(1)),
    KeyboardMapping::new(MOD_KEY, XK_3, Action::ToggleWorkspace(2)),
    KeyboardMapping::new(MOD_KEY, XK_4, Action::ToggleWorkspace(3)),
    KeyboardMapping::new(MOD_KEY, XK_5, Action::ToggleWorkspace(4)),
    KeyboardMapping::new(MOD_KEY, XK_6, Action::ToggleWorkspace(5)),
    KeyboardMapping::new(MOD_KEY, XK_7, Action::ToggleWorkspace(6)),
    KeyboardMapping::new(MOD_KEY, XK_8, Action::ToggleWorkspace(7)),
    KeyboardMapping::new(MOD_KEY, XK_9, Action::ToggleWorkspace(8)),
    // Send the currently focused window to another workspace.
    // The number is an index, and if that index does not match an existing workspace
    // the WM will immediately crash.
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_1,
        Action::SendToWorkspace(0),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_2,
        Action::SendToWorkspace(1),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_3,
        Action::SendToWorkspace(2),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_4,
        Action::SendToWorkspace(3),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_5,
        Action::SendToWorkspace(4),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_6,
        Action::SendToWorkspace(5),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_7,
        Action::SendToWorkspace(6),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_8,
        Action::SendToWorkspace(7),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_9,
        Action::SendToWorkspace(8),
    ),
    KeyboardMapping::new(
        ModMask(MOD_KEY.0 | ModMask::SHIFT.0),
        XK_Return,
        Action::Spawn(
            UnixStr::from_str_checked("/home/gramar/.local/bin/alacritty\0"),
            &[],
        ),
    ),
    KeyboardMapping::new(
        MOD_KEY,
        XK_d,
        Action::Spawn(
            UnixStr::from_str_checked("/usr/bin/dmenu_run\0"),
            &[
                UnixStr::from_str_checked("-i\0"),
                UnixStr::from_str_checked("-p\0"),
                UnixStr::from_str_checked("Run: \0"),
            ],
        ),
    ),
    KeyboardMapping::new(
        ModMask(0u16),
        XK_Print,
        Action::Spawn(
            UnixStr::from_str_checked("/bin/bash\0"),
            &[
                UnixStr::from_str_checked("-c\0"),
                // Piping through string pipes ('|') is not valid Rust, just send it to shell instead
                UnixStr::from_str_checked(
                    "/usr/bin/maim -s -u | xclip -selection clipboard -t image/png -i\0",
                ),
            ],
        ),
    ),
];
const ICON_FONT: &FontCfg<'static> = &FontCfg::new(
    UnixStr::from_str_checked("/usr/share/fonts/fontawesome/Font Awesome 6 Free-Solid-900.otf\0"),
    "13.0",
);
const BRAND_FONT: &FontCfg<'static> = &FontCfg::new(
    UnixStr::from_str_checked(
        "/usr/share/fonts/fontawesome/Font Awesome 6 Brands-Regular-400.otf\0",
    ),
    "13.0",
);

pub const CHAR_REMAP_FONTS: [&FontCfg<'static>; 2] = [ICON_FONT, BRAND_FONT];
/**
Overrides specific character drawing.
If some character needs icons from a certain render, they should be mapped below.
 **/
pub const CHAR_REMAP: &[(char, &FontCfg<'static>)] = &[
    ('\u{f121}', ICON_FONT),
    ('\u{f120}', ICON_FONT),
    ('\u{f086}', ICON_FONT),
    ('\u{e007}', BRAND_FONT),
    ('\u{f1bc}', BRAND_FONT),
    ('\u{f11b}', ICON_FONT),
    ('\u{f7d9}', ICON_FONT),
    ('\u{f02b}', ICON_FONT),
    ('\u{f02c}', ICON_FONT),
    ('\u{f2db}', ICON_FONT),
    ('\u{f538}', ICON_FONT),
    ('\u{f019}', ICON_FONT),
    ('\u{f093}', ICON_FONT),
    ('\u{f502}', ICON_FONT),
    ('\u{f304}', ICON_FONT),
    ('\u{f073}', ICON_FONT),
];

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub enum Action {
    Quit,
    Restart,
    Spawn(&'static UnixStr, &'static [&'static UnixStr]),
    Close,
    ToggleWorkspace(usize),
    SendToWorkspace(usize),
    SendToFront,
    UnFloat,
    ToggleFullscreen,
    CycleDrawMode,
    MoveWindow,
    NextTilingMode,
    ResizeWindow(i16),
    ResizePadding(i16),
    ResizeBorders(i16),
    ResetToDefaultSizeModifiers,
    FocusNextWindow,
    FocusPreviousWindow,
    FocusNextMonitor,
    FocusPreviousMonitor,
    ToggleBar,
}
