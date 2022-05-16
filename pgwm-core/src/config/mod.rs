use crate::colors::ColorBuilder;
use crate::config::key_map::KeyboardMapping;
use crate::config::mouse_map::{MouseMapping, MouseTarget};
use crate::config::workspaces::UserWorkspace;
use std::collections::HashMap;
use x11_keysyms::{
    XK_Print, XK_Return, XK_b, XK_c, XK_comma, XK_d, XK_f, XK_h, XK_j, XK_k, XK_l, XK_n, XK_period,
    XK_q, XK_r, XK_space, XK_t, XK_1, XK_2, XK_3, XK_4, XK_5, XK_6, XK_7, XK_8, XK_9,
};
use x11rb::protocol::xproto::{ButtonIndex, Keysym, ModMask};

pub mod key_map;
pub mod mouse_map;
pub mod workspaces;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_CHECK_CONTENT_LIMIT: usize = 32;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_DATE_PATTERN_LIMIT: usize = 128;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_BAT_SEGMENT_LIMIT: usize = 5;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_TOTAL_LENGTH_LIMIT: usize = 256;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_UNIQUE_CHECK_LIMIT: usize = 8;

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_CHECK_SEP: &str = " | ";

#[cfg(feature = "status-bar")]
pub const STATUS_BAR_FIRST_SEP: &str = " ";

// This is just a constant to avoid magic 4's everywhere, changing this is almost guaranteed to be a mistake
pub const UTF8_CHAR_MAX_BYTES: usize = 4;

pub const WM_NAME_LIMIT: usize = 256;

pub const WM_CLASS_NAME_LIMIT: usize = 128;

/**
    The name that the window manager will broadcast itself as. Will also affect where
    configuration is placed/read from.
**/
pub const WINDOW_MANAGER_NAME: &str = "pgwm";
/**
   Should not be changed, internally used.
**/
pub const WINDOW_MANAGER_NAME_BUF_SIZE: usize = WINDOW_MANAGER_NAME.len() * 2;
/**
   How many different color segments are used. eg. tab bar text and status bar text makes 2,
   even if they use the same color,
   this should not be touched for simple configuration.
**/
pub const USED_DIFFERENT_COLOR_SEGMENTS: usize = 17;
// Configuration that necessarily need to be comptime for working with heapless datastructures
/**
    How many windows can reside in a workspace, loosely used but if tiling into really small windows
    is desired, this can be raised an arbitrary amount.
    Not too harsh on stack space.
**/
pub const WS_WINDOW_LIMIT: usize = 16;

/**
   How many windows that can be managed simultaneously, can be arbitrarily chosen with risk of
   crashing if the number is exceeded.
   Not too harsh on stack space.
**/
pub const APPLICATION_WINDOW_LIMIT: usize = 128;

/**
    Size of the binary which stores events to ignore. Since it's flushed on every incoming event above
    the given ignored sequence its max required size could be statically determined, but that's a pain,
    64 should be enough.
**/
pub const BINARY_HEAP_LIMIT: usize = 64;

/**
    Cache size of windows that have been closed but not destroyed yet. These will be destroyed
    and later killed if no destroy-notify is received. Can be arbitrarily chosen but will cause
    a crash if too low.
    Only triggered in the event that for some reason a lot of windows that are misbehaving are manually
    closed at the same time and refuse to die within timeout.
**/
pub const DYING_WINDOW_CACHE: usize = 16;

/**
    Internally used for writing the render buffer to xft when drawing, 32 gives good performance.
**/
pub const FONT_WRITE_BUF_LIMIT: usize = 32;

