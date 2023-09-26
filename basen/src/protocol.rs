use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct KeyLease {
    pub id: Uuid, // replace w/ something readable, including cluster ID, epoch, etc.
    //pub expiration: DateTime<Utc>,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct BlockLease {
    pub lease: KeyLease,
    pub block: Block,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct BlockState {
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct Heartbeat {
    pub id: SegmentNodeID,
    pub addr: SocketAddr,
}

#[derive(Eq, Hash, PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct Block {
    pub pool: String, // cluster ID
    pub id: u64,      // block ID
    //pub epoch: u64, // append count
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct LocatedBlock {
    pub block: Block,
    pub locations: Vec<SocketAddr>,
    // pub token: bytes, // authentication token
}

#[derive(Eq, Hash, PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct ClientID {
    pub id: Uuid,
    // add metadata for KN to determine location for replica ordering
}

// internal, clean up some nested tuples by making this a join later
#[derive(Eq, Hash, PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub struct ClientInfo {
    pub id: ClientID,
    pub addr: SocketAddr,
}


#[derive(Eq, Hash, PartialEq, Clone, Serialize, Deserialize, Debug, Default)]
#[rustfmt::skip]
pub struct SegmentNodeID {
    pub id: Uuid,
    //pub id: u64,
    // add metadata for KN to determine location
}

// client -> key node
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum CKRequest {
    Create   { id: ClientID, key: String },
    AddBlock { id: ClientID, lease: KeyLease },
    Open     { id: ClientID, key: String },
    Info     { id: ClientID, key: String },
    Close    { id: ClientID, lease: KeyLease, blocks: Vec<Block> }, // flag to block?
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum CKResponse {
    Create   { klease: KeyLease },
    AddBlock { blease: BlockLease },
    Open     { blocks: Vec<Block> },
    Info     { key: String, blocks: Vec<LocatedBlock> }, // break into multiple responses, to avoid blocking merge?
    Close    { },
}

// client -> segment node
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum CSRequest {
    Read  { id: ClientID, block: Block }, // eventually include lease
    Write { id: ClientID, lease: BlockLease, data: String, offset: u64 }, // V0: single write per block
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum CSResponse {
    Read  { id: ClientID, block: Block, data: String, offset: u64 }, // string for now, change to bytes
    Write { lease: BlockLease, offset: u64 },
}

// segment node -> key node
#[derive(PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum SKRequest {
    Register  { id: SegmentNodeID, svc_addr: SocketAddr },
    Heartbeat { id: SegmentNodeID, svc_addr: SocketAddr, blocks: Vec<Block> },
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
#[rustfmt::skip]
pub enum SKResponse {
    Register  { pool: String, epoch: u64 },
    Heartbeat { },
}
