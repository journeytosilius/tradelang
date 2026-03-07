//! Builtin function identifiers and metadata shared across the compiler, IDE,
//! and VM.
//!
//! The builtin registry is the source of truth for reserved names, callable
//! surface, signatures, and broad implementation class.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinArity {
    NonCallable,
    Exact(usize),
    Range { min: usize, max: usize },
}

impl BuiltinArity {
    pub const fn accepts(self, found: usize) -> bool {
        match self {
            Self::NonCallable => false,
            Self::Exact(expected) => found == expected,
            Self::Range { min, max } => found >= min && found <= max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinKind {
    MarketSeries,
    Plot,
    Indicator,
    MovingAverage,
    MaOscillator,
    IndicatorTuple,
    UnaryMathTransform,
    NumericBinary,
    PriceTransform,
    RollingSingleInput,
    RollingSingleInputFactor,
    RollingSingleInputTuple,
    RollingDoubleInput,
    RollingHighLow,
    RollingHighLowClose,
    VolumeIndicator,
    VolatilityIndicator,
    Relation2,
    Relation3,
    Cross,
    Change,
    Roc,
    Highest,
    Lowest,
    Rising,
    Falling,
    BarsSince,
    ValueWhen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum BuiltinId {
    Open = 0,
    High = 1,
    Low = 2,
    Close = 3,
    Volume = 4,
    Time = 5,
    Sma = 6,
    Ema = 7,
    Rsi = 8,
    Plot = 9,
    Above = 10,
    Below = 11,
    Between = 12,
    Outside = 13,
    Cross = 14,
    Crossover = 15,
    Crossunder = 16,
    Change = 17,
    Roc = 18,
    Highest = 19,
    Lowest = 20,
    Rising = 21,
    Falling = 22,
    BarsSince = 23,
    ValueWhen = 24,
    Ma = 25,
    Macd = 26,
    Acos = 27,
    Asin = 28,
    Atan = 29,
    Avgprice = 30,
    Ceil = 31,
    Cos = 32,
    Cosh = 33,
    Exp = 34,
    Floor = 35,
    Ln = 36,
    Log10 = 37,
    Sin = 38,
    Sinh = 39,
    Sqrt = 40,
    Tan = 41,
    Tanh = 42,
    Add = 43,
    Div = 44,
    Mult = 45,
    Sub = 46,
    Max = 47,
    Min = 48,
    Sum = 49,
    Midpoint = 50,
    Midprice = 51,
    Medprice = 52,
    Typprice = 53,
    Wclprice = 54,
    Obv = 55,
    Trange = 56,
    Wma = 57,
    Avgdev = 58,
    MaxIndex = 59,
    MinIndex = 60,
    MinMax = 61,
    MinMaxIndex = 62,
    Stddev = 63,
    Var = 64,
    LinearReg = 65,
    LinearRegAngle = 66,
    LinearRegIntercept = 67,
    LinearRegSlope = 68,
    Tsf = 69,
    Beta = 70,
    Correl = 71,
    Mom = 72,
    Rocp = 73,
    Rocr = 74,
    Rocr100 = 75,
    Apo = 76,
    Ppo = 77,
    Cmo = 78,
    Willr = 79,
}

impl BuiltinId {
    pub const RESERVED: [Self; 80] = [
        Self::Open,
        Self::High,
        Self::Low,
        Self::Close,
        Self::Volume,
        Self::Time,
        Self::Sma,
        Self::Ema,
        Self::Rsi,
        Self::Plot,
        Self::Above,
        Self::Below,
        Self::Between,
        Self::Outside,
        Self::Cross,
        Self::Crossover,
        Self::Crossunder,
        Self::Change,
        Self::Roc,
        Self::Highest,
        Self::Lowest,
        Self::Rising,
        Self::Falling,
        Self::BarsSince,
        Self::ValueWhen,
        Self::Ma,
        Self::Macd,
        Self::Acos,
        Self::Asin,
        Self::Atan,
        Self::Avgprice,
        Self::Ceil,
        Self::Cos,
        Self::Cosh,
        Self::Exp,
        Self::Floor,
        Self::Ln,
        Self::Log10,
        Self::Sin,
        Self::Sinh,
        Self::Sqrt,
        Self::Tan,
        Self::Tanh,
        Self::Add,
        Self::Div,
        Self::Mult,
        Self::Sub,
        Self::Max,
        Self::Min,
        Self::Sum,
        Self::Midpoint,
        Self::Midprice,
        Self::Medprice,
        Self::Typprice,
        Self::Wclprice,
        Self::Obv,
        Self::Trange,
        Self::Wma,
        Self::Avgdev,
        Self::MaxIndex,
        Self::MinIndex,
        Self::MinMax,
        Self::MinMaxIndex,
        Self::Stddev,
        Self::Var,
        Self::LinearReg,
        Self::LinearRegAngle,
        Self::LinearRegIntercept,
        Self::LinearRegSlope,
        Self::Tsf,
        Self::Beta,
        Self::Correl,
        Self::Mom,
        Self::Rocp,
        Self::Rocr,
        Self::Rocr100,
        Self::Apo,
        Self::Ppo,
        Self::Cmo,
        Self::Willr,
    ];

    pub const CALLABLE: [Self; 74] = [
        Self::Sma,
        Self::Ema,
        Self::Rsi,
        Self::Plot,
        Self::Above,
        Self::Below,
        Self::Between,
        Self::Outside,
        Self::Cross,
        Self::Crossover,
        Self::Crossunder,
        Self::Change,
        Self::Roc,
        Self::Highest,
        Self::Lowest,
        Self::Rising,
        Self::Falling,
        Self::BarsSince,
        Self::ValueWhen,
        Self::Ma,
        Self::Macd,
        Self::Acos,
        Self::Asin,
        Self::Atan,
        Self::Avgprice,
        Self::Ceil,
        Self::Cos,
        Self::Cosh,
        Self::Exp,
        Self::Floor,
        Self::Ln,
        Self::Log10,
        Self::Sin,
        Self::Sinh,
        Self::Sqrt,
        Self::Tan,
        Self::Tanh,
        Self::Add,
        Self::Div,
        Self::Mult,
        Self::Sub,
        Self::Max,
        Self::Min,
        Self::Sum,
        Self::Midpoint,
        Self::Midprice,
        Self::Medprice,
        Self::Typprice,
        Self::Wclprice,
        Self::Obv,
        Self::Trange,
        Self::Wma,
        Self::Avgdev,
        Self::MaxIndex,
        Self::MinIndex,
        Self::MinMax,
        Self::MinMaxIndex,
        Self::Stddev,
        Self::Var,
        Self::LinearReg,
        Self::LinearRegAngle,
        Self::LinearRegIntercept,
        Self::LinearRegSlope,
        Self::Tsf,
        Self::Beta,
        Self::Correl,
        Self::Mom,
        Self::Rocp,
        Self::Rocr,
        Self::Rocr100,
        Self::Apo,
        Self::Ppo,
        Self::Cmo,
        Self::Willr,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "open" => Some(Self::Open),
            "high" => Some(Self::High),
            "low" => Some(Self::Low),
            "close" => Some(Self::Close),
            "volume" => Some(Self::Volume),
            "time" => Some(Self::Time),
            "sma" => Some(Self::Sma),
            "ema" => Some(Self::Ema),
            "rsi" => Some(Self::Rsi),
            "plot" => Some(Self::Plot),
            "above" => Some(Self::Above),
            "below" => Some(Self::Below),
            "between" => Some(Self::Between),
            "outside" => Some(Self::Outside),
            "cross" => Some(Self::Cross),
            "crossover" => Some(Self::Crossover),
            "crossunder" => Some(Self::Crossunder),
            "change" => Some(Self::Change),
            "roc" => Some(Self::Roc),
            "highest" => Some(Self::Highest),
            "lowest" => Some(Self::Lowest),
            "rising" => Some(Self::Rising),
            "falling" => Some(Self::Falling),
            "barssince" => Some(Self::BarsSince),
            "valuewhen" => Some(Self::ValueWhen),
            "ma" => Some(Self::Ma),
            "macd" => Some(Self::Macd),
            "acos" => Some(Self::Acos),
            "asin" => Some(Self::Asin),
            "atan" => Some(Self::Atan),
            "avgprice" => Some(Self::Avgprice),
            "ceil" => Some(Self::Ceil),
            "cos" => Some(Self::Cos),
            "cosh" => Some(Self::Cosh),
            "exp" => Some(Self::Exp),
            "floor" => Some(Self::Floor),
            "ln" => Some(Self::Ln),
            "log10" => Some(Self::Log10),
            "sin" => Some(Self::Sin),
            "sinh" => Some(Self::Sinh),
            "sqrt" => Some(Self::Sqrt),
            "tan" => Some(Self::Tan),
            "tanh" => Some(Self::Tanh),
            "add" => Some(Self::Add),
            "div" => Some(Self::Div),
            "mult" => Some(Self::Mult),
            "sub" => Some(Self::Sub),
            "max" => Some(Self::Max),
            "min" => Some(Self::Min),
            "sum" => Some(Self::Sum),
            "midpoint" => Some(Self::Midpoint),
            "midprice" => Some(Self::Midprice),
            "medprice" => Some(Self::Medprice),
            "typprice" => Some(Self::Typprice),
            "wclprice" => Some(Self::Wclprice),
            "obv" => Some(Self::Obv),
            "trange" => Some(Self::Trange),
            "wma" => Some(Self::Wma),
            "avgdev" => Some(Self::Avgdev),
            "maxindex" => Some(Self::MaxIndex),
            "minindex" => Some(Self::MinIndex),
            "minmax" => Some(Self::MinMax),
            "minmaxindex" => Some(Self::MinMaxIndex),
            "stddev" => Some(Self::Stddev),
            "var" => Some(Self::Var),
            "linearreg" => Some(Self::LinearReg),
            "linearreg_angle" => Some(Self::LinearRegAngle),
            "linearreg_intercept" => Some(Self::LinearRegIntercept),
            "linearreg_slope" => Some(Self::LinearRegSlope),
            "tsf" => Some(Self::Tsf),
            "beta" => Some(Self::Beta),
            "correl" => Some(Self::Correl),
            "mom" => Some(Self::Mom),
            "rocp" => Some(Self::Rocp),
            "rocr" => Some(Self::Rocr),
            "rocr100" => Some(Self::Rocr100),
            "apo" => Some(Self::Apo),
            "ppo" => Some(Self::Ppo),
            "cmo" => Some(Self::Cmo),
            "willr" => Some(Self::Willr),
            _ => None,
        }
    }

    pub fn from_u16(id: u16) -> Option<Self> {
        match id {
            0 => Some(Self::Open),
            1 => Some(Self::High),
            2 => Some(Self::Low),
            3 => Some(Self::Close),
            4 => Some(Self::Volume),
            5 => Some(Self::Time),
            6 => Some(Self::Sma),
            7 => Some(Self::Ema),
            8 => Some(Self::Rsi),
            9 => Some(Self::Plot),
            10 => Some(Self::Above),
            11 => Some(Self::Below),
            12 => Some(Self::Between),
            13 => Some(Self::Outside),
            14 => Some(Self::Cross),
            15 => Some(Self::Crossover),
            16 => Some(Self::Crossunder),
            17 => Some(Self::Change),
            18 => Some(Self::Roc),
            19 => Some(Self::Highest),
            20 => Some(Self::Lowest),
            21 => Some(Self::Rising),
            22 => Some(Self::Falling),
            23 => Some(Self::BarsSince),
            24 => Some(Self::ValueWhen),
            25 => Some(Self::Ma),
            26 => Some(Self::Macd),
            27 => Some(Self::Acos),
            28 => Some(Self::Asin),
            29 => Some(Self::Atan),
            30 => Some(Self::Avgprice),
            31 => Some(Self::Ceil),
            32 => Some(Self::Cos),
            33 => Some(Self::Cosh),
            34 => Some(Self::Exp),
            35 => Some(Self::Floor),
            36 => Some(Self::Ln),
            37 => Some(Self::Log10),
            38 => Some(Self::Sin),
            39 => Some(Self::Sinh),
            40 => Some(Self::Sqrt),
            41 => Some(Self::Tan),
            42 => Some(Self::Tanh),
            43 => Some(Self::Add),
            44 => Some(Self::Div),
            45 => Some(Self::Mult),
            46 => Some(Self::Sub),
            47 => Some(Self::Max),
            48 => Some(Self::Min),
            49 => Some(Self::Sum),
            50 => Some(Self::Midpoint),
            51 => Some(Self::Midprice),
            52 => Some(Self::Medprice),
            53 => Some(Self::Typprice),
            54 => Some(Self::Wclprice),
            55 => Some(Self::Obv),
            56 => Some(Self::Trange),
            57 => Some(Self::Wma),
            58 => Some(Self::Avgdev),
            59 => Some(Self::MaxIndex),
            60 => Some(Self::MinIndex),
            61 => Some(Self::MinMax),
            62 => Some(Self::MinMaxIndex),
            63 => Some(Self::Stddev),
            64 => Some(Self::Var),
            65 => Some(Self::LinearReg),
            66 => Some(Self::LinearRegAngle),
            67 => Some(Self::LinearRegIntercept),
            68 => Some(Self::LinearRegSlope),
            69 => Some(Self::Tsf),
            70 => Some(Self::Beta),
            71 => Some(Self::Correl),
            72 => Some(Self::Mom),
            73 => Some(Self::Rocp),
            74 => Some(Self::Rocr),
            75 => Some(Self::Rocr100),
            76 => Some(Self::Apo),
            77 => Some(Self::Ppo),
            78 => Some(Self::Cmo),
            79 => Some(Self::Willr),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::High => "high",
            Self::Low => "low",
            Self::Close => "close",
            Self::Volume => "volume",
            Self::Time => "time",
            Self::Sma => "sma",
            Self::Ema => "ema",
            Self::Rsi => "rsi",
            Self::Plot => "plot",
            Self::Above => "above",
            Self::Below => "below",
            Self::Between => "between",
            Self::Outside => "outside",
            Self::Cross => "cross",
            Self::Crossover => "crossover",
            Self::Crossunder => "crossunder",
            Self::Change => "change",
            Self::Roc => "roc",
            Self::Highest => "highest",
            Self::Lowest => "lowest",
            Self::Rising => "rising",
            Self::Falling => "falling",
            Self::BarsSince => "barssince",
            Self::ValueWhen => "valuewhen",
            Self::Ma => "ma",
            Self::Macd => "macd",
            Self::Acos => "acos",
            Self::Asin => "asin",
            Self::Atan => "atan",
            Self::Avgprice => "avgprice",
            Self::Ceil => "ceil",
            Self::Cos => "cos",
            Self::Cosh => "cosh",
            Self::Exp => "exp",
            Self::Floor => "floor",
            Self::Ln => "ln",
            Self::Log10 => "log10",
            Self::Sin => "sin",
            Self::Sinh => "sinh",
            Self::Sqrt => "sqrt",
            Self::Tan => "tan",
            Self::Tanh => "tanh",
            Self::Add => "add",
            Self::Div => "div",
            Self::Mult => "mult",
            Self::Sub => "sub",
            Self::Max => "max",
            Self::Min => "min",
            Self::Sum => "sum",
            Self::Midpoint => "midpoint",
            Self::Midprice => "midprice",
            Self::Medprice => "medprice",
            Self::Typprice => "typprice",
            Self::Wclprice => "wclprice",
            Self::Obv => "obv",
            Self::Trange => "trange",
            Self::Wma => "wma",
            Self::Avgdev => "avgdev",
            Self::MaxIndex => "maxindex",
            Self::MinIndex => "minindex",
            Self::MinMax => "minmax",
            Self::MinMaxIndex => "minmaxindex",
            Self::Stddev => "stddev",
            Self::Var => "var",
            Self::LinearReg => "linearreg",
            Self::LinearRegAngle => "linearreg_angle",
            Self::LinearRegIntercept => "linearreg_intercept",
            Self::LinearRegSlope => "linearreg_slope",
            Self::Tsf => "tsf",
            Self::Beta => "beta",
            Self::Correl => "correl",
            Self::Mom => "mom",
            Self::Rocp => "rocp",
            Self::Rocr => "rocr",
            Self::Rocr100 => "rocr100",
            Self::Apo => "apo",
            Self::Ppo => "ppo",
            Self::Cmo => "cmo",
            Self::Willr => "willr",
        }
    }

    pub const fn kind(self) -> BuiltinKind {
        match self {
            Self::Open | Self::High | Self::Low | Self::Close | Self::Volume | Self::Time => {
                BuiltinKind::MarketSeries
            }
            Self::Plot => BuiltinKind::Plot,
            Self::Sma | Self::Ema | Self::Rsi => BuiltinKind::Indicator,
            Self::Ma => BuiltinKind::MovingAverage,
            Self::Apo | Self::Ppo => BuiltinKind::MaOscillator,
            Self::Macd => BuiltinKind::IndicatorTuple,
            Self::Acos
            | Self::Asin
            | Self::Atan
            | Self::Ceil
            | Self::Cos
            | Self::Cosh
            | Self::Exp
            | Self::Floor
            | Self::Ln
            | Self::Log10
            | Self::Sin
            | Self::Sinh
            | Self::Sqrt
            | Self::Tan
            | Self::Tanh => BuiltinKind::UnaryMathTransform,
            Self::Add | Self::Div | Self::Mult | Self::Sub => BuiltinKind::NumericBinary,
            Self::Avgprice | Self::Medprice | Self::Typprice | Self::Wclprice => {
                BuiltinKind::PriceTransform
            }
            Self::Max | Self::Min | Self::Sum | Self::Midpoint => BuiltinKind::RollingSingleInput,
            Self::Wma | Self::Avgdev | Self::MaxIndex | Self::MinIndex => {
                BuiltinKind::RollingSingleInput
            }
            Self::Stddev | Self::Var => BuiltinKind::RollingSingleInputFactor,
            Self::MinMax | Self::MinMaxIndex => BuiltinKind::RollingSingleInputTuple,
            Self::LinearReg
            | Self::LinearRegAngle
            | Self::LinearRegIntercept
            | Self::LinearRegSlope
            | Self::Tsf
            | Self::Cmo => BuiltinKind::RollingSingleInput,
            Self::Beta | Self::Correl => BuiltinKind::RollingDoubleInput,
            Self::Midprice => BuiltinKind::RollingHighLow,
            Self::Willr => BuiltinKind::RollingHighLowClose,
            Self::Obv => BuiltinKind::VolumeIndicator,
            Self::Trange => BuiltinKind::VolatilityIndicator,
            Self::Above | Self::Below => BuiltinKind::Relation2,
            Self::Between | Self::Outside => BuiltinKind::Relation3,
            Self::Cross | Self::Crossover | Self::Crossunder => BuiltinKind::Cross,
            Self::Change => BuiltinKind::Change,
            Self::Roc | Self::Mom | Self::Rocp | Self::Rocr | Self::Rocr100 => BuiltinKind::Roc,
            Self::Highest => BuiltinKind::Highest,
            Self::Lowest => BuiltinKind::Lowest,
            Self::Rising => BuiltinKind::Rising,
            Self::Falling => BuiltinKind::Falling,
            Self::BarsSince => BuiltinKind::BarsSince,
            Self::ValueWhen => BuiltinKind::ValueWhen,
        }
    }

    pub const fn is_callable(self) -> bool {
        !matches!(self.arity(), BuiltinArity::NonCallable)
    }

    pub const fn arity(self) -> BuiltinArity {
        match self {
            Self::Open | Self::High | Self::Low | Self::Close | Self::Volume | Self::Time => {
                BuiltinArity::NonCallable
            }
            Self::Plot
            | Self::BarsSince
            | Self::Acos
            | Self::Asin
            | Self::Atan
            | Self::Ceil
            | Self::Cos
            | Self::Cosh
            | Self::Exp
            | Self::Floor
            | Self::Ln
            | Self::Log10
            | Self::Sin
            | Self::Sinh
            | Self::Sqrt
            | Self::Tan
            | Self::Tanh => BuiltinArity::Exact(1),
            Self::Sma
            | Self::Ema
            | Self::Rsi
            | Self::Above
            | Self::Below
            | Self::Cross
            | Self::Crossover
            | Self::Crossunder
            | Self::Change
            | Self::Highest
            | Self::Lowest
            | Self::Rising
            | Self::Falling
            | Self::Add
            | Self::Div
            | Self::Mult
            | Self::Sub
            | Self::Medprice
            | Self::Obv => BuiltinArity::Exact(2),
            Self::Roc | Self::Mom | Self::Rocp | Self::Rocr | Self::Rocr100 => {
                BuiltinArity::Range { min: 1, max: 2 }
            }
            Self::Cmo => BuiltinArity::Range { min: 1, max: 2 },
            Self::Avgprice => BuiltinArity::Exact(4),
            Self::Typprice
            | Self::Wclprice
            | Self::Ma
            | Self::Between
            | Self::Outside
            | Self::ValueWhen
            | Self::Trange => BuiltinArity::Exact(3),
            Self::Willr => BuiltinArity::Range { min: 3, max: 4 },
            Self::Macd => BuiltinArity::Exact(4),
            Self::Max
            | Self::Min
            | Self::Sum
            | Self::Midpoint
            | Self::Wma
            | Self::Avgdev
            | Self::MaxIndex
            | Self::MinIndex
            | Self::MinMax
            | Self::MinMaxIndex
            | Self::LinearReg
            | Self::LinearRegAngle
            | Self::LinearRegIntercept
            | Self::LinearRegSlope
            | Self::Tsf => BuiltinArity::Range { min: 1, max: 2 },
            Self::Beta | Self::Correl => BuiltinArity::Range { min: 2, max: 3 },
            Self::Apo | Self::Ppo => BuiltinArity::Range { min: 1, max: 4 },
            Self::Stddev | Self::Var => BuiltinArity::Range { min: 1, max: 3 },
            Self::Midprice => BuiltinArity::Range { min: 2, max: 3 },
        }
    }

    pub const fn signature(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::High => "high",
            Self::Low => "low",
            Self::Close => "close",
            Self::Volume => "volume",
            Self::Time => "time",
            Self::Sma => "sma(series, length)",
            Self::Ema => "ema(series, length)",
            Self::Rsi => "rsi(series, length)",
            Self::Plot => "plot(value)",
            Self::Above => "above(a, b)",
            Self::Below => "below(a, b)",
            Self::Between => "between(x, low, high)",
            Self::Outside => "outside(x, low, high)",
            Self::Cross => "cross(a, b)",
            Self::Crossover => "crossover(a, b)",
            Self::Crossunder => "crossunder(a, b)",
            Self::Change => "change(series, length)",
            Self::Roc => "roc(series[, length=10])",
            Self::Highest => "highest(series, length)",
            Self::Lowest => "lowest(series, length)",
            Self::Rising => "rising(series, length)",
            Self::Falling => "falling(series, length)",
            Self::BarsSince => "barssince(condition)",
            Self::ValueWhen => "valuewhen(condition, source, occurrence)",
            Self::Ma => "ma(series, length, ma_type)",
            Self::Apo => "apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])",
            Self::Ppo => "ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])",
            Self::Macd => "macd(series, fast_length, slow_length, signal_length)",
            Self::Cmo => "cmo(series[, length=14])",
            Self::Acos => "acos(real)",
            Self::Asin => "asin(real)",
            Self::Atan => "atan(real)",
            Self::Avgprice => "avgprice(open, high, low, close)",
            Self::Ceil => "ceil(real)",
            Self::Cos => "cos(real)",
            Self::Cosh => "cosh(real)",
            Self::Exp => "exp(real)",
            Self::Floor => "floor(real)",
            Self::Ln => "ln(real)",
            Self::Log10 => "log10(real)",
            Self::Sin => "sin(real)",
            Self::Sinh => "sinh(real)",
            Self::Sqrt => "sqrt(real)",
            Self::Tan => "tan(real)",
            Self::Tanh => "tanh(real)",
            Self::Add => "add(a, b)",
            Self::Div => "div(a, b)",
            Self::Mult => "mult(a, b)",
            Self::Sub => "sub(a, b)",
            Self::Max => "max(series[, length=30])",
            Self::Min => "min(series[, length=30])",
            Self::Sum => "sum(series[, length=30])",
            Self::Midpoint => "midpoint(series[, length=14])",
            Self::Midprice => "midprice(high, low[, length=14])",
            Self::Medprice => "medprice(high, low)",
            Self::Typprice => "typprice(high, low, close)",
            Self::Wclprice => "wclprice(high, low, close)",
            Self::Obv => "obv(series, volume)",
            Self::Trange => "trange(high, low, close)",
            Self::Wma => "wma(series[, length=30])",
            Self::Avgdev => "avgdev(series[, length=14])",
            Self::MaxIndex => "maxindex(series[, length=30])",
            Self::MinIndex => "minindex(series[, length=30])",
            Self::MinMax => "minmax(series[, length=30])",
            Self::MinMaxIndex => "minmaxindex(series[, length=30])",
            Self::Stddev => "stddev(series[, length=5[, deviations=1.0]])",
            Self::Var => "var(series[, length=5[, deviations=1.0]])",
            Self::LinearReg => "linearreg(series[, length=14])",
            Self::LinearRegAngle => "linearreg_angle(series[, length=14])",
            Self::LinearRegIntercept => "linearreg_intercept(series[, length=14])",
            Self::LinearRegSlope => "linearreg_slope(series[, length=14])",
            Self::Tsf => "tsf(series[, length=14])",
            Self::Beta => "beta(series0, series1[, length=5])",
            Self::Correl => "correl(series0, series1[, length=30])",
            Self::Mom => "mom(series[, length=10])",
            Self::Rocp => "rocp(series[, length=10])",
            Self::Rocr => "rocr(series[, length=10])",
            Self::Rocr100 => "rocr100(series[, length=10])",
            Self::Willr => "willr(high, low, close[, length=14])",
        }
    }

    pub const fn summary(self) -> &'static str {
        match self {
            Self::Open => "series<float> for the base-interval open.",
            Self::High => "series<float> for the base-interval high.",
            Self::Low => "series<float> for the base-interval low.",
            Self::Close => "series<float> for the base-interval close.",
            Self::Volume => "series<float> for the base-interval volume.",
            Self::Time => "series<float> for the base-interval candle open time.",
            Self::Sma => "Simple moving average.",
            Self::Ema => "Exponential moving average.",
            Self::Rsi => "Relative strength index.",
            Self::Plot => "Emit a plot output for the current bar.",
            Self::Above => "True when `a > b`.",
            Self::Below => "True when `a < b`.",
            Self::Between => "True when `low < x` and `x < high`.",
            Self::Outside => "True when `x < low` or `x > high`.",
            Self::Cross => "True when `a` crosses `b` in either direction.",
            Self::Crossover => "True when `a` crosses above `b`.",
            Self::Crossunder => "True when `a` crosses below `b`.",
            Self::Change => "Difference between the current sample and a prior sample.",
            Self::Roc => "Rate of change in percent.",
            Self::Mom => "Momentum over a trailing period.",
            Self::Rocp => "Rate of change ratio.",
            Self::Rocr => "Rate of change ratio over one.",
            Self::Rocr100 => "Rate of change ratio scaled by 100.",
            Self::Highest => "Highest value over a trailing window including the current sample.",
            Self::Lowest => "Lowest value over a trailing window including the current sample.",
            Self::Rising => "True when the current sample is strictly greater than every prior sample in the trailing window.",
            Self::Falling => "True when the current sample is strictly less than every prior sample in the trailing window.",
            Self::BarsSince => "Bars since the last true condition on the condition's update clock.",
            Self::ValueWhen => "Captured source value from the Nth most recent true condition.",
            Self::Ma => "TA-Lib moving average with typed ma_type selection.",
            Self::Apo => "Absolute price oscillator using a typed moving-average family.",
            Self::Ppo => "Percentage price oscillator using a typed moving-average family.",
            Self::Macd => "Moving average convergence/divergence tuple (macd, signal, histogram).",
            Self::Cmo => "Chande momentum oscillator.",
            Self::Acos => "Vector trigonometric acos.",
            Self::Asin => "Vector trigonometric asin.",
            Self::Atan => "Vector trigonometric atan.",
            Self::Avgprice => "Average Price.",
            Self::Ceil => "Vector ceil transform.",
            Self::Cos => "Vector trigonometric cos.",
            Self::Cosh => "Vector hyperbolic cos.",
            Self::Exp => "Vector exponential.",
            Self::Floor => "Vector floor transform.",
            Self::Ln => "Vector natural logarithm.",
            Self::Log10 => "Vector base-10 logarithm.",
            Self::Sin => "Vector trigonometric sin.",
            Self::Sinh => "Vector hyperbolic sin.",
            Self::Sqrt => "Vector square root.",
            Self::Tan => "Vector trigonometric tan.",
            Self::Tanh => "Vector hyperbolic tan.",
            Self::Add => "Vector arithmetic addition.",
            Self::Div => "Vector arithmetic division.",
            Self::Mult => "Vector arithmetic multiplication.",
            Self::Sub => "Vector arithmetic subtraction.",
            Self::Max => "Highest value over a specified period.",
            Self::Min => "Lowest value over a specified period.",
            Self::Sum => "Summation over a trailing period.",
            Self::Midpoint => "Midpoint over a trailing period.",
            Self::Midprice => "Midpoint price over a trailing period.",
            Self::Medprice => "Median price.",
            Self::Typprice => "Typical price.",
            Self::Wclprice => "Weighted close price.",
            Self::Obv => "On-balance volume.",
            Self::Trange => "True range.",
            Self::Wma => "Weighted moving average.",
            Self::Avgdev => "Average deviation over a trailing period.",
            Self::MaxIndex => "Absolute index of the highest value over a specified period.",
            Self::MinIndex => "Absolute index of the lowest value over a specified period.",
            Self::MinMax => "Lowest and highest values over a specified period.",
            Self::MinMaxIndex => "Absolute indexes of the lowest and highest values over a specified period.",
            Self::Stddev => "Standard deviation over a trailing period.",
            Self::Var => "Variance over a trailing period.",
            Self::LinearReg => "Linear regression value at the current bar.",
            Self::LinearRegAngle => "Linear regression angle in degrees.",
            Self::LinearRegIntercept => "Linear regression intercept.",
            Self::LinearRegSlope => "Linear regression slope.",
            Self::Tsf => "Time series forecast for the next bar.",
            Self::Beta => "Beta over paired trailing series returns.",
            Self::Correl => "Pearson correlation over paired trailing series values.",
            Self::Willr => "Williams' %R over a trailing high-low-close window.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BuiltinId;

    #[test]
    fn builtin_name_and_numeric_lookups_round_trip() {
        for builtin in BuiltinId::RESERVED {
            assert_eq!(BuiltinId::from_name(builtin.as_str()), Some(builtin));
            assert_eq!(BuiltinId::from_u16(builtin as u16), Some(builtin));
        }
        assert_eq!(BuiltinId::from_name("missing"), None);
        assert_eq!(BuiltinId::from_u16(99), None);
    }

    #[test]
    fn callable_builtins_have_arity_and_market_series_do_not() {
        for builtin in BuiltinId::CALLABLE {
            assert!(builtin.is_callable());
            assert_ne!(
                builtin.arity(),
                super::BuiltinArity::NonCallable,
                "{builtin:?}"
            );
        }
        for builtin in [
            BuiltinId::Open,
            BuiltinId::High,
            BuiltinId::Low,
            BuiltinId::Close,
            BuiltinId::Volume,
            BuiltinId::Time,
        ] {
            assert!(!builtin.is_callable());
            assert_eq!(builtin.arity(), super::BuiltinArity::NonCallable);
        }
    }
}