/**
    Convenience constant
**/
pub const NUM_TILING_MODIFIERS: usize = WS_WINDOW_LIMIT - 1;

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(feature = "config-file", serde(default))]
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Cfg {
    pub sizing: Sizing,
    pub options: Options,
    #[cfg_attr(feature = "config-file", serde(alias = "tiling-modifiers"))]
    pub tiling_modifiers: TilingModifiers,
    pub fonts: Fonts,
    #[cfg_attr(feature = "config-file", serde(default = "init_colors"))]
    pub colors: ColorBuilder,
    #[cfg_attr(
        feature = "config-file",
        serde(alias = "char-remap", default = "init_char_remap")
    )]
    pub char_remap: HashMap<heapless::String<UTF8_CHAR_MAX_BYTES>, FontCfg>,
    #[cfg_attr(
        feature = "config-file",
        serde(alias = "workspace", default = "init_workspaces")
    )]
    pub workspaces: Vec<UserWorkspace>,
    #[cfg_attr(
        feature = "config-file",
        serde(alias = "mouse-mapping", default = "init_mouse_mappings")
    )]
    pub mouse_mappings: Vec<SimpleMouseMapping>,
    #[cfg_attr(
        feature = "config-file",
        serde(alias = "key-mapping", default = "init_key_mappings")
    )]
    pub key_mappings: Vec<SimpleKeyMapping>,

    #[cfg_attr(feature = "config-file", serde(alias = "key-spawn-mapping", default))]
    pub bar: BarCfg,
}

impl Cfg {
    pub fn new() -> crate::error::Result<Self> {
        #[cfg(feature = "config-file")]
        {
            let mut cfg = match crate::util::load_cfg::load_cfg() {
                Ok(cfg) => Ok(cfg),
                // Not having a config file is not an error, fallback to default hard-coded
                Err(e) => match e {
                    crate::error::Error::ConfigDirFind
                    | crate::error::Error::ConfigFileFind
                    | crate::error::Error::Io(_) => Ok(Cfg::default()),
                    // Having a bad config file is an error
                    _ => Err(e),
                },
            }?;
            // Having an invalid config file is an error however
            validate_config(&mut cfg)?;
            Ok(cfg)
        }
        #[cfg(not(feature = "config-file"))]
        Ok(Cfg::default())
    }
}

#[cfg(feature = "config-file")]
fn validate_config(cfg: &mut Cfg) -> crate::error::Result<()> {
    let num_used_workspaces = cfg.workspaces.len();
    if num_used_workspaces == 0 {
        return Err(crate::error::Error::ConfigLogic("No workspaces configured"));
    }
    if cfg.sizing.status_bar_height <= 0 {
        return Err(crate::error::Error::ConfigLogic(
            "Status bar height less than 0",
        ));
    }
    if cfg.sizing.tab_bar_height <= 0 {
        return Err(crate::error::Error::ConfigLogic(
            "Status bar height less than 0",
        ));
    }
    if cfg.tiling_modifiers.left_leader <= 0.0 {
        return Err(crate::error::Error::ConfigLogic(
            "Left leader tiling modifier less than 0",
        ));
    }
    if cfg.tiling_modifiers.center_leader <= 0.0 {
        return Err(crate::error::Error::ConfigLogic(
            "Center leader tiling modifier less than 0",
        ));
    }
    for i in 0..NUM_TILING_MODIFIERS {
        if let Some(modifier) = cfg.tiling_modifiers.vertically_tiled.get(i) {
            if *modifier <= 0.0 {
                return Err(crate::error::Error::ConfigLogic(
                    "Vertical tiling modifier less than 0",
                ));
            }
        } else {
            cfg.tiling_modifiers.vertically_tiled.push(1.0);
        }
    }

    cfg.key_mappings.iter()
        .map(|kb| &kb.on_click)
        .chain(cfg.mouse_mappings.iter().map(|mm| &mm.on_click))
        .try_for_each(|action| {
            match action {
                Action::SendToWorkspace(n) | Action::ToggleWorkspace(n) => {
                    if *n >= num_used_workspaces {
                        return Err(crate::error::Error::ConfigLogic("Key/mouse mapping(s) out of configured workspace bounds and will cause a crash on activation"));
                    }
                }
                _ => {},
            }
            Ok(())
        })?;
    Ok(())
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            sizing: Sizing::default(),
            options: Options::default(),
            tiling_modifiers: TilingModifiers::default(),
            fonts: Fonts::default(),
            colors: init_colors(),
            char_remap: init_char_remap(),
            workspaces: init_workspaces(),
            mouse_mappings: init_mouse_mappings(),
            key_mappings: init_key_mappings(),
            bar: BarCfg::default(),
        }
    }
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Copy, Clone, Debug)]
pub struct Sizing {
    pub status_bar_height: i16,
    pub tab_bar_height: i16,
    pub window_padding: i16,
    pub window_border_width: u32,
    pub workspace_bar_window_name_padding: u16,
}

