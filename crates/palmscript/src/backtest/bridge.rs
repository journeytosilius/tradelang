use std::collections::HashMap;

use crate::backtest::orders::CapturedOrderRequest;
use crate::backtest::venue::{validate_order_for_template, VenueOrderProfile};
use crate::backtest::BacktestError;
use crate::bytecode::{
    ArbOrderDecl, ArbSignalDecl, ArbSignalKind, ExecutionPriceDecl, LastExitFieldDecl,
    LedgerFieldDecl, OrderDecl, OutputKind, PortfolioControlDecl, PortfolioGroupDecl,
    PositionEventFieldDecl, PositionFieldDecl, RiskControlDecl, SignalRole, TransferAssetKind,
    TransferDecl,
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
    pub signal_modules: HashMap<SignalRole, String>,
    pub order_templates: HashMap<SignalRole, OrderDecl>,
    pub arb_surface: Option<PreparedArbSurface>,
    pub transfer_surface: Option<PreparedTransferSurface>,
    pub risk_controls: Vec<RiskControlDecl>,
    pub portfolio_controls: Vec<PortfolioControlDecl>,
    pub portfolio_groups: Vec<PortfolioGroupDecl>,
    pub position_fields: Vec<PositionFieldDecl>,
    pub position_event_fields: Vec<PositionEventFieldDecl>,
    pub last_exit_fields: Vec<LastExitFieldDecl>,
    pub ledger_fields: Vec<LedgerFieldDecl>,
    pub execution_price_fields: Vec<ExecutionPriceDecl>,
    pub exports: Vec<PreparedExport>,
}

pub(crate) struct PreparedArbSurface {
    pub entry_signal: ArbSignalDecl,
    pub exit_signal: ArbSignalDecl,
    pub entry_order: ArbOrderDecl,
    pub exit_order: ArbOrderDecl,
}

pub(crate) struct PreparedTransferSurface {
    pub quote_transfer: Option<TransferDecl>,
}

pub(crate) struct ExecutionSource {
    pub execution_id: u16,
    pub source_id: u16,
    pub template: SourceTemplate,
    pub symbol: String,
}

pub(crate) fn resolve_execution_sources(
    compiled: &CompiledProgram,
    aliases: &[String],
) -> Result<Vec<ExecutionSource>, BacktestError> {
    if compiled.program.declared_executions.is_empty() {
        return Err(BacktestError::MissingExecutionDeclarations);
    }
    aliases
        .iter()
        .map(|alias| resolve_execution_source(compiled, alias))
        .collect()
}

