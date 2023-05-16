use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq, Hash, Clone, Serialize, Deserialize, Debug)]
pub struct EchoMsg {
    pub payload: String,
    pub ts: DateTime<Utc>,
}
