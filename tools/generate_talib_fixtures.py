#!/usr/bin/env python3
"""Generate committed TA-Lib oracle fixtures for PalmScript parity tests.

The generator builds the pinned upstream TA-Lib commit in a temporary
workspace, evaluates a deterministic corpus through selected TA-Lib functions,
and writes committed JSON fixtures under `tests/data/ta_lib/`.

CI must consume the committed JSON only. This script is an offline refresh tool.
"""

from __future__ import annotations

import ctypes
import dataclasses
import json
import math
import pathlib
import subprocess
import tarfile
import tempfile
import urllib.request


COMMIT = "1bdf54384036852952b8b4cb97c09359ae407bd0"
ARCHIVE_URL = f"https://codeload.github.com/TA-Lib/ta-lib/tar.gz/{COMMIT}"
OUTPUT_PATH = pathlib.Path("tests/data/ta_lib/implemented_oracle.json")
EPSILON = 1e-9
SUCCESS = 0
TA_MATYPE = {
    "sma": 0,
    "ema": 1,
    "wma": 2,
    "dema": 3,
    "tema": 4,
    "trima": 5,
    "kama": 6,
    "mama": 7,
    "t3": 8,
}


@dataclasses.dataclass(frozen=True)
class Bar:
    open: float
    high: float
    low: float
    close: float
    volume: float
    time: float


@dataclasses.dataclass(frozen=True)
class Case:
    name: str
    script: str
    export_names: tuple[str, ...]
    family: str
    function: str
    dataset: str = "oscillating_ohlcv_v1"
    input_fields: tuple[str, ...] = ()
    int_options: tuple[int, ...] = ()
    ma_type: str | None = None


def main() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent
    output_path = root / OUTPUT_PATH
    output_path.parent.mkdir(parents=True, exist_ok=True)

    dataset = build_dataset()
    with tempfile.TemporaryDirectory(prefix="palmscript-talib-") as tempdir:
        tempdir_path = pathlib.Path(tempdir)
        source_dir = unpack_source(tempdir_path)
        library_path = build_talib(source_dir)
        oracle = TalibOracle(library_path)
        fixtures = render_fixture_document(dataset, oracle)

    output_path.write_text(json.dumps(fixtures, indent=2, sort_keys=True) + "\n")
    print(f"wrote TA-Lib fixtures to {output_path}")


def build_dataset() -> list[Bar]:
    bars: list[Bar] = []
    base_time = 1_704_067_200_000.0
    for index in range(48):
        close = 0.56 + 0.18 * math.sin(index * 0.37) + 0.12 * math.cos(index * 0.19)
        close = clamp(close, 0.08, 0.92)
        open_ = clamp(close + 0.045 * math.sin(index * 0.23 - 0.8), 0.05, 0.95)
        high = max(open_, close) + 0.03 + 0.008 * (index % 3)
        low = max(0.01, min(open_, close) - 0.025 - 0.006 * (index % 4))
        volume = 900.0 + (index % 5) * 37.0 + (index % 2) * 19.0 + index * 7.0
        bars.append(
            Bar(
                open=round(open_, 12),
                high=round(high, 12),
                low=round(low, 12),
                close=round(close, 12),
                volume=round(volume, 12),
                time=base_time + index * 60_000.0,
            )
        )
    return bars


def clamp(value: float, low: float, high: float) -> float:
    return min(max(value, low), high)


def unpack_source(tempdir: pathlib.Path) -> pathlib.Path:
    archive_path = tempdir / "ta-lib.tar.gz"
    with urllib.request.urlopen(ARCHIVE_URL) as response:
        archive_path.write_bytes(response.read())
    with tarfile.open(archive_path, mode="r:gz") as archive:
        archive.extractall(tempdir)
    matches = [path for path in tempdir.iterdir() if path.is_dir() and path.name.startswith("ta-lib-")]
    if len(matches) != 1:
        raise RuntimeError(f"expected one extracted source directory, found {matches}")
    return matches[0]


def build_talib(source_dir: pathlib.Path) -> pathlib.Path:
    build_dir = source_dir / "build"
    run(
        [
            "cmake",
            "-S",
            str(source_dir),
            "-B",
            str(build_dir),
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_DEV_TOOLS=OFF",
            "-DBUILD_SHARED_LIBS=ON",
        ]
    )
    run(["cmake", "--build", str(build_dir), "-j2"])
    library_candidates = sorted(
        path
        for path in build_dir.rglob("*")
        if path.is_file()
        and (
            path.name.startswith("libta-lib")
            or path.name.startswith("libta_lib")
            or path.name == "ta-lib.dll"
            or path.name == "ta_lib.dll"
        )
        and (path.suffix in {".so", ".dylib", ".dll"} or ".so." in path.name)
    )
    if not library_candidates:
        raise RuntimeError(f"could not find TA-Lib shared library under {build_dir}")
    return library_candidates[0]