impl Default for Sizing {
    fn default() -> Self {
        Self {
            // Height of the status bar, the top bar with status and workspace info
            status_bar_height: 20,

            // Height of the tab-bar when in tabbed-mode
            tab_bar_height: 20,

            // Space between windows that are not decorated with a border, neighbouring windows share this space ie. 2 windows tiled
            // horizontally [a, b] will have a total length of 3 * window_padding, one left of a, one in the middle, and one right of b
            window_padding: 8,

            // Decorated space around windows, neighbouring windows do not share this space ie. 2 windows tiled horizontally
            // [a, b] will have a total length of 4 * window_border_width, , one left of a, one right of a, one left of b, and one right of b
            window_border_width: 3,

            // Padding to the left of where in the workspace bar the window's WM_NAME or _NET_WM_NAME property is displayed
            workspace_bar_window_name_padding: 8,
        }
    }
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(PartialEq))]
#[derive(Clone, Debug)]
pub struct Options {
    pub pad_while_tabbed: bool,
    pub destroy_after: u64, // Millis before force-close
    pub kill_after: u64,    // Millis before we kill the client
    pub cursor_name: String,
    pub show_bar_initially: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            // Whether or not to have window padding in the tabbed layout
            pad_while_tabbed: true,

            // When a window is signalled to be killed a delete request is sent to the client this is a timeout in milliseconds
            // starting from when that request is sent to when a destroy-window for that client is sent to x11
            destroy_after: 2000,

            // If a window is not destroyed after sending a destroy-window, a kill request will be sent after this timeout in milliseconds
            kill_after: 5000,

            // X11 cursor name, can be found online somewhere, currently unknown where.
            cursor_name: String::from("left_ptr"),

