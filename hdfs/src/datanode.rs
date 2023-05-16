use crate::helpers::print_graph;
use crate::protocol::Message;
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{bind_udp_bytes, ipv4_resolve, UdpSink, UdpStream};
use std::net::SocketAddr;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;

pub(crate) async fn run_server(outbound: UdpSink, inbound: UdpStream, opts: crate::Opts) {
    println!("Server live!");

    let (nn_outbound, nn_inbound, nn_addr) =
        bind_udp_bytes(ipv4_resolve("localhost:4345").unwrap()).await;

    let hb_stream = IntervalStream::new(time::interval(time::Duration::from_secs(10)));
    let mut nnflow: Hydroflow = hydroflow_syntax! {
        source_stream(hb_stream) -> map(|_| (Message::Heartbeat, nn_addr)) -> dest_sink_serde(nn_outbound);
    };

    let mut clientflow: Hydroflow = hydroflow_syntax! {
        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(inbound) -> map(Result::unwrap) -> tee();
        outbound_chan = merge() -> dest_sink_serde(outbound);

        // Print all messages for debugging purposes
        inbound_chan[1]
            -> for_each(|(m, a): (Message, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        // Demux and destructure the inbound messages into separate streams
        inbound_demuxed = inbound_chan[0]
            ->  demux(|(msg, addr), var_args!(echo, heartbeat, errs)|
                    match msg {
                        _ => errs.give((msg, addr)),
                    }
                );

        // Echo back the Echo messages with updated timestamp
        inbound_demuxed[echo]
            -> map(|(payload, addr)| (Message::Echo { payload, ts: Utc::now() }, addr) ) -> [0]outbound_chan;

        // Respond to Heartbeat messages
        inbound_demuxed[heartbeat] -> map(|addr| (Message::HeartbeatAck, addr)) -> [2]outbound_chan;

        // Print unexpected messages
        inbound_demuxed[errs]
            -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));

        // timeouts
    };

    if let Some(graph) = opts.graph {
        //print_graph(&nnflow, graph);
        print_graph(&clientflow, graph);
    }

    // run the server
    // TODO how to run the nnflow with the clientflow?
    clientflow.run_async().await;
}
