use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write, BufWriter, Read};
use std::{fs::OpenOptions, path::PathBuf};
use std::path::Path;
use std::ops::Range;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::StreamDeserializer;

use crate::protocol::Block;

enum State {
    RECOVERY { segments: Vec<LogSegment> },
    ACTIVE,
    CLOSED,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NSOperation {
    CREATE { key: String, replication: u8 },
    ADDBLOCK { key: String, blkid: Block, offset: u64 },
}

pub struct Logger {
    state: State,
    lsn: u64,
}

pub struct LogSegment {
    loc: PathBuf, // TODO abstract resolution
    lsn_range: Range<u64>,
}

//impl <T: Serialize + Deserialize> Logger<T> {
impl Logger {
    pub fn new(id: &str) -> Self {
        // TODO get id, recovery info (e.g., dir) from config
        if let Ok(mut logs) = Self::find_logs(Path::new("/tmp/nn")) {
            logs.sort_by(|a, b| match a.lsn_range.start.cmp(&b.lsn_range.start) {
                    Ordering::Equal => a.lsn_range.end.cmp(&b.lsn_range.end), // empty/corrupt log [x, x)
                    other => other,
            });
            Logger {
                state: State::RECOVERY {
                    segments: logs,
                },
                lsn: 0,
            }
        } else {
            std::fs::create_dir_all(Path::new("/tmp/nn")).unwrap();
            Logger {
                state: State::RECOVERY { segments: vec![] },
                lsn: 0,
            }
        }
    }

    /// Find all files in the given directory matching the given regex.
    fn find_matching(dir: &Path, re: &Regex) -> io::Result<Vec<String>> {
        Ok(std::fs::read_dir(dir)?.into_iter()
            .filter_map(|x| Some(x.ok()?.file_name().to_str()?.to_string())) // extract filename, where defined
            .filter(|f| re.is_match(f))
            .collect())
    }

    /// Find all log segments in the given directory.
    /// start_[end].log (inclusive, exclusive)
    fn find_logs(dir: &Path) -> io::Result<Vec<LogSegment>> {
        let re = Regex::new(r"([[:xdigit:]]{16})_([[:xdigit:]]{16})?.log").unwrap();
        Ok(Self::find_matching(dir, &re)?
            .into_iter()
            .filter_map(|m| re.captures(&m)
                .and_then(|x| Some(LogSegment {
                    loc: Path::new(&dir).join(Path::new(&m)).to_path_buf(),
                    lsn_range: Range {
                        start: u64::from_str_radix(x.get(1)?.as_str(), 16).unwrap(),
                        end: u64::from_str_radix(x.get(2).map_or(&u64::to_string(&u64::MAX), |y| y.as_str()), 16).unwrap(),
                    }
                })))
            .collect())
    }

    //fn append(&self, msg: T) {
    //    // no CLR entries; just fail if in RECOVERY
    //    //assert!(self.state == State::ACTIVE);
    //}

    //fn close(&self) {
    //    //assert!(self.state == State::ACTIVE);
    //}

    // TODO: define for any T implementing Serialize, Deserialize
    fn next(&mut self) -> Option<NSOperation> {
        //assert!(self.state == State::RECOVERY);
        match &self.state {
            State::RECOVERY { segments } => {
                let mut ret = None;
                loop {
                    if segments.len() == 0 {
                        // TODO rename last segment to recovery LSN
                        self.state = State::ACTIVE;
                        return ret;
                    }
                    // pull next (batch of) NSOperation
                    self.lsn += 1;
                    break;
                }
            }
            _ => panic!("Invalid state"),
        }
        None
    }
}

struct LogWriter<W: Write> {
    inner: BufWriter<W>,
}

impl LogWriter<File> {
    fn new() -> Self {
        let inner = OpenOptions::new().write(true).create(true).open("test.log").unwrap();
        LogWriter {
            inner: BufWriter::new(inner),
        }
    }

    fn append(&mut self, op: NSOperation) -> std::io::Result<()> {
        self.inner.write_all(serde_json::to_string(&op)?.as_bytes())?;
        Ok(())
    }

    fn close(&mut self) -> std::io::Result<()> {
        self.inner.flush()?;
        // drop?
        Ok(())
    }
}

struct JsonLogReader<'a, R: Read> {
    inner: StreamDeserializer<'a, serde_json::de::IoRead<R>, NSOperation>,
}

impl JsonLogReader<'_, File> {
    fn new() -> Self {
        let logfile = OpenOptions::new().read(true).open("test.log").unwrap();
        let deser = serde_json::Deserializer::from_reader(logfile);
        let tmp = deser.into_iter::<NSOperation>();
        JsonLogReader {
            inner: tmp,
        }
    }

    fn read(&mut self) -> Option<Result<NSOperation, serde_json::Error>> {
        let tmp = self.inner.next();
        if let Some(Err(x)) = tmp {
            None
        } else {
            tmp
        }
    }
}

pub mod tests {
    use super::*;

}