use alloc::format;
use alloc::string::String;
use core::fmt::{Formatter, Write};

use serde::de::{EnumAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use xcb_rust_protocol::proto::xproto::{ButtonIndexEnum, ModMask};

use crate::config::{Button, ButtonMask, Cfg, ModMaskEnum, ModMasks, WINDOW_MANAGER_NAME};
use crate::error::{Error, Result};

pub fn load_cfg(config_home: Option<&str>, home: Option<&str>) -> Result<Cfg> {
    if let Some(mut user_cfg_dir) = find_cfg_dir(config_home, home) {
        let _ = user_cfg_dir.write_fmt(format_args!(
            "/{WINDOW_MANAGER_NAME}/{WINDOW_MANAGER_NAME}.toml"
        ));
        pgwm_utils::debug!("Attempting config read at {user_cfg_dir}");
        let buf = tiny_std::fs::read(&user_cfg_dir)?;
        Ok(toml::from_slice(buf.as_slice())?)
    } else {
        Err(Error::ConfigDirFind)
    }
}

fn find_cfg_dir(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<String> {
    xdg_config_home
        .map(alloc::string::ToString::to_string)
        .or_else(|| home.map(|dir| format!("{dir}/.config")))
}

impl ModMaskEnum {
    pub(crate) fn to_mod_mask(&self) -> ModMask {
        match self {
            ModMaskEnum::Shift => ModMask::SHIFT,
            ModMaskEnum::Lock => ModMask::LOCK,
            ModMaskEnum::Control => ModMask::CONTROL,
            ModMaskEnum::M1 => ModMask::ONE,
            ModMaskEnum::M2 => ModMask::TWO,
            ModMaskEnum::M3 => ModMask::THREE,
            ModMaskEnum::M4 => ModMask::FOUR,
            ModMaskEnum::M5 => ModMask::FIVE,
            ModMaskEnum::Any => ModMask::ANY,
        }
    }
}

impl ButtonMask {
    pub(crate) fn to_button_index(&self) -> ButtonIndexEnum {
        match self {
            ButtonMask::Any => ButtonIndexEnum::ANY,
            ButtonMask::M1 => ButtonIndexEnum::ONE,
            ButtonMask::M2 => ButtonIndexEnum::TWO,
            ButtonMask::M3 => ButtonIndexEnum::THREE,
            ButtonMask::M4 => ButtonIndexEnum::FOUR,
            ButtonMask::M5 => ButtonIndexEnum::FIVE,
        }
    }
}

pub(crate) struct ModMaskVisitor;

impl<'de> Visitor<'de> for ModMaskVisitor {
    type Value = ModMasks;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("Expected an array of ModMaskEnums")
    }

    fn visit_seq<A>(self, mut seq: A) -> core::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut base = ModMask::from(0u16);
        while let Some(e) = seq.next_element::<ModMaskEnum>()? {
            base |= e.to_mod_mask();
        }
        Ok(ModMasks { inner: base })
    }
}

impl<'de> Deserialize<'de> for ModMasks {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(ModMaskVisitor)
    }
}

struct ButtonVisitor;

impl<'de> Visitor<'de> for ButtonVisitor {
    type Value = Button;

    fn expecting(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str("Expected one of the ButtonMask enum")
    }

    fn visit_enum<A>(self, data: A) -> core::result::Result<Self::Value, A::Error>
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
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
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
    use alloc::string::ToString;

    use std::env;
    use std::path::PathBuf;

    use crate::config::{Cfg, WINDOW_MANAGER_NAME};
    use crate::util::load_cfg::find_cfg_dir;

    #[test]
    fn will_read_environment_variables_to_find_config_falling_back() {
        assert!(find_cfg_dir(None, None).is_none());
        assert_eq!(
            Some("here/.config".to_string()),
            find_cfg_dir(None, Some("here"))
        );
        assert_eq!(
            Some("there".to_string()),
            find_cfg_dir(Some("there"), Some("here"))
        );
        assert_eq!(Some("there".to_string()), find_cfg_dir(Some("there"), None));
        assert!(find_cfg_dir(None, None).is_none());
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
        assert_eq!(cfg, default);
    }

    fn read_cfg_from_root() -> Cfg {
        let project_root = find_project_root();
        let cfg_path = project_root.join(alloc::format!("{WINDOW_MANAGER_NAME}.toml"));
        let cfg = std::fs::read(cfg_path).unwrap();
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
                    let children = std::fs::read_dir(dir_entry.path()).unwrap();
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
