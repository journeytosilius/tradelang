use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::engine::process_paper_session;
use super::state::{
    default_execution_state_root, list_paper_sessions, read_json_file, write_json_file,
};
use super::{now_ms, ExecutionError, ExecutionSessionStatus};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionDaemonConfig {
    pub poll_interval_ms: u64,
    pub once: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionDaemonStatus {
    pub pid: u32,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    pub poll_interval_ms: u64,
    pub once: bool,
    pub running: bool,
    pub stop_requested: bool,
    pub active_sessions: Vec<String>,
    pub state_root: String,
}

pub fn serve_execution_daemon(
    config: ExecutionDaemonConfig,
) -> Result<ExecutionDaemonStatus, ExecutionError> {
    let state_root = default_execution_state_root()?;
    fs::create_dir_all(&state_root).map_err(|err| ExecutionError::Io {
        path: state_root.display().to_string(),
        message: err.to_string(),
    })?;
    let daemon_path = daemon_status_path(&state_root);
    let stop_path = daemon_stop_path(&state_root);
    let started_at_ms = now_ms();
    if stop_path.exists() {
        fs::remove_file(&stop_path).map_err(|err| ExecutionError::Io {
            path: stop_path.display().to_string(),
            message: err.to_string(),
        })?;
    }

    loop {
        let manifests = list_paper_sessions()?;
        let active_sessions = manifests
            .iter()
            .filter(|manifest| {
                matches!(
                    manifest.status,
                    ExecutionSessionStatus::Queued
                        | ExecutionSessionStatus::Starting
                        | ExecutionSessionStatus::WarmingUp
                        | ExecutionSessionStatus::Live
                )
            })
            .map(|manifest| manifest.session_id.clone())
            .collect::<Vec<_>>();
        let stop_requested = stop_path.exists();
        let status = ExecutionDaemonStatus {
            pid: std::process::id(),
            started_at_ms,
            updated_at_ms: now_ms(),
            poll_interval_ms: config.poll_interval_ms,
            once: config.once,
            running: true,
            stop_requested,
            active_sessions: active_sessions.clone(),
            state_root: state_root.display().to_string(),
        };
        write_json_file(&daemon_path, &status)?;
        if stop_requested {
            break;
        }
        for session_id in active_sessions {
            let _ = process_paper_session(&session_id, now_ms())?;
        }
        if config.once {
            let mut final_status = status.clone();
            final_status.running = false;
            final_status.updated_at_ms = now_ms();
            write_json_file(&daemon_path, &final_status)?;
            return Ok(final_status);
        }
        thread::sleep(Duration::from_millis(config.poll_interval_ms.max(1)));
    }

    let final_status = ExecutionDaemonStatus {
        pid: std::process::id(),
        started_at_ms,
        updated_at_ms: now_ms(),
        poll_interval_ms: config.poll_interval_ms,
        once: config.once,
        running: false,
        stop_requested: true,
        active_sessions: Vec::new(),
        state_root: state_root.display().to_string(),
    };
    write_json_file(&daemon_path, &final_status)?;
    Ok(final_status)
}

pub fn execution_daemon_status() -> Result<Option<ExecutionDaemonStatus>, ExecutionError> {
    let state_root = default_execution_state_root()?;
    let daemon_path = daemon_status_path(&state_root);
    if !daemon_path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json_file(&daemon_path)?))
}

pub fn request_execution_daemon_stop() -> Result<PathBuf, ExecutionError> {
    let state_root = default_execution_state_root()?;
    fs::create_dir_all(&state_root).map_err(|err| ExecutionError::Io {
        path: state_root.display().to_string(),
        message: err.to_string(),
    })?;
    let stop_path = daemon_stop_path(&state_root);
    fs::write(&stop_path, b"stop").map_err(|err| ExecutionError::Io {
        path: stop_path.display().to_string(),
        message: err.to_string(),
    })?;
    Ok(stop_path)
}

fn daemon_status_path(root: &std::path::Path) -> PathBuf {
    root.join("daemon.json")
}

fn daemon_stop_path(root: &std::path::Path) -> PathBuf {
    root.join("daemon.stop")
}