pub(crate) fn resolve_execution_source(
    compiled: &CompiledProgram,
    alias: &str,
) -> Result<ExecutionSource, BacktestError> {
    if compiled.program.declared_executions.is_empty() {
        return Err(BacktestError::MissingExecutionDeclarations);
    }
    compiled
        .program
        .declared_executions
        .iter()
        .find(|source| source.alias == alias)
        .map(|source| {
            let feed_source_id = compiled
                .program
                .declared_sources
                .iter()
                .find(|candidate| {
                    candidate.alias == source.alias
                        || (candidate.template == source.template
                            && candidate.symbol == source.symbol)
                })
                .map(|candidate| candidate.id)
                .unwrap_or(source.id);
            ExecutionSource {
                execution_id: source.id,
                source_id: feed_source_id,
                template: source.template,
                symbol: source.symbol.clone(),
            }
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
    let arb_surface = resolve_arb_surface(compiled)?;
    let transfer_surface = resolve_transfer_surface(compiled)?;
    let transfer_only_surface =
        transfer_surface.is_some() && compiled.program.orders.is_empty() && arb_surface.is_none();
    let signal_roles = if arb_surface.is_some() || transfer_only_surface {
        HashMap::new()
    } else {
        resolve_signals(compiled)?
    };
    let order_templates = if arb_surface.is_some() {
        if !compiled.program.orders.is_empty() || !signal_roles.is_empty() {
            return Err(BacktestError::ArbitrageStandardSurfaceMixUnsupported);
        }
        HashMap::new()
    } else {
        explicit_orders_by_role(compiled)
    };

    for (execution_alias, template) in executions {
        let venue = VenueOrderProfile::from_template(*template);
        for order in order_templates.values() {
            if let Some(bound_alias) = &order.execution_alias {
                if bound_alias != execution_alias {
                    continue;
                }
            }
            validate_order_for_template(venue, execution_alias, order)?;
        }
    }

    for order in order_templates.values() {
        if let Some(bound_alias) = &order.execution_alias {
            if executions.iter().all(|(alias, _)| alias != bound_alias) {
                return Err(BacktestError::UnknownExecutionSource {
                    alias: bound_alias.clone(),
                });
            }
        }
    }

    for field in &compiled.program.ledger_fields {
        let Some(execution) = compiled
            .program
            .declared_executions
            .iter()
            .find(|execution| execution.id == field.execution_id)
        else {
            return Err(BacktestError::UnknownExecutionSource {
                alias: format!("execution#{}", field.execution_id),
            });
        };
        if executions
            .iter()
            .all(|(alias, _)| alias != &execution.alias)
        {
            return Err(BacktestError::UnknownExecutionSource {
                alias: execution.alias.clone(),
            });
        }
    }

    Ok(PreparedBacktest {
        signal_roles,
        signal_modules: explicit_modules_by_role(compiled),
        order_templates,
        arb_surface,
        transfer_surface,
        risk_controls: compiled.program.risk_controls.clone(),
        portfolio_controls: compiled.program.portfolio_controls.clone(),
        portfolio_groups: compiled.program.portfolio_groups.clone(),
        position_fields: compiled.program.position_fields.clone(),
        position_event_fields: compiled.program.position_event_fields.clone(),
        last_exit_fields: compiled.program.last_exit_fields.clone(),
        ledger_fields: compiled.program.ledger_fields.clone(),
        execution_price_fields: compiled.program.execution_price_fields.clone(),
        exports: collect_exports(compiled),
    })
}

fn resolve_arb_surface(
    compiled: &CompiledProgram,
) -> Result<Option<PreparedArbSurface>, BacktestError> {
    if compiled.program.arb_signals.is_empty() && compiled.program.arb_orders.is_empty() {
        return Ok(None);
    }
    if !compiled.program.orders.is_empty()
        || compiled
            .program
            .outputs
            .iter()
            .any(|decl| matches!(decl.kind, OutputKind::Trigger) && decl.signal_role.is_some())
    {
        return Err(BacktestError::ArbitrageStandardSurfaceMixUnsupported);
    }

    let entry_signal = compiled
        .program
        .arb_signals
        .iter()
        .find(|decl| matches!(decl.kind, ArbSignalKind::Entry))
        .copied()
        .ok_or(BacktestError::IncompleteArbitrageSurface)?;
    let exit_signal = compiled
        .program
        .arb_signals
        .iter()
        .find(|decl| matches!(decl.kind, ArbSignalKind::Exit))
        .copied()
        .ok_or(BacktestError::IncompleteArbitrageSurface)?;
    let entry_order = compiled
        .program
        .arb_orders
        .iter()
        .find(|decl| matches!(decl.kind, ArbSignalKind::Entry))
        .cloned()
        .ok_or(BacktestError::IncompleteArbitrageSurface)?;
    let exit_order = compiled
        .program
        .arb_orders
        .iter()
        .find(|decl| matches!(decl.kind, ArbSignalKind::Exit))
        .cloned()
        .ok_or(BacktestError::IncompleteArbitrageSurface)?;

    Ok(Some(PreparedArbSurface {
        entry_signal,
        exit_signal,
        entry_order,
        exit_order,
    }))
}

fn resolve_transfer_surface(
    compiled: &CompiledProgram,
) -> Result<Option<PreparedTransferSurface>, BacktestError> {
    if compiled.program.transfers.is_empty() {
        return Ok(None);
    }
    let mut quote_transfer = None;
    for transfer in &compiled.program.transfers {
        match transfer.asset_kind {
            TransferAssetKind::Quote => {
                if quote_transfer.is_some() {
                    return Err(BacktestError::UnsupportedTransferAsset);
                }
                quote_transfer = Some(*transfer);
            }
            TransferAssetKind::Base => return Err(BacktestError::UnsupportedTransferAsset),
        }
    }
    Ok(Some(PreparedTransferSurface { quote_transfer }))
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

fn explicit_modules_by_role(compiled: &CompiledProgram) -> HashMap<SignalRole, String> {
    compiled
        .program
        .signal_modules
        .iter()
        .cloned()
        .map(|module| (module.role, module.name))
        .collect()
}

fn resolve_signals(
    compiled: &CompiledProgram,
) -> Result<HashMap<usize, SignalRole>, BacktestError> {
    let mut roles = HashMap::new();

    for (output_id, decl) in compiled.program.outputs.iter().enumerate() {
        if !matches!(decl.kind, crate::bytecode::OutputKind::Trigger) {
            continue;
        }
        if let Some(role) = decl.signal_role {
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
