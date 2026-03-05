use tradelang::{
    compile, compile_with_env, CompileEnvironment, ExternalInputDecl, ExternalInputKind, Interval,
    PipelineEdge, PipelineEngine, PipelineNodeSpec, PipelineSpec, RuntimeError, Type, VmLimits,
};
use tradelang::{Bar, MultiIntervalConfig};

const MINUTE_MS: i64 = 60_000;
const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;

fn bars_with_spacing(start_ms: i64, spacing_ms: i64, closes: &[f64]) -> Vec<Bar> {
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

#[test]
fn pipeline_passes_exported_series_same_bar() {
    let producer = compile("export trend = close > close[1]\nplot(0)").expect("producer");
    let consumer = compile_with_env(
        "if trend and close > open { plot(1) } else { plot(0) }",
        &CompileEnvironment {
            external_inputs: vec![ExternalInputDecl {
                name: "trend".into(),
                ty: Type::SeriesBool,
                kind: ExternalInputKind::ExportSeries,
            }],
        },
    )
    .expect("consumer");

    let outputs = PipelineEngine::new(
        PipelineSpec {
            nodes: vec![
                PipelineNodeSpec {
                    name: "producer".into(),
                    compiled: producer,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
                PipelineNodeSpec {
                    name: "consumer".into(),
                    compiled: consumer,
                    base_interval: Interval::Min1,
                    data_config: Some(MultiIntervalConfig {
                        base_interval: Interval::Min1,
                        supplemental: vec![],
                    }),
                },
            ],
            edges: vec![PipelineEdge {
                from_node: "producer".into(),
                output: "trend".into(),
                to_node: "consumer".into(),
                input: "trend".into(),
            }],
        },
        VmLimits::default(),
    )
    .expect("pipeline")
    .run(&bars_with_spacing(
        JAN_1_2024_UTC_MS,
        MINUTE_MS,
        &[10.0, 11.0, 9.0],
    ))
    .expect("run");

    let consumer_outputs = &outputs.nodes[1].outputs;
    assert_eq!(consumer_outputs.plots[0].points[0].value, Some(0.0));
    assert_eq!(consumer_outputs.plots[0].points[1].value, Some(1.0));
    assert_eq!(consumer_outputs.plots[0].points[2].value, Some(0.0));
}

#[test]
fn pipeline_passes_trigger_series_and_events() {
    let producer = compile("trigger long_entry = close > close[1]\nplot(0)").expect("producer");
    let consumer = compile_with_env(
        "if long_entry { plot(1) } else { plot(0) }",
        &CompileEnvironment {
            external_inputs: vec![ExternalInputDecl {
                name: "long_entry".into(),
                ty: Type::SeriesBool,
                kind: ExternalInputKind::TriggerSeries,
            }],
        },
    )
    .expect("consumer");

    let outputs = PipelineEngine::new(
        PipelineSpec {
            nodes: vec![
                PipelineNodeSpec {
                    name: "producer".into(),
                    compiled: producer,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
                PipelineNodeSpec {
                    name: "consumer".into(),
                    compiled: consumer,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
            ],
            edges: vec![PipelineEdge {
                from_node: "producer".into(),
                output: "long_entry".into(),
                to_node: "consumer".into(),
                input: "long_entry".into(),
            }],
        },
        VmLimits::default(),
    )
    .expect("pipeline")
    .run(&bars_with_spacing(
        JAN_1_2024_UTC_MS,
        MINUTE_MS,
        &[10.0, 11.0, 9.0, 12.0],
    ))
    .expect("run");

    let producer_outputs = &outputs.nodes[0].outputs;
    assert_eq!(producer_outputs.trigger_events.len(), 2);
    let consumer_outputs = &outputs.nodes[1].outputs;
    assert_eq!(consumer_outputs.plots[0].points[1].value, Some(1.0));
    assert_eq!(consumer_outputs.plots[0].points[2].value, Some(0.0));
    assert_eq!(consumer_outputs.plots[0].points[3].value, Some(1.0));
}

#[test]
fn pipeline_rejects_cycles() {
    let a = compile_with_env(
        "export a_out = a_in\nplot(0)",
        &CompileEnvironment {
            external_inputs: vec![ExternalInputDecl {
                name: "a_in".into(),
                ty: Type::SeriesF64,
                kind: ExternalInputKind::ExportSeries,
            }],
        },
    )
    .expect("a");
    let b = compile_with_env(
        "export b_out = b_in\nplot(0)",
        &CompileEnvironment {
            external_inputs: vec![ExternalInputDecl {
                name: "b_in".into(),
                ty: Type::SeriesF64,
                kind: ExternalInputKind::ExportSeries,
            }],
        },
    )
    .expect("b");

    let err = match PipelineEngine::new(
        PipelineSpec {
            nodes: vec![
                PipelineNodeSpec {
                    name: "a".into(),
                    compiled: a,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
                PipelineNodeSpec {
                    name: "b".into(),
                    compiled: b,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
            ],
            edges: vec![
                PipelineEdge {
                    from_node: "a".into(),
                    output: "a_out".into(),
                    to_node: "b".into(),
                    input: "b_in".into(),
                },
                PipelineEdge {
                    from_node: "b".into(),
                    output: "b_out".into(),
                    to_node: "a".into(),
                    input: "a_in".into(),
                },
            ],
        },
        VmLimits::default(),
    ) {
        Ok(_) => panic!("cycle should reject"),
        Err(err) => err,
    };
    assert!(matches!(err, RuntimeError::PipelineCycle));
}
