// Logger configuration and setup
use tracing_subscriber::{
    fmt::format::FmtSpan,
    EnvFilter,
};

/// Initialize logger with different formats
pub fn init_logger(format: LogFormat, level: LogLevel) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.as_str()));

    match format {
        LogFormat::Compact => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .with_span_events(FmtSpan::CLOSE)
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .pretty()
                .with_span_events(FmtSpan::CLOSE)
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(false)
                .json()
                .flatten_event(true)  // fields를 최상위 레벨로 flatten
                .with_current_span(false)  // current span 정보 제거 (spans 배열만 유지)
                .with_span_events(FmtSpan::CLOSE)
                .init();
        }
    }
}

/// Log format options
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum LogFormat {
    Compact,
    Pretty,
    Json,
}

impl LogFormat {
    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pretty" => LogFormat::Pretty,
            "json" => LogFormat::Json,
            _ => LogFormat::Compact,
        }
    }
}

/// Log level options
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

