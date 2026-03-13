use std::fs;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use sha2::Digest;

use crate::compiler::compile;

use super::{
    now_ms, ExecutionError, ExecutionMode, ExecutionSessionHealth, ExecutionSessionStatus,
    PaperExecutionSource, PaperSessionExport, PaperSessionLogEvent, PaperSessionManifest,
    PaperSessionSnapshot, SubmitPaperSession,
};

const EXECUTION_ENV_VAR: &str = "PALMSCRIPT_EXECUTION_STATE_DIR";

pub(crate) struct SessionPaths {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub script: PathBuf,
    pub snapshot: PathBuf,
    pub result: PathBuf,
    pub events: PathBuf,
}

pub fn default_execution_state_root() -> Result<PathBuf, ExecutionError> {
    if let Ok(path) = std::env::var(EXECUTION_ENV_VAR) {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("palmscript").join("execution"));
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| ExecutionError::StateRoot("HOME is not set".to_string()))?;
    Ok(home
        .join(".local")
        .join("state")
        .join("palmscript")
        .join("execution"))
}

pub(crate) fn state_root() -> Result<PathBuf, ExecutionError> {
    let root = default_execution_state_root()?;
    fs::create_dir_all(root.join("sessions")).map_err(|err| ExecutionError::Io {
        path: root.display().to_string(),
        message: err.to_string(),
    })?;
    Ok(root)
}

pub(crate) fn session_paths(session_id: &str) -> Result<SessionPaths, ExecutionError> {
    let root = state_root()?.join("sessions").join(session_id);
    Ok(SessionPaths {
        manifest: root.join("manifest.json"),
        script: root.join("script.ps"),
        snapshot: root.join("snapshot.json"),
        result: root.join("latest_result.json"),
        events: root.join("events.jsonl"),
        root,
    })
}

pub(crate) fn write_text_file(path: &Path, value: &str) -> Result<(), ExecutionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| ExecutionError::Io {
            path: parent.display().to_string(),
            message: err.to_string(),
        })?;
    }
    fs::write(path, value).map_err(|err| ExecutionError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })
}

pub(crate) fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), ExecutionError> {
    let json = serde_json::to_string_pretty(value).map_err(|err| ExecutionError::Json {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    write_text_file(path, &json)
}

pub(crate) fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T, ExecutionError> {
    let raw = fs::read_to_string(path).map_err(|err| ExecutionError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|err| ExecutionError::Json {
        path: path.display().to_string(),
        message: err.to_string(),
    })
}

pub(crate) fn persist_session_manifest(
    manifest: &PaperSessionManifest,
) -> Result<(), ExecutionError> {
    let paths = session_paths(&manifest.session_id)?;
    write_json_file(&paths.manifest, manifest)
}

pub(crate) fn persist_session_snapshot(
    session_id: &str,
    snapshot: &PaperSessionSnapshot,
) -> Result<(), ExecutionError> {
    let paths = session_paths(session_id)?;
    write_json_file(&paths.snapshot, snapshot)
}

pub(crate) fn persist_session_result(
    session_id: &str,
    result: &crate::backtest::BacktestResult,
) -> Result<(), ExecutionError> {
    let paths = session_paths(session_id)?;
    write_json_file(&paths.result, result)
}

pub(crate) fn append_log_event(
    session_id: &str,
    event: &PaperSessionLogEvent,
) -> Result<(), ExecutionError> {
    let paths = session_paths(session_id)?;
    if let Some(parent) = paths.events.parent() {
        fs::create_dir_all(parent).map_err(|err| ExecutionError::Io {
            path: parent.display().to_string(),
            message: err.to_string(),
        })?;
    }
    let mut line = serde_json::to_string(event).map_err(|err| ExecutionError::Json {
        path: paths.events.display().to_string(),
        message: err.to_string(),
    })?;
    line.push('\n');
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.events)
        .map_err(|err| ExecutionError::Io {
            path: paths.events.display().to_string(),
            message: err.to_string(),
        })?;
    file.write_all(line.as_bytes())
        .map_err(|err| ExecutionError::Io {
            path: paths.events.display().to_string(),
            message: err.to_string(),
        })
}

pub fn submit_paper_session(
    request: SubmitPaperSession,
) -> Result<PaperSessionManifest, ExecutionError> {
    let compiled =
        compile(&request.source).map_err(|err| ExecutionError::Compile(err.to_string()))?;
    if compiled.program.declared_sources.is_empty() {
        return Err(ExecutionError::MissingSources);
    }
    if compiled.program.declared_executions.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "paper execution requires at least one declared `execution` target"
                .to_string(),
        });
    }
    let base_interval = compiled
        .program
        .base_interval
        .ok_or(ExecutionError::MissingBaseInterval)?;
    if request.config.execution_source_aliases.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "paper sessions require at least one execution source alias".to_string(),
        });
    }
    let created_at_ms = now_ms();
    let session_hash = hex::encode(sha2::Sha256::digest(request.source.as_bytes()));
    let session_id = format!(
        "paper-{:x}-{:x}-{}",
        created_at_ms,
        std::process::id(),
        &session_hash[..8]
    );
    let manifest = PaperSessionManifest {
        session_id: session_id.clone(),
        mode: ExecutionMode::Paper,
        created_at_ms,
        updated_at_ms: created_at_ms,
        start_time_ms: request.start_time_ms,
        status: ExecutionSessionStatus::Queued,
        health: ExecutionSessionHealth::Starting,
        stop_requested: false,
        failure_message: None,
        script_path: request
            .script_path
            .as_ref()
            .map(|path| path.display().to_string()),
        script_sha256: {
            use sha2::{Digest as _, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(request.source.as_bytes());
            format!("{:x}", hasher.finalize())
        },
        base_interval,
        history_capacity: compiled.program.history_capacity,
        endpoints: request.endpoints,
        config: request.config,
        execution_sources: compiled
            .program
            .declared_executions
            .iter()
            .map(|source| PaperExecutionSource {
                alias: source.alias.clone(),
                template: source.template,
                symbol: source.symbol.clone(),
            })
            .collect(),
        warmup_from_ms: None,
        latest_runtime_to_ms: None,
    };
    let paths = session_paths(&session_id)?;
    fs::create_dir_all(&paths.root).map_err(|err| ExecutionError::Io {
        path: paths.root.display().to_string(),
        message: err.to_string(),
    })?;
    write_text_file(&paths.script, &request.source)?;
    write_json_file(&paths.manifest, &manifest)?;
    append_log_event(
        &session_id,
        &PaperSessionLogEvent {
            time_ms: created_at_ms,
            status: ExecutionSessionStatus::Queued,
            health: ExecutionSessionHealth::Starting,
            message: "paper session submitted".to_string(),
            latest_runtime_to_ms: None,
        },
    )?;
    Ok(manifest)
}

