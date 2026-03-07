use std::collections::{BTreeMap, HashMap};

use crate::backtest::orders::{empty_request_slots, CapturedOrderRequest, SignalBatch};
use crate::backtest::venue::{validate_order_for_template, VenueOrderProfile};
use crate::backtest::{BacktestError, FeatureSnapshot, FeatureValue, OrderKind};
use crate::bytecode::{OrderDecl, SignalRole};
use crate::compiler::CompiledProgram;
use crate::interval::SourceTemplate;
use crate::output::{OutputValue, Outputs};

pub(crate) struct PreparedBacktest {
    pub signal_batches: Vec<SignalBatch>,
    pub export_lookup: ExportLookup,
}

pub(crate) struct ExecutionSource {
    pub source_id: u16,
    pub template: SourceTemplate,
}

pub(crate) struct ExportLookup {
    point_indices_by_time: Vec<HashMap<i64, usize>>,
}

impl ExportLookup {
    fn from_outputs(outputs: &Outputs) -> Self {
        let point_indices_by_time = outputs
            .exports
            .iter()
            .map(|series| {
                series
                    .points
                    .iter()
                    .enumerate()
                    .filter_map(|(index, point)| {
                        point.time.and_then(time_key).map(|time| (time, index))
                    })
                    .collect()
            })
            .collect();
        Self {
            point_indices_by_time,
        }
    }

    pub(crate) fn snapshot_at(&self, outputs: &Outputs, time: f64) -> Option<FeatureSnapshot> {
        let time_key = time_key(time)?;
        let mut bar_index = None;
        let mut values = Vec::with_capacity(outputs.exports.len());
        for (series, indices) in outputs.exports.iter().zip(&self.point_indices_by_time) {
            let Some(point_index) = indices.get(&time_key).copied() else {
                continue;
            };
            let point = &series.points[point_index];
            bar_index.get_or_insert(point.bar_index);
            values.push(FeatureValue {
                name: series.name.clone(),
                value: point.value.clone(),
            });
        }
        if values.is_empty() {
            None
        } else {
            Some(FeatureSnapshot {
                bar_index: bar_index.unwrap_or_default(),
                time,
                values,
            })
        }
    }
}

pub(crate) fn resolve_execution_source(
    compiled: &CompiledProgram,
    alias: &str,
) -> Result<ExecutionSource, BacktestError> {
    compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == alias)
        .map(|source| ExecutionSource {
            source_id: source.id,
            template: source.template,
        })
        .ok_or_else(|| BacktestError::UnknownExecutionSource {
            alias: alias.to_string(),
        })
}

pub(crate) fn prepare_backtest(
    compiled: &CompiledProgram,
    outputs: &Outputs,
    execution_alias: &str,
    template: SourceTemplate,
) -> Result<PreparedBacktest, BacktestError> {
    let signal_roles = resolve_signals(compiled)?;
    let explicit_orders = explicit_orders_by_role(compiled);
    let venue = VenueOrderProfile::from_template(template);

    for role in signal_roles.values().copied() {
        let order = explicit_orders
            .get(&role)
            .copied()
            .unwrap_or_else(|| default_market_order(role));
        validate_order_for_template(venue, execution_alias, &order)?;
    }

    Ok(PreparedBacktest {
        signal_batches: collect_signal_batches(outputs, signal_roles, explicit_orders),
        export_lookup: ExportLookup::from_outputs(outputs),
    })
}

fn explicit_orders_by_role(compiled: &CompiledProgram) -> HashMap<SignalRole, OrderDecl> {
    compiled
        .program
        .orders
        .iter()
        .cloned()
        .map(|order| (order.role, order))
        .collect()
}

