#!/bin/sh
set -eu

CONFIG_PATH="${PALMSCRIPT_PAPER_CONFIG:-/etc/palmscript/paper-sessions.toml}"
STRATEGY_ROOT="${PALMSCRIPT_STRATEGY_ROOT:-/strategies}"
BUNDLED_STRATEGY_ROOT="${PALMSCRIPT_BUNDLED_STRATEGY_ROOT:-/usr/share/palmscript/strategies}"
STATE_DIR="${PALMSCRIPT_EXECUTION_STATE_DIR:-/var/lib/palmscript/execution}"
FORCE_SUBMIT="${PALMSCRIPT_FORCE_SUBMIT:-0}"
PAPER_DASHBOARD="${PALMSCRIPT_PAPER_DASHBOARD:-1}"
DASHBOARD_BIND="${PALMSCRIPT_IDE_BIND:-0.0.0.0:8080}"
dashboard_pid=""

cleanup() {
    if [ -n "$dashboard_pid" ]; then
        kill "$dashboard_pid" 2>/dev/null || true
        wait "$dashboard_pid" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

mkdir -p "$STATE_DIR"
export PALMSCRIPT_EXECUTION_STATE_DIR="$STATE_DIR"

if [ ! -f "$CONFIG_PATH" ]; then
    echo "paper entrypoint: missing config at $CONFIG_PATH" >&2
    exit 1
fi

if [ "$PAPER_DASHBOARD" = "1" ]; then
    PALMSCRIPT_IDE_BIND="$DASHBOARD_BIND" palmscript-ide-server &
    dashboard_pid="$!"
fi

python3 - "$CONFIG_PATH" "$STRATEGY_ROOT" "$BUNDLED_STRATEGY_ROOT" "$FORCE_SUBMIT" <<'PY'
import json
import os
import subprocess
import sys
import tomllib


def fail(message: str) -> None:
    print(f"paper entrypoint: {message}", file=sys.stderr)
    raise SystemExit(1)


config_path, strategy_root, bundled_strategy_root, force_submit = sys.argv[1:5]
with open(config_path, "rb") as handle:
    config = tomllib.load(handle)

daemon = config.get("daemon", {})
sessions = config.get("session", [])

if not isinstance(daemon, dict):
    fail("`[daemon]` must be a table")
if not isinstance(sessions, list):
    fail("`[[session]]` entries must be an array of tables")

submit_required = force_submit == "1"
if not submit_required:
    listed = subprocess.run(
        ["palmscript", "run", "paper-list", "--format", "json"],
        check=True,
        capture_output=True,
        text=True,
    )
    existing = json.loads(listed.stdout)
    submit_required = len(existing) == 0

if submit_required:
    for index, session in enumerate(sessions, start=1):
        if not isinstance(session, dict):
            fail(f"session #{index} must be a table")
        if not session.get("enabled", True):
            continue

        raw_script = session.get("script")
        if not isinstance(raw_script, str) or not raw_script:
            fail(f"session #{index} is missing `script`")
        if os.path.isabs(raw_script):
            script_path = raw_script
        else:
            script_path = os.path.join(strategy_root, raw_script)
            if not os.path.exists(script_path):
                bundled_script_path = os.path.join(bundled_strategy_root, raw_script)
                if os.path.exists(bundled_script_path):
                    script_path = bundled_script_path
        if not os.path.exists(script_path):
            fail(f"session #{index} script does not exist: {script_path}")

        maker_fee_bps = session.get("maker_fee_bps")
        taker_fee_bps = session.get("taker_fee_bps")
        if maker_fee_bps is None or taker_fee_bps is None:
            fail(
                f"session #{index} must set both `maker_fee_bps` and `taker_fee_bps`"
            )

        command = [
            "palmscript",
            "run",
            "paper",
            script_path,
            "--initial-capital",
            str(session.get("initial_capital", 10000.0)),
            "--maker-fee-bps",
            str(maker_fee_bps),
            "--taker-fee-bps",
            str(taker_fee_bps),
            "--slippage-bps",
            str(session.get("slippage_bps", 2.0)),
            "--diagnostics",
            str(session.get("diagnostics", "summary")),
        ]

        execution_sources = session.get("execution_sources", [])
        if not isinstance(execution_sources, list):
            fail(f"session #{index} `execution_sources` must be an array")
        for alias in execution_sources:
            command.extend(["--execution-source", str(alias)])

        fee_schedules = session.get("fee_schedules", [])
        if not isinstance(fee_schedules, list):
            fail(f"session #{index} `fee_schedules` must be an array")
        for spec in fee_schedules:
            command.extend(["--fee-schedule", str(spec)])

        leverage = session.get("leverage")
        if leverage is not None:
            command.extend(["--leverage", str(leverage)])

        margin_mode = session.get("margin_mode")
        if margin_mode is not None:
            command.extend(["--margin-mode", str(margin_mode)])

        subprocess.run(command, check=True)

poll_interval_ms = int(daemon.get("poll_interval_ms", 30000))
once = bool(daemon.get("once", False))

daemon_command = [
    "palmscript",
    "execution",
    "serve",
    "--poll-interval-ms",
    str(poll_interval_ms),
]
if once:
    daemon_command.append("--once")

os.execvp(daemon_command[0], daemon_command)
PY
