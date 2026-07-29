// Load the instrumentation implementation as a normal Rust module. Using a
// module path keeps the `//!` comments at the top of lib.rs valid as inner
// module documentation, while the crate's Rust 2024 edition retains the
// callback-guard temporary lifetime fix.
pub mod diagnostics;

#[allow(unused_imports)]
#[path = "lib.rs"]
mod implementation;

pub use implementation::*;
