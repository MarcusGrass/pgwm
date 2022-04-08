use crate::config::{Button, ButtonMask, Cfg, ModMaskEnum, ModMasks, WINDOW_MANAGER_NAME};
use crate::error::{Error, Result};
use serde::de::{EnumAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use std::path::PathBuf;
use x11rb::protocol::xproto::{ButtonIndex, ModMask};

pub(crate) fn load_cfg() -> Result<Cfg> {
    if let Some(user_cfg_dir) = find_cfg_dir() {
        let wm_cfg_dir = user_cfg_dir.join(WINDOW_MANAGER_NAME);
        let file_path = wm_cfg_dir.join(format!("{WINDOW_MANAGER_NAME}.toml"));
        crate::debug!("Attempting config read at {file_path:?}");
        match std::fs::read(&file_path) {
            Ok(content) => Ok(toml::from_slice(content.as_slice())?),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Err(Error::ConfigFileFind)
                } else {
                    Err(e.into())
                }
            }
        }
    } else {
        Err(Error::ConfigDirFind)
    }
}

fn find_cfg_dir() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
                .ok()
        })
}

impl ModMaskEnum {
    pub(crate) fn to_mod_mask(&self) -> ModMask {
        match self {
            ModMaskEnum::Shift => ModMask::SHIFT,
            ModMaskEnum::Lock => ModMask::LOCK,
            ModMaskEnum::Control => ModMask::CONTROL,
            ModMaskEnum::M1 => ModMask::M1,
            ModMaskEnum::M2 => ModMask::M2,
            ModMaskEnum::M3 => ModMask::M3,
            ModMaskEnum::M4 => ModMask::M4,
            ModMaskEnum::M5 => ModMask::M5,
            ModMaskEnum::Any => ModMask::ANY,
        }
    }
}

impl ButtonMask {
    pub(crate) fn to_button_index(&self) -> ButtonIndex {
        match self {
            ButtonMask::Any => ButtonIndex::ANY,
            ButtonMask::M1 => ButtonIndex::M1,
            ButtonMask::M2 => ButtonIndex::M2,
            ButtonMask::M3 => ButtonIndex::M3,
            ButtonMask::M4 => ButtonIndex::M4,
            ButtonMask::M5 => ButtonIndex::M5,
        }
    }
}
pub(crate) struct ModMaskVisitor;

impl<'de> Visitor<'de> for ModMaskVisitor {
    type Value = ModMasks;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("Expected an array of ModMaskEnums")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut base = ModMask::from(0u16);
        while let Some(e) = seq.next_element::<ModMaskEnum>()? {
            base = base | e.to_mod_mask();
        }
        Ok(ModMasks { inner: base })
    }
}

impl<'de> Deserialize<'de> for ModMasks {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(ModMaskVisitor)
    }
}
struct ButtonVisitor;

impl<'de> Visitor<'de> for ButtonVisitor {
    type Value = Button;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("Expected one of the ButtonMask enum")
    }

    fn visit_enum<A>(self, data: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: EnumAccess<'de>,
    {
        let (mask, _) = data.variant::<ButtonMask>()?;
        Ok(Button {
            inner: mask.to_button_index(),
        })
    }
}

impl<'de> Deserialize<'de> for Button {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_enum(
            "ButtonMask",
            &["Any", "M1", "M2", "M3", "M4", "M5"],
            ButtonVisitor,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{Cfg, WINDOW_MANAGER_NAME};
    use crate::util::load_cfg::find_cfg_dir;
    use std::env;
    use std::path::PathBuf;

    #[test]
    fn will_read_environment_variables_to_find_config_falling_back() {
        env::remove_var("XDG_CONFIG_HOME");
        env::remove_var("HOME");
        assert!(find_cfg_dir().is_none());
        env::set_var("HOME", "here");
        assert_eq!(Some(PathBuf::from("here/.config")), find_cfg_dir());
        env::set_var("XDG_CONFIG_HOME", "there");
        assert_eq!(Some(PathBuf::from("there")), find_cfg_dir());
        env::remove_var("HOME");
        assert_eq!(Some(PathBuf::from("there")), find_cfg_dir());
        env::remove_var("XDG_CONFIG_HOME");
        assert!(find_cfg_dir().is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn can_deserialize_cfg() {
        read_cfg_from_root();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    // Relax this as the default shipping config shouldn't be my weird config
    fn example_cfg_is_same_as_default() {
        let cfg = read_cfg_from_root();
        let default = Cfg::default();
        assert_eq!(default.sizing, cfg.sizing);
        assert_eq!(default.colors, cfg.colors);
        assert_eq!(default.mouse_mappings, cfg.mouse_mappings);
        #[cfg(feature = "status-bar")]
        assert_eq!(default.key_mappings, cfg.key_mappings);
        assert_eq!(default.char_remap, cfg.char_remap);
        assert_eq!(default.fonts, cfg.fonts);
        assert_eq!(default.workspaces, cfg.workspaces);
    }

    fn read_cfg_from_root() -> Cfg {
        let project_root = find_project_root();
        let cfg_path = project_root.join(format!("{WINDOW_MANAGER_NAME}.toml"));
        let cfg = std::fs::read(&cfg_path).unwrap();
        toml::from_slice(cfg.as_slice()).unwrap()
    }

    fn find_project_root() -> PathBuf {
        let pb = env::current_dir().unwrap();
        let mut search_dir = Some(pb);
        while let Some(search) = search_dir.take() {
            let parent = std::fs::read_dir(&search).unwrap();
            for dir_entry in parent {
                let dir_entry = dir_entry.unwrap();
                let meta = dir_entry.metadata().unwrap();
                if meta.is_dir() && dir_entry.file_name() == WINDOW_MANAGER_NAME {
                    let children = std::fs::read_dir(&dir_entry.path()).unwrap();
                    for child in children {
                        let child = child.unwrap();
                        if child.file_name() == "Cargo.lock" {
                            return dir_entry.path();
                        }
                    }
                }
            }
            search_dir = search.parent().map(PathBuf::from)
        }

        panic!("Could not find project root")
    }
}
