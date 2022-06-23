#[macro_export]
macro_rules! push_heapless {
    ($heapless_vec: expr,$push_item: expr) => {
        $heapless_vec
            .push($push_item)
            .map_err(|_| $crate::error::Error::HeaplessPush($heapless_vec.len()))
    };
}

#[macro_export]
macro_rules! hvec {
    () => {
        {
            Ok::<_, $crate::error::Error>(heapless::Vec::new())
        }
    };
    ( $( $x: expr),*) => {
        {
            let mut heapless_vec = heapless::Vec::new();
            let mut any_err = false;
            $(
                if heapless_vec.push($x).is_err() {
                    any_err = true;
                }
            )*
            if any_err {
                Err($crate::error::Error::HeaplessInstantiate)
            } else {
                Ok(heapless_vec)
            }
        }
    };
    ($elem: expr ; $n : expr) => {
        {
            let mut heapless_vec = heapless::Vec::new();
            let mut any_err = false;
            for _ in 0..$n {
                if heapless_vec.push($elem).is_err() {
                    any_err = true;
                }
            }
            if any_err {
                Err($crate::error::Error::HeaplessInstantiate)
            } else {
                Ok(heapless_vec)
            }
        }
    }
}
#[macro_export]
#[cfg(feature = "debug")]
macro_rules! debug {
    ($($arg:tt)*) => {{
        eprintln!("[{}:L#{}] {}", file!(), line!(), format_args!($($arg)*));
    }}
}
#[macro_export]
#[cfg(not(feature = "debug"))]
macro_rules! debug {
    ($($arg:tt)*) => {{}};
}

#[macro_export]
macro_rules! format_heapless {
    ($($arg:tt)*) => {{
        let mut s = heapless::String::new();
        let _ = core::fmt::write(&mut s, format_args!($($arg)*));
        s
    }}
}
