//! Runtime output structures produced while executing scripts.
//!
//! Outputs are grouped into per-step values and accumulated series so callers
//! can inspect plot data and alerts after VM execution.

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepOutput {
    pub plots: Vec<PlotPoint>,
    pub alerts: Vec<Alert>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outputs {
    pub plots: Vec<PlotSeries>,
    pub alerts: Vec<Alert>,
}
