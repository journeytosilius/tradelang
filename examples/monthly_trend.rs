#[path = "support/mod.rs"]
mod support;

use palmscript::{compile, run_multi_interval, Interval, MultiIntervalConfig, VmLimits};

fn main() {
    let source = "interval 1w\nuse 1M\nuse 1d\nif 1M.close > 1M.close[1] and 1d.volume > 1d.volume[1] { plot(1) } else { plot(0) }";
    let compiled = compile(source).expect("script compiles");
    let base_bars = support::flat_bars(
        support::JAN_1_2024_UTC_MS,
        support::WEEK_MS,
        &[
            100.0, 100.0, 100.0, 100.0, 101.0, 101.0, 101.0, 101.0, 102.0,
        ],
    );
    let config = MultiIntervalConfig {
        base_interval: Interval::Week1,
        supplemental: vec![
            support::monthly_feed(&[100.0, 120.0, 110.0]),
            support::daily_feed(
                support::JAN_1_2024_UTC_MS,
                &[
                    1_000.0, 1_010.0, 1_020.0, 1_030.0, 1_040.0, 1_050.0, 1_060.0, 1_070.0,
                    1_080.0, 1_090.0, 1_100.0, 1_110.0, 1_120.0, 1_130.0, 1_140.0, 1_150.0,
                    1_160.0, 1_170.0, 1_180.0, 1_190.0, 1_200.0, 1_210.0, 1_220.0, 1_230.0,
                    1_240.0, 1_250.0, 1_260.0, 1_270.0, 1_280.0, 1_290.0, 1_300.0, 1_310.0,
                    1_320.0, 1_330.0, 1_340.0, 1_350.0, 1_360.0, 1_370.0, 1_380.0, 1_390.0,
                    1_400.0, 1_410.0, 1_420.0, 1_430.0, 1_440.0, 1_450.0, 1_460.0, 1_470.0,
                    1_480.0, 1_490.0, 1_500.0, 1_510.0, 1_520.0, 1_530.0, 1_540.0, 1_550.0,
                    1_560.0, 1_570.0, 1_580.0, 1_590.0, 1_600.0, 1_610.0, 1_620.0,
                ],
            ),
        ],
    };
    let outputs =
        run_multi_interval(&compiled, &base_bars, config, VmLimits::default()).expect("runs");

    println!("script:\n{source}");
    support::print_step_values("monthly trend with daily participation filter:", &outputs);
    support::print_outputs(&outputs);
}
