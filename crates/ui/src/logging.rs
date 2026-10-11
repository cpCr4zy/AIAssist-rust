//! The Log tab's data: a [`log::Log`] that keeps recent lines for the UI and passes every record
//! on to the platform logger (stderr on the desktop, the console in the browser).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Older lines are dropped beyond this.
pub const MAX_LOG_LINES: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// Local time, `HH:MM:SS`.
    pub time: String,
    pub level: log::Level,
    pub text: String,
}

/// Recent log lines, shared between the logger and the UI.
#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<VecDeque<LogLine>>>);

impl LogBuffer {
    fn lines(&self) -> MutexGuard<'_, VecDeque<LogLine>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, level: log::Level, text: String) {
        let time = chrono::Local::now().format("%H:%M:%S").to_string();
        let mut lines = self.lines();
        while lines.len() >= MAX_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(LogLine { time, level, text });
    }

    pub fn snapshot(&self) -> Vec<LogLine> {
        self.lines().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.lines().clear();
    }
}

/// Captures this app's records at debug and above, and everyone's warnings and errors.
pub struct CaptureLogger {
    buffer: LogBuffer,
    inner: Option<Box<dyn log::Log>>,
}

impl CaptureLogger {
    fn captures(metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn || (metadata.level() <= log::Level::Debug && metadata.target().starts_with("aiassist"))
    }
}

impl log::Log for CaptureLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        Self::captures(metadata) || self.inner.as_ref().is_some_and(|l| l.enabled(metadata))
    }

    fn log(&self, record: &log::Record<'_>) {
        if Self::captures(record.metadata()) {
            self.buffer.push(record.level(), record.args().to_string());
        }
        if let Some(inner) = &self.inner {
            inner.log(record);
        }
    }

    fn flush(&self) {
        if let Some(inner) = &self.inner {
            inner.flush();
        }
    }
}

/// Installs the capturing logger in front of `inner` (whose own filter allows up to `inner_max`)
/// and returns the buffer the Log tab reads. If a logger is already installed, the buffer stays
/// empty and `inner` is dropped.
pub fn install(inner: Option<Box<dyn log::Log>>, inner_max: log::LevelFilter) -> LogBuffer {
    let buffer = LogBuffer::default();
    let logger = CaptureLogger { buffer: buffer.clone(), inner };
    match log::set_boxed_logger(Box::new(logger)) {
        Ok(()) => log::set_max_level(inner_max.max(log::LevelFilter::Debug)),
        Err(e) => eprintln!("aiassist: logging not captured: {e}"),
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log as _;

    #[test]
    fn keeps_only_the_newest_lines() {
        let buffer = LogBuffer::default();
        for i in 0..MAX_LOG_LINES + 5 {
            buffer.push(log::Level::Info, i.to_string());
        }
        let lines = buffer.snapshot();
        assert_eq!(lines.len(), MAX_LOG_LINES);
        assert_eq!(lines.first().map(|l| l.text.as_str()), Some("5"));
        buffer.clear();
        assert!(buffer.snapshot().is_empty());
    }

    #[test]
    fn captures_own_debug_and_foreign_warnings_only() {
        let buffer = LogBuffer::default();
        let logger = CaptureLogger { buffer: buffer.clone(), inner: None };
        let record = |level, target| {
            logger.log(&log::Record::builder().level(level).target(target).args(format_args!("{target} {level}")).build());
        };
        record(log::Level::Debug, "aiassist_ui::app");
        record(log::Level::Trace, "aiassist_ui::app");
        record(log::Level::Info, "ureq");
        record(log::Level::Warn, "ureq");
        let texts: Vec<String> = buffer.snapshot().into_iter().map(|l| l.text).collect();
        assert_eq!(texts, ["aiassist_ui::app DEBUG", "ureq WARN"]);
    }
}
