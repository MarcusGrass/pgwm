#![no_std]
#![no_main]
// Start function, looks promising for stabilization https://github.com/rust-lang/rust/pull/93587
#![feature(naked_functions)]
// Seemingly moving towards stability https://github.com/rust-lang/rust/pull/102318
// Would be a lot nicer to just accept a symbol instead of forcing #[alloc_error_handler]
// Could fork the compiler and remove this check https://github.com/rust-lang/rust/blob/56074b5231ceef266a1097ea355f62c951e1b468/compiler/rustc_metadata/src/creader.rs#L1063 but ugh
#![feature(default_alloc_error_handler)]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

use dlmalloc::Dlmalloc;
use tiny_std::process::exit;
use unix_print::unix_eprintln;

use pgwm_app::main_loop;

#[global_allocator]
static ALLOCATOR: SingleThreadedAlloc = SingleThreadedAlloc::new();

struct SingleThreadedAlloc {
    inner: UnsafeCell<Dlmalloc>,
}

impl SingleThreadedAlloc {
    pub(crate) const fn new() -> Self {
        SingleThreadedAlloc {
            inner: UnsafeCell::new(Dlmalloc::new()),
        }
    }
}

unsafe impl GlobalAlloc for SingleThreadedAlloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        (*self.inner.get()).malloc(layout.size(), layout.align())
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        (*self.inner.get()).free(ptr, layout.size(), layout.align())
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        (*self.inner.get()).calloc(layout.size(), layout.align())
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        (*self.inner.get()).realloc(ptr, layout.size(), layout.align(), new_size)
    }
}

/// Extremely unsafe, this program is not thread safe at all will immediately segfault on more threads
unsafe impl Sync for SingleThreadedAlloc {}

unsafe impl Send for SingleThreadedAlloc {}

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    unix_eprintln!("{info}");
    exit(1)
}

/// Compiler complains about this symbol being missing for some reason
/// we don't unwind anyway so it shouldn't be needed.
/// # Safety
/// Just another necessary symbol
#[no_mangle]
pub unsafe extern "C" fn _Unwind_Resume() -> ! {
    exit(2);
}

#[no_mangle]
fn main() -> i32 {
    main_loop()
}
