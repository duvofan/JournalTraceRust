//! Opens the UI with generated entries. Needs no administrator rights.
//!
//!     cargo run --release --example demo

fn main() -> eframe::Result {
    journaltrace::run_demo()
}
