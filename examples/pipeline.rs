#[path = "support/mod.rs"]
mod support;

use tradelang::{
    compile, compile_with_env, CompileEnvironment, ExternalInputDecl, ExternalInputKind, Interval,
    PipelineEdge, PipelineEngine, PipelineNodeSpec, PipelineSpec, Type, VmLimits,
};

fn main() {
    let producer = compile(
        "export trend = close > ema(close, 3)\ntrigger breakout = close > high[1]\nplot(0)",
    )
    .expect("producer compiles");
    let consumer = compile_with_env(
        "if trend and breakout { plot(1) } else { plot(0) }",
        &CompileEnvironment {
            external_inputs: vec![
                ExternalInputDecl {
                    name: "trend".into(),
                    ty: Type::SeriesBool,
                    kind: ExternalInputKind::ExportSeries,
                },
                ExternalInputDecl {
                    name: "breakout".into(),
                    ty: Type::SeriesBool,
                    kind: ExternalInputKind::TriggerSeries,
                },
            ],
        },
    )
    .expect("consumer compiles");

    let pipeline = PipelineEngine::new(
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
            edges: vec![
                PipelineEdge {
                    from_node: "producer".into(),
                    output: "trend".into(),
                    to_node: "consumer".into(),
                    input: "trend".into(),
                },
                PipelineEdge {
                    from_node: "producer".into(),
                    output: "breakout".into(),
                    to_node: "consumer".into(),
                    input: "breakout".into(),
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("pipeline builds");

    let outputs = pipeline
        .run(&support::rising_bars(
            support::JAN_1_2024_UTC_MS,
            support::MINUTE_MS,
            8,
            100.0,
        ))
        .expect("pipeline runs");

    let json = serde_json::to_string_pretty(&outputs).expect("pipeline outputs serialize");
    println!("{json}");
}
