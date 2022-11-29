use alloc::string::String;
use alloc::vec::Vec;

use crate::config::DefaultDraw;

#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug, Clone)]
pub struct UserWorkspace {
    pub name: String,

    #[cfg_attr(feature = "config-file", serde(default))]
    pub mapped_class_names: Vec<String>,

    #[cfg_attr(feature = "config-file", serde(default))]
    pub default_draw: DefaultDraw,
}

impl UserWorkspace {
    pub(crate) const fn new(
        name: String,
        mapped_class_names: Vec<String>,
        default_draw: DefaultDraw,
    ) -> Self {
        Self {
            name,
            mapped_class_names,
            default_draw,
        }
    }
}
