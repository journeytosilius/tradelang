use palmscript::lexer::lex;
use palmscript::{Interval, TokenKind};

#[test]
fn lexes_fn_keyword() {
    let tokens = lex("fn helper() = close > open").expect("source should lex");
    assert!(matches!(tokens[0].kind, TokenKind::Fn));
}

#[test]
fn reserves_fn_as_keyword() {
    let tokens = lex("let fn = 1").expect("source should lex");
    assert!(matches!(tokens[1].kind, TokenKind::Fn));
}

#[test]
fn reserves_export_trigger_interval_and_use_as_keywords() {
    let tokens = lex("let export = 1\nlet trigger = 2\nlet interval = 3\nlet use = 4")
        .expect("source should lex");
    assert!(matches!(tokens[1].kind, TokenKind::Export));
    assert!(matches!(tokens[6].kind, TokenKind::Trigger));
    assert!(matches!(tokens[11].kind, TokenKind::IntervalKw));
    assert!(matches!(tokens[16].kind, TokenKind::Use));
}

#[test]
fn lexes_all_binance_intervals() {
    let source = "1s.close 1m.close 3m.close 5m.close 15m.close 30m.close 1h.close 2h.close 4h.close 6h.close 8h.close 12h.close 1d.close 3d.close 1w.close 1M.close";
    let tokens = lex(source).expect("intervals should lex");
    let intervals: Vec<Interval> = tokens
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::Interval(interval) => Some(interval),
            _ => None,
        })
        .collect();
    assert_eq!(
        intervals,
        vec![
            Interval::Sec1,
            Interval::Min1,
            Interval::Min3,
            Interval::Min5,
            Interval::Min15,
            Interval::Min30,
            Interval::Hour1,
            Interval::Hour2,
            Interval::Hour4,
            Interval::Hour6,
            Interval::Hour8,
            Interval::Hour12,
            Interval::Day1,
            Interval::Day3,
            Interval::Week1,
            Interval::Month1,
        ]
    );
}

#[test]
fn rejects_invalid_interval_literals() {
    for source in [
        "plot(1W.close)",
        "plot(1H.close)",
        "plot(7m.close)",
        "plot(2d.close)",
        "plot(0m.close)",
    ] {
        let err = lex(source).expect_err("invalid interval should reject");
        let message = err
            .diagnostics
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(message.contains("unknown interval literal"));
    }
}

#[test]
fn preserves_case_for_month_and_minute_intervals() {
    let tokens = lex("1M.close 1m.close").expect("source should lex");
    assert!(matches!(
        tokens[0].kind,
        TokenKind::Interval(Interval::Month1)
    ));
    assert!(matches!(
        tokens[3].kind,
        TokenKind::Interval(Interval::Min1)
    ));
}

#[test]
fn keeps_decimal_numbers_as_single_tokens() {
    let tokens = lex("plot(1.5)").expect("source should lex");
    assert!(matches!(tokens[2].kind, TokenKind::Number(ref value) if value == "1.5"));
}
