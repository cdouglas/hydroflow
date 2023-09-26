use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use hydroflow::util::{bind_tcp_bytes, connect_tcp_bytes, ipv4_resolve};
use hydroflow::{hydroflow_syntax, tokio};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;

use crate::protocol::*;
use crate::helpers::print_graph;

use crate::Opts;

pub async fn key_node(opts: &Opts, keynode_sn_addr: &'static str, keynode_client_addr: SocketAddr, init_keys: &[(String, Vec<Block>)]) {
    let (cl_outbound, cl_inbound, cl_addr) = bind_tcp_bytes(keynode_client_addr).await;
    let (sn_outbound, sn_inbound, sn_addr) = bind_tcp_bytes(ipv4_resolve(keynode_sn_addr).unwrap()).await;
    println!("{}: KN<->SN: Listening on {}", Utc::now(), sn_addr);
    println!("{}: KN<->CL: Listening on {}", Utc::now(), cl_addr);

    let cli_a_id = Uuid::new_v4();
    let cli_b_id = Uuid::new_v4();
    let _canned_reqs = vec![
        (
            CKRequest::Info {
                id: ClientID { id: cli_a_id },
                key: "dingo".to_owned(),
            },
            SocketAddr::from(([127, 0, 0, 1], 4344))
        ),
        (
            CKRequest::Info {
                id: ClientID { id: cli_b_id },
                key: "dingo".to_owned(),
            },
            SocketAddr::from(([127, 0, 0, 1], 4345))
        ),
        (
            CKRequest::Info {
                id: ClientID { id: cli_a_id },
                key: "yak".to_owned(),
            },
            SocketAddr::from(([127, 0, 0, 1], 4344))
        ),
    ];

    let init_keys = init_keys.to_vec();

    let mut df = hydroflow_syntax! {

        // client
        client_demux = source_stream_serde(cl_inbound)
            -> map(Result::unwrap)
            -> inspect(|(m, a)| { println!("{}: KN: CL {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(cl_req, addr), var_args!(info, errs)|
                    match cl_req {
                        CKRequest::Info { id, key, .. } => info.give((key, (id, addr))), // TODO: keep id->addr map elsewhere?
                        _ => errs.give((cl_req, addr)),
                    }
                );

        client_demux[info]
            -> [0]key_map;
        client_demux[errs]
            -> for_each(|(msg, addr)| println!("KN: Unexpected CL message type: {:?} from {:?}", msg, addr));

        // segnode
        segnode_demux = source_stream_serde(sn_inbound)
            -> map(|m| m.unwrap())
            //-> inspect(|(m, a)| { println!("{}: KN: SN {:?} from {:?}", Utc::now(), m, a); })
            -> demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, svc_addr, blocks, } => heartbeat.give((id, svc_addr, blocks, addr, Utc::now())),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );

        heartbeats = segnode_demux[heartbeat]
            -> tee();

        // Heartbeat response
        heartbeats
            -> map(|(_, _, _, addr, _)| (SKResponse::Heartbeat { }, addr))
            -> dest_sink_serde(sn_outbound);

        last_contact_map = join::<'tick, 'static>()
            -> inspect(|x: &(SegmentNodeID, ((Block, String, ClientInfo), (DateTime<Utc>, SocketAddr)))| println!("{}: KN: LCM: {x:?} -> ", Utc::now()))
            -> filter(|(_, (_, (last_contact, _)))| Utc::now() - *last_contact < chrono::Duration::seconds(10))
            // XXX need to clone the current value in reduce_keyed; can this select the max by reference?
            -> reduce_keyed(
                |cur: &mut ((Block, String, ClientInfo), (DateTime<Utc>, SocketAddr)),
                 val: ((Block, String, ClientInfo), (DateTime<Utc>, SocketAddr))|
                    *cur = std::cmp::max_by(cur.clone(), val, |c, v| c.1.0.cmp(&v.1.0)))
            // remove unnecessary duplication
            -> map(|(_, ((block, key, cli), (_, addr)))| ((key, cli, block.clone()), LocatedBlock { block: block.clone(), locations: vec![addr] }))
            -> reduce_keyed(|cur: &mut LocatedBlock, val: LocatedBlock| cur.locations.extend(val.locations))
            -> inspect(|x: &((String, ClientInfo, Block), LocatedBlock)| println!("{}: KN: LCM: {x:?} ******* ", Utc::now()))
            -> null();

        //last_contact_map = lattice_join::<'tick, 'static,
        //        MapUnionHashMap<Block, MapUnionHashMap<String,SetUnionHashSet<(ClientID, SocketAddr)>>>,
        //        DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>>()
        //    -> inspect(|x: &(SegmentNodeID,
        //        (MapUnionHashMap<Block, MapUnionHashMap<String,SetUnionHashSet<(ClientID, SocketAddr)>>>,
        //         DomPair<Max<DateTime<Utc>>, Point<SocketAddr, ()>>))|
        //                    println!("{}: <- LCM: {x:?}", Utc::now()))
        //    // XXX want this filter to yield... a MIN/BOT value s.t. if no valid replicas, will merge into an error value?
        //    // remove segment nodes that haven't sent a heartbeat in the last 10 seconds
        //    -> filter(|(_, (_, hbts))| Utc::now() - *hbts.as_reveal_ref().0.as_reveal_ref() < chrono::Duration::seconds(10))
        //    // extract the service address from those nodes
        //    -> flat_map(|(_, (block_clikey, hbts))| block_clikey.into_reveal().into_iter().map(move
        //        |(block, clikeys)| MapUnionHashMap::new_from([(block,
        //            Pair::<MapUnionHashMap<String,SetUnionHashSet<(ClientID,SocketAddr)>>, SetUnionHashSet<SocketAddr>>
        //                ::new_from(clikeys, SetUnionHashSet::new_from([hbts.into_reveal().1.val])))])))
        //    -> lattice_fold::<'tick,
        //        MapUnionHashMap<Block,
        //                        Pair<MapUnionHashMap<String, SetUnionHashSet<(ClientID,SocketAddr)>>,
        //                             SetUnionHashSet<SocketAddr>>>>()
        //    // unpack map into tuples
        //    -> flat_map(|block_keycli_addr| block_keycli_addr.into_reveal().into_iter().map(move
        //        |(block, req_sn_addrs)| (block, req_sn_addrs.into_reveal())))
        //    -> flat_map(move |(block, (reqs, sn_addrs)): (Block, (MapUnionHashMap<String, SetUnionHashSet<(ClientID,SocketAddr)>>,
        //                                                     SetUnionHashSet<SocketAddr>))|
        //            reqs.into_reveal().into_iter().map(move |(key, cliset)|
        //            (key,
        //            LocatedBlock {
        //                block: block.clone(),
        //                locations: sn_addrs.clone().into_reveal().into_iter().collect::<Vec<_>>(), // XXX clone() avoidable?
        //            },
        //            cliset)))
        //    -> flat_map(move |(key, lblock, cliset)| cliset.into_reveal().into_iter().map(move |(id, addr)| ((id, addr, key.clone()), lblock.clone())))
        //    -> fold_keyed::<'tick>(Vec::new, |lblocks: &mut Vec<LocatedBlock>, lblock| {
        //        lblocks.push(lblock);
        //    })
        //    -> map(|((_, addr, key), lblocks)| (CKResponse::Info { key: key, blocks: lblocks }, addr))
        //    -> inspect(|x| println!("{}: -> LCM: {x:?}", Utc::now()))
        //    -> dest_sink_serde(cl_outbound);

        heartbeats
            -> map(|(id, svc_addr, _, _, last_contact)| (id, (last_contact, svc_addr)))
            -> inspect(|x: &(SegmentNodeID, (DateTime<Utc>, SocketAddr))| println!("{}: KN: HB_LC: {x:?}", Utc::now()))
            -> [1]last_contact_map;

        //// join all requests for blocks
        //block_map = lattice_join::<'tick, 'static, MapUnionHashMap<String, SetUnionHashSet<(ClientID, SocketAddr)>>, SetUnionHashSet<SegmentNodeID>>()
        //    // can we replace `clone()` with `to_owned()`? The compiler thinks so!
        //    -> flat_map(|(block, (clikeyset, sn_set))| sn_set.into_reveal().into_iter().map(move |sn|
        //            (sn, MapUnionHashMap::new_from([(block.clone(), clikeyset.clone())]))))
        //    //-> inspect(|x: &(SegmentNodeID, MapUnionHashMap<Block, MapUnionHashMap<String,SetUnionHashSet<(ClientID, SocketAddr)>>>)| println!("{}: KN: RPC_LC: {x:?}", Utc::now()))
        //    -> [0]last_contact_map;

        block_map = join::<'tick, 'static>()
            -> inspect(|x: &(Block, ((String, ClientInfo), SegmentNodeID))| println!("{}: KN: LOOKUP_BLOCK_MAP: {x:?}", Utc::now()))
            -> map(|(block, ((key, cli), sn_id))| (sn_id, (block, key, cli)))
            -> [0]last_contact_map;

        heartbeats
            -> flat_map(|(id, _, blocks, _, _): (SegmentNodeID, _, Vec<Block>, _, _)| blocks.into_iter().map(move |block| (block, id.clone())))
            // add filter for blocks here?
            -> [1]block_map;

        // join (key, client) requests with existing key map (key, blocks)
        key_map = join::<'tick, 'static>()
            -> inspect(|x: &(String, ((ClientID, SocketAddr), Block))| println!("{}: KN: LOOKUP_KEY_MAP: {x:?}", Utc::now()))
            -> map(|(key, ((id, addr), block))| (block, (key, ClientInfo { id, addr })))
            -> [0]block_map;

        // flatten into relation
        source_iter(init_keys)
            -> flat_map(|(key, blocks)| blocks.into_iter().map(move |block| (key.clone(), block)))
            -> [1]key_map;

        segnode_demux[errs]
          -> for_each(|(msg, addr)| println!("KN: Unexpected SN message type: {:?} from {:?}", msg, addr));
    };

    //df.meta_graph().unwrap().write_mermaid(std::fs::File::create("lattice.md")).unwrap();
    if let Some(graph) = &opts.graph {
        print_graph(&df, graph);
    }

    df.run_async().await.unwrap();
}

pub async fn segment_node(opts: &Opts, keynode_server_addr: &'static str, sn_uuid: Uuid, init_blocks: &[Block]) {
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
