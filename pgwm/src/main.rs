#![deny(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::let_underscore_drop)]
#![allow(clippy::too_many_lines)]
// X11 uses inconsistent integer types fairly interchangeably
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
extern crate core;

pub(crate) mod error;
mod manager;
mod wm;
mod x11;

use crate::error::{Error, Result};
use crate::wm::run_wm;
use pgwm_core::debug;

fn main() -> Result<()> {
    debug!("Starting pgwm");
    loop {
        return match run_wm() {
            Ok(_) => {
                debug!("Exiting");
                Ok(())
            }
            Err(e) => {
                if let Error::FullRestart = e {
                    debug!("Restarting");
                    continue;
                }
                debug!("Fatal error {e}");
                Err(e)
            }
        };
    }
}
