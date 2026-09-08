//! The composite [`log::Log`] that fans every record to stderr, the ring, and the file.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{Log, Metadata, Record};

use crate::line::LogLine;
use crate::repeat::ConsecutiveRecords;
use crate::{redact, ring};

/// Wraps `env_logger`'s built `Logger` and tees every record to the in-memory ring and
/// (optionally) the on-disk log file, with secret redaction applied once at this single
/// write choke point. **All** sinks (ring, file, and stderr) receive the redacted text.
/// Consecutive duplicate records are summarized with their original severity,
/// target, redacted message, repetition count and monotonic elapsed time.
pub struct TeeLogger {
    inner: env_logger::Logger,
    // One lock covers grouping AND all sink writes: concurrent threads cannot
    // interleave a summary and its next record or reorder the file vs. the ring.
    output: Mutex<Output>,
}

struct Output {
    file: Option<BufWriter<File>>,
    consecutive: ConsecutiveRecords,
    file_flush: FileFlush,
}

/// How stale the on-disk log may be. The file sink is a `BufWriter`, so
/// without a bound a process KILLED while wedged loses up to 8 KiB of the
/// most interesting lines — and "I had to force-kill it" is the state every
/// hang report is filed from (#749). Flushing per line instead would turn a
/// burst (CMAF segment progress, discovery chatter) into one write syscall
/// per record, so the file is allowed to lag by at most this much.
const FILE_FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// When the file sink must reach the disk. Split out so the policy is
/// testable without touching a file.
struct FileFlush {
    last: Instant,
    interval: Duration,
}

impl FileFlush {
    fn new(now: Instant, interval: Duration) -> Self {
        Self {
            last: now,
            interval,
        }
    }

    /// `urgent` — a warning/error or a repeat summary — never waits.
    fn due(&mut self, now: Instant, urgent: bool) -> bool {
        if urgent || now.duration_since(self.last) >= self.interval {
            self.last = now;
            return true;
        }
        false
    }
}

impl TeeLogger {
    pub(crate) fn new(inner: env_logger::Logger, file: Option<BufWriter<File>>) -> Self {
        Self {
            inner,
            output: Mutex::new(Output {
                file,
                consecutive: ConsecutiveRecords::default(),
                file_flush: FileFlush::new(Instant::now(), FILE_FLUSH_INTERVAL),
            }),
        }
    }
}

impl Output {
    fn write(&mut self, line: LogLine) {
        let formatted = format_line(&line);
        ring::push(line);
        if let Some(writer) = &mut self.file {
            let _ = writeln!(writer, "{formatted}");
        }
        // Never delegate to inner.log(record): it would bypass redaction.
        let _ = writeln!(std::io::stderr(), "{formatted}");
    }
}

fn now_epoch_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

/// Format a redacted log line the same way the file sink does (stable, greppable).
fn format_line(line: &LogLine) -> String {
    format!(
        "{} {:5} {} {}",
        line.format_ts(),
        line.level_str(),
        line.target,
        line.message
    )
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
        let mut output = self.output.lock().unwrap_or_else(|p| p.into_inner());
        let line = LogLine {
            ts: now_epoch_ms(),
            level: record.level(),
            target: record.target().to_owned(),
            message: msg,
        };
        let now = Instant::now();
        let [summary, first] = output.consecutive.push(line, now);
        let summarized = summary.is_some();
        for line in [summary, first].into_iter().flatten() {
            output.write(line);
        }
        // Compaction may no longer fill BufWriter for minutes, and a wedged
        // process is killed rather than exiting: make periodic summaries,
        // anything at WARN or worse, and otherwise the last
        // FILE_FLUSH_INTERVAL of ordinary records visible to a live file tail
        // and survivable across a SIGKILL.
        let urgent = summarized || record.level() <= log::Level::Warn;
        if output.file_flush.due(now, urgent) {
            if let Some(writer) = &mut output.file {
                let _ = writer.flush();
            }
        }
    }

    fn flush(&self) {
        let mut output = self.output.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(summary) = output.consecutive.flush() {
            output.write(summary);
        }
        self.inner.flush();
        if let Some(writer) = &mut output.file {
            let _ = writer.flush();
        }
        let _ = std::io::stderr().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Level;

    /// A wedged process is force-killed, so the file sink may never lag by
    /// more than the interval, and a warning must never sit in the buffer.
    #[test]
    fn file_flush_is_bounded_and_urgent_records_never_wait() {
        let start = Instant::now();
        let mut policy = FileFlush::new(start, Duration::from_millis(200));

        assert!(
            !policy.due(start + Duration::from_millis(10), false),
            "an ordinary record inside the window stays buffered"
        );
        assert!(
            policy.due(start + Duration::from_millis(20), true),
            "a warning or a summary flushes immediately"
        );
        assert!(
            !policy.due(start + Duration::from_millis(100), false),
            "the urgent flush restarts the window"
        );
        assert!(
            policy.due(start + Duration::from_millis(220), false),
            "an ordinary record past the window flushes"
        );
    }

    #[test]
    fn format_line_includes_redacted_message_not_raw() {
        let line = LogLine {
            ts: 0,
            level: Level::Info,
            target: "qbz".into(),
            message: "token=***REDACTED***".into(),
        };
        let s = format_line(&line);
        assert!(s.contains("***REDACTED***"));
        assert!(!s.contains("SEKRET"));
    }
}
