#[path = "support/mod.rs"]
mod support;

use palmscript::{compile, run, VmLimits};

fn main() {
    let source = "interval 1m\nplot(sma(close, 5))";
    let compiled = compile(source).expect("script compiles");
    let bars = support::fixture_bars(12);
    let outputs = run(&compiled, &bars, VmLimits::default()).expect("script runs");

    println!("script:\n{source}");
    support::print_outputs(&outputs);
}
