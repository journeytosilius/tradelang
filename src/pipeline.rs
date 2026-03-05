//! Host-managed strategy composition over multiple compiled TradeLang programs.
//!
//! Pipelines execute an acyclic graph of strategies on the same base interval,
//! exposing upstream `export`/`trigger` outputs as external series inputs to
//! downstream nodes.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::bytecode::{ExternalInputKind, OutputDecl, OutputKind};
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::output::{OutputValue, Outputs};
use crate::runtime::{Bar, Engine, ExternalInputFrame, MultiIntervalConfig, VmLimits};
use crate::types::Value;
use crate::Interval;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineSpec {
    pub nodes: Vec<PipelineNodeSpec>,
    pub edges: Vec<PipelineEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineNodeSpec {
    pub name: String,
    pub compiled: CompiledProgram,
    pub base_interval: Interval,
    pub data_config: Option<MultiIntervalConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineEdge {
    pub from_node: String,
    pub output: String,
    pub to_node: String,
    pub input: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PipelineOutputs {
    pub nodes: Vec<PipelineNodeOutput>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipelineNodeOutput {
    pub name: String,
    pub outputs: Outputs,
}

#[derive(Clone, Copy)]
struct InputBinding {
    input_index: usize,
    source_node: usize,
    source_output_id: usize,
}

struct PipelineNodeState {
    name: String,
    engine: Engine,
    output_decls: Vec<OutputDecl>,
    output_cache: Vec<Value>,
    input_buffer: Vec<Value>,
}

pub struct PipelineEngine {
    nodes: Vec<PipelineNodeState>,
    bindings: Vec<Vec<InputBinding>>,
    order: Vec<usize>,
    base_interval: Interval,
}

impl PipelineEngine {
    pub fn new(spec: PipelineSpec, limits: VmLimits) -> Result<Self, RuntimeError> {
        let mut name_to_index = HashMap::new();
        let mut expected_interval = None;
        for (index, node) in spec.nodes.iter().enumerate() {
            if name_to_index.insert(node.name.clone(), index).is_some() {
                return Err(RuntimeError::DuplicatePipelineNode {
                    node: node.name.clone(),
                });
            }
            if let Some(interval) = expected_interval {
                if node.base_interval != interval {
                    return Err(RuntimeError::PipelineIntervalMismatch {
                        node: node.name.clone(),
                        interval: node.base_interval,
                        expected: interval,
                    });
                }
            } else {
                expected_interval = Some(node.base_interval);
            }
            if let Some(config) = &node.data_config {
                if config.base_interval != node.base_interval {
                    return Err(RuntimeError::PipelineIntervalMismatch {
                        node: node.name.clone(),
                        interval: config.base_interval,
                        expected: node.base_interval,
                    });
                }
            }
        }
        let base_interval = expected_interval.unwrap_or(Interval::Min1);

        let mut bindings = vec![Vec::<InputBinding>::new(); spec.nodes.len()];
        let mut indegree = vec![0usize; spec.nodes.len()];
        let mut adjacency = vec![Vec::<usize>::new(); spec.nodes.len()];

        for edge in &spec.edges {
            let Some(&source_node) = name_to_index.get(&edge.from_node) else {
                return Err(RuntimeError::MissingPipelineNode {
                    node: edge.from_node.clone(),
                });
            };
            let Some(&target_node) = name_to_index.get(&edge.to_node) else {
                return Err(RuntimeError::MissingPipelineNode {
                    node: edge.to_node.clone(),
                });
            };
            let target = &spec.nodes[target_node];
            let input_index = target
                .compiled
                .program
                .external_inputs
                .iter()
                .position(|input| input.name == edge.input)
                .ok_or_else(|| RuntimeError::MissingPipelineInput {
                    node: edge.to_node.clone(),
                    input: edge.input.clone(),
                })?;
            if bindings[target_node]
                .iter()
                .any(|binding| binding.input_index == input_index)
            {
                return Err(RuntimeError::DuplicatePipelineInput {
                    node: edge.to_node.clone(),
                    input: edge.input.clone(),
                });
            }
            let source = &spec.nodes[source_node];
            let output_index = source
                .compiled
                .program
                .outputs
                .iter()
                .position(|output| output.name == edge.output)
                .ok_or_else(|| RuntimeError::MissingPipelineOutput {
                    node: edge.from_node.clone(),
                    output: edge.output.clone(),
                })?;

            let input_decl = &target.compiled.program.external_inputs[input_index];
            let output_decl = &source.compiled.program.outputs[output_index];
            if input_decl.ty != output_decl.ty
                || input_decl.kind != external_kind_for_output(output_decl.kind)
            {
                return Err(RuntimeError::PipelineInputTypeMismatch {
                    node: edge.to_node.clone(),
                    input: edge.input.clone(),
                    expected: external_input_kind_name(input_decl.kind, input_decl.ty),
                    found: output_kind_name(output_decl.kind, output_decl.ty),
                });
            }

            bindings[target_node].push(InputBinding {
                input_index,
                source_node,
                source_output_id: output_index,
            });
            if !adjacency[source_node].contains(&target_node) {
                adjacency[source_node].push(target_node);
                indegree[target_node] += 1;
            }
        }

        for (node_index, node) in spec.nodes.iter().enumerate() {
            for (input_index, input) in node.compiled.program.external_inputs.iter().enumerate() {
                if !bindings[node_index]
                    .iter()
                    .any(|binding| binding.input_index == input_index)
                {
                    return Err(RuntimeError::MissingPipelineInput {
                        node: node.name.clone(),
                        input: input.name.clone(),
                    });
                }
            }
        }

        let mut queue = indegree
            .iter()
            .enumerate()
            .filter(|(_, degree)| **degree == 0)
            .map(|(index, _)| index)
            .collect::<VecDeque<_>>();
        let mut order = Vec::with_capacity(spec.nodes.len());
        while let Some(node_index) = queue.pop_front() {
            order.push(node_index);
            for &next in &adjacency[node_index] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        if order.len() != spec.nodes.len() {
            return Err(RuntimeError::PipelineCycle);
        }

        let mut nodes = Vec::with_capacity(spec.nodes.len());
        for node in spec.nodes {
            let output_count = node.compiled.program.outputs.len();
            let input_count = node.compiled.program.external_inputs.len();
            let output_decls = node.compiled.program.outputs.clone();
            let engine = match node.data_config {
                Some(config) => Engine::new_multi_interval(node.compiled, config, limits)?,
                None => Engine::try_new(node.compiled, limits)?,
            };
            nodes.push(PipelineNodeState {
                name: node.name,
                engine,
                output_decls,
                output_cache: vec![Value::NA; output_count],
                input_buffer: Vec::with_capacity(input_count),
            });
        }

        for node_bindings in &mut bindings {
            node_bindings.sort_by_key(|binding| binding.input_index);
        }

        Ok(Self {
            nodes,
            bindings,
            order,
            base_interval,
        })
    }

    pub const fn base_interval(&self) -> Interval {
        self.base_interval
    }

    pub fn run_step(&mut self, bar: Bar) -> Result<(), RuntimeError> {
        for &node_index in &self.order {
            let bindings = &self.bindings[node_index];
            let (left, right) = self.nodes.split_at_mut(node_index);
            let (node, right) = right.split_first_mut().expect("node exists");
            node.input_buffer.clear();
            for binding in bindings {
                let source_cache = if binding.source_node < node_index {
                    &left[binding.source_node].output_cache
                } else {
                    &right[binding.source_node - node_index - 1].output_cache
                };
                node.input_buffer
                    .push(source_cache[binding.source_output_id].clone());
            }

            let step = node
                .engine
                .run_step_with_inputs(bar, ExternalInputFrame::new(&node.input_buffer))?;
            update_output_cache(
                &mut node.output_cache,
                &node.output_decls,
                &step.exports,
                &step.triggers,
            );
        }
        Ok(())
    }

    pub fn run(mut self, bars: &[Bar]) -> Result<PipelineOutputs, RuntimeError> {
        for &bar in bars {
            self.run_step(bar)?;
        }
        Ok(self.finish())
    }

    pub fn finish(self) -> PipelineOutputs {
        PipelineOutputs {
            nodes: self
                .nodes
                .into_iter()
                .map(|node| PipelineNodeOutput {
                    name: node.name,
                    outputs: node.engine.finish(),
                })
                .collect(),
        }
    }
}

fn update_output_cache(
    cache: &mut [Value],
    decls: &[crate::bytecode::OutputDecl],
    exports: &[crate::output::OutputSample],
    triggers: &[crate::output::OutputSample],
) {
    let mut export_index = 0usize;
    let mut trigger_index = 0usize;
    for (output_id, decl) in decls.iter().enumerate() {
        let value = match decl.kind {
            OutputKind::ExportSeries => {
                let sample = &exports[export_index];
                export_index += 1;
                output_value_to_value(&sample.value)
            }
            OutputKind::Trigger => {
                let sample = &triggers[trigger_index];
                trigger_index += 1;
                output_value_to_value(&sample.value)
            }
        };
        cache[output_id] = value;
    }
}

fn output_value_to_value(value: &OutputValue) -> Value {
    match value {
        OutputValue::F64(value) => Value::F64(*value),
        OutputValue::Bool(value) => Value::Bool(*value),
        OutputValue::NA => Value::NA,
    }
}

fn external_kind_for_output(kind: OutputKind) -> ExternalInputKind {
    match kind {
        OutputKind::ExportSeries => ExternalInputKind::ExportSeries,
        OutputKind::Trigger => ExternalInputKind::TriggerSeries,
    }
}

fn external_input_kind_name(kind: ExternalInputKind, ty: crate::types::Type) -> &'static str {
    match (kind, ty) {
        (ExternalInputKind::ExportSeries, crate::types::Type::SeriesF64) => "export series<float>",
        (ExternalInputKind::ExportSeries, crate::types::Type::SeriesBool) => "export series<bool>",
        (ExternalInputKind::TriggerSeries, _) => "trigger series<bool>",
        _ => "external input",
    }
}

fn output_kind_name(kind: OutputKind, ty: crate::types::Type) -> &'static str {
    match (kind, ty) {
        (OutputKind::ExportSeries, crate::types::Type::SeriesF64) => "export series<float>",
        (OutputKind::ExportSeries, crate::types::Type::SeriesBool) => "export series<bool>",
        (OutputKind::Trigger, _) => "trigger series<bool>",
        _ => "output",
    }
}
