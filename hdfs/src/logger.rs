use std::io;
use std::{fs::OpenOptions, path::PathBuf};
use std::path::Path;
use std::ops::Range;
use regex::Regex;
use serde::{Deserialize, Serialize};

enum State {
    RECOVERY { segments: Vec<LogSegment> },
    ACTIVE,
    CLOSED,
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
        if let Ok(logs) = Self::find_logs(Path::new("/tmp/nn")) {
            Logger {
                state: State::RECOVERY {
                    segments: logs,
                },
                lsn: 0,
            }
        } else {
            // TODO create directory
            Logger {
                state: State::ACTIVE,
                lsn: 0,
            }
        // read until EOF/failure
        // create new file at LSN..
        // rename old file to LSN-1?
        }
    }

    fn find_matching(dir: &Path, re: &Regex) -> io::Result<Vec<String>> { //io::Result<Vec<PathBuf>> {
        // FFS why is this so hard
        Ok(std::fs::read_dir(dir)?.into_iter()
            .filter_map(|x| Some(x.ok()?.file_name().to_str()?.to_string())) // extract filename, where defined
            .filter(|f| re.is_match(f))
            //.map(|f| Path::new(&dir).join(Path::new(f.as_str()).to_path_buf()))
            .collect())
    }

    fn find_logs(dir: &Path) -> io::Result<Vec<LogSegment>> {
        let re = Regex::new(r"(\d{8})_(\d{8})?.log").unwrap();
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

    //fn next(&self) -> T {
    //    //assert!(self.state == State::RECOVERY);
    //}
}

pub mod tests {
    use super::*;

}