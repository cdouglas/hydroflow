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
mod latless;

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

pub struct Cmd {
    req: CKRequest,
    addr: SocketAddr,
}

static EMPTY_BLK: Block = Block { pool: "".to_owned(), id: 0u64 };
static EMPTY_KEY: INode = INode { id: 0u64 };

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
        ("dingo".to_owned(), 5678u64, vec![
            Block { pool: "x".to_owned(), id: 200u64 },
            Block { pool: "x".to_owned(), id: 300u64 },
        ]),
        ("yak".to_owned(), 1234u64, vec![
            Block { pool: "x".to_owned(), id: 400u64 }
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
        //crate::latless::segment_node(&opts, KEYNODE_SN_ADDR, Uuid::new_v4(), &canned_blocks[0..1]),
        //crate::latless::segment_node(&opts, KEYNODE_SN_ADDR, Uuid::new_v4(), &canned_blocks[..]),
        //crate::latless::key_node(&opts, KEYNODE_SN_ADDR, keynode_client_addr, &canned_keys[..]),
    );
}

#[allow(dead_code)]
async fn key_node(opts: &Opts, keynode_sn_addr: &'static str, keynode_client_addr: SocketAddr, init_keys: &[(String, u64, Vec<Block>)]) {
    let (cl_outbound, cl_inbound, cl_addr) = bind_tcp_bytes(keynode_client_addr).await;
    let (sn_outbound, sn_inbound, sn_addr) = bind_tcp_bytes(ipv4_resolve(keynode_sn_addr).unwrap()).await;
    println!("{}: KN<->SN: Listening on {}", Utc::now(), sn_addr);
    println!("{}: KN<->CL: Listening on {}", Utc::now(), cl_addr);

    let init_keys = init_keys.to_vec();

    let mut seq = 1u64 + init_keys.iter()
        .fold(0u64, |acc, (_, inode, _)| acc.max(*inode));

    let mut df = hydroflow_syntax! {

        // client
        // TODO: keep id->addr map elsewhere?
        client_demux = source_stream_serde(cl_inbound)
            -> map(Result::unwrap)
            -> inspect(|(m, a)| { println!("{}: KN: CL {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(cl_req, addr), var_args!(create, info, errs)|
                    match cl_req {
                        CKRequest::Info { id, key, .. } => info.give(cl_req, addr), //(key, ClientInfo { id, addr })),
                        CKRequest::Create { id, key, .. } => create.give(cl_req, addr), //(key, ClientInfo { id, addr})),
                        _ => errs.give((cl_req, addr)),
                    }
                );

        // (key, inode)
        key_inode = union()
                //-> persist() // not necessary w/ 'static on the join?
                -> [1]key_map;
        disk_key_inode = source_iter(init_keys)
                -> map(|(key, inode, blocks)| (key, inode))
                -> [0]key_inode;

        // (inode, seq, block)
        inode_block = union();
        disk_inode_block = source_iter(init_keys)
                // "store" as (inode, seq, block) to preserve order
                -> flat_map(|(_, inode, blocks)| blocks.enumerate().map(move |(i, block)| (INode { id: inode, }, i, block)))
            //-> map(|(key, blocks)| (key, SetUnionHashSet::new_from([blocks])))
            //-> [1]key_map;
            -> [0]inode_block;

        creates = client_demux[create]
                -> inspect(|(key, (_, addr))| println!("{}: KN: CREATE {:?} from {:?}", Utc::now(), key, addr))
                -> null();
                //-> tee();

        //// creates in key_inode return err (already exist)
        //// creates \not in key_node are allocated an inode, added to key_inode, EMPTY_BLOCK added to inode_block
        //creates[0]
        //    -> map(|(key, clinfo)| (key, (EMPTY_KEY, clinfo)))
        //    -> 

        //creates[1]
        //    -> key_map[0]

        client_demux[info] // (Cmd)
            -> inspect(|(req, addr)| println!("{}: KN: INFO {:?} from {:?}", Utc::now(), req.key, addr))
            -> map(|(req, addr)| (req.key, (req, addr)))
            -> [0]key_map;

        client_demux[errs]
            -> for_each(|(msg, addr)| println!("KN: Unexpected CL message type: {:?} from {:?}", msg, addr));

        // join requests LHS:(key, req) with existing key map RHS:(key, inode)
        key_map = join::<'tick, 'static>()
            -> demux(|(key, (req, inode)), var_args!(create, info, errs)|
                    match req {
                        CKRequest::Create { id, key, .. } => create.give(req, inode),
                        CKRequest::Info { id, key, .. } => info.give(req, inode),
                        _ => errs.give((key, (req, inode))),
                    }
                );

        key_map[create]
            -> map(|req, inode)|
            -> [0]create_key_resolve;

        key_map[info]
            -> 

        // LHS: lookup inode by key, RHS: requests
        create_key_resolve =
            -> flat_map(|(key, (cli, blocks))| blocks.into_iter().map(move
                |block| (block, MapUnionHashMap::new_from([(key.clone(), SetUnionHashSet::new_from([cli.clone()]))]))))
            -> inspect(|x| println!("{}: KN: LOOKUP_KEY_MAP: {x:?}", Utc::now()))
            -> [0]block_map;

        // segnode
        segnode_demux = source_stream_serde(sn_inbound)
            -> map(|m| m.unwrap())
            //-> inspect(|(m, a)| { println!("{}: KN: SN {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, svc_addr, blocks, } => heartbeat.give((id, svc_addr, blocks, addr, Max::new(Utc::now()))),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );

        heartbeats = segnode_demux[heartbeat]
            -> tee();
        segnode_demux[errs]
          -> for_each(|(msg, addr)| println!("KN: Unexpected SN message type: {:?} from {:?}", msg, addr));

        // Heartbeat response
        heartbeats
            -> map(|(_, _, _, addr, _)| (SKResponse::Heartbeat { }, addr))
            -> dest_sink_serde(sn_outbound);

        // XXX note equivalent t lattice_fold() -> join() on both sides, but this is more efficient
        last_contact_map = _lattice_join_fused_join::<'tick, 'static,
                MapUnionHashMap<Block, MapUnionHashMap<String,SetUnionHashSet<(ClientID, SocketAddr)>>>,
                DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>>()
            -> inspect(|x: &(SegmentNodeID,
                (MapUnionHashMap<Block, MapUnionHashMap<String,SetUnionHashSet<(ClientID, SocketAddr)>>>,
                 DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>))|
                            println!("{}: <- LCM: {x:?}", Utc::now()))
            // XXX want this filter to yield... a MIN/BOT value s.t. if no valid replicas, will merge into an error value?
            // remove segment nodes that haven't sent a heartbeat in the last 10 seconds
            -> filter(|(_, (_, hbts))| Utc::now() - *hbts.as_reveal_ref().0.as_reveal_ref() < chrono::Duration::seconds(10))
            // extract the service address from those nodes
            -> flat_map(|(_, (block_clikey, hbts))| block_clikey.into_reveal().into_iter().map(move
                |(block, clikeys)| MapUnionHashMap::new_from([(block,
                    Pair::<MapUnionHashMap<String,SetUnionHashSet<(ClientID,SocketAddr)>>, SetUnionHashSet<SocketAddr>>
                        ::new_from(clikeys, SetUnionHashSet::new_from([hbts.into_reveal().1.val])))])))
            //-> lattice_fold::<'tick,
            //    MapUnionHashMap<Block,
            //                    Pair<MapUnionHashMap<String, SetUnionHashSet<(ClientID,SocketAddr)>>,
            //                         SetUnionHashSet<SocketAddr>>>>()
            -> lattice_fold::<'tick>(
                MapUnionHashMap::<Block,
                                Pair<MapUnionHashMap<String, SetUnionHashSet<(ClientID,SocketAddr)>>,
                                     SetUnionHashSet<SocketAddr>>>::default())
            // unpack map into tuples
            -> flat_map(|block_keycli_addr| block_keycli_addr.into_reveal().into_iter().map(move
                |(block, req_sn_addrs)| (block, req_sn_addrs.into_reveal())))
            -> flat_map(move |(block, (reqs, sn_addrs)): (Block, (MapUnionHashMap<String, SetUnionHashSet<(ClientID,SocketAddr)>>,
                                                             SetUnionHashSet<SocketAddr>))|
                    reqs.into_reveal().into_iter().map(move |(key, cliset)|
                    (key,
                    LocatedBlock {
                        block: block.clone(),
                        locations: sn_addrs.clone().into_reveal().into_iter().collect::<Vec<_>>(), // XXX clone() avoidable?
                    },
                    cliset)))
            -> flat_map(move |(key, lblock, cliset)| cliset.into_reveal().into_iter().map(move |(id, addr)| ((id, addr, key.clone()), lblock.clone())))
            -> fold_keyed::<'tick>(Vec::new, |lblocks: &mut Vec<LocatedBlock>, lblock| {
                lblocks.push(lblock);
            })
            -> map(|((_, addr, key), lblocks)| (CKResponse::Info { key: key, blocks: lblocks }, addr))
            -> inspect(|x| println!("{}: -> LCM: {x:?}", Utc::now()))
            -> dest_sink_serde(cl_outbound);

        heartbeats
            -> map(|(id, svc_addr, _, _, last_contact)| (id, DomPair::<Max<DateTime<Utc>>,Point<SocketAddr, ()>>::new_from(last_contact, Point::new_from(svc_addr))))
            //-> inspect(|x| println!("{}: KN: HB_LC: {x:?}", Utc::now()))
            -> [1]last_contact_map;

        // join all requests for blocks reported from segment nodes
        block_map = _lattice_join_fused_join::<'tick, 'static, MapUnionHashMap<String, SetUnionHashSet<(ClientID, SocketAddr)>>, SetUnionHashSet<SegmentNodeID>>()
            // can we replace `clone()` with `to_owned()`? The compiler thinks so!
            -> flat_map(|(block, (clikeyset, sn_set))| sn_set.into_reveal().into_iter().map(move |sn|
                    (sn, MapUnionHashMap::new_from([(block.clone(), clikeyset.clone())]))))
            -> [0]last_contact_map;

        heartbeats
            -> flat_map(|(id, _, blocks, _, _): (SegmentNodeID, SocketAddr, Vec<Block>, _, Max<DateTime<Utc>>)| blocks.into_iter().map(move |block| (block, SetUnionHashSet::new_from([id.clone()]))))
            -> [1]block_map;


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