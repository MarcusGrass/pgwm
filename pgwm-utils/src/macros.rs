#[macro_export]
#[cfg(feature = "debug")]
macro_rules! debug {
    ($($arg:tt)*) => {{
        tiny_std::eprintln!("[{}:L#{}] {}", file!(), line!(), format_args!($($arg)*));
    }}
}
#[macro_export]
#[cfg(not(feature = "debug"))]
macro_rules! debug {
    ($($arg:tt)*) => {{}};
}

#[macro_export]
macro_rules! from_error {
    ($from: ty, $to: ty, $tag: ident) => {
        impl From<$from> for $to {
            #[inline]
            fn from(e: $from) -> $to {
                Self::$tag(e)
            }
        }
    };
}
