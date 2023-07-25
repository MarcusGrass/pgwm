use crate::config::DefaultDraw;

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Debug, Clone)]
pub struct UserWorkspace {
    pub name: &'static str,

    pub mapped_class_names: &'static [&'static str],

    pub default_draw: DefaultDraw,
}

impl UserWorkspace {
    pub(crate) const fn new(
        name: &'static str,
        mapped_class_names: &'static [&'static str],
        default_draw: DefaultDraw,
    ) -> Self {
        Self {
            name,
            mapped_class_names,
            default_draw,
        }
    }
}