pub fn list_paper_sessions() -> Result<Vec<PaperSessionManifest>, ExecutionError> {
    let sessions_root = state_root()?.join("sessions");
    let mut manifests = Vec::new();
    if !sessions_root.exists() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&sessions_root).map_err(|err| ExecutionError::Io {
        path: sessions_root.display().to_string(),
        message: err.to_string(),
    })? {
        let entry = entry.map_err(|err| ExecutionError::Io {
            path: sessions_root.display().to_string(),
            message: err.to_string(),
        })?;
        let path = entry.path().join("manifest.json");
        if path.exists() {
            manifests.push(read_json_file(&path)?);
        }
    }
    manifests.sort_by_key(|manifest: &PaperSessionManifest| manifest.created_at_ms);
    Ok(manifests)
}

pub fn load_paper_session_manifest(
    session_id: &str,
) -> Result<PaperSessionManifest, ExecutionError> {
    let paths = session_paths(session_id)?;
    if !paths.manifest.exists() {
        return Err(ExecutionError::UnknownSession {
            session_id: session_id.to_string(),
        });
    }
    read_json_file(&paths.manifest)
}

pub fn load_paper_session_script(session_id: &str) -> Result<String, ExecutionError> {
    let paths = session_paths(session_id)?;
    fs::read_to_string(&paths.script).map_err(|err| ExecutionError::Io {
        path: paths.script.display().to_string(),
        message: err.to_string(),
    })
}

pub fn load_paper_session_snapshot(
    session_id: &str,
) -> Result<PaperSessionSnapshot, ExecutionError> {
    let paths = session_paths(session_id)?;
    if !paths.snapshot.exists() {
        return Err(ExecutionError::MissingSnapshot {
            session_id: session_id.to_string(),
        });
    }
    read_json_file(&paths.snapshot)
}

pub fn load_paper_session_export(session_id: &str) -> Result<PaperSessionExport, ExecutionError> {
    let manifest = load_paper_session_manifest(session_id)?;
    let paths = session_paths(session_id)?;
    let snapshot = if paths.snapshot.exists() {
        Some(read_json_file(&paths.snapshot)?)
    } else {
        None
    };
    let latest_result = if paths.result.exists() {
        Some(read_json_file(&paths.result)?)
    } else {
        None
    };
    Ok(PaperSessionExport {
        manifest,
        snapshot,
        latest_result,
    })
}

pub fn load_paper_session_logs(
    session_id: &str,
) -> Result<Vec<PaperSessionLogEvent>, ExecutionError> {
    let paths = session_paths(session_id)?;
    if !paths.events.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&paths.events).map_err(|err| ExecutionError::Io {
        path: paths.events.display().to_string(),
        message: err.to_string(),
    })?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<PaperSessionLogEvent>(line).map_err(|err| ExecutionError::Json {
                path: paths.events.display().to_string(),
                message: err.to_string(),
            })
        })
        .collect()
}

pub fn stop_paper_session(session_id: &str) -> Result<PaperSessionManifest, ExecutionError> {
    let mut manifest = load_paper_session_manifest(session_id)?;
    if manifest.status == ExecutionSessionStatus::Stopped {
        return Err(ExecutionError::AlreadyStopped {
            session_id: session_id.to_string(),
        });
    }
    let timestamp = now_ms();
    manifest.stop_requested = true;
    manifest.updated_at_ms = timestamp;
    if manifest.status == ExecutionSessionStatus::Queued {
        manifest.status = ExecutionSessionStatus::Stopped;
        manifest.health = ExecutionSessionHealth::Stopped;
    }
    persist_session_manifest(&manifest)?;
    append_log_event(
        session_id,
        &PaperSessionLogEvent {
            time_ms: timestamp,
            status: manifest.status,
            health: manifest.health,
            message: "paper session stop requested".to_string(),
            latest_runtime_to_ms: manifest.latest_runtime_to_ms,
        },
    )?;
    Ok(manifest)
}
