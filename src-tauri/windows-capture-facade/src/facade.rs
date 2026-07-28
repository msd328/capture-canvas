// Compile the instrumentation implementation as a nested module so its private
// delivery state remains encapsulated while the public facade modules are
// re-exported at the crate root.
mod implementation {
    include!("lib.rs");

    // `lib.rs` historically imported `Instant` at its top level while the encoder
    // module also imported it locally. Keep the top-level binding intentionally
    // used until the implementation file is split into smaller modules.
    const _: fn() -> Instant = Instant::now;
}

pub use implementation::*;
