#![deny(unsafe_code)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(let_underscore_drop)]
#![allow(clippy::needless_pass_by_value)]
// X11 uses inconsistent integer types fairly interchangeably
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_field_names)]
// Debug log complaints
#![allow(clippy::used_underscore_binding)]
#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod colors;
pub mod config;
pub mod error;
pub mod geometry;
pub mod render;
pub mod state;
#[cfg(feature = "status-bar")]
pub mod status;
pub mod util;
