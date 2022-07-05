#![cfg_attr(not(feature = "full"), allow(unused_macros))]

#[macro_use]
mod cfg;

#[macro_use]
mod loom;

#[macro_use]
mod pin;

#[macro_use]
mod ready;

#[macro_use]
mod thread_local;

cfg_trace! {
    #[macro_use]
    mod trace;
}

#[macro_use]
#[cfg(feature = "rt")]
pub(crate) mod scoped_tls;

cfg_macros! {
    #[macro_use]
    mod select;

    #[macro_use]
    mod join;

    #[macro_use]
    mod try_join;
}

// Includes re-exports needed to implement macros
#[doc(hidden)]
pub mod support;

cfg_sim! {
    #[macro_export]
    macro_rules! tprintln {
        () => {{
            println!()
        }};
        ($($arg:tt)*) => {
            println!("[{}] {}", $crate::time::SimTime::now(), format!($($arg)*))
        };
    }
    
    #[macro_export]
    macro_rules! tprint {
        () => {{
            print!()
        }};
        ($($arg:tt)*) => {
            print!("[{}], {}", $crate::time::SimTime::now(), format!($($arg)*))
        };
    }
}
