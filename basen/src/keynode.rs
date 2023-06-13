use chrono::prelude::*;
use uuid::Uuid;
use crate::helpers::print_graph;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{bind_tcp_bytes, ipv4_resolve, UdpSink, UdpStream};
use std::collections::HashSet;
use std::net::SocketAddr;
use crate::protocol::*;

#[allow(dead_code)]
pub(crate) async fn run_keynode(_cl_outbound: UdpSink, cl_inbound: UdpStream, opts: crate::Opts) {

    // open separate channel for SN traffic
    let (sn_outbound, sn_inbound, _sn_addr) =
        bind_tcp_bytes(ipv4_resolve("localhost:4345").unwrap()).await;
    let _sn_id = SegmentNodeID { id: Uuid::parse_str("454147e2-ef1c-4a2f-bcbc-a9a774a4bb62").unwrap() };

    let mut flow: Hydroflow = hydroflow_syntax! {
        //
        // client
        //
        client_in = source_stream_serde(cl_inbound) -> map(|udp_msg| udp_msg.unwrap()) -> tee();
        //client_out = union() -> dest_sink_serde(outbound);

        client_in[1]
            -> for_each(|(m, a): (CKRequest, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        client_demux = client_in[0]
            ->  demux(|(msg, addr), var_args!(create, addblock, errs)|
                    match msg {
                        CKRequest::Create {key, ..} => create.give((key, addr)),
                        CKRequest::AddBlock {lease, ..} => addblock.give((lease, addr)),
                        // fill out the rest of the client request messages
                        _ => errs.give((msg, addr)),
                    }
                );

        client_demux[create]
            //-> map(|(key, addr)| (CKRequest::Create { key: String, ts: Utc::now() }, addr) ) -> [0]client_out;
            -> for_each(|(msg, addr)| println!("Received create message type: {:?} from {:?}", msg, addr));
        client_demux[addblock] //-> map(|addr| (Message::HeartbeatAck, addr)) -> [1]client_out;
            -> for_each(|(msg, addr)| println!("Received addblock message type: {:?} from {:?}", msg, addr));
        client_demux[errs]
            -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));

        //
        // segment
        //
        segnode_in = source_stream_serde(sn_inbound) -> map(|m| m.unwrap()) -> tee();
        //segnode_out = union() -> dest_sink_serde(sn_outbound);
        segnode_out = dest_sink_serde(sn_outbound);
        segnode_in[1]
            -> for_each(|(m, a): (SKRequest, SocketAddr)| println!("{}: SN {:?} from {:?}", Utc::now(), m, a));

        segnode_demux = segnode_in[0]
            //->  demux(|(msg, addr), var_args!(heartbeat, errs)|
            ->  demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, blocks, } => heartbeat.give((id, blocks, addr, hydroflow::lattices::Max::new(Utc::now()))),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );
        heartbeats = segnode_demux[heartbeat] -> tee();
        sn_last_contact = heartbeats[0]
          -> map(|(id, _, addr, last_contact)| (id, (addr, last_contact)))
          -> tee();

        sn_last_contact[0]
          -> for_each(|(id, (addr, last_contact))| println!("{}: LC {:?} at {:?} from {:?}", Utc::now(), id, last_contact, addr));

        sn_last_contact[1]
            -> map(|(_, (addr, _))| (SKResponse::Heartbeat { }, addr))
            -> segnode_out;

        // block -> Vec<SegmentNodeID>
        block_map = heartbeats[1]
          -> flat_map(|(id, blocks, _, _): (SegmentNodeID, Vec<Block>, _, _)| blocks.into_iter().map(move |block| (block, id.clone())))
          -> fold_keyed(HashSet::new, |acc: &mut HashSet<SegmentNodeID>, id| {
                acc.insert(id);
                acc.to_owned()
            })
          -> tee();

        block_map[1] -> null();

        // DBG
        block_map[0]
            -> for_each(|(block, id): (Block, HashSet<SegmentNodeID>)| println!("{}: Blockmap: {:?} -> {:?}", Utc::now(), block, id));

        //segnode_demux[register] // todo
        //    -> for_each(|(msg, addr)| println!("Received register message type: {:?} from {:?}", msg, addr));

        // DBG as if SKRequest::Register already occurred
        //source_iter([((sn_id, vec![
        //    Block { pool: "2023874_0".to_owned(), id: 2348980u64 },
        //    Block { pool: "2023874_0".to_owned(), id: 2348985u64 },
        //]), hydroflow::lattices::Max::new(true))]) -> [1]sn_last_contact;

        //sn_last_contact = lattice_join::<'static, 'tick, hydroflow::lattices::Max<chrono::DateTime<Utc>>, hydroflow::lattices::Max<bool>>();

        //sn_last_contact
        //    -> for_each(|((id, blocks), last_contact)| println!("{}: Got {:?} {:?} from {:?}", Utc::now(), id, blocks, last_contact));

        segnode_demux[errs]
            -> for_each(|(msg, addr)| println!("Unexpected SN message type: {:?} from {:?}", msg, addr));

        //// heartbeat handling
        //hb_response = lattice_join<'static, 'tick, hydroflow::lattices::Max<(_, u64)>, hydroflow::lattices::Max<(_, u64)>>() -> 
        //heartbeats[0] -> [1]hb_response
        ////sn_last_contact
        ////    -> filter_map(|(, addr)| (lease, addr, Utc::now()) )
    };

    if let Some(graph) = opts.graph {
        print_graph(&flow, graph);
    }

    // run the server
    flow.run_async().await;
}
