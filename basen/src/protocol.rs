use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use uuid::Uuid;

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct KeyLease {
    pub id: Uuid, // replace w/ something readable, including cluster ID, epoch, etc.
    //pub expiration: DateTime<Utc>,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct BlockLease {
    pub lease: KeyLease,
    pub block: Block,
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct BlockState {
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Heartbeat {
    pub id: Uuid,
    pub addr: SocketAddr,
}

#[derive(Eq, Hash, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct Block {
    pub pool: String, // cluster ID
    pub id: u64,      // block ID
    //pub epoch: u64, // append count
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct LocatedBlock {
    pub block: Block,
    pub locations: Vec<SocketAddr>,
    // pub token: bytes, // authentication token
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct ClientID {
    pub id: Uuid,
    // add metadata for KN to determine location for replica ordering
}

#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct SegmentNodeID {
    //id: Uuid,
    pub id: u64,
    // add metadata for KN to determine location
}

// client -> key node
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum CKRequest {
    Create   { id: ClientID, key: String },
    AddBlock { id: ClientID, lease: KeyLease },
    Open     { id: ClientID, key: String },
    Info     { id: ClientID, key: String },
    Close    { id: ClientID, lease: KeyLease, blocks: Vec<Block> }, // flag to block?
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum CKResponse {
    Create   { klease: KeyLease },
    AddBlock { blease: BlockLease },
    Open     { blocks: Vec<Block> },
    Info     { blocks: Vec<Block> },
    Close    { },
}

// client -> segment node
#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum CSRequest {
    Read  { id: ClientID, block: Block }, // eventually include lease
    Write { id: ClientID, lease: BlockLease, data: String, offset: u64 }, // V0: single write per block
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum CSResponse {
    Read  { id: ClientID, block: Block, data: String, offset: u64 }, // string for now, change to bytes
    Write { lease: BlockLease, offset: u64 },
}

// segment node -> key node
#[derive(PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub enum SKRequest {
    Register  { id: SegmentNodeID },
    Heartbeat { id: SegmentNodeID, blocks: Vec<Block> },
}

#[derive(PartialEq, Clone, Serialize, Deserialize, Debug)]
pub enum SKResponse {
    Register  { pool: String, epoch: u64 },
    Heartbeat { },
}