//! # Slog -  Structured, composable logging for Rust
//!
//! ## Features
//!
//! * easy to use
//! * great performance; see: [slog bench log](https://github.com/dpc/slog-rs/wiki/Bench-log)
//! * `#![no_std]` support (with opt-out `std` cargo feature flag)
//! * hierarchical loggers
//! * lazily evaluated values
//! * modular, lightweight and very extensible
//! 	* tiny core create that does not pull any other dependencies
//! 	* feature-crates for specific functionality
//! * backward compatibility for standard `log` crate (`slog-stdlog` crate)
//! 	* supports logging-scopes
//! 	* using slog in library does not force users of the library to use slog
//! 	  (but gives benefits); see `crates/example-lib`
//! * drains & output formatting
//! 	* filtering
//! 		* compile-time log level filter using cargo features (same as in `log` crate)
//! 		* by level, msg, and any other meta-data
//! 		* [`slog-envlogger`](https://github.com/dpc/slog-envlogger) - port of `env_logger`
//! 	* multiple outputs
//! 	* asynchronous IO writing
//! 	* terminal output, with color support (`slog-term` crate)
//! 	* Json (`slog-json` crate)
//! 		* Bunyan (`slog-bunyan` crate)
//! 	* syslog (`slog-syslog` crate)
//! 	* first class custom drains
//!
//! ## Advantages over `log` crate
//!
//! * **extensible** - `slog` provides core functionality, and some standard
//!   feature-set. But using traits, anyone can easily implement as
//!   powerful fully-custom features, publish separately and grow `slog` feature-set
//!   for everyone.
//! * **composable** - Wouldn't it be nice if you could use
//!   [`env_logger`][env_logger], but output authentication messages to syslog,
//!   while reporting errors over network in json format? With `slog` drains can
//!   reuse other drains! You can combine them together, chain, wrap - you name it.
//! * **context aware** - It's not just one global logger. Hierarchical
//!   loggers carry information about context of logging. When logging an error
//!   condition, you want to know which resource was being handled, on which
//!   instance of your service, using which source code build, talking with what
//!   peer, etc. In standard `log` you would have to repeat this information in
//!   every log statement. In `slog` it will happen automatically. See
//!   [slog-rs functional overview page][functional-overview] to understand better
//!   logger and drain hierarchies and log record flow through them.
//! * both **human and machine readable** - By keeping the key-value data format,
//!   meaning of logging data is preserved. Dump your logging to a JSON file, and
//!   send it to your data-mining system for further analysis. Don't parse it from
//!   lines of text anymore!
//! * **lazy evaluation** and **asynchronous IO** included. Waiting to
//!   finish writing logging information to disk, or spending time calculating
//!   data that will be thrown away at the current logging level, are sources of big
//!   performance waste. Use [`AsyncStreamer`][async-streamer] drain, and closures
//!   to make your logging fast.
//! * **run-time configuration** - [`AtomicSwitch`][atomic-switch] drain allows
//!   changing logging behavior in the running program. You could use eg. signal
//!   handlers to change logging level or logging destinations. See
//!   [`signal` example][signal].
//!
//! ## Notable details
//!
//! `slog` by default removes at compile time trace and debug level statements
//! in release builds, and trace level records in debug builds. This makes
//! `trace` and `debug` level logging records practically free, which should
//! encourage using them freely. If you want to enable trace/debug messages
//! or raise the compile time logging level limit, use the following in your
//! `Cargo.toml`:
//!
//! ```norust
//! slog = { version = "1.0.0", features = ["max_level_trace", "release_max_level_warn"] }
//! ```
//!
//! Due to the `macro_rules` limitation log macros syntax comes in several
//! versions. See `log!` macro, and pay attention to `;` and `,`
//! details.
//!
//! Root drain (passed to `Logger::root`) must be one that does not ever
//! return errors, which forces user to pick error handing strategy. You
//! can use `.fuse()` or `.ignore_err()` methods from `DrainExt` to do
//! it conveniently.
//!
//! [signal]: https://github.com/dpc/slog-rs/blob/master/examples/signal.rs
//! [env_logger]: https://crates.io/crates/env_logger
//! [functional-overview]: https://github.com/dpc/slog-rs/wiki/Functional-overview
//! [async-streamer]: http://dpc.pw/slog-rs/slog/drain/struct.AsyncStreamer.html
//! [atomic-switch]: http://dpc.pw/slog-rs/slog/drain/struct.AtomicSwitch.html

#![cfg_attr(not(feature = "std"), feature(alloc))]
#![cfg_attr(not(feature = "std"), feature(collections))]
#![warn(missing_docs)]

#![no_std]

#[macro_use]
#[cfg(feature = "std")]
extern crate std;
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
extern crate collections;

use core::str::FromStr;
use core::fmt;
use core::result;

#[cfg(feature = "std")]
use std::sync::Arc;
#[cfg(not(feature = "std"))]
use alloc::arc::Arc;

