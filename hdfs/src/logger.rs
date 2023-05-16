use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::StreamDeserializer;
use std::cmp::Ordering;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::ops::Range;
use std::path::Path;
use std::{fs::OpenOptions, path::PathBuf};

use crate::protocol::{Block, Lease};

enum State {
    RECOVERY { segments: Vec<LogSegment> },
    ACTIVE,
    CLOSED,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NSOperation {
    CREATE {
        key: String,
        replication: u8,
        lease: Lease,
    },
    ADDBLOCK {
        lease: Lease,
        blkid: Block,
        offset: u64,
    },
    SEAL_BLOCK {
        lease: Lease,
        blkid: Block,
        len: u64,
    },
}

pub struct Logger<'a> {
    current_segment: Option<JsonLogReader<'a, File>>, // TODO move this to RECOVERY state (lifetime?)
    state: State,
    lsn: u64,
}

pub struct LogSegment {
    loc: PathBuf, // TODO abstract resolution
    lsn_range: Range<u64>,
}

//impl <T: Serialize + Deserialize> Logger<T> {
impl Logger<'_> {
    pub fn new(id: &str) -> Self {
        // TODO get id, recovery info (e.g., dir) from config
        if let Ok(mut logs) = Self::find_logs(Path::new("/tmp/nn")) {
            logs.sort_by(|a, b| match a.lsn_range.start.cmp(&b.lsn_range.start) {
                Ordering::Equal => a.lsn_range.end.cmp(&b.lsn_range.end), // empty/corrupt log [x, x)
                other => other,
            });
            Logger {
                current_segment: Some(JsonLogReader::new(&logs[0].loc)),
                state: State::RECOVERY { segments: logs },
                lsn: 0,
            }
        } else {
            std::fs::create_dir_all(Path::new("/tmp/nn")).unwrap();
            Logger {
                current_segment: None,
                state: State::RECOVERY { segments: vec![] },
                lsn: 0,
            }
        }
    }

    /// Find all files in the given directory matching the given regex.
    fn find_matching(dir: &Path, re: &Regex) -> io::Result<Vec<String>> {
        Ok(std::fs::read_dir(dir)?
            .into_iter()
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
            .filter_map(|m| {
                re.captures(&m).and_then(|x| {
                    Some(LogSegment {
                        loc: Path::new(&dir).join(Path::new(&m)).to_path_buf(),
                        lsn_range: Range {
                            start: u64::from_str_radix(x.get(1)?.as_str(), 16).unwrap(),
                            end: u64::from_str_radix(
                                x.get(2).map_or(&u64::to_string(&u64::MAX), |y| y.as_str()),
                                16,
                            )
                            .unwrap(),
                        },
                    })
                })
            })
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
    pub fn next(&mut self) -> Option<NSOperation> {
        match &mut self.state {
            State::RECOVERY { segments } => {
                loop {
                    if self.current_segment.is_none() {
                        self.state = State::ACTIVE;
                        return None;
                    }
                    if let Some(Ok(ret)) = self.current_segment.as_mut().unwrap().read() {
                        self.lsn += 1;
                        return Some(ret);
                    }

                    // current segment empty
                    if self.lsn != segments[0].lsn_range.end + 1 {
                        // log not closed properly
                        println!(
                            "Fixing log segment {:?} ({:?}-{:?})",
                            segments[0].loc, segments[0].lsn_range.start, self.lsn
                        );
                        drop(self.current_segment.take());
                        let new_name = segments[0].loc.parent()?.join(format!(
                            "{:016x}_{:016x}.log",
                            segments[0].lsn_range.start, self.lsn
                        ));
                        println!("Rename {:?} -> {:?}", &segments[0].loc, &new_name);
                        std::fs::rename(&segments[0].loc, &new_name).expect(
                            format!("Failed to rename {:?} to {:?}", segments[0].loc, &new_name)
                                .as_str(),
                        );
                        segments.remove(0); // TODO replace w/ VecDeque
                    } else {
                        println!(
                            "Closing log segment {:?} ({:?})",
                            segments[0].loc, segments[0].lsn_range
                        );
                        drop(self.current_segment.take());
                        segments.remove(0); // TODO replace w/ VecDeque
                    }

                    // ensure contiguous log segments (TODO: handle overlapping segments)
                    if segments[0].lsn_range.start == self.lsn {
                        self.current_segment = Some(JsonLogReader::new(&segments[0].loc));
                    }

                    if segments.len() == 0 {
                        self.state = State::ACTIVE;
                        return None;
                    }
                }
            }
            _ => {
                return None;
            }
        }
        None
    }
}

struct LogWriter<W: Write> {
    inner: BufWriter<W>,
}

impl LogWriter<File> {
    fn new() -> Self {
        let inner = OpenOptions::new()
            .write(true)
            .create(true)
            .open("test.log")
            .unwrap();
        LogWriter {
            inner: BufWriter::new(inner),
        }
    }

    fn append(&mut self, op: NSOperation) -> std::io::Result<()> {
        self.inner
            .write_all(serde_json::to_string(&op)?.as_bytes())?;
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
    fn new(logfile: &Path) -> Self {
        let logfile = OpenOptions::new().read(true).open(&logfile).unwrap();
        let deser = serde_json::Deserializer::from_reader(logfile);
        let tmp = deser.into_iter::<NSOperation>();
        JsonLogReader { inner: tmp }
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
