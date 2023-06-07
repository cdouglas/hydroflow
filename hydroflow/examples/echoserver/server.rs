use std::net::SocketAddr;

use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{UdpSink, UdpStream};

use crate::protocol::EchoMsg;

pub(crate) async fn run_server(outbound: UdpSink, inbound: UdpStream, _opts: crate::Opts) {
    println!("Server live!");

    let mut flow: Hydroflow = hydroflow_syntax! {
        // Define a shared inbound channel
        inbound_chan = source_stream_serde(inbound) -> map(Result::unwrap) -> tee();

        ready = inbound_chan[1] -> filter(|(t,_) : &(EchoMsg, _)| t.payload.starts_with("ready"))
                                -> map(|(e, _) : (EchoMsg, _)| e.payload)
                                -> unique::<'static>();

        // Print all messages for debugging purposes
        inbound_chan[0] -> [0]now_ready;
        ready -> [1]now_ready;

        inbound_chan[2]-> for_each(|(msg, addr): (EchoMsg, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), msg, addr));

        // Echo back the Echo messages with updated timestamp
        // ready across ticks, not between ticks
        now_ready = cross_join::<'tick, 'static>() -> map(|(x, _)| x)
            -> map(|(EchoMsg {payload, ..}, addr)| (EchoMsg { payload, ts: Utc::now() }, addr) ) -> dest_sink_serde(outbound);
    };

    // run the server
    flow.run_async().await;
}