#[cfg(feature = "std")]
use std::slice;
#[cfg(not(feature = "std"))]
use core::slice;

#[cfg(feature = "std")]
use std::vec::Vec;
#[cfg(not(feature = "std"))]
use collections::vec::Vec;

#[cfg(feature = "std")]
use std::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::boxed::Box;

/// Convenience function for building `&[OwnedKeyValue]`
///
/// ```
/// #[macro_use]
/// extern crate slog;
///
/// fn main() {
///     let drain = slog::Discard;
///     let root = slog::Logger::root(drain, o!("key1" => "value1", "key2" => "value2"));
/// }
/// ```
#[macro_export]
macro_rules! o(
    () => {
        vec![]
    };
    ($($k:expr => $v:expr),*) => {
        {
        vec![$(($k, std::boxed::Box::new($v))),*]
        }
    };
);

/// Log message of a given level
///
/// Use wrappers `error!`, `warn!` etc. instead
///
/// The `max_level_*` features can be used to statically disable logging at
/// various levels.
///
/// Use longer name version macros if you want to prevent clash with legacy `log`
/// crate macro names (see `examples/alternative_names.rs`).
///
/// The following invocations are supported.
///
/// Simple:
///
/// ```
/// #[macro_use]
/// extern crate slog;
///
/// fn main() {
///     let drain = slog::Discard;
///     let root = slog::Logger::root(drain, o!("key1" => "value1", "key2" => "value2"));
///     info!(root, "test info log"; "log-key" => true);
/// }
/// ```
///
/// Note that `"key" => value` part is optional.
///
/// ```
/// #[macro_use]
/// extern crate slog;
///
/// fn main() {
///     let drain = slog::Discard;
///     let root = slog::Logger::root(drain, o!("key1" => "value1", "key2" => "value2"));
///     info!(root, "test info log");
/// }
/// ```
///
/// Formatting support:
///
/// ```
/// #[macro_use]
/// extern crate slog;
///
/// fn main() {
///     let drain = slog::Discard;
///     let root = slog::Logger::root(drain, o!("key1" => "value1", "key2" => "value2"));
///     info!(root, "log-key" => true; "formatted: {}", 1);
/// }
/// ```
///
/// Again, `"key" => value` is optional.
///
/// ```
/// #[macro_use]
/// extern crate slog;
///
/// fn main() {
///     let drain = slog::Discard;
///     let root = slog::Logger::root(drain, o!("key1" => "value1", "key2" => "value2"));
///     info!(root, "formatted: {}", 1);
/// }
/// ```

#[macro_export]
macro_rules! log(
    ($lvl:expr, $l:expr, $($k:expr => $v:expr),*; $($args:tt)+ ) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!($($args)+), &[$(($k, &$v)),*]))
        }
    };
    ($lvl:expr, $l:expr, $($args:tt),+) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!($($args),+), &[]))
        }
    };
    ($lvl:expr, $l:expr, $msg:expr) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!("{}", $msg), &[]))
        }
    };
    ($lvl:expr, $l:expr, $msg:expr; $($k:expr => $v:expr),*) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!("{}", $msg), &[$(($k, &$v)),*]))
        }
    };
);

/// Log message of a given level (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_log(
    ($lvl:expr, $l:expr, $($k:expr => $v:expr),*; $($args:tt)+ ) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!($($args)+), &[$(($k, &$v)),*]))
        }
    };
    ($lvl:expr, $l:expr, $($args:tt),+) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!($($args),+), &[]))
        }
    };
    ($lvl:expr, $l:expr, $msg:expr) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!("{}", $msg), &[]))
        }
    };
    ($lvl:expr, $l:expr, $msg:expr; $($k:expr => $v:expr),*) => {
        if $lvl.as_usize() <= $crate::__slog_static_max_level().as_usize() {
            // prevent generating big `Record` over and over
            static RS : $crate::RecordStatic<'static> = $crate::RecordStatic {
                level: $lvl,
                file: file!(),
                line: line!(),
                column: column!(),
                function: "",
                module: module_path!(),
                target: module_path!(),
            };
            $l.log(&$crate::Record::new(&RS, format_args!("{}", $msg), &[$(($k, &$v)),*]))
        }
    };
);

/// Log critical level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! crit(
    ($($args:tt)+) => {
        log!($crate::Level::Critical, $($args)+)
    };
);

/// Log critical level record (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_crit(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Critical, $($args)+)
    };
);

/// Log error level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! error(
    ($($args:tt)+) => {
        log!($crate::Level::Error, $($args)+)
    };
);

/// Log error level record
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_error(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Error, $($args)+)
    };
);


/// Log warning level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! warn(
    ($($args:tt)+) => {
        log!($crate::Level::Warning, $($args)+)
    };
);

/// Log warning level record (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_warn(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Warning, $($args)+)
    };
);

/// Log info level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! info(
    ($($args:tt)+) => {
        log!($crate::Level::Info, $($args)+)
    };
);

/// Log info level record (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_info(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Info, $($args)+)
    };
);

