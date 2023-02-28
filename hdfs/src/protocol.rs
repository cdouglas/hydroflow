use std::net::SocketAddr;
use uuid::Uuid;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum NSRequest {
    Create {
        key: String,
        replication: u8,
    },
    NextBlock {
        lease: Lease,
    },
    Open {
        key: String,
    },
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum NSResponse {
    CreateResponse {
        lease: Lease,
    },
    NextBlockResponse {
        block: Block,
    },
    OpenResponse {
    },
    Error { // must be a better way to do this...
        message: String,
    },
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Lease {
    id: Uuid, // TODO add more metadata, timeouts, etc.
    block: Block,
    len: u64,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum Message {
    Echo {
        payload: String,
        ts: DateTime<Utc>,
    },
    ReplEcho {
        payload: String,
        stream_id: Block,
        fwd: Vec<SocketAddr>,
        offset: u64,
        gen_stamp: u64,
        ts: DateTime<Utc>,
    },
    AckEcho {
        stream_id: String,
    },
    Heartbeat,
    HeartbeatAck,
}

pub enum Checksum {
    crc32c,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Block {
    pool: String,
    id: String,
    stamp: u64,
}

//#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
//pub enum Block {
//    pool_id: String,
//    block_id: u64,
//    generation_stamp: u64,
//    num_bytes: u64,
//    checksum: Checksum,
//    // storage_ids: Vec<String>,
//    // storage_types: Vec<StorageType>,
//    // min_bytes_rsvd: u64,
//    // max_bytes_rsvd: u64,
//    // is_corrupt: bool,
//    // primary_node_index: u32,
//    // locations: Vec<DatanodeInfo>,
//
//}

pub enum ClientOperation {
    OP_WRITE_BLOCK,
    //OP_APPEND_BLOCK,
    OP_READ_BLOCK,
    //OP_REPLACE_BLOCK,
    //OP_COPY_BLOCK,
}