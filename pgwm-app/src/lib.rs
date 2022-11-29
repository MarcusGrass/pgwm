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
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use unix_print::{unix_eprintln, unix_println};

use pgwm_utils::debug;

use crate::error::Error;
use crate::wm::run_wm;

pub(crate) mod error;
mod manager;
pub(crate) mod util;
mod wm;
mod x11;

#[must_use]
pub fn main_loop() -> i32 {
    debug!("Starting pgwm");
    if check_cfg() {
        return 0;
    }
    loop {
        return match run_wm() {
            Ok(_) => {
                debug!("Exiting WM");
                0
            }
            Err(e) => {
                if let Error::FullRestart = e {
                    debug!("Restarting WM");
                    continue;
                }
                debug!("Fatal error {e}");
                1
            }
        };
    }
}

fn check_cfg() -> bool {
    let mut arg_iter = tiny_std::env::args();
    let _ = arg_iter.next(); // drain program argument
    if let Some(Ok(k)) = arg_iter.next() {
        if k == "--check-cfg" {
            #[cfg(feature = "config-file")]
            match pgwm_core::util::load_cfg::load_cfg(
                tiny_std::env::var("XDG_CONFIG_HOME").ok(),
                tiny_std::env::var("HOME").ok(),
            ) {
                Ok(cfg) => {
                    let collected_fonts = cfg
                        .fonts
                        .get_all_font_paths()
                        .into_iter()
                        .chain(cfg.char_remap.values().map(|v| v.path.clone()));
                    for font in collected_fonts {
                        unix_println!("Checking font at {font}");
                        match tiny_std::fs::metadata(&font) {
                            Ok(meta) => {
                                if meta.is_file() {
                                    unix_println!("Found font.");
                                } else {
                                    unix_eprintln!("Invalid configuration, path for font {font} which is not a file");
                                    return true;
                                }
                            }
                            Err(e) => {
                                unix_eprintln!("Failed to check font file for font {font} {e}");
                                return true;
                            }
                        }
                    }
                    unix_println!("Configuration valid!");
                }
                Err(e) => {
                    unix_println!("Invalid configuration: {e}");
                }
            }
            #[cfg(not(feature = "config-file"))]
            {
                unix_eprintln!(
                    "Cannot check configuration if not compiled with 'config-file' feature"
                );
                return true;
            }
        }
        unix_println!("The only valid argument is `--check-cfg` to check if configuration is valid and can be found");
        true
    } else {
        false
    }
}
