#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::needless_pass_by_value)]
#![allow(let_underscore_drop)]
#![allow(clippy::too_many_lines)]
// X11 uses inconsistent integer types fairly interchangeably
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
// Debug log complaints
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::struct_field_names)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use pgwm_utils::debug;
use tiny_std::{eprintln, println};

use crate::error::Error;
use crate::wm::run_wm;

pub(crate) mod error;
mod manager;
mod uring;
pub(crate) mod util;
mod wm;
mod x11;

#[must_use]
pub fn main_loop() -> i32 {
    debug!("Starting pgwm");
    loop {
        return match run_wm() {
            Ok(()) => {
                println!("Exiting WM");
                0
            }
            Err(e) => {
                if let Error::FullRestart = e {
                    debug!("Restarting WM");
                    continue;
                }
                eprintln!("Fatal error {e}");
                1
            }
        };
    }
}
