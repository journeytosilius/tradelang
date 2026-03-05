use tradelang::{Bar, Outputs};

pub fn fixture_bars(len: usize) -> Vec<Bar> {
    (0..len)
        .map(|index| {
            let close = 100.0 + index as f64;
            Bar {
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + index as f64,
                time: 1_700_000_000_000.0 + index as f64 * 60_000.0,
            }
        })
        .collect()
}

pub fn print_outputs(outputs: &Outputs) {
    let json = serde_json::to_string_pretty(outputs).expect("outputs serialize to json");
    println!("{json}");
}
