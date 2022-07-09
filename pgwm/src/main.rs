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

pub(crate) mod error;
mod manager;
pub(crate) mod util;
mod wm;
mod x11;

use crate::error::{Error, Result};
use crate::wm::run_wm;
use pgwm_core::debug;
use std::path::PathBuf;

fn main() -> Result<()> {
    debug!("Starting pgwm");
    if check_cfg() {
        return Ok(());
    }
    loop {
        return match run_wm() {
            Ok(_) => {
                debug!("Exiting WM");
                Ok(())
            }
            Err(e) => {
                if let Error::FullRestart = e {
                    debug!("Restarting WM");
                    continue;
                }
                debug!("Fatal error {e}");
                Err(e)
            }
        };
    }
}

fn check_cfg() -> bool {
    let mut args = std::env::args();
    args.next(); // drain program argument
    if let Some(k) = args.next() {
        if k.as_str() == "--check-cfg" {
            match pgwm_core::config::Cfg::new() {
                Ok(cfg) => {
                    let collected_fonts = cfg
                        .fonts
                        .get_all_font_paths()
                        .into_iter()
                        .chain(cfg.char_remap.values().map(|v| PathBuf::from(&v.path)));
                    for font in collected_fonts {
                        match std::fs::metadata(&font) {
                            Ok(meta) => {
                                if !meta.is_file() {
                                    eprintln!("Invalid configuration, specified font {font:?} points to something that isn't a file.");
                                }
                            }
                            Err(e) => {
                                eprintln!("Invalid configuration, could not read metadata for font {font:?}: {e}");
                            }
                        }
                    }
                    println!("Configuration valid!");
                }
                Err(e) => {
                    println!("Invalid configuration: {e}");
                }
            }
        } else {
            println!("The only valid argument is `--check-cfg` to check if configuration is valid and can be found");
        }
        true
    } else {
        false
    }
}
