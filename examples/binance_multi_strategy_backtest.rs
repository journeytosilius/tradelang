use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use palmscript::{
    compile, fetch_source_runtime_config, run_backtest_with_sources, BacktestConfig,
    ExchangeEndpoints, PositionSide, SignalContract, VmLimits,
};

const YEAR_MS: i64 = 365 * 24 * 60 * 60 * 1_000;
const FOUR_HOUR_MS: i64 = 4 * 60 * 60 * 1_000;

fn main() -> Result<(), String> {
    let strategy_path = repo_path("examples/strategies/multi_strategy_backtest.palm");
    let source = fs::read_to_string(&strategy_path)
        .map_err(|err| format!("failed to read `{}`: {err}", strategy_path.display()))?;
    let compiled = compile(&source).map_err(|err| format!("compile failed: {err}"))?;

    let to_ms = aligned_now_ms(FOUR_HOUR_MS)?;
    let from_ms = to_ms - YEAR_MS;
    let runtime =
        fetch_source_runtime_config(&compiled, from_ms, to_ms, &ExchangeEndpoints::from_env())
            .map_err(|err| format!("market fetch failed: {err}"))?;
    let result = run_backtest_with_sources(
        &compiled,
        runtime.clone(),
        VmLimits::default(),
        BacktestConfig {
            execution_source_alias: "spot".to_string(),
            initial_capital: 10_000.0,
            fee_bps: 10.0,
            slippage_bps: 2.0,
            signals: SignalContract::default(),
        },
    )
    .map_err(|err| format!("backtest failed: {err}"))?;

    println!("strategy={}", strategy_path.display());
    println!("from_ms={from_ms}");
    println!("to_ms={to_ms}");
    println!("feed_count={}", runtime.feeds.len());
    for feed in &runtime.feeds {
        println!(
            "feed source_id={} interval={} bars={}",
            feed.source_id,
            feed.interval.as_str(),
            feed.bars.len()
        );
    }

    let mut trigger_counts = BTreeMap::<&str, usize>::new();
    for event in &result.outputs.trigger_events {
        *trigger_counts.entry(event.name.as_str()).or_default() += 1;
    }

    println!(
        "summary.starting_equity={:.2}",
        result.summary.starting_equity
    );
    println!("summary.ending_equity={:.2}", result.summary.ending_equity);
    println!("summary.realized_pnl={:.2}", result.summary.realized_pnl);
    println!(
        "summary.unrealized_pnl={:.2}",
        result.summary.unrealized_pnl
    );
    println!(
        "summary.total_return_pct={:.2}",
        result.summary.total_return * 100.0
    );
    println!("summary.trade_count={}", result.summary.trade_count);
    println!(
        "summary.win_rate_pct={:.2}",
        result.summary.win_rate * 100.0
    );
    println!("summary.max_drawdown={:.2}", result.summary.max_drawdown);
    println!(
        "summary.max_gross_exposure={:.2}",
        result.summary.max_gross_exposure
    );

    for (name, count) in trigger_counts {
        println!("triggers.{name}={count}");
    }

    if let Some(position) = &result.open_position {
        println!(
            "open_position.side={}",
            match position.side {
                PositionSide::Long => "long",
                PositionSide::Short => "short",
            }
        );
        println!("open_position.quantity={:.6}", position.quantity);
        println!("open_position.entry_price={:.2}", position.entry_price);
        println!("open_position.market_price={:.2}", position.market_price);
        println!(
            "open_position.unrealized_pnl={:.2}",
            position.unrealized_pnl
        );
    } else {
        println!("open_position=flat");
    }

    println!("recent_trades={}", result.trades.len().min(5));
    let recent_trades = result.trades.iter().rev().take(5).collect::<Vec<_>>();
    for trade in recent_trades.iter().rev() {
        println!(
            "trade side={} entry_time={} exit_time={} entry_price={:.2} exit_price={:.2} qty={:.6} pnl={:.2}",
            match trade.side {
                PositionSide::Long => "long",
                PositionSide::Short => "short",
            },
            trade.entry.time,
            trade.exit.time,
            trade.entry.price,
            trade.exit.price,
            trade.quantity,
            trade.realized_pnl
        );
    }

    Ok(())
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn aligned_now_ms(step_ms: i64) -> Result<i64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock error: {err}"))?;
    let now_ms = i64::try_from(now.as_millis()).map_err(|_| "time overflow".to_string())?;
    Ok(now_ms - now_ms.rem_euclid(step_ms))
}
