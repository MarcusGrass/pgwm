#[macro_export]
macro_rules! dbg_win {
    ($call_wrapper: expr, $win: expr) => {
        #[cfg(feature = "debug")]
        let _ = $call_wrapper.debug_window($win);
    };
}