def run(command: list[str]) -> None:
    subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


class TalibOracle:
    def __init__(self, library_path: pathlib.Path) -> None:
        self.lib = ctypes.CDLL(str(library_path))
        self.lib.TA_Initialize.argtypes = []
        self.lib.TA_Initialize.restype = ctypes.c_int
        self.lib.TA_Shutdown.argtypes = []
        self.lib.TA_Shutdown.restype = ctypes.c_int
        self._check(self.lib.TA_Initialize(), "TA_Initialize")

    def __del__(self) -> None:
        shutdown = getattr(self, "lib", None)
        if shutdown is not None:
            self.lib.TA_Shutdown()

    def compute(
        self,
        family: str,
        function: str,
        inputs: list[list[float]],
        int_options: tuple[int, ...],
        ma_type: str | None,
    ) -> list[list[float | None]]:
        if family == "unary":
            return [self.call_unary(function, inputs[0])]
        if family == "binary":
            return [self.call_binary(function, inputs[0], inputs[1])]
        if family == "ternary":
            return [self.call_ternary(function, inputs[0], inputs[1], inputs[2])]
        if family == "quaternary":
            return [self.call_quaternary(function, inputs[0], inputs[1], inputs[2], inputs[3])]
        if family == "window":
            return [self.call_window(function, inputs[0], int_options[0])]
        if family == "window_high_low":
            return [self.call_window_high_low(function, inputs[0], inputs[1], int_options[0])]
        if family == "window_index":
            return [self.call_window_index(function, inputs[0], int_options[0])]
        if family == "window_tuple":
            return self.call_window_tuple(function, inputs[0], int_options[0])
        if family == "window_index_tuple":
            return self.call_window_index_tuple(function, inputs[0], int_options[0])
        if family == "ma":
            return [self.call_ma(inputs[0], int_options[0], ma_type or "sma")]
        if family == "macd":
            return self.call_macd(inputs[0], *int_options)
        raise RuntimeError(f"unsupported oracle family {family}")

    def call_unary(self, function: str, input0: list[float]) -> list[float | None]:
        c_name = function.upper()
        return self._call_1in_1out_0opt(c_name, input0)

    def call_binary(self, function: str, input0: list[float], input1: list[float]) -> list[float | None]:
        c_name = function.upper()
        return self._call_2in_1out_0opt(c_name, input0, input1)

    def call_ternary(
        self,
        function: str,
        input0: list[float],
        input1: list[float],
        input2: list[float],
    ) -> list[float | None]:
        c_name = function.upper()
        return self._call_3in_1out_0opt(c_name, input0, input1, input2)

    def call_quaternary(
        self,
        function: str,
        input0: list[float],
        input1: list[float],
        input2: list[float],
        input3: list[float],
    ) -> list[float | None]:
        c_name = function.upper()
        return self._call_4in_1out_0opt(c_name, input0, input1, input2, input3)

    def call_window(self, function: str, input0: list[float], time_period: int) -> list[float | None]:
        c_name = function.upper()
        return self._call_1in_1out_1int(c_name, input0, time_period)

    def call_window_index(self, function: str, input0: list[float], time_period: int) -> list[float | None]:
        c_name = function.upper()
        return self._call_1in_1out_1int_index(c_name, input0, time_period)

    def call_window_tuple(
        self, function: str, input0: list[float], time_period: int
    ) -> list[list[float | None]]:
        c_name = function.upper()
        return self._call_1in_2out_1int(c_name, input0, time_period)

    def call_window_index_tuple(
        self, function: str, input0: list[float], time_period: int
    ) -> list[list[float | None]]:
        c_name = function.upper()
        return self._call_1in_2out_1int_index(c_name, input0, time_period)

    def call_window_high_low(
        self, function: str, high: list[float], low: list[float], time_period: int
    ) -> list[float | None]:
        c_name = function.upper()
        return self._call_2in_1out_1int(c_name, high, low, time_period)

    def call_ma(self, input0: list[float], time_period: int, ma_type: str) -> list[float | None]:
        c_name = "MA"
        return self._call_1in_1out_2int(c_name, input0, time_period, TA_MATYPE[ma_type])

    def call_macd(
        self, input0: list[float], fast_period: int, slow_period: int, signal_period: int
    ) -> list[list[float | None]]:
        return self._call_1in_3out_3int("MACD", input0, fast_period, slow_period, signal_period)

    def _call_1in_1out_0opt(self, c_name: str, input0: list[float]) -> list[float | None]:
        lookback = self._lookup_void(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0])

    def _call_1in_1out_1int(self, c_name: str, input0: list[float], opt0: int) -> list[float | None]:
        lookback = self._lookup_int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0], [opt0])

    def _call_1in_1out_1int_index(
        self, c_name: str, input0: list[float], opt0: int
    ) -> list[float | None]:
        lookback = self._lookup_int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
        ]
        return self._invoke_1out_int(func, lookback, [input0], [opt0])

    def _call_1in_1out_2int(
        self, c_name: str, input0: list[float], opt0: int, opt1: int
    ) -> list[float | None]:
        lookback = self._lookup_2int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0], [opt0, opt1])

    def _call_1in_3out_3int(
        self, c_name: str, input0: list[float], opt0: int, opt1: int, opt2: int
    ) -> list[list[float | None]]:
        lookback = self._lookup_3int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_3out(func, lookback, [input0], [opt0, opt1, opt2])

    def _call_1in_2out_1int(
        self, c_name: str, input0: list[float], opt0: int
    ) -> list[list[float | None]]:
        lookback = self._lookup_int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_2out(func, lookback, [input0], [opt0])

    def _call_1in_2out_1int_index(
        self, c_name: str, input0: list[float], opt0: int
    ) -> list[list[float | None]]:
        lookback = self._lookup_int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
        ]
        return self._invoke_2out_int(func, lookback, [input0], [opt0])

    def _call_2in_1out_0opt(
        self, c_name: str, input0: list[float], input1: list[float]
    ) -> list[float | None]:
        lookback = self._lookup_void(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0, input1])

    def _call_2in_1out_1int(
        self, c_name: str, input0: list[float], input1: list[float], opt0: int
    ) -> list[float | None]:
        lookback = self._lookup_int(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0, input1], [opt0])

    def _call_3in_1out_0opt(
        self, c_name: str, input0: list[float], input1: list[float], input2: list[float]
    ) -> list[float | None]:
        lookback = self._lookup_void(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0, input1, input2])

    def _call_4in_1out_0opt(
        self,
        c_name: str,
        input0: list[float],
        input1: list[float],
        input2: list[float],
        input3: list[float],
    ) -> list[float | None]:
        lookback = self._lookup_void(f"TA_{c_name}_Lookback")
        func = getattr(self.lib, f"TA_{c_name}")
        func.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_double),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_int),
            ctypes.POINTER(ctypes.c_double),
        ]
        return self._invoke_1out(func, lookback, [input0, input1, input2, input3])

    def _invoke_1out(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int] | None = None,
    ) -> list[float | None]:
        aligned, *_ = self._invoke_common(func, lookback_fn, inputs, options or [], 1)
        return aligned[0]

    def _invoke_1out_int(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int] | None = None,
    ) -> list[float | None]:
        aligned, *_ = self._invoke_common_int(func, lookback_fn, inputs, options or [], 1)
        return aligned[0]

    def _invoke_2out(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int] | None = None,
    ) -> list[list[float | None]]:
        aligned, *_ = self._invoke_common(func, lookback_fn, inputs, options or [], 2)
        return aligned

    def _invoke_2out_int(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int] | None = None,
    ) -> list[list[float | None]]:
        aligned, *_ = self._invoke_common_int(func, lookback_fn, inputs, options or [], 2)
        return aligned

    def _invoke_3out(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int] | None = None,
    ) -> list[list[float | None]]:
        aligned, *_ = self._invoke_common(func, lookback_fn, inputs, options or [], 3)
        return aligned

    def _invoke_common(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int],
        output_count: int,
    ) -> tuple[list[list[float | None]], int, int]:
        length = len(inputs[0])
        arrays = [(ctypes.c_double * length)(*values) for values in inputs]
        out_beg = ctypes.c_int()
        out_nb = ctypes.c_int()
        outputs = [(ctypes.c_double * length)() for _ in range(output_count)]
        rc = func(
            0,
            length - 1,
            *arrays,
            *options,
            ctypes.byref(out_beg),
            ctypes.byref(out_nb),
            *outputs,
        )
        self._check(rc, getattr(func, "__name__", "TA function"))
        aligned = []
        for output in outputs:
            aligned.append(align_output(length, out_beg.value, out_nb.value, output))
        return aligned, out_beg.value, out_nb.value

    def _invoke_common_int(
        self,
        func: ctypes._CFuncPtr,
        lookback_fn,
        inputs: list[list[float]],
        options: list[int],
        output_count: int,
    ) -> tuple[list[list[float | None]], int, int]:
        length = len(inputs[0])
        arrays = [(ctypes.c_double * length)(*values) for values in inputs]
        out_beg = ctypes.c_int()
        out_nb = ctypes.c_int()
        outputs = [(ctypes.c_int * length)() for _ in range(output_count)]
        rc = func(
            0,
            length - 1,
            *arrays,
            *options,
            ctypes.byref(out_beg),
            ctypes.byref(out_nb),
            *outputs,
        )
        self._check(rc, getattr(func, "__name__", "TA function"))
        aligned = []
        for output in outputs:
            aligned.append(align_output_int(length, out_beg.value, out_nb.value, output))
        return aligned, out_beg.value, out_nb.value

    def _lookup_void(self, name: str):
        func = getattr(self.lib, name)
        func.argtypes = []
        func.restype = ctypes.c_int
        return func

    def _lookup_int(self, name: str):
        func = getattr(self.lib, name)
        func.argtypes = [ctypes.c_int]
        func.restype = ctypes.c_int
        return func

    def _lookup_2int(self, name: str):
        func = getattr(self.lib, name)
        func.argtypes = [ctypes.c_int, ctypes.c_int]
        func.restype = ctypes.c_int
        return func

    def _lookup_3int(self, name: str):
        func = getattr(self.lib, name)
        func.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
        func.restype = ctypes.c_int
        return func

    def _check(self, rc: int, name: str) -> None:
        if rc != SUCCESS:
            raise RuntimeError(f"{name} returned error code {rc}")


