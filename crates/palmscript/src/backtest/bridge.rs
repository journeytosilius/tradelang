use std::collections::HashMap;

use crate::backtest::orders::CapturedOrderRequest;
use crate::backtest::venue::{validate_order_for_template, VenueOrderProfile};
use crate::backtest::{BacktestError, OrderKind};
use crate::bytecode::{
    LastExitFieldDecl, OrderDecl, OutputKind, PortfolioControlDecl, PortfolioGroupDecl,
    PositionEventFieldDecl, PositionFieldDecl, RiskControlDecl, SignalRole,
};
use crate::compiler::CompiledProgram;
use crate::interval::SourceTemplate;
use crate::order::OrderFieldKind;
use crate::output::{OrderFieldSample, OutputValue, StepOutput};
use crate::types::Type;

#[derive(Clone, Debug)]
pub(crate) struct PreparedExport {
    pub output_id: usize,
    pub name: String,
    pub value_type: crate::backtest::ExportValueType,
}

pub(crate) struct PreparedBacktest {
    pub signal_roles: HashMap<usize, SignalRole>,
    pub order_templates: HashMap<SignalRole, OrderDecl>,
    pub risk_controls: Vec<RiskControlDecl>,
    pub portfolio_controls: Vec<PortfolioControlDecl>,
    pub portfolio_groups: Vec<PortfolioGroupDecl>,
    pub position_fields: Vec<PositionFieldDecl>,
    pub position_event_fields: Vec<PositionEventFieldDecl>,
    pub last_exit_fields: Vec<LastExitFieldDecl>,
    pub exports: Vec<PreparedExport>,
}

pub(crate) struct ExecutionSource {
    pub source_id: u16,
    pub template: SourceTemplate,
}

pub(crate) fn resolve_execution_sources(
    compiled: &CompiledProgram,
    aliases: &[String],
) -> Result<Vec<ExecutionSource>, BacktestError> {
    aliases
        .iter()
        .map(|alias| resolve_execution_source(compiled, alias))
        .collect()
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
    execution_alias: &str,
    template: SourceTemplate,
) -> Result<PreparedBacktest, BacktestError> {
    prepare_backtest_for_aliases(compiled, &[(execution_alias.to_string(), template)])
}

pub(crate) fn prepare_backtest_for_aliases(
    compiled: &CompiledProgram,
    executions: &[(String, SourceTemplate)],
) -> Result<PreparedBacktest, BacktestError> {
    let signal_roles = resolve_signals(compiled)?;
    let mut order_templates = explicit_orders_by_role(compiled);

    for role in signal_roles.values().copied() {
        order_templates
            .entry(role)
            .or_insert_with(|| default_market_order(role));
    }

    for (execution_alias, template) in executions {
        let venue = VenueOrderProfile::from_template(*template);
        for order in order_templates.values() {
            validate_order_for_template(venue, execution_alias, order)?;
        }
    }

    Ok(PreparedBacktest {
        signal_roles,
        order_templates,
        risk_controls: compiled.program.risk_controls.clone(),
        portfolio_controls: compiled.program.portfolio_controls.clone(),
        portfolio_groups: compiled.program.portfolio_groups.clone(),
        position_fields: compiled.program.position_fields.clone(),
        position_event_fields: compiled.program.position_event_fields.clone(),
        last_exit_fields: compiled.program.last_exit_fields.clone(),
        exports: collect_exports(compiled),
    })
}

pub(crate) fn capture_request(
    template: OrderDecl,
    signal_time: f64,
    step: &StepOutput,
) -> CapturedOrderRequest {
    let lookup = |role: SignalRole, kind: OrderFieldKind| lookup_order_field(step, role, kind);
    CapturedOrderRequest {
        role: template.role,
        kind: template.kind,
        tif: template.tif,
        post_only: template.post_only,
        trigger_ref: template.trigger_ref,
        size_mode: template.size_mode,
        price: template
            .price_field_id
            .and_then(|_| lookup(template.role, OrderFieldKind::Price)),
        trigger_price: template
            .trigger_price_field_id
            .and_then(|_| lookup(template.role, OrderFieldKind::TriggerPrice)),
        expire_time: template
            .expire_time_field_id
            .and_then(|_| lookup(template.role, OrderFieldKind::ExpireTime)),
        has_size_field: template.size_field_id.is_some(),
        size_value: template
            .size_field_id
            .and_then(|_| lookup(template.role, OrderFieldKind::SizeFraction)),
        size_stop_price: template
            .risk_stop_field_id
            .and_then(|_| lookup(template.role, OrderFieldKind::RiskStopPrice)),
        signal_time,
    }
}

fn lookup_order_field(step: &StepOutput, role: SignalRole, kind: OrderFieldKind) -> Option<f64> {
    step.order_fields.iter().find_map(|sample| match sample {
        OrderFieldSample {
            role: sample_role,
            kind: sample_kind,
            value: OutputValue::F64(value),
            ..
        } if *sample_role == role && *sample_kind == kind => Some(*value),
        _ => None,
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

fn default_market_order(role: SignalRole) -> OrderDecl {
    OrderDecl {
        role,
        kind: OrderKind::Market,
        tif: None,
        post_only: false,
        trigger_ref: None,
        size_mode: None,
        price_field_id: None,
        trigger_price_field_id: None,
        expire_time_field_id: None,
        size_field_id: None,
        risk_stop_field_id: None,
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

fn collect_exports(compiled: &CompiledProgram) -> Vec<PreparedExport> {
    compiled
        .program
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(output_id, decl)| {
            if !matches!(decl.kind, OutputKind::ExportSeries) {
                return None;
            }
            let value_type = match decl.ty.scalar() {
                Some(Type::Bool) => crate::backtest::ExportValueType::Bool,
                _ => crate::backtest::ExportValueType::Numeric,
            };
            Some(PreparedExport {
                output_id,
                name: decl.name.clone(),
                value_type,
            })
        })
        .collect()
}
