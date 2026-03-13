#![allow(dead_code)]

use palmscript::{
    compile, run_with_sources, Bar, CompileError, CompiledProgram, DiagnosticKind, Interval,
    Outputs, SourceFeed, SourceRuntimeConfig, VmLimits,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedDiagnostic {
    pub kind: DiagnosticKind,
    pub message: &'static str,
}

pub fn compile_error(source: &str) -> CompileError {
    compile(source).expect_err("expected compile error")
}

pub fn compile_diagnostics(source: &str) -> Vec<(DiagnosticKind, String)> {
    compile_error(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| (diagnostic.kind, diagnostic.message))
        .collect()
}

pub fn assert_compile_diagnostics(name: &str, source: &str, expected: &[ExpectedDiagnostic]) {
    let actual = compile_diagnostics(source);
    let expected = expected
        .iter()
        .map(|diagnostic| (diagnostic.kind.clone(), diagnostic.message.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "{name}");
}

pub const DEFAULT_SOURCE_ALIAS: &str = "src";
pub const DEFAULT_SOURCE_DECL: &str = "source src = binance.spot(\"BTCUSDT\")";
pub const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
pub const MINUTE_MS: i64 = 60_000;
pub const HOUR_MS: i64 = 60 * MINUTE_MS;
pub const DAY_MS: i64 = 24 * HOUR_MS;
pub const WEEK_MS: i64 = 7 * DAY_MS;

pub fn mirror_execution_decls(source: &str) -> String {
    if source
        .lines()
        .any(|line| line.trim_start().starts_with("execution "))
    {
        return source.to_string();
    }

    let mut out = Vec::new();
    for line in source.lines() {
        out.push(line.to_string());
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("source ") {
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            out.push(format!("{indent}execution {rest}"));
        }
    }
    out.join("\n")
}

pub fn with_single_source_interval(source: &str) -> String {
    with_single_source_intervals("1m", &[], source)
}

pub fn with_single_source_intervals(base: &str, supplemental: &[&str], source: &str) -> String {
    let mut legacy = format!("interval {base}\n");
    for interval in supplemental {
        legacy.push_str("use ");
        legacy.push_str(interval);
        legacy.push('\n');
    }
    legacy.push_str(source);
    upgrade_legacy_script(&legacy, DEFAULT_SOURCE_ALIAS)
}

pub fn upgrade_legacy_script(source: &str, alias: &str) -> String {
    let mut lines = Vec::new();
    let mut inserted_source = false;

    for line in source.lines() {
        lines.push(rewrite_legacy_use(line, alias));
        if !inserted_source && line.trim_start().starts_with("interval ") {
            lines.push(format!("source {alias} = binance.spot(\"BTCUSDT\")"));
            inserted_source = true;
        }
    }

    qualify_market_series(&lines.join("\n"), alias)
}

pub fn source_feed(interval: Interval, bars: Vec<Bar>) -> SourceFeed {
    SourceFeed {
        source_id: 0,
        interval,
        bars,
    }
}

pub fn source_runtime_config(
    base_interval: Interval,
    base_bars: Vec<Bar>,
    supplemental: Vec<SourceFeed>,
) -> SourceRuntimeConfig {
    let mut feeds = Vec::with_capacity(1 + supplemental.len());
    feeds.push(source_feed(base_interval, base_bars));
    feeds.extend(supplemental);
    SourceRuntimeConfig {
        base_interval,
        feeds,
    }
}

pub fn run_single_source(
    compiled: &CompiledProgram,
    base_interval: Interval,
    base_bars: Vec<Bar>,
    supplemental: Vec<SourceFeed>,
) -> Outputs {
    run_with_sources(
        compiled,
        source_runtime_config(base_interval, base_bars, supplemental),
        VmLimits::default(),
    )
    .expect("single-source runtime should succeed")
}

pub fn rising_bars(start_ms: i64, spacing_ms: i64, len: usize, start_close: f64) -> Vec<Bar> {
    (0..len)
        .map(|index| {
            let close = start_close + index as f64;
            Bar {
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + index as f64,
                time: (start_ms + spacing_ms * index as i64) as f64,
            }
        })
        .collect()
}

pub fn flat_bars(start_ms: i64, spacing_ms: i64, closes: &[f64]) -> Vec<Bar> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| Bar {
            open: *close - 0.5,
            high: *close + 1.0,
            low: *close - 1.0,
            close: *close,
            volume: 1_000.0 + index as f64,
            time: (start_ms + spacing_ms * index as i64) as f64,
        })
        .collect()
}

fn rewrite_legacy_use(line: &str, alias: &str) -> String {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("use ") {
        return line.to_string();
    }

    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];
    let rest = &trimmed["use ".len()..];
    let Some(interval) = rest.split_whitespace().next() else {
        return line.to_string();
    };
    if interval.contains('.') {
        return line.to_string();
    }
    format!("{indent}use {alias} {interval}")
}

fn qualify_market_series(source: &str, alias: &str) -> String {
    let mut out = String::with_capacity(source.len() + 32);
    let mut chars = source.char_indices().peekable();
    let mut previous = None;

    while let Some((index, ch)) = chars.next() {
        if ch == '/' && matches!(chars.peek(), Some((_, '/'))) {
            out.push(ch);
            let (_, next) = chars.next().expect("comment marker");
            out.push(next);
            for (_, ch) in chars.by_ref() {
                out.push(ch);
                if ch == '\n' {
                    previous = Some('\n');
                    break;
                }
            }
            continue;
        }

        if ch == '"' {
            out.push(ch);
            previous = Some(ch);
            let mut escaped = false;
            for (_, next) in chars.by_ref() {
                out.push(next);
                if escaped {
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == '"' {
                    break;
                }
            }
            continue;
        }

        if is_boundary(previous) {
            if let Some((token, replacement)) =
                interval_qualified_replacement(&source[index..], alias)
            {
                out.push_str(&replacement);
                for _ in 0..token.len().saturating_sub(1) {
                    let _ = chars.next();
                }
                previous = replacement.chars().last();
                continue;
            }
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = index;
            let mut end = index + ch.len_utf8();
            while let Some((next_index, next)) = chars.peek().copied() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    end = next_index + next.len_utf8();
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            let ident = &source[start..end];
            if is_market_field(ident) && is_boundary(previous) {
                out.push_str(alias);
                out.push('.');
                out.push_str(ident);
                previous = Some(ident.chars().last().unwrap_or('.'));
            } else {
                out.push_str(ident);
                previous = ident.chars().last();
            }
            continue;
        }

        out.push(ch);
        previous = Some(ch);
    }

    out
}

fn interval_qualified_replacement(source: &str, alias: &str) -> Option<(String, String)> {
    const INTERVALS: [&str; 16] = [
        "1s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
        "1w", "1M",
    ];
    const FIELDS: [&str; 6] = ["open", "high", "low", "close", "volume", "time"];

    for interval in INTERVALS {
        let Some(rest) = source.strip_prefix(interval) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('.') else {
            continue;
        };
        for field in FIELDS {
            let Some(remaining) = rest.strip_prefix(field) else {
                continue;
            };
            if remaining
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_')
            {
                continue;
            }
            let token = format!("{interval}.{field}");
            let replacement = format!("{alias}.{interval}.{field}");
            return Some((token, replacement));
        }
    }
    None
}

fn is_market_field(ident: &str) -> bool {
    matches!(ident, "open" | "high" | "low" | "close" | "volume" | "time")
}

fn is_boundary(previous: Option<char>) -> bool {
    !matches!(previous, Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}