def align_output(length: int, out_beg: int, out_nb: int, output) -> list[float | None]:
    aligned: list[float | None] = [None] * length
    for index in range(out_nb):
        aligned[out_beg + index] = round(float(output[index]), 12)
    return aligned


def align_output_int(length: int, out_beg: int, out_nb: int, output) -> list[float | None]:
    aligned: list[float | None] = [None] * length
    for index in range(out_nb):
        aligned[out_beg + index] = float(output[index])
    return aligned


def render_fixture_document(dataset: list[Bar], oracle: TalibOracle) -> dict:
    fields = extract_fields(dataset)
    cases = []
    for case in fixture_cases():
        inputs = [fields[name] for name in case.input_fields]
        outputs = oracle.compute(case.family, case.function, inputs, case.int_options, case.ma_type)
        cases.append(
            {
                "name": case.name,
                "dataset": case.dataset,
                "script": case.script,
                "epsilon": EPSILON,
                "expected_exports": {
                    export_name: output for export_name, output in zip(case.export_names, outputs)
                },
            }
        )

    return {
        "generator": "tools/generate_talib_fixtures.py",
        "upstream_commit": COMMIT,
        "datasets": {
            "oscillating_ohlcv_v1": {
                "bars": [dataclasses.asdict(bar) for bar in dataset],
            }
        },
        "cases": cases,
    }


