use chrono::prelude::*;
use crate::helpers::print_graph;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{bind_udp_bytes, ipv4_resolve, UdpSink, UdpStream};
use std::net::SocketAddr;
use crate::protocol::*;

pub(crate) async fn run_keynode(outbound: UdpSink, inbound: UdpStream, opts: crate::Opts) {

    // open separate channel for SN traffic
    let (sn_outbound, sn_inbound, sn_addr) =
        bind_udp_bytes(ipv4_resolve("localhost:4345").unwrap()).await;

    let mut flow: Hydroflow = hydroflow_syntax! {
        //
        // client
        //
        client_in = source_stream_serde(inbound) -> map(|udp_msg| udp_msg.unwrap()) -> tee();
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
        //segnode_in = source_stream_serde(sn_inbound) -> map(|udp_msg| udp_msg.unwrap()) -> tee();
        segnode_in = source_iter([SKRequest::Heartbeat {id: SegmentNodeID { id: 1234u64 }, blocks: vec![]}]); // -> tee();
        //segnode_out = union() -> dest_sink_serde(sn_outbound);
        //segnode_out = dest_sink_serde(sn_outbound);
        //segnode_in[1]
        //    -> for_each(|(m, a): (SKRequest, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        segnode_demux = segnode_in
            //->  demux(|(msg, addr), var_args!(heartbeat, errs)|
            ->  demux(|msg, var_args!(heartbeat, errs)|
                    match msg {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat {id, ..} => heartbeat.give((id.id, hydroflow::lattices::Max::new(Utc::now()))),
                        //_ => errs.give((msg, addr)),
                        _ => errs.give(msg),
                    }
                );
        heartbeats = segnode_demux[heartbeat];

        segnode_demux[errs] -> null();

        sn_last_contact = lattice_join::<'static, 'tick, hydroflow::lattices::Max<chrono::DateTime<Utc>>, hydroflow::lattices::Max<bool>>();
        heartbeats -> [0]sn_last_contact;
        source_iter([(1234u64, hydroflow::lattices::Max::new(true))]) -> [1]sn_last_contact;

        sn_last_contact
            -> for_each(|(id, (last_contact, is_alive))| println!("{}: Got {:?} from {:?}", Utc::now(), id, last_contact));
        //segnode_demux[register] // todo
        //    -> for_each(|(msg, addr)| println!("Received register message type: {:?} from {:?}", msg, addr));

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
