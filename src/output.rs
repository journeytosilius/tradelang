//! Runtime output structures produced while executing scripts.
//!
//! Outputs are grouped into per-step values and accumulated series so callers
//! can inspect plot data, exported series, trigger events, and alerts after VM
//! execution.

use crate::bytecode::OutputKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlotPoint {
    #[serde(skip)]
    pub plot_id: usize,
    pub bar_index: usize,
    pub time: Option<f64>,
    pub value: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlotSeries {
    pub id: usize,
    pub name: Option<String>,
    pub points: Vec<PlotPoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub bar_index: usize,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OutputValue {
    F64(f64),
    Bool(bool),
    NA,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputSample {
    #[serde(skip)]
    pub output_id: usize,
    pub name: String,
    pub bar_index: usize,
    pub time: Option<f64>,
    pub value: OutputValue,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputSeries {
    pub id: usize,
    pub name: String,
    pub kind: OutputKind,
    pub points: Vec<OutputSample>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerEvent {
    pub output_id: usize,
    pub name: String,
    pub bar_index: usize,
    pub time: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepOutput {
    pub plots: Vec<PlotPoint>,
    pub exports: Vec<OutputSample>,
    pub triggers: Vec<OutputSample>,
    pub trigger_events: Vec<TriggerEvent>,
    pub alerts: Vec<Alert>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outputs {
    pub plots: Vec<PlotSeries>,
    pub exports: Vec<OutputSeries>,
    pub triggers: Vec<OutputSeries>,
    pub trigger_events: Vec<TriggerEvent>,
    pub alerts: Vec<Alert>,
}