            show_bar_initially: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
pub struct TilingModifiers {
    pub left_leader: f32,
    pub center_leader: f32,
    pub vertically_tiled: Vec<f32>,
}

impl Default for TilingModifiers {
    fn default() -> Self {
        TilingModifiers {
            left_leader: 2.0,
            center_leader: 2.0,
            vertically_tiled: vec![1.0; NUM_TILING_MODIFIERS],
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(PartialEq))]
pub struct Fonts {
    pub workspace_section: Vec<FontCfg>,
    pub window_name_display_section: Vec<FontCfg>,
    pub status_section: Vec<FontCfg>,
    pub tab_bar_section: Vec<FontCfg>,
    pub shortcut_section: Vec<FontCfg>,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
pub struct FontCfg {
    pub path: String,
    // Can't have an f32 as a map key.. sigh
    pub size: String,
}

impl FontCfg {
    pub fn new(path: impl Into<String>, size: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            size: size.into(),
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct BarCfg {
    pub shortcuts: Vec<Shortcut>,
    #[cfg(feature = "status-bar")]
    #[cfg_attr(feature = "config-file", serde(default = "init_status_checks"))]
    pub status_checks: heapless::Vec<crate::status::checker::Check, STATUS_BAR_UNIQUE_CHECK_LIMIT>,
}

impl Default for BarCfg {
    fn default() -> Self {
        BarCfg {
            shortcuts: init_shortcuts(),
            #[cfg(feature = "status-bar")]
            status_checks: init_status_checks(),
        }
    }
}

fn init_shortcuts() -> Vec<Shortcut> {
    vec![
        Shortcut::new("\u{f304}".to_owned()),
        Shortcut::new("\u{f502}".to_owned()),
    ]
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct Shortcut {
    pub name: String,
}

impl Shortcut {
    fn new(name: String) -> Self {
        Self { name }
    }
}

/**
This is a mapping of fonts to be drawn at different sections.
It will use fonts left-to-right and draw single characters with the backup render if
the previous render does not provide them. There can at most be `FALLBACK_FONTS_LIMIT`
per target segment.
 **/
impl Default for Fonts {
    fn default() -> Self {
        Self {
            workspace_section: vec![FontCfg::new(
                "/usr/share/fonts/TTF/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
                "14.0",
            )],
            window_name_display_section: vec![FontCfg::new(
                "/usr/share/fonts/TTF/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
                "14.0",
            )],
            status_section: vec![FontCfg::new(
                "/usr/share/fonts/TTF/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
                "14.0",
            )],
            tab_bar_section: vec![FontCfg::new(
                "/usr/share/fonts/TTF/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
                "14.0",
            )],
            shortcut_section: vec![FontCfg::new(
                "/usr/share/fonts/TTF/JetBrains Mono Regular Nerd Font Complete Mono.ttf",
                "14.0",
            )],
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct SimpleMouseMapping {
    target: MouseTarget,
    pub mods: ModMasks,
    pub button: Button,
    on_click: Action,
}

impl SimpleMouseMapping {
    #[must_use]
    pub fn to_mouse_mapping(self) -> MouseMapping {
        MouseMapping {
            target: self.target,
            action: self.on_click,
            mods: self.mods.inner,
            button: self.button.inner,
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct SimpleKeyMapping {
    mods: ModMasks,
    key: Keysym,
    on_click: Action,
}

impl SimpleKeyMapping {
    fn new(mod_mask: ModMask, keysym: Keysym, action: Action) -> Self {
        Self {
            mods: ModMasks::from(mod_mask),
            key: keysym,
            on_click: action,
        }
    }
    #[must_use]
    pub fn to_key_mapping(self) -> KeyboardMapping {
        KeyboardMapping {
            modmask: self.mods.inner,
            keysym: self.key,
            action: self.on_click,
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct ModMasks {
    pub inner: ModMask,
}

impl From<ModMask> for ModMasks {
    fn from(inner: ModMask) -> Self {
        ModMasks { inner }
    }
}

#[cfg(feature = "config-file")]
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub(crate) enum ModMaskEnum {
    Shift,
    Lock,
    Control,
    M1,
    M2,
    M3,
    M4,
    M5,
    Any,
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub struct Button {
    pub inner: ButtonIndex,
}

impl From<ButtonIndex> for Button {
    fn from(inner: ButtonIndex) -> Self {
        Button { inner }
    }
}

#[cfg(feature = "config-file")]
#[derive(Debug, serde::Deserialize)]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub(crate) enum ButtonMask {
    Any,
    M1,
    M2,
    M3,
    M4,
    M5,
}

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug, Copy, Clone)]
pub enum DefaultDraw {
    LeftLeader,
    CenterLeader,
    Tabbed,
}

impl Default for DefaultDraw {
    fn default() -> Self {
        DefaultDraw::LeftLeader
    }
}

/**
    Workspace configuration, names and which window classes should map to each workspace is put here.
    If the name is longer than `WS_NAME_LIMIT` the wm will crash on startup.
    Similarly if any class name is longer than `MAX_WM_CLASS_NAME` it will crash.
    Increase those parameters as needed.
**/
fn init_workspaces() -> Vec<UserWorkspace> {
    vec![
        UserWorkspace::new(
            String::from("\u{f121}"),
            vec![
                String::from("jetbrains-clion"),
                String::from("jetbrains-idea"),
            ],
            DefaultDraw::LeftLeader,
        ),
        UserWorkspace::new(String::from("\u{f120}"), vec![], DefaultDraw::LeftLeader),
        UserWorkspace::new(
            String::from("\u{e007}"),
            vec![String::from("firefox")],
            DefaultDraw::LeftLeader,
        ),
        UserWorkspace::new(
            String::from("\u{f086}"),
            vec![String::from("Slack"), String::from("discord")],
            DefaultDraw::LeftLeader,
        ),
        UserWorkspace::new(
            String::from("\u{f1bc}"),
            vec![String::from("spotify")],
            DefaultDraw::LeftLeader,
        ),
        UserWorkspace::new(String::from("\u{f11b}"), vec![], DefaultDraw::LeftLeader),
        UserWorkspace::new(
            String::from("\u{f7d9}"),
            vec![String::from("Pavucontrol")],
            DefaultDraw::LeftLeader,
        ),
        UserWorkspace::new(String::from("\u{f02b}"), vec![], DefaultDraw::LeftLeader),
        UserWorkspace::new(String::from("\u{f02c}"), vec![], DefaultDraw::LeftLeader),
    ]
}

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
fn init_mouse_mappings() -> Vec<SimpleMouseMapping> {
    vec![
        SimpleMouseMapping {
            target: MouseTarget::ClientWindow,
            mods: ModMasks::from(MOD_KEY),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::MoveWindow,
        },
        SimpleMouseMapping {
            target: MouseTarget::ClientWindow,
            mods: ModMasks::from(MOD_KEY),
            button: Button::from(ButtonIndex::M4),
            on_click: Action::ResizeWindow(4),
        },
        SimpleMouseMapping {
            target: MouseTarget::ClientWindow,
            mods: ModMasks::from(MOD_KEY),
            button: Button::from(ButtonIndex::M5),
            on_click: Action::ResizeWindow(-4),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(0),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(0),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(1),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(1),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(2),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(2),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(3),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(3),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(4),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(4),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(5),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(5),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(6),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(6),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(7),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(7),
        },
        SimpleMouseMapping {
            target: MouseTarget::WorkspaceBarComponent(8),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::ToggleWorkspace(8),
        },
        SimpleMouseMapping {
            target: MouseTarget::StatusComponent(0),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::Spawn("alacritty".into(), vec!["-e".into(), "htop".into()]),
        },
        SimpleMouseMapping {
            target: MouseTarget::StatusComponent(3),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::Spawn(
                "firefox".into(),
                vec!["-new-tab".into(), "https://calendar.google.com".into()],
            ),
        },
        SimpleMouseMapping {
            target: MouseTarget::ShortcutComponent(0),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::Spawn(
                "alacritty".into(),
                vec![
                    "-e".into(),
                    // Using bash to access '~' as home
                    "bash".into(),
                    "-c".into(),
                    // Pop some configuration files in a new terminal
                    "nvim ~/.bashrc ~/.xinitrc ~/.config/pgwm/pgwm.toml".into(),
                ],
            ),
        },
        SimpleMouseMapping {
            target: MouseTarget::ShortcutComponent(1),
            mods: ModMasks::from(ModMask::from(0u16)),
            button: Button::from(ButtonIndex::M1),
            on_click: Action::Spawn("xscreensaver-command".into(), vec!["-lock".into()]),
        },
    ]
}

/**
Just some default colors
 **/
const WHITE: (u8, u8, u8, u8) = (223, 223, 223, 0);
const DARK_GRAY: (u8, u8, u8, u8) = (40, 44, 52, 1);
const LIGHT_GRAY: (u8, u8, u8, u8) = (56, 66, 82, 0);
const BLACK: (u8, u8, u8, u8) = (28, 31, 36, 0);
const BLUE: (u8, u8, u8, u8) = (48, 53, 168, 0);
const ORANGE: (u8, u8, u8, u8) = (224, 44, 16, 0);
/**
   Color configuration, Here colors are set for different segments that the WM draws.
   Naming is hopefully fairly self-explanatory for what each color does.
   A constant can be declared as above for reuse.
**/
fn init_colors() -> ColorBuilder {
    ColorBuilder::default()
        .window_border(BLACK)
        .window_border_highlighted(WHITE)
        .window_border_urgent(ORANGE)
        .workspace_bar_selected_unfocused_workspace_background(LIGHT_GRAY)
        .workspace_bar_unfocused_workspace_background(BLACK)
        .workspace_bar_focused_workspace_background(BLUE)
        .workspace_bar_urgent_workspace_background(ORANGE)
        .workspace_bar_workspace_section_text(WHITE)
        .workspace_bar_current_window_title_text(WHITE)
        .workspace_bar_current_window_title_background(DARK_GRAY)
        .status_bar_text(WHITE)
        .status_bar_background(LIGHT_GRAY)
        .tab_bar_text(WHITE)
        .tab_bar_focused_tab_background(LIGHT_GRAY)
        .tab_bar_unfocused_tab_background(BLACK)
        .shortcut_text(WHITE)
        .shortcut_background(BLACK)
}

/**
Status bar configuration. The code here doesn't even need to be compileable if not using
The checks all take an `icon` parameter which is an arbitrary string drawn next to the value.
 **/
#[cfg(feature = "status-bar")]
fn init_status_checks(
) -> heapless::Vec<crate::status::checker::Check, STATUS_BAR_UNIQUE_CHECK_LIMIT> {
    use crate::status::checker::{Check, CheckType, CpuFormat, DateFormat, MemFormat, NetFormat};
    let mut checks = heapless::Vec::new();
    /* Commented out because I'm usually not using a computer with batteries and configure those with config files
    let mut battery_threshholds = heapless::Vec::new();
    // BatFormat takes a threshold and an associated icon which is displayed when below that threshold.
    crate::push_heapless!(
        battery_threshholds,
        BatFormat::new(90, heapless::String::from("\u{f240} "))
    )
    .unwrap();
    crate::push_heapless!(
        battery_threshholds,
        BatFormat::new(75, heapless::String::from("\u{f241} "))
    )
    .unwrap();
    crate::push_heapless!(
        battery_threshholds,
        BatFormat::new(50, heapless::String::from("\u{f242} "))
    )
    .unwrap();
    crate::push_heapless!(
        battery_threshholds,
        BatFormat::new(25, heapless::String::from("\u{f243} "))
    )
    .unwrap();
    crate::push_heapless!(
        battery_threshholds,
        BatFormat::new(0, heapless::String::from("\u{f244} "))
    )
    .unwrap();
    crate::push_heapless!(
        checks,
        Check {
            check_type: CheckType::Battery(battery_threshholds),
            interval: 1000
        }
    )
    .unwrap();
     */

    // The CpuFormat takes an amount of decimals to use when displaying load
    crate::push_heapless!(
        checks,
        Check {
            check_type: CheckType::Cpu(CpuFormat::new(heapless::String::from("\u{f2db}"), 1)),
            interval: 1000
        }
    )
    .unwrap();

    // The MemFormat takes a display-size, see (your editor) or it's associated file for valid values
    crate::push_heapless!(
        checks,
        Check {
            check_type: CheckType::Mem(MemFormat::new(heapless::String::from("\u{f538}"), 1)),
            interval: 1000
        }
    )
    .unwrap();
    // The NetFormat takes two icon values, download and upload respectively, as well as a display-size
    // same as above, and decimals, same as CpuFormat.
    crate::push_heapless!(
        checks,
        Check {
            check_type: CheckType::Net(NetFormat::new(
                heapless::String::from("\u{f093}"),
                heapless::String::from("\u{f019}"),
                1
            )),
            interval: 1000
        }
    )
    .unwrap();
    crate::push_heapless!(
            checks,
            // The Dateformat takes a format string, over-engineered explanation here
            // https://time-rs.github.io/book/api/format-description.html
            // Hopefully the below examples will help
            Check {
                check_type: CheckType::Date(DateFormat::new(
                heapless::String::from("\u{f073}"),
                heapless::String::from("[weekday repr:short] [month repr:short] [day] w[week_number] [hour]:[minute]:[second]"),
                time::UtcOffset::from_hms(2, 0, 0).unwrap(),
            )),
                interval: 1000
            }

        )
            .unwrap();
    checks
}

/**
    The mod key, maps to super on my machine/key_board, can be changed to any of the available
    ModMasks, check the ModMask struct.
**/
const MOD_KEY: ModMask = ModMask::M4;
/**
    Keyboard mapping.
    The first argument is a bitwise or of all applied masks or `ModMask::from(0u16)` denoting none.
    The second argument is the x11 Keysyms, found here https://cgit.freedesktop.org/xorg/proto/x11proto/tree/keysymdef.h
    if more are needed they can be qualified as `x11::keysym::XK_b` or imported at the top of the file with the
    others and used more concisely as `XK_b`.
    The third parameter is the action that should be taken when the mods and key gets pressed.
    It's an enum of which all values are exemplified in the below default configuration.
**/
fn init_key_mappings() -> Vec<SimpleKeyMapping> {
    vec![
        // Shows or hides the top bar
        SimpleKeyMapping::new(MOD_KEY, XK_b, Action::ToggleBar),
        // Focuses the (logically) previous window of the focused workspace (if any)
        SimpleKeyMapping::new(MOD_KEY, XK_k, Action::FocusPreviousWindow),
        // Focuses the (logically) next window of the focused workspace (if any)
        SimpleKeyMapping::new(MOD_KEY, XK_j, Action::FocusNextWindow),
        // Focuses the (logically) previous monitor of the focused monitor (if any)
        SimpleKeyMapping::new(MOD_KEY, XK_comma, Action::FocusPreviousMonitor),
        // Focuses the (logically) next monitor of the focused monitor (if any)
        SimpleKeyMapping::new(MOD_KEY, XK_period, Action::FocusNextMonitor),
        // Cycles the DrawMode from tiled to tabbed
        SimpleKeyMapping::new(MOD_KEY, XK_space, Action::CycleDrawMode),
        // Cycles the Tiling layout from left-leader to center-leader to left-leader to ... etc.
        SimpleKeyMapping::new(MOD_KEY, XK_n, Action::NextTilingMode),
        // Updates the window size, if positive increases size, negative decreases.
        // If tiled the window will expand in its tiling direction.
        // If floating it expands equally in size and width (percentage wise, although this isn't a percentage)
        SimpleKeyMapping::new(MOD_KEY, XK_l, Action::ResizeWindow(4)),
        SimpleKeyMapping::new(MOD_KEY, XK_h, Action::ResizeWindow(-4)),
        // Updates the window border, same as above.
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_l, Action::ResizeBorders(1)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_h, Action::ResizeBorders(-1)),
        // Updates the window padding, same as above.
        SimpleKeyMapping::new(
            MOD_KEY | ModMask::CONTROL | ModMask::SHIFT,
            XK_l,
            Action::ResizePadding(1),
        ),
        SimpleKeyMapping::new(
            MOD_KEY | ModMask::CONTROL | ModMask::SHIFT,
            XK_h,
            Action::ResizePadding(-1),
        ),
        // Reset runtime window resizing to configured defaults.
        SimpleKeyMapping::new(MOD_KEY, XK_r, Action::ResetToDefaultSizeModifiers),
        // Restart the wm.
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_r, Action::Restart),
        // Send a window to logically 0th position of the tiling stack
        SimpleKeyMapping::new(MOD_KEY, XK_Return, Action::SendToFront),
        // Close a window
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_c, Action::Close),
        // Gracefully exit the WM
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_q, Action::Quit),
        // Unfloat a tiling window, placing it at the 0th position of the tile-set
        SimpleKeyMapping::new(MOD_KEY, XK_t, Action::UnFloat),
        // Toggle fullscreen on the currently focused workspace
        SimpleKeyMapping::new(MOD_KEY, XK_f, Action::ToggleFullscreen),
        // Toggle a workspace on the currently focused monitor.
        // The number is an index, and if that index does not match an existing workspace
        // the WM will immediately crash.
        SimpleKeyMapping::new(MOD_KEY, XK_1, Action::ToggleWorkspace(0)),
        SimpleKeyMapping::new(MOD_KEY, XK_2, Action::ToggleWorkspace(1)),
        SimpleKeyMapping::new(MOD_KEY, XK_3, Action::ToggleWorkspace(2)),
        SimpleKeyMapping::new(MOD_KEY, XK_4, Action::ToggleWorkspace(3)),
        SimpleKeyMapping::new(MOD_KEY, XK_5, Action::ToggleWorkspace(4)),
        SimpleKeyMapping::new(MOD_KEY, XK_6, Action::ToggleWorkspace(5)),
        SimpleKeyMapping::new(MOD_KEY, XK_7, Action::ToggleWorkspace(6)),
        SimpleKeyMapping::new(MOD_KEY, XK_8, Action::ToggleWorkspace(7)),
        SimpleKeyMapping::new(MOD_KEY, XK_9, Action::ToggleWorkspace(8)),
        // Send the currently focused window to another workspace.
        // The number is an index, and if that index does not match an existing workspace
        // the WM will immediately crash.
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_1, Action::SendToWorkspace(0)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_2, Action::SendToWorkspace(1)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_3, Action::SendToWorkspace(2)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_4, Action::SendToWorkspace(3)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_5, Action::SendToWorkspace(4)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_6, Action::SendToWorkspace(5)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_7, Action::SendToWorkspace(6)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_8, Action::SendToWorkspace(7)),
        SimpleKeyMapping::new(MOD_KEY | ModMask::SHIFT, XK_9, Action::SendToWorkspace(8)),
        SimpleKeyMapping::new(
            MOD_KEY | ModMask::SHIFT,
            XK_Return,
            Action::Spawn("alacritty".to_owned(), vec![]),
        ),
        SimpleKeyMapping::new(
            MOD_KEY,
            XK_d,
            Action::Spawn(
                "dmenu_run".to_owned(),
                vec!["-i".into(), "-p".into(), "Run: ".into()],
            ),
        ),
        SimpleKeyMapping::new(
            ModMask::from(0u16),
            XK_Print,
            Action::Spawn(
                "bash".into(),
                vec![
                    "-c".into(),
                    // Piping through string pipes ('|') is not valid Rust, just send it to shell instead
                    "maim -s -u | xclip -selection clipboard -t image/png -i".into(),
                ],
            ),
        ),
    ]
}

/**
    Overrides specific character drawing.
    If some character needs icons from a certain render, they should be mapped below.
**/
fn init_char_remap() -> HashMap<heapless::String<UTF8_CHAR_MAX_BYTES>, FontCfg> {
    let mut icon_map = HashMap::new();
    let icon_font = FontCfg::new(
        "/usr/share/fonts/OTF/Font Awesome 6 Free-Solid-900.otf",
        "13.0",
    );
    let brand_font = FontCfg::new(
        "/usr/share/fonts/OTF/Font Awesome 6 Brands-Regular-400.otf",
        "13.0",
    );
    let _ = icon_map.insert(heapless::String::from("\u{f121}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f120}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f086}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{e007}"), brand_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f1bc}"), brand_font);
    let _ = icon_map.insert(heapless::String::from("\u{f11b}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f7d9}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f02b}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f02c}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f2db}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f538}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f019}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f093}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f502}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f304}"), icon_font.clone());
    let _ = icon_map.insert(heapless::String::from("\u{f073}"), icon_font);
    icon_map
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(feature = "config-file", serde(tag = "action", content = "args"))]
#[cfg_attr(test, derive(Eq, PartialEq))]
pub enum Action {
    Quit,
    Restart,
    Spawn(String, Vec<String>),
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

#[cfg(test)]
mod config_tests {
    #[cfg(feature = "config-file")]
    use crate::config::{validate_config, Cfg};

    #[cfg(feature = "config-file")]
    #[test]
    fn well_formed_cfg_passes_validation() {
        let mut cfg = Cfg::default();
        assert!(validate_config(&mut cfg).is_ok());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_no_workspaces() {
        let mut cfg = Cfg::default();
        cfg.workspaces.clear();
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_out_of_bounds_mapped_workspaces() {
        let mut cfg = Cfg::default();
        cfg.workspaces.truncate(1);
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_bad_status_bar_height() {
        let mut cfg = Cfg::default();
        cfg.sizing.status_bar_height = 0;
        assert!(validate_config(&mut cfg).is_err());
        cfg.sizing.status_bar_height = -5;
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_bad_tab_bar_height() {
        let mut cfg = Cfg::default();
        cfg.sizing.tab_bar_height = 0;
        assert!(validate_config(&mut cfg).is_err());
        cfg.sizing.tab_bar_height = -5;
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_bad_left_leader_modifier() {
        let mut cfg = Cfg::default();
        cfg.tiling_modifiers.left_leader = 0.0;
        assert!(validate_config(&mut cfg).is_err());
        cfg.tiling_modifiers.left_leader = -0.1;
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_bad_center_leader_modifier() {
        let mut cfg = Cfg::default();
        cfg.tiling_modifiers.center_leader = 0.0;
        assert!(validate_config(&mut cfg).is_err());
        cfg.tiling_modifiers.center_leader = -0.1;
        assert!(validate_config(&mut cfg).is_err());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_validate_bad_vertical_modifiers() {
        let mut cfg = Cfg::default();
        cfg.tiling_modifiers.vertically_tiled[5] = 0.0;
        assert!(validate_config(&mut cfg).is_err());
        cfg.tiling_modifiers.vertically_tiled[6] = -0.1;
        assert!(validate_config(&mut cfg).is_err());
        cfg.tiling_modifiers.vertically_tiled[5] = 0.1;
        assert!(validate_config(&mut cfg).is_err());
        cfg.tiling_modifiers.vertically_tiled[6] = 0.1;
        assert!(validate_config(&mut cfg).is_ok());
    }

    #[cfg(feature = "config-file")]
    #[test]
    fn will_complement_too_few_vertical_tiling_modifiers() {
        let mut cfg = Cfg::default();
        cfg.tiling_modifiers.vertically_tiled = Vec::new();
        assert!(validate_config(&mut cfg).is_ok());
        assert_eq!(
            super::NUM_TILING_MODIFIERS,
            cfg.tiling_modifiers.vertically_tiled.len()
        );
    }
}
