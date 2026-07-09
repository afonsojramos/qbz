//! The composite [`log::Log`] that fans every record to stderr, the ring, and the file.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Mutex;

use log::{Log, Metadata, Record};

use crate::line::LogLine;
use crate::{redact, ring};

/// Wraps `env_logger`'s built `Logger` and tees every record to the in-memory ring and
/// (optionally) the on-disk log file, with secret redaction applied once at this single
/// write choke point. stderr output is preserved verbatim by delegating to the inner
/// logger after the tee.
pub struct TeeLogger {
    pub(crate) inner: env_logger::Logger,
    pub(crate) file: Option<Mutex<BufWriter<File>>>,
}

fn now_epoch_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

impl Log for TeeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        // Honor the inner logger's filter so the ring matches what stderr would show.
        if !self.inner.enabled(record.metadata()) {
            return;
        }

        // Redact ONCE; every downstream sink gets the cleaned text.
        let msg = redact::redact(&record.args().to_string());
        let line = LogLine {
            ts: now_epoch_ms(),
            level: record.level(),
            target: record.target().to_owned(),
            message: msg.clone(),
        };

        ring::push(line.clone());

        if let Some(file) = &self.file {
            if let Ok(mut writer) = file.lock() {
                let _ = writeln!(
                    writer,
                    "{} {:5} {} {}",
                    line.format_ts(),
                    line.level_str(),
                    line.target,
                    msg
                );
            }
        }

        // Preserve the original env_logger stderr output verbatim.
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
        if let Some(file) = &self.file {
            if let Ok(mut writer) = file.lock() {
                let _ = writer.flush();
            }
        }
    }
}
