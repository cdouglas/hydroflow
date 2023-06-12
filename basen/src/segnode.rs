use chrono::prelude::*;
use uuid::Uuid;
use crate::helpers::print_graph;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{connect_tcp_bytes, ipv4_resolve, UdpSink, UdpStream};
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use std::net::SocketAddr;
use crate::protocol::*;

pub(crate) async fn run_segnode(_cl_outbound: UdpSink, cl_inbound: UdpStream, opts: crate::Opts) {

    //let (kn_outbound, _kn_inbound, kn_addr) =
    //    bind_udp_bytes(ipv4_resolve("localhost:4345").unwrap()).await;
    let (kn_outbound, kn_inbound) = connect_tcp_bytes();
    //let (input_send, input_recv) = hydroflow::util::unbounded_channel::<u32>();

    let sn_id = Uuid::parse_str("454147e2-ef1c-4a2f-bcbc-a9a774a4bb62").unwrap();
    let sn_id = SegmentNodeID { id: sn_id, }; //Uuid::new_v4(), };
    let kn_addr = ipv4_resolve("localhost:4345").unwrap();

    let hb_stream = IntervalStream::new(time::interval(time::Duration::from_secs(1)));
    let mut flow: Hydroflow = hydroflow_syntax! {
        canned_blocks = source_iter(vec![
            Block { pool: "2023874_0".to_owned(), id: 2348980u64 },
            Block { pool: "2023874_0".to_owned(), id: 2348985u64 },
        ]);
        // each hb, should include all the blocks not-yet reported in the KN epoch
        // join w/ KN pool to determine to which KN the blocks should be reported

        hb_timer = source_stream(hb_stream)
            -> map(|_| (kn_addr, ()))
            -> [0]hb_report;
        canned_blocks
            -> map(|b| (kn_addr, b))
            -> [1]hb_report;
        hb_report = join::<'tick,'static>()
            -> multiset_delta()
            -> fold_keyed(Vec::new,
                |acc: &mut Vec<Block>, (_, blk): ((), Block)| {
                    acc.push(blk);
                    acc.to_owned()
                })
            -> map(|(addr, blk): (SocketAddr, Vec<Block>)| (SKRequest::Heartbeat {id: sn_id.clone(), blocks: blk }, addr))
            -> tee();
        // DBG
        hb_report[0]
            -> for_each(|(m, a): (SKRequest, SocketAddr)| println!("{}: HB {:?} to {:?}", Utc::now(), m, a));
        hb_report[1] -> sk_chan;

        kn_demux = source_stream_serde(kn_inbound)
                -> map(Result::unwrap)
                -> demux(|(kn_resp, addr), var_args!(heartbeat, errs)|
                    match kn_resp {
                        SKResponse::Heartbeat { } => heartbeat.give(addr),
                        _ => errs.give((kn_resp, addr)),
                    }
                );
        kn_demux[heartbeat]
          -> for_each(|a: SocketAddr| println!("{}: HB response from {:?}", Utc::now(), a));
        kn_demux[errs] -> null();

        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(cl_inbound) -> map(|udp_msg| udp_msg.unwrap()) /* -> tee() */; // commented out since we only use this once in the client template

        sk_chan = // union()
            dest_sink_serde(kn_outbound);

        // Print all messages for debugging purposes
        inbound_chan[1]
            -> for_each(|(m, a): (SKResponse, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        //// take stdin and send to server as an Message::Echo
        //source_stdin() -> map(|l| (Message::Echo{ payload: l.unwrap(), ts: Utc::now(), }, server_addr) )
        //    -> outbound_chan;
    };

    if let Some(graph) = opts.graph {
        print_graph(&flow, graph);
    }

    flow.run_async().await;
}