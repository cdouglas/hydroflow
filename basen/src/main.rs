use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use hydroflow::lattices::map_union::MapUnionHashMap;
use hydroflow::lattices::set_union::SetUnionHashSet;
use hydroflow::lattices::{DomPair, Max, Pair, Point};
use hydroflow::util::{bind_tcp_bytes, connect_tcp_bytes, ipv4_resolve};
use hydroflow::{hydroflow_syntax, tokio};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

use crate::protocol::*;
use crate::client::run_client;
use crate::helpers::print_graph;

mod client;
mod helpers;
mod protocol;

#[derive(Clone, ValueEnum, Debug)]
enum Role {
    Servers, // run all servers in one process, for now
    Client,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum GraphType {
    Mermaid,
    Dot,
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
struct PartialResult {
    reqid: ClientID,
    //seq: u64,
    action: Action,
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
enum Action {
    NSLookup    { key: String, inode: Option<INode>, },
    // Avoiding nesting...
    //INodeLookup { inode: INode,        block: Option<Vec<Block>>,        lease: Option<KeyLease> },
    //BlockLookup { block: Block,        node: Option<Vec<SegmentNodeID>>, lease: Option<BlockLease> },
    //HostLookup  { node: SegmentNodeID, addr: Option<Vec<SocketAddr>> },
    INodeLookup { inode: INode, block: Option<(usize, Block)>, }, // , lease: Option<KeyLease> },
    BlockLookup { block: Block, node: Option<SegmentNodeID>, }, //lease: Option<BlockLease> },
    HostLookup  { node: SegmentNodeID, addr: Option<SocketAddr> },
}

#[derive(Parser, Debug)]
pub struct Opts {
    #[clap(value_enum, long)]
    role: Role,
    // #[clap(long)]
    #[clap(long, value_parser = ipv4_resolve)]
    addr: Option<SocketAddr>,
    // #[clap(long)]
    #[clap(long, value_parser = ipv4_resolve)]
    server_addr: Option<SocketAddr>,
    #[clap(value_enum, long)]
    graph: Option<GraphType>,
}

#[hydroflow::main]
async fn main() {
    let opts = Opts::parse();
    // if no addr was provided, we ask the OS to assign a local port by passing in "localhost:0"
    let addr = opts
        .addr
        .unwrap_or_else(|| ipv4_resolve("localhost:0").unwrap());

    match opts.role {
        Role::Servers => {
            run_servers(addr, opts).await;
        }
        Role::Client => {
            run_client(addr, opts).await;
        }
    }
}

async fn run_servers(keynode_client_addr: SocketAddr, opts: Opts) {
    const KEYNODE_SN_ADDR: &str = "127.0.0.55:4345";

    // name, inode, blocks
    let canned_keys = vec![
        ("dingo".to_owned(), INode { id: 5678u64, }, vec![
            Block { pool: "x".to_owned(), id: 200u64 },
            Block { pool: "x".to_owned(), id: 300u64 },
        ]),
        ("yak".to_owned(), INode { id: 1234u64, }, vec![
            Block { pool: "x".to_owned(), id: 400u64 }
        ]),
        ("zebra".to_owned(), INode { id: 2345u64, }, vec![
        ]),
    ];

    let canned_blocks = vec![
        Block { pool: "x".to_owned(), id: 200u64 },
        Block { pool: "x".to_owned(), id: 300u64 },
        Block { pool: "x".to_owned(), id: 400u64 },
    ];
    futures::join!(
        // super-lazy switch, here...
        segment_node(&opts, KEYNODE_SN_ADDR, Uuid::new_v4(), &canned_blocks[0..1]),
        segment_node(&opts, KEYNODE_SN_ADDR, Uuid::new_v4(), &canned_blocks[..]),
        key_node(&opts, KEYNODE_SN_ADDR, keynode_client_addr, &canned_keys[..]),
    );
}

#[allow(dead_code)]
async fn key_node(opts: &Opts, keynode_sn_addr: &'static str, keynode_client_addr: SocketAddr, init_keys: &[(String, INode, Vec<Block>)]) {
    let (cl_outbound, cl_inbound, cl_addr) = bind_tcp_bytes(keynode_client_addr).await;
    let (sn_outbound, sn_inbound, sn_addr) = bind_tcp_bytes(ipv4_resolve(keynode_sn_addr).unwrap()).await;
    println!("{}: KN<->SN: Listening on {}", Utc::now(), sn_addr);
    println!("{}: KN<->CL: Listening on {}", Utc::now(), cl_addr);

    let init_keys = init_keys.to_vec();

    //let mut seq = 1u64 + init_keys.iter()
    //    .fold(0u64, |acc, (_, inode, _)| acc.max(*inode));

    let mut df = hydroflow_syntax! {
        // load initial keyset
        log = source_iter(init_keys)
            -> tee();
        disk_key_inode = log[0]   // (key, inode)
            -> map(|(key, inode, _): (String, INode, Vec<Block>)| (key, inode))
            -> unique() // necessary?
            -> [1]key_inode;

        disk_inode_block = log[1] // (inode, seq, block)
            // store as (inode, seq, block) to preserve order
            -> flat_map(|(_, inode, blocks)| blocks.into_iter().enumerate().map(move |(i, block)| (inode.clone(), (i, block))))
            -> [1]inode_block;

        new_reqs = source_stream_serde(cl_inbound)
            -> map(Result::unwrap)
            -> tee();
        // merge all incoming requests from this tick (TODO: should be able to defer reqs to next tick)
        tick_reqs = new_reqs[0] // TODO idx still necessary?
            -> map(|(cl_req, addr): (CKRequest, SocketAddr)| (cl_req.id.clone(), (cl_req, addr)))
            -> tee();

        // client
        client_demux = new_reqs[1]
            -> inspect(|(m, a): &(CKRequest, SocketAddr)| { println!("{}: KN: CL {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(cl_req, addr): (CKRequest, SocketAddr), var_args!(create, info, errs)|
                    match &cl_req.payload {
                        CKRequestType::Info { key, .. } => info.give((key.clone(), cl_req.id.clone())),
                        CKRequestType::Create { key, .. } => create.give((key.clone(), cl_req.id.clone())),
                        _ => errs.give((cl_req, addr)),
                    }
                );
        client_demux[errs] // TODO: error response
            -> for_each(|(req, addr)| println!("KN: Unexpected CL message type: {:?} from {:?}", req, addr));

        // CREATE: client_demux
        create_key_inode = client_demux[create]
            -> inspect(|(key, id): &(String, ClientID)| println!("{}: KN: {:?} CREATE {:?}", Utc::now(), id, key))
            -> req_key_inode;

        // INFO: client_demux
        info_key_inode = client_demux[info]
            -> inspect(|(key, id): &(String, ClientID)| println!("{}: KN: {:?} INFO {:?}", Utc::now(), id, key))
            -> req_key_inode;

        // LHS of join with key->inode map
        // (key, reqid): String, ClientID
        req_key_inode = union() -> tee();
        req_key_inode[0] // record EMPTY result
            -> map(|(key, id): (String, ClientID)| PartialResult { reqid: id, action: Action::NSLookup { key: key.clone(), inode: None } })
            -> req_merge;
        req_key_inode[1] // attempt join
            -> [0]key_inode;

        // join requests LHS:(key, req) with existing key map RHS:(key, inode)
        key_inode = join::<'tick, 'static>()
            -> inspect(|(key, (id, inode)) : &(String, (ClientID, INode))| println!("{}: KN: KEY {:?} found existing inode {:?}", Utc::now(), key, inode))
            -> map(|(key, (id, inode))| (id, (key, inode)))
            -> [0]resp_key_inode;
        tick_reqs
            -> [1]resp_key_inode;
        resp_key_inode = join::<'tick, 'tick>()
            -> tee();
        resp_key_inode[0]
            -> map(|(id, ((key, inode), (req, _)))| PartialResult { reqid: id, action: Action::NSLookup { key: key.clone(), inode: Some(inode.clone()) } })
            -> req_merge;

        key_inode_result = resp_key_inode[1]
            -> demux(|(id, ((key, inode), (req, _))) : (ClientID, ((String, INode), (CKRequest, SocketAddr))), var_args!(create, info)|
                    match &req.payload {
                        CKRequestType::Create {   .. } => create.give((inode, id)),
                        CKRequestType::Info {   .. } => info.give((inode, id)),
                        _ => panic!(),
                    }
                );
        key_inode_result[create]
            -> inspect(|(inode, req)| println!("{}: KN: CREATE {:?} found existing inode {:?}", Utc::now(), req, inode))
            -> null(); // TODO: overwrite flag; for now, this is sufficient to construct an error in req_merge

        key_inode_result[info]
            -> req_inode_block;
        
        req_inode_block = union() -> tee();
        req_inode_block[0] // EMPTY result
            -> map(|(inode, id): (INode, ClientID)| PartialResult { reqid: id, action: Action::INodeLookup { inode: inode.clone(), block: None, } })
            -> req_merge;
        req_inode_block[1] // attempt join
            -> [0]inode_block;

        inode_block = join::<'tick, 'static>()
            -> inspect(|(inode, (id, (seq, block))): &(INode, (ClientID, (usize, Block)))| println!("{}: KN: INODE {:?} found existing {:?} block {:?}", Utc::now(), inode, seq, block))
            -> map(|(inode, (id, (seq, block)))| (id, (inode, seq, block)))
            -> [0]resp_inode_block;
        tick_reqs
            -> [1]resp_inode_block;
        resp_inode_block = join::<'tick, 'tick>() -> tee();
        resp_inode_block[0]
            -> map(|(id, ((inode, seq, block), (req, _)))| PartialResult { reqid: id, action: Action::INodeLookup { inode: inode.clone(), block: Some((seq, block.clone())), } })
            -> req_merge;
        inode_block_result = resp_inode_block[1]
            -> demux(|(id, ((inode, _seq, block), (req, _))): (ClientID, ((INode, usize, Block), (CKRequest, _))), var_args!(info, errs)|
                    match &req.payload {
                        //CKRequestType::Create {  key, .. } => create.give((block, (seq, inode, key.clone(), req, addr))), // Create doesn't reach here
                        CKRequestType::Info { key, .. } => info.give((block, id)),
                        _ => errs.give(id),
                    }
                );
        inode_block_result[info]
            -> req_block_host;
        inode_block_result[errs] // demux must have at least 2 outputs
            -> null();

        req_block_host = union() -> tee();
        req_block_host[0] // EMPTY result
            -> map(|(block, id): (Block, ClientID)| PartialResult { reqid: id.clone(), action: Action::BlockLookup { block: block.clone(), node: None, } })
            -> req_merge;
        req_block_host[1] // attempt join
            -> [0]block_host;

        block_host = join::<'tick, 'static>()
            -> inspect(|(block, (id, sn_id)): &(Block, (ClientID, SegmentNodeID))| println!("{}: KN: BLOCK {:?} found existing host {:?}", Utc::now(), block, sn_id))
            -> map(|(block, (id, sn_id))| (id, (block, sn_id)))
            -> [0]resp_block_host;
        tick_reqs
            -> [1]resp_block_host;
        resp_block_host = join::<'tick, 'tick>() -> tee();
        resp_block_host[0]
            -> map(|(id, ((block, sn_id), (req, _)))| PartialResult { reqid: id, action: Action::BlockLookup { block: block.clone(), node: Some(sn_id.clone()), } })
            -> req_merge;
        block_host_result = resp_block_host[1]
            -> demux(|(id, ((block, sn_id), (req, _))): (ClientID, ((Block, SegmentNodeID), (CKRequest, SocketAddr))), var_args!(info, errs)|
                    match &req.payload {
                        CKRequestType::Info { key, .. } => info.give((sn_id, id)),
                        _ => errs.give(id),
                    }
                );
        block_host_result[info]
            -> req_host_live;
        block_host_result[errs] // demux must have at least 2 outputs
            -> null();

        req_host_live = union() -> tee();
        req_host_live[0] // EMPTY
            -> map(|(sn_id, id): (SegmentNodeID, ClientID)| PartialResult { reqid: id.clone(), action: Action::HostLookup { node: sn_id.clone(), addr: None, } })
            -> req_merge;
        req_host_live[1]
            -> [0]host_live;
        host_live = join::<'tick, 'static>()
            -> inspect(|(sn_id, (id, (svc_addr, last_contact))): &(SegmentNodeID, (ClientID, (SocketAddr, DateTime<Utc>)))| println!("{}: KN: HOST {:?} found existing {:?}", Utc::now(), sn_id, svc_addr))
            -> filter(|(_, (_, (_, last_contact)))| Utc::now() - last_contact < chrono::Duration::seconds(10))
            -> map(|(sn_id, (id, (svc_addr, last_contact)))| (id, (sn_id, svc_addr, last_contact)))
            -> [0]resp_host_live;
        tick_reqs
            -> [1]resp_host_live;
        resp_host_live = join::<'tick, 'tick>() -> tee();
        resp_host_live[0]
            -> map(|(id, ((sn_id, svc_addr, last_contact), (req, _)))| PartialResult { reqid: id, action: Action::HostLookup { node: sn_id.clone(), addr: Some(svc_addr.clone()), } })
            -> req_merge;
        host_live_result = resp_host_live[1]
            -> demux(|(id, ((sn_id, svc_addr, last_contact), (req, _))): (ClientID, ((SegmentNodeID, SocketAddr, DateTime<Utc>), (CKRequest, SocketAddr))), var_args!(info, errs)|
                    match &req.payload {
                        CKRequestType::Info { key, .. } => info.give((sn_id, id)),
                        _ => errs.give(id),
                    }
                );
        host_live_result[info]
            -> null();
        host_live_result[errs] // demux must have at least 2 outputs
            -> null();


        // XXX assuming ClientID is unique, so we can use it as a key
        req_merge = union()
            -> inspect(|fragment: &PartialResult | println!("REQ_MERGE: {:?} {:?}", fragment.reqid, fragment))
            -> map(|fragment| (fragment.reqid.clone(), fragment))
            -> [1]resolve;
        tick_reqs
            -> [0]resolve;
        resolve = join::<'tick, 'tick>()
            -> demux(|(id, ((req, addr), result)) : (ClientID, ((CKRequest, SocketAddr), PartialResult)), var_args!(create, info)|
                    match &req.payload {
                        CKRequestType::Create { key, .. } => create.give((id, key.clone(), result, addr)),
                        CKRequestType::Info { key, .. } => info.give((id, key.clone(), result, addr)),
                        _ => panic!(),
                    }
                );

        // merge all Actions produced as exhaust
        // TODO: need an internal ID for each request, so we can merge on that
        resolve[create]
            -> inspect(|(id, key, result, addr)| println!("{}: KN: CREATE {:?} RESOLVE({:?})", Utc::now(), key, result))
            -> null();

        resolve[info]
            -> inspect(|(id, key, result, addr)| println!("{}: KN: INFO {:?} RESOLVE({:?})", Utc::now(), key, result))
            -> null();

        // segnode
        segnode_demux = source_stream_serde(sn_inbound)
            -> map(|m| m.unwrap())
            -> demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, svc_addr, blocks, } => heartbeat.give((id, svc_addr, blocks, addr, Utc::now())),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );

        heartbeats = segnode_demux[heartbeat] -> tee();
        segnode_demux[errs] -> for_each(|(msg, addr)| println!("KN: Unexpected SN message type: {:?} from {:?}", msg, addr));

        // Heartbeat response
        heartbeats
            -> map(|(_, _, _, addr, _)| (SKResponse::Heartbeat { }, addr))
            -> dest_sink_serde(sn_outbound);
        heartbeats // (block, sn_id)
            -> flat_map(|(id, _, blocks, _, _): (SegmentNodeID, _, Vec<Block>, _, _)| blocks.into_iter().map(move |block| (block, id.clone())))
            -> [1]block_host;
        heartbeats // (sn_id, (last_contact, svc_addr))
            -> map(|(segid, svc_addr, _, _, last_contact)| (segid, (svc_addr, last_contact)))
            -> [1]host_live;
    };

    //df.meta_graph().unwrap().write_mermaid(std::fs::File::create("lattice.md")).unwrap();
    if let Some(graph) = &opts.graph {
        print_graph(&df, graph);
    }

    df.run_async().await.unwrap();
}

#[allow(dead_code)]
async fn segment_node(opts: &Opts, keynode_server_addr: &'static str, sn_uuid: Uuid, init_blocks: &[Block]) {
    let (kn_outbound, kn_inbound) = connect_tcp_bytes();

    // clone blocks
    let init_blocks = init_blocks.to_vec();

    let sn_id = SegmentNodeID { id: sn_uuid }; // Uuid::new_v4(), ;
    let kn_addr: SocketAddr = ipv4_resolve(keynode_server_addr).unwrap();
    let cl_sn_addr: SocketAddr = ipv4_resolve("127.0.0.1:0").unwrap(); // random service port

    let hb_interval_stream = IntervalStream::new(time::interval(time::Duration::from_secs(1)));
    let (_cl_outbound, _cl_inbound, cl_addr) = bind_tcp_bytes(cl_sn_addr).await;

    println!("{}: SN: Starting {sn_id:?}@{cl_addr:?} with {init_blocks:?}", Utc::now());
    let mut df = hydroflow_syntax! {
        // each hb, should include all the blocks not-yet reported in the KN epoch
        // join w/ KN pool to determine to which KN the blocks should be reported

        hb_timer = source_stream(hb_interval_stream)
            -> map(|_| (kn_addr, ())) // wtf, no. Wrong address.
            -> [0]hb_report;
        source_iter(init_blocks)
            -> map(|block| (kn_addr, block))
            -> [1]hb_report;

        hb_report = join::<'tick, 'static>()
            -> fold_keyed::<'tick>(Vec::new,
                |acc: &mut Vec<Block>, (_, blk): ((), Block)| {
                    acc.push(blk);
                })
            -> map(|(addr, blk): (SocketAddr, Vec<Block>)| (SKRequest::Heartbeat {id: sn_id.clone(), svc_addr: cl_addr, blocks: blk }, addr))
            //-> inspect(|(m, a)| println!("{}: {} SN: HB {:?} to {:?}", Utc::now(), context.current_tick(), m, a))
            -> dest_sink_serde(kn_outbound);

        kn_demux = source_stream_serde(kn_inbound)
            -> map(Result::unwrap)
            -> demux(|(kn_resp, addr), var_args!(heartbeat, errs)|
                match kn_resp {
                    SKResponse::Heartbeat { /* other things */ } => heartbeat.give(addr),
                    _ => errs.give(()),
                }
            );

        kn_demux[heartbeat]
          //-> inspect(|addr| println!("{}: SN: HB response from {:?}", Utc::now(), addr))
          -> null();

        kn_demux[errs]
            -> null();
    };

    if let Some(graph) = &opts.graph {
        print_graph(&df, graph);
    }

    df.run_async().await;
}