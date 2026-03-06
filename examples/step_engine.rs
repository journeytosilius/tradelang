#[path = "support/mod.rs"]
mod support;

use palmscript::{compile, Engine, VmLimits};

fn main() {
    let source = "interval 1m\nif close > ema(close, 3) { plot(1) } else { plot(0) }";
    let compiled = compile(source).expect("script compiles");
    let bars = support::fixture_bars(8);
    let mut engine = Engine::new(compiled, VmLimits::default());

    println!("script:\n{source}");
    for bar in bars {
        let step = engine.run_step(bar).expect("engine step succeeds");
        let value = step.plots.first().and_then(|point| point.value);
        println!("bar {} -> plot {:?}", step.plots[0].bar_index, value);
    }

    let outputs = engine.finish();
    support::print_outputs(&outputs);
}
