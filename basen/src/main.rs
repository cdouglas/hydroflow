use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use hydroflow::lattices::set_union::{SetUnionHashSet};
use hydroflow::lattices::{DomPair, Max, Pair, Point};
use hydroflow::util::{bind_tcp_bytes, connect_tcp_bytes, ipv4_resolve};
use hydroflow::{hydroflow_syntax, tokio};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

use crate::protocol::*;
use crate::client::run_client;

mod client;
mod helpers;
mod keynode;
mod protocol;
mod segnode;

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

#[derive(Parser, Debug)]
struct Opts {
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

async fn run_servers(keynode_cl_addr: SocketAddr, _opts: Opts) {
    const KEYNODE_SN_ADDR: &str = "127.0.0.55:4345";

    let canned_keys = vec![
        ("dingo".to_owned(), vec![
            Block { pool: "2023874_0".to_owned(), id: 2348980u64 },
            Block { pool: "2023874_0".to_owned(), id: 2348985u64 },
        ]),
        ("yak".to_owned(), vec![
            Block { pool: "2023874_0".to_owned(), id: 2348990u64 }
        ]),
    ];

    let canned_blocks = vec![
        Block { pool: "2023874_0".to_owned(), id: 2348980u64 },
        Block { pool: "2023874_0".to_owned(), id: 2348985u64 },
        Block { pool: "2023874_0".to_owned(), id: 2348990u64 },
    ];
    futures::join!(
        segment_node(KEYNODE_SN_ADDR, Uuid::new_v4(), &canned_blocks[0..1]),
        segment_node(KEYNODE_SN_ADDR, Uuid::parse_str("454147e2-ef1c-4a2f-bcbc-a9a774a4bb62").unwrap(), &canned_blocks[..]),
        key_node(KEYNODE_SN_ADDR, keynode_cl_addr, &canned_keys[..]),
    );
}

async fn key_node(keynode_sn_addr: &'static str, keynode_client_addr: SocketAddr, init_keys: &[(String, Vec<Block>)]) {
    let (_cl_outbound, _cl_inbound, cl_addr) = bind_tcp_bytes(keynode_client_addr).await;
    let (sn_outbound, sn_inbound, sn_addr) = bind_tcp_bytes(ipv4_resolve(keynode_sn_addr).unwrap()).await;
    println!("{}: KN<->SN: Listening on {}", Utc::now(), sn_addr);
    println!("{}: KN<->CL: Listening on {}", Utc::now(), cl_addr);

    let canned_reqs = vec![
        (CKRequest::Info {
            id: ClientID { id: Uuid::new_v4() },
            key: "dingo".to_owned(),
        },
        SocketAddr::from(([127, 0, 0, 1], 4344))
        ),
    ];

    // LastContactMap: HashMap<SegmentNodeID, (SocketAddr, Time)>
    let init_keys = init_keys.to_vec();

    // changed to a regular join
    //type LastContactLattice = DomPair<Max<DateTime<Utc>>, SetUnionHashSet<SocketAddr>>;
    type BlockSetLattice = SetUnionHashSet<SegmentNodeID>;
    type Joe = Pair<SetUnionHashSet<((ClientID, SocketAddr), String)>, SetUnionHashSet<Block>>;
    type Chris = DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>;

    let mut df = hydroflow_syntax! {

        // client
        client_demux = //source_stream_serde(cl_inbound)
            //-> map(Result::unwrap)
        ////////
            source_iter(canned_reqs)
            -> persist()
        /////////
            -> inspect(|(m, a)| { println!("{}: KN: CL {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(cl_req, addr), var_args!(info, errs)|
                    match cl_req {
                        CKRequest::Info { id, key, .. } => info.give((key, (id, addr))), // TODO: keep id->addr map elsewhere?
                        _ => errs.give((cl_req, addr)),
                    }
                );

        client_demux[info]
            -> [0]key_map;
            //-> map(|(_cl_req, addr)| (CKResponse::Info { blocks: vec![] }, addr))
            //-> dest_sink_serde(cl_outbound);
        client_demux[errs]
            -> for_each(|(msg, addr)| println!("KN: Unexpected CL message type: {:?} from {:?}", msg, addr));

        // segnode
        segnode_demux = source_stream_serde(sn_inbound)
            -> map(|m| m.unwrap())
            -> inspect(|(m, a)| { println!("{}: KN: SN {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, blocks, } => heartbeat.give((id, blocks, addr, Max::new(Utc::now()))),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );

        heartbeats = segnode_demux[heartbeat]
            -> tee();

        // Heartbeat response
        heartbeats
            -> map(|(_, _, addr, _)| (SKResponse::Heartbeat { }, addr))
            -> dest_sink_serde(sn_outbound);

        // LastContactMap: MapUnion<SegmentNodeID, DomPair<Max<DateTime<Utc>>, SetUnionHashSet<SocketAddr>>>
        //last_contact_map = lattice_join::<'tick, 'static, SetUnionHashSet<Block>, LastContactLattice>();
        //last_contact_map = lattice_join::<'tick, 'static, Joe, DomPair<Max<DateTime<Utc>>,SetUnionHashSet<SocketAddr>>>();
        //last_contact_map
        //    -> inspect(|x| println!("{}: KN: DEBUG2: {x:?}", Utc::now()))
        //    // uff.
        //    //-> inspect(|x| println!("{}: DEBUG0: {:?}", Utc::now(), x))
        //    -> map(|(_, ((clikey, block), last_contact)):
        //                        (SegmentNodeID,
        //                            (Joe,
        //                             DomPair<Max<DateTime<Utc>>,SetUnionHashSet<SocketAddr>>)) | 
        //        (block, (clikey, last_contact.into_reveal().1.into_reveal())))
        //    //-> inspect(|x| println!("{}: DEBUG1: {:?}", Utc::now(), x))
        //    -> flat_map(|(block, (clikey, addrset)):
        //        (Block, (SetUnionHashSet<((ClientID, SocketAddr), String)>, HashSet<SocketAddr>)
        //        )| addrset.into_iter().map(move |addr| (block.clone(), (addr, clikey.clone()))))
        //    //-> inspect(|x| println!("{}: DEBUG2: {:?}", Utc::now(), x))
        //    -> fold_keyed::<'tick>(|| vec![], |acc: &mut Vec<(SocketAddr, SetUnionHashSet<((ClientID, SocketAddr), String)>)>, (block, addr): (SocketAddr, SetUnionHashSet<((ClientID, SocketAddr), String)>)| {
        //        acc.push((block, addr));
        //        //acc.to_owned() // why is this to_owned() necessary? // XXX it's not; fold_keyed doesn't retain this context
        //    })
        //    //-> map(|(b, s): (Block, SetUnionHashSet<SocketAddr>)| hydroflow::lattices::map_union::MapUnion::new(vec![(b, s)]))
        //    //-> lattice_fold::<'tick, MapUnionHashMap<Block,SetUnionHashSet<SocketAddr>>>()
        //    -> inspect(|x| println!("{}: LOOKUP_LAST_CONTACT_MAP: KN: {x:?}", Utc::now()))
        //    -> null();
        //    //-> dest_sink_serde(cl_outbound);

        // LCM: (SegmentNodeID { id: 454147e2-ef1c-4a2f-bcbc-a9a774a4bb62 },
        //    (Pair { a: SetUnion({((ClientID { id: 2ddb484b-4f32-4a26-acc2-fb5eca607a8a }, 127.0.0.1:4344), "dingo")}),
        //            b: SetUnion({Block { pool: "2023874_0", id: 2348980 }, Block { pool: "2023874_0", id: 2348985 }})
        //          },
        //     DomPair { key: Max(2023-07-07T17:53:05.782872965Z), val: SetUnion({127.0.0.1:45052}) }))

        //type Joe = Pair<SetUnionHashSet<((ClientID, SocketAddr), String)>, SetUnionHashSet<Block>>;
        //type Chris = DomPair<Max<DateTime<Utc>>, SetUnionHashSet<SocketAddr>>;
        last_contact_map = lattice_join::<'tick, 'static, Joe, Chris>()
        // uff.
            //-> filter(|(_, (_, hbts)): &(_, (_, DomPair<Max<DateTime<Utc>>, SetUnionHashSet<SocketAddr>>))| Utc::now() - *hbts.as_reveal_ref().0.as_reveal_ref() < chrono::Duration::seconds(10))
            -> inspect(|x: &(SegmentNodeID, (Pair<SetUnionHashSet<((ClientID, SocketAddr), String)>, SetUnionHashSet<Block>>,
                                            DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>))|
                            println!("{}: KN: LCM: {x:?}", Utc::now()))
            -> filter(|(_, (_, hbts))| Utc::now() - *hbts.as_reveal_ref().0.as_reveal_ref() < chrono::Duration::seconds(10))
            -> map(|(_, (clikeyblock, hbts))| (clikeyblock.into_reveal(), hbts.into_reveal().1.val))
            -> flat_map(|((clikeyset, blockset), sn_addr)| clikeyset.into_reveal().into_iter().map(move |((id, addr), key)| (key, id, addr, blockset.clone(), sn_addr)))
            -> flat_map(|(key, cl_id, cl_addr, blockset, sn_addr)|
                    blockset.into_reveal().into_iter().map(move |block|
                        (key.clone(), (cl_id.clone(), cl_addr, block, sn_addr))))
            // FUCK. NO. Needs multiple folds???
            -> inspect(|x: &(String, (ClientID, SocketAddr, Block, SocketAddr))|
                    println!("{}: LCM: {:?}", Utc::now(), x))
            //-> inspect(|x: &((SetUnionHashSet<((ClientID, SocketAddr), String)>, SetUnionHashSet<Block>), SocketAddr)|
            //        println!("{}: LCM: {:?}", Utc::now(), x))
            -> null();

        heartbeats
            -> map(|(id, _, addr, last_contact)| (id, DomPair::<Max<DateTime<Utc>>,Point<SocketAddr, ()>>::new_from(last_contact, Point::new_from(addr))))
            -> inspect(|x| println!("{}: KN: HB_LC: {x:?}", Utc::now()))
            -> [1]last_contact_map;

        // BlockMap: HashMap<BlockId, Set<SegmentNodeID>>
        // XXX convert from a lattice_join?
        // Form a Joe: SetUnion<HashSet<(((ClientID, SocketAddr), String), Block)>>;

        block_map = lattice_join::<'tick, 'static, SetUnionHashSet<((ClientID, SocketAddr), String)>, BlockSetLattice>()
            // can we replace `clone()` with `to_owned()`? The compiler thinks so!
            -> flat_map(|(block, (clikeyset, sn_set))| sn_set.into_reveal().into_iter().map(move |sn| (sn, Pair::new_from(clikeyset.clone(), SetUnionHashSet::new_from([block.clone()])))))
            -> inspect(|x| println!("{}: KN: RPC_LC: {x:?}", Utc::now()))
            // We think the block is unique, but let's check
            -> map(|(key, val)| {
                let b: &SetUnionHashSet<Block> = val.as_reveal_ref().1;
                let b2 = b.as_reveal_ref();
                if b2.len() > 1 {
                    panic!()
                }
                (key, val)
              })
            -> [0]last_contact_map;

        heartbeats
            -> flat_map(|(id, blocks, _, _): (SegmentNodeID, Vec<Block>, _, Max<DateTime<Utc>>)| blocks.into_iter().map(move |block| (block, SetUnionHashSet::new_from([id.clone()]))))
            -> [1]block_map;

        key_map = join::<'tick, 'static>() // where to stash the client context?
            -> flat_map(|(key, (cli, blocks))| blocks.into_iter().map(move |block| (block, SetUnionHashSet::new_from([(cli.clone(), key.clone())]))))
            -> inspect(|x| println!("{}: KN: LOOKUP_KEY_MAP: {x:?}", Utc::now()))
            -> [0]block_map;

        source_iter(init_keys)
            //-> map(|(key, blocks)| (key, SetUnionHashSet::new_from([blocks])))
            -> [1]key_map;

        segnode_demux[errs]
          -> for_each(|(msg, addr)| println!("KN: Unexpected SN message type: {:?} from {:?}", msg, addr));
    };

    df.run_async().await.unwrap();
}

async fn segment_node(keynode_server_addr: &'static str, sn_uuid: Uuid, init_blocks: &[Block]) {
    let (kn_outbound, kn_inbound) = connect_tcp_bytes();

    // clone blocks
    let init_blocks = init_blocks.to_vec();

    //let sn_id = Uuid::new_v4(); //Uuid::parse_str("454147e2-ef1c-4a2f-bcbc-a9a774a4bb62").unwrap();
    let sn_id = SegmentNodeID { id: sn_uuid }; // Uuid::new_v4(), };
    let kn_addr: SocketAddr = ipv4_resolve(keynode_server_addr).unwrap();

    let hb_interval_stream = IntervalStream::new(time::interval(time::Duration::from_secs(1)));
    let mut flow = hydroflow_syntax! {
        // each hb, should include all the blocks not-yet reported in the KN epoch
        // join w/ KN pool to determine to which KN the blocks should be reported

        hb_timer = source_stream(hb_interval_stream)
            -> map(|_| (kn_addr, ()))
            -> [0]hb_report;
        source_iter(init_blocks)
            -> map(|block| (kn_addr, block))
            -> [1]hb_report;

        hb_report = join::<'tick, 'static>()
            -> fold_keyed::<'tick>(Vec::new,
                |acc: &mut Vec<Block>, (_, blk): ((), Block)| {
                    acc.push(blk);
                    //acc.to_owned() // !#! not necessary for _keyed operators; not moved into the fold closure
                })
            -> map(|(addr, blk): (SocketAddr, Vec<Block>)| (SKRequest::Heartbeat {id: sn_id.clone(), blocks: blk }, addr))
            -> inspect(|(m, a)| println!("{}: {} SN: HB {:?} to {:?}", Utc::now(), context.current_tick(), m, a))
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
          -> for_each(|addr| println!("{}: SN: HB response from {:?}", Utc::now(), addr));

        kn_demux[errs]
            -> null();




        // // Define shared inbound and outbound channels
        // inbound_chan = source_stream_serde(cl_inbound)
        //     -> map(|udp_msg| udp_msg.unwrap()) /* -> tee() */; // commented out since we only use this once in the client template

        // // Print all messages for debugging purposes
        // inbound_chan[1]
        //     -> for_each(|(m, a): (SKResponse, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        //// take stdin and send to server as an Message::Echo
        //source_stdin() -> map(|l| (Message::Echo{ payload: l.unwrap(), ts: Utc::now(), }, server_addr) )
        //    -> outbound_chan;
    };

    flow.run_async().await;
}

// for regular join:
//     insert: ("a", 0) - ("a", 1)
//     insert: ("a", 0) - ("a", 2)

//     lhs: ("a", [0])
//     rhs: ("a", [1, 2])

//     output:
//     ("a", (0, 1))
//     ("a", (0, 2))

// for multiset_join
//     insert: ("a", 0) - ("a", 1)
//     insert: ("a", 0) - ("a", 2)

//     lhs: ("a", [0, 0])
//     rhs: ("a", [1, 2])

//     output:
//     ("a", (0, 1))
//     ("a", (0, 2))
//     ("a", (0, 1))
//     ("a", (0, 2))

// for lattice_join:
//     insert: ("a", SetUnion([0])) -> LHS
//     insert: ("a", SetUnion([3])) -> LHS
//     insert: ("a", Min(0)) -> RHS
//     insert: ("a", Max(2)) -> RHS

//     lhs: ("a", SetUnion([0, 3]))
//     rhs: ("a", Max(2))

//     output:
//     ("a", (SetUnion([0, 3]), Max(2))) -> flat_map() ->

//     ("a", (0, Max(2)))
//     ("a", (3, Max(2)))