/// Log debug level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! debug(
    ($($args:tt)+) => {
        log!($crate::Level::Debug, $($args)+)
    };
);

/// Log debug level record (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_debug(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Debug, $($args)+)
    };
);


/// Log trace level record
///
/// See `log` for documentation.
#[macro_export]
macro_rules! trace(
    ($($args:tt)+) => {
        log!($crate::Level::Trace, $($args)+)
    };
);

/// Log trace level record (alias)
///
/// Prefer shorter version, unless it clashes with
/// existing `log` crate macro.
///
/// See `log` for documentation.
#[macro_export]
macro_rules! slog_trace(
    ($($args:tt)+) => {
        slog_log!($crate::Level::Trace, $($args)+)
    };
);

/// Serialization
pub mod ser;

pub use ser::{PushLazy, ValueSerializer, Serializer, Serialize};

include!("_level.rs");
include!("_logger.rs");
include!("_drain.rs");

/// Key value pair that can be part of a logging record
pub type BorrowedKeyValue<'a> = (&'static str, &'a ser::Serialize);

/// Key value pair that can be owned by `Logger`
///
/// See `o!(...)` macro.
pub type OwnedKeyValue = (&'static str, Box<ser::SyncSerialize>);

/// Chain of `OwnedKeyValue`-s of a `Logger` and its ancestors
pub struct OwnedKeyValueList {
    parent: Option<Arc<OwnedKeyValueList>>,
    values: Vec<OwnedKeyValue>,
}

impl OwnedKeyValueList {
    /// New `OwnedKeyValue` with a parent
    pub fn new(values: Vec<OwnedKeyValue>, parent: Arc<OwnedKeyValueList>) -> Self {
        OwnedKeyValueList {
            parent: Some(parent),
            values: values,
        }
    }

    /// New `OwnedKeyValue` without a parent (root)
    pub fn root(values: Vec<OwnedKeyValue>) -> Self {
        OwnedKeyValueList {
            parent: None,
            values: values,
        }
    }

    /// Get the parent element on the chain of values
    pub fn parent(&self) -> &Option<Arc<OwnedKeyValueList>> {
        &self.parent
    }

    /// Get the top-node values
    ///
    /// Since `OwnedKeyValueList` is just a chain of `Vec<OwnedKeyValue>`
    /// it's possible to iterate through it group-by-group.
    pub fn values(&self) -> &Vec<OwnedKeyValue> {
        &self.values
    }

    /// Iterator over `OwnedKeyValue`-s
    pub fn iter(&self) -> OwnedKeyValueListIterator {
        OwnedKeyValueListIterator::new(self)
    }
}

/// Iterator over `OwnedKeyValue`-s
pub struct OwnedKeyValueListIterator<'a> {
    next_node: &'a Option<Arc<OwnedKeyValueList>>,
    iter: slice::Iter<'a, OwnedKeyValue>,
}

impl<'a> OwnedKeyValueListIterator<'a> {
    fn new(node: &'a OwnedKeyValueList) -> Self {
        OwnedKeyValueListIterator {
            next_node: &node.parent,
            iter: node.values.iter(),
        }
    }
}

impl<'a> Iterator for OwnedKeyValueListIterator<'a> {
    type Item = &'a OwnedKeyValue;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(x) => return Some(&*x),
                None => {
                    match *self.next_node {
                        Some(ref node) => {
                            self.iter = node.values.iter();
                            self.next_node = &node.parent;
                        }
                        None => return None,
                    }
                }
            }
        }
    }
}

#[allow(unknown_lints)]
#[allow(inline_always)]
#[inline(always)]
#[doc(hidden)]
/// Not an API
pub fn __slog_static_max_level() -> FilterLevel {
    if !cfg!(debug_assertions) {
        if cfg!(feature = "release_max_level_off") {
            return FilterLevel::Off;
        } else if cfg!(feature = "release_max_level_error") {
            return FilterLevel::Error;
        } else if cfg!(feature = "release_max_level_warn") {
            return FilterLevel::Warning;
        } else if cfg!(feature = "release_max_level_info") {
            return FilterLevel::Info;
        } else if cfg!(feature = "release_max_level_debug") {
            return FilterLevel::Debug;
        } else if cfg!(feature = "release_max_level_trace") {
            return FilterLevel::Trace;
        }
    }
    if cfg!(feature = "max_level_off") {
        FilterLevel::Off
    } else if cfg!(feature = "max_level_error") {
        FilterLevel::Error
    } else if cfg!(feature = "max_level_warn") {
        FilterLevel::Warning
    } else if cfg!(feature = "max_level_info") {
        FilterLevel::Info
    } else if cfg!(feature = "max_level_debug") {
        FilterLevel::Debug
    } else if cfg!(feature = "max_level_trace") {
        FilterLevel::Trace
    } else {
        if !cfg!(debug_assertions) {
            FilterLevel::Info
        } else {
            FilterLevel::Debug
        }
    }
}

#[cfg(test)]
mod tests;
