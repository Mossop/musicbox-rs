use std::io::{stdout, Stdout, Write};
use std::sync::Mutex;

use crossterm::cursor::MoveToColumn;
use crossterm::style::{style, Color, Print, PrintStyledContent};
use crossterm::QueueableCommand;
use log::{Level, LevelFilter, Log, Metadata, Record};
use time::Time;

use crate::error::{ErrorExt, MusicResult, VoidResult};

struct Logger {
    output: Stdout,
}

impl Logger {
    fn enabled(&self, metadata: &Metadata) -> MusicResult<bool> {
        let target = metadata.target();
        if target.starts_with("musicbox::") || target.starts_with("rpi_futures::") {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn log(&mut self, record: &Record) -> VoidResult {
        if !self.enabled(record.metadata())? {
            return Ok(());
        }

        let time = Time::now();
        self.output
            .queue(Print(format!("[{} ", time.format("%H:%M:%S"))))
            .as_err()?;

        self.output
            .queue(PrintStyledContent(match record.level() {
                Level::Error => style("ERROR").with(Color::Red),
                Level::Warn => style(" WARN").with(Color::Yellow),
                Level::Info => style(" INFO").with(Color::White),
                Level::Debug => style("DEBUG").with(Color::Grey),
                Level::Trace => style("TRACE").with(Color::DarkGrey),
            }))
            .as_err()?;

        self.output
            .queue(Print(format!(" {}] {}\n", record.target(), record.args())))
            .as_err()?;

        self.output.queue(MoveToColumn(0)).as_err()?;
        Ok(())
    }

    fn flush(&mut self) -> VoidResult {
        self.output
            .flush()
            .map_err(|_| String::from("Failed to flush output."))?;
        Ok(())
    }
}

pub struct TermLogger {
    inner: Mutex<Logger>,
}

impl TermLogger {
    pub fn init() -> VoidResult {
        log::set_boxed_logger(Box::new(TermLogger {
            inner: Mutex::new(Logger { output: stdout() }),
        }))
        .map_err(|_| String::from("Logging already initialized."))?;
        log::set_max_level(LevelFilter::Trace);
        Ok(())
    }
}

impl Log for TermLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner
            .lock()
            .map_err(|_| String::from("Failed to lock logger."))
            .and_then(|inner| inner.enabled(metadata))
            .unwrap()
    }

    fn log(&self, record: &Record) {
        self.inner
            .lock()
            .map_err(|_| String::from("Failed to lock logger."))
            .and_then(|mut inner| inner.log(record))
            .unwrap();
    }

    fn flush(&self) {
        self.inner
            .lock()
            .map_err(|_| String::from("Failed to lock logger."))
            .and_then(|mut inner| inner.flush())
            .unwrap();
    }
}