fn resolve_signals(
    compiled: &CompiledProgram,
) -> Result<HashMap<usize, SignalRole>, BacktestError> {
    let has_first_class = compiled
        .program
        .outputs
        .iter()
        .any(|decl| decl.signal_role.is_some());
    let mut roles = HashMap::new();

    for (output_id, decl) in compiled.program.outputs.iter().enumerate() {
        if !matches!(decl.kind, crate::bytecode::OutputKind::Trigger) {
            continue;
        }
        let role = if has_first_class {
            decl.signal_role
        } else {
            legacy_signal_role(&decl.name)
        };
        if let Some(role) = role {
            roles.insert(output_id, role);
        }
    }

    let missing = if roles
        .values()
        .any(|role| matches!(role, SignalRole::LongEntry | SignalRole::ShortEntry))
    {
        Vec::new()
    } else {
        vec!["long_entry".to_string(), "short_entry".to_string()]
    };
    if !missing.is_empty() {
        return Err(BacktestError::MissingSignalRoles {
            missing,
            available: compiled
                .program
                .outputs
                .iter()
                .filter(|decl| matches!(decl.kind, crate::bytecode::OutputKind::Trigger))
                .map(|decl| decl.name.clone())
                .collect(),
        });
    }
    Ok(roles)
}

fn collect_signal_batches(
    outputs: &Outputs,
    signal_roles: HashMap<usize, SignalRole>,
    explicit_orders: HashMap<SignalRole, OrderDecl>,
) -> Vec<SignalBatch> {
    let field_values = collect_order_field_values(outputs);
    let mut grouped = BTreeMap::<i64, [Option<CapturedOrderRequest>; 4]>::new();

    for event in &outputs.trigger_events {
        let Some(role) = signal_roles.get(&event.output_id).copied() else {
            continue;
        };
        let Some(time) = event.time.and_then(time_key) else {
            continue;
        };
        let template = explicit_orders
            .get(&role)
            .copied()
            .unwrap_or_else(|| default_market_order(role));
        let slot = &mut grouped.entry(time).or_insert_with(empty_request_slots)
            [crate::backtest::orders::role_index(role)];
        *slot = Some(capture_request(template, time as f64, &field_values));
    }

    grouped
        .into_iter()
        .map(|(time, requests)| SignalBatch {
            time: time as f64,
            requests,
        })
        .collect()
}

fn capture_request(
    template: OrderDecl,
    signal_time: f64,
    field_values: &HashMap<(u16, i64), f64>,
) -> CapturedOrderRequest {
    let lookup = |field_id: Option<u16>| {
        field_id.and_then(|field_id| field_values.get(&(field_id, signal_time as i64)).copied())
    };
    CapturedOrderRequest {
        role: template.role,
        kind: template.kind,
        tif: template.tif,
        post_only: template.post_only,
        trigger_ref: template.trigger_ref,
        price: lookup(template.price_field_id),
        trigger_price: lookup(template.trigger_price_field_id),
        expire_time: lookup(template.expire_time_field_id),
        signal_time,
    }
}

fn collect_order_field_values(outputs: &Outputs) -> HashMap<(u16, i64), f64> {
    let mut values = HashMap::new();
    for series in &outputs.order_fields {
        for point in &series.points {
            let Some(time) = point.time.and_then(time_key) else {
                continue;
            };
            if let OutputValue::F64(value) = point.value {
                values.insert((series.id as u16, time), value);
            }
        }
    }
    values
}

fn default_market_order(role: SignalRole) -> OrderDecl {
    OrderDecl {
        role,
        kind: OrderKind::Market,
        tif: None,
        post_only: false,
        trigger_ref: None,
        price_field_id: None,
        trigger_price_field_id: None,
        expire_time_field_id: None,
    }
}

fn legacy_signal_role(name: &str) -> Option<SignalRole> {
    match name {
        "long_entry" => Some(SignalRole::LongEntry),
        "long_exit" => Some(SignalRole::LongExit),
        "short_entry" => Some(SignalRole::ShortEntry),
        "short_exit" => Some(SignalRole::ShortExit),
        _ => None,
    }
}

fn time_key(time: f64) -> Option<i64> {
    if time.is_finite() && time.fract() == 0.0 {
        Some(time as i64)
    } else {
        None
    }
}
