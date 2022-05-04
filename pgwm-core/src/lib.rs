#![deny(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::let_underscore_drop)]
#![allow(clippy::needless_pass_by_value)]
// X11 uses inconsistent integer types fairly interchangeably
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
pub mod colors;
pub mod config;
pub mod error;
pub mod geometry;
pub mod render;
pub mod state;
#[cfg(feature = "status-bar")]
pub mod status;
pub mod util;

#[cfg(test)]
mod tests {
    use crate::config::Cfg;
    use crate::state::State;

    #[test]
    fn pin_stack_sizes() {
        let cfg_stack_size = std::mem::size_of::<Cfg>();
        let state_stack_size = std::mem::size_of::<State>();
        assert!(cfg_stack_size < 20_000);
        assert!(state_stack_size < 40_000);
        println!("{cfg_stack_size}");
        println!("{state_stack_size}");
    }
}
