use std::fs::OpenOptions;
use std::path::Path;
use std::ops::Range;
use regex::Regex;
use serde::{Deserialize, Serialize};

enum State {
    RECOVERY,
    ACTIVE,
    CLOSED,
}

pub struct Logger<T> {
    state: State,
    lsn: u64,
}

pub struct LogSegment {
    loc: Box<Path>, // TODO abstract resolution
    lsn_range: Range<u64>,
}

impl <T: Serialize + Deserialize> Logger<T> {
    fn new(id: String) -> Self {
        let state = match OpenOptions.read(true).open("/tmp/nn/recovery.log") {
            Ok(f) => RECOVERY(f),
            Err(e) => ACTIVE(),
        };
        // read until EOF/failure
        // create new file at LSN..
        // rename old file to LSN-1?
    }

    fn find_matching(dir: Path, re: &Regex) -> Vec<Box<Path>> {
        std::fs::read_dir(dir)?.into_iter()
            .filter_map(|x| Some((x, x.ok()?.file_name().to_str()?.to_string()))) // extract filename, where defined
            .filter(|(p, f)| re.is_match(f))
            .map(|(p, _)| p)
            .collect()
    }

    fn find_logs(dir: Path) -> Vec<LogSegment> {
        let re = Regex::new(r"(\d{8})_(\d{8})?.log").unwrap();
        Self::find_matching(dir, &re)
            .into_iter()
            .filter_map(|m| re.captures(&m)
                .and_then(|x| Some(LogSegment {
                    loc: x.to_owned(),
                    lsn_range: Range {
                        start: u64::from_str(x.get(1)?.as_str()).unwrap(),
                        end: u64::from_str(x.get(2).map_or(&u64::to_string(&u64::MAX), |y| y.as_str())).unwrap(),
                    }
                })))
            .collect()
    }

    fn append(&self, msg: T) {
        // no CLR entries; just fail if in RECOVERY
        assert!(self.state == State::ACTIVE);
    }

    fn close(&self) {
        assert!(self.state == State::ACTIVE);
    }

    fn next(&self) -> T {
        assert!(self.state == State::RECOVERY);
    }
}

pub mod tests {
    use super::*;

}