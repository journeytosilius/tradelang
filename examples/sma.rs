#[path = "support/mod.rs"]
mod support;

use tradelang::{compile, run, VmLimits};

fn main() {
    let source = "plot(sma(close, 5))";
    let compiled = compile(source).expect("script compiles");
    let bars = support::fixture_bars(12);
    let outputs = run(&compiled, &bars, VmLimits::default()).expect("script runs");

    println!("script: {source}");
    support::print_outputs(&outputs);
}
