#![no_std]
#![no_main]

use pgwm_app::main_loop;

#[unsafe(no_mangle)]
fn main() -> i32 {
    main_loop()
}