def extract_fields(dataset: list[Bar]) -> dict[str, list[float]]:
    return {
        "open": [bar.open for bar in dataset],
        "high": [bar.high for bar in dataset],
        "low": [bar.low for bar in dataset],
        "close": [bar.close for bar in dataset],
        "volume": [bar.volume for bar in dataset],
        "time": [bar.time for bar in dataset],
    }


def fixture_cases() -> list[Case]:
    cases = [
        Case("sma_close_5", script_for_single_export("value", "sma(close, 5)"), ("value",), "window", "sma", input_fields=("close",), int_options=(5,)),
        Case("ema_close_5", script_for_single_export("value", "ema(close, 5)"), ("value",), "window", "ema", input_fields=("close",), int_options=(5,)),
        Case("rsi_close_5", script_for_single_export("value", "rsi(close, 5)"), ("value",), "window", "rsi", input_fields=("close",), int_options=(5,)),
        Case("ma_close_5_wma", script_for_single_export("value", "ma(close, 5, ma_type.wma)"), ("value",), "ma", "ma", input_fields=("close",), int_options=(5,), ma_type="wma"),
        Case(
            "macd_close_3_5_2",
            "interval 1m\nlet (line, signal, hist) = macd(close, 3, 5, 2)\nexport macd_line = line\nexport macd_signal = signal\nexport macd_hist = hist\nplot(0)",
            ("macd_line", "macd_signal", "macd_hist"),
            "macd",
            "macd",
            input_fields=("close",),
            int_options=(3, 5, 2),
        ),
    ]

    for name in [
        "acos",
        "asin",
        "atan",
        "ceil",
        "cos",
        "cosh",
        "exp",
        "floor",
        "ln",
        "log10",
        "sin",
        "sinh",
        "sqrt",
        "tan",
        "tanh",
    ]:
        cases.append(
            Case(
                f"{name}_close",
                script_for_single_export("value", f"{name}(close)"),
                ("value",),
                "unary",
                name,
                input_fields=("close",),
            )
        )

    cases.extend(
        [
            Case("add_open_close", script_for_single_export("value", "add(open, close)"), ("value",), "binary", "add", input_fields=("open", "close")),
            Case("div_high_low", script_for_single_export("value", "div(high, low)"), ("value",), "binary", "div", input_fields=("high", "low")),
            Case("mult_open_close", script_for_single_export("value", "mult(open, close)"), ("value",), "binary", "mult", input_fields=("open", "close")),
            Case("sub_high_low", script_for_single_export("value", "sub(high, low)"), ("value",), "binary", "sub", input_fields=("high", "low")),
            Case("avgprice", script_for_single_export("value", "avgprice(open, high, low, close)"), ("value",), "quaternary", "avgprice", input_fields=("open", "high", "low", "close")),
            Case("medprice", script_for_single_export("value", "medprice(high, low)"), ("value",), "binary", "medprice", input_fields=("high", "low")),
            Case("typprice", script_for_single_export("value", "typprice(high, low, close)"), ("value",), "ternary", "typprice", input_fields=("high", "low", "close")),
            Case("wclprice", script_for_single_export("value", "wclprice(high, low, close)"), ("value",), "ternary", "wclprice", input_fields=("high", "low", "close")),
            Case("max_default", script_for_single_export("value", "max(close)"), ("value",), "window", "max", input_fields=("close",), int_options=(30,)),
            Case("min_default", script_for_single_export("value", "min(close)"), ("value",), "window", "min", input_fields=("close",), int_options=(30,)),
            Case("sum_default", script_for_single_export("value", "sum(close)"), ("value",), "window", "sum", input_fields=("close",), int_options=(30,)),
            Case("midpoint_default", script_for_single_export("value", "midpoint(close)"), ("value",), "window", "midpoint", input_fields=("close",), int_options=(14,)),
            Case("midprice_default", script_for_single_export("value", "midprice(high, low)"), ("value",), "window_high_low", "midprice", input_fields=("high", "low"), int_options=(14,)),
            Case("wma_default", script_for_single_export("value", "wma(close)"), ("value",), "window", "wma", input_fields=("close",), int_options=(30,)),
            Case("avgdev_default", script_for_single_export("value", "avgdev(close)"), ("value",), "window", "avgdev", input_fields=("close",), int_options=(14,)),
            Case("maxindex_default", script_for_single_export("value", "maxindex(close)"), ("value",), "window_index", "maxindex", input_fields=("close",), int_options=(30,)),
            Case("minindex_default", script_for_single_export("value", "minindex(close)"), ("value",), "window_index", "minindex", input_fields=("close",), int_options=(30,)),
            Case(
                "minmax_default",
                "interval 1m\nlet (lo, hi) = minmax(close)\nexport min_value = lo\nexport max_value = hi\nplot(0)",
                ("min_value", "max_value"),
                "window_tuple",
                "minmax",
                input_fields=("close",),
                int_options=(30,),
            ),
            Case(
                "minmaxindex_default",
                "interval 1m\nlet (lo_index, hi_index) = minmaxindex(close)\nexport min_index = lo_index\nexport max_index = hi_index\nplot(0)",
                ("min_index", "max_index"),
                "window_index_tuple",
                "minmaxindex",
                input_fields=("close",),
                int_options=(30,),
            ),
            Case("obv_close_volume", script_for_single_export("value", "obv(close, volume)"), ("value",), "binary", "obv", input_fields=("close", "volume")),
            Case("trange", script_for_single_export("value", "trange(high, low, close)"), ("value",), "ternary", "trange", input_fields=("high", "low", "close")),
        ]
    )
    return cases


def script_for_single_export(name: str, expr: str) -> str:
    return f"interval 1m\nexport {name} = {expr}\nplot(0)"


if __name__ == "__main__":
    main()
