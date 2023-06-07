use std::net::SocketAddr;
use uuid::Uuid;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};

// crude client protocol
// Create (key, replication) -> (lease): assign block, create lease; may be sparse?
// AddBlock (lease, offset) -> (lease, datanodes): return new lease, report bytes written
// Close (lease, blkid, len) -> (): close block, update fileinfo, return when all blocks are closed

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum NSRequest {
    Create { key: String, replication: u8 },
    AddBlock { lease: Lease, offset: u64 },
    Close { lease: Lease },
    Open { key: String },
    RenewLease { lease: Lease },
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum NSResponse {
    CreateResponse {
        lease: Lease,
        block: Block,
    },
    AddBlockResponse {
        lease: Lease,
        datanodes: Vec<SocketAddr>,
    },
    OpenResponse {},
    Error {
        // must be a better way to do this...
        message: String,
    },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum DNRequest {
    Heartbeat { id: Uuid, clientaddr: SocketAddr },
    BlockReport { blocks: Vec<Block> },
}

pub enum DNResponse {
    HeartbeatAck {},
    HelloAck {},
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Lease {
    pub id: Uuid, // TODO add more metadata, timeouts, etc.
    //pub block: Block,
    //len: u64, // TODO restrict length of block?
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum Message {
    Echo { payload: String, ts: DateTime<Utc> },
    Heartbeat,
    HeartbeatAck,
}

pub enum Checksum {
    crc32c,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Block {
    //pool: String,
    pub id: u64,
    pub stamp: u64,
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
