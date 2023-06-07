use crate::helpers::print_graph;
use crate::protocol::Message;
use crate::Opts;
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::util::{UdpSink, UdpStream};
use std::net::SocketAddr;
use tokio::time;
use tokio::time::Duration;
use tokio_stream::wrappers::IntervalStream;

pub(crate) async fn run_client(outbound: UdpSink, inbound: UdpStream, opts: Opts) {
    // server_addr is required for client
    let server_addr = match opts.server_addr {
        Some(addr) => {
            println!("Connecting to server at {:?}", addr);
            addr
        }
        None => panic!("Client requires a server address"),
    };
    println!("Client live!");

    let in_stream = IntervalStream::new(time::interval(Duration::from_secs(1)));
    let mut i = 0;

    let mut flow = hydroflow_syntax!{};

    //let mut flow = hydroflow_syntax! {
    //    // Define shared inbound and outbound channels
    //    inbound_chan = source_stream_serde(inbound)
    //        // -> tee() // commented out since we only use this once in the client template
    //    ;
    //    outbound_chan = // merge() ->  // commented out since we only use this once in the client template
    //        dest_sink_serde(outbound);

    //    // Print all messages for debugging purposes
    //    inbound_chan[1] // TODO this is broken
    //        -> for_each(|(m, a): (Message, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

    //    // take stdin and send to server as an Message::Echo
    //    source_stream(in_stream) -> map(|_| {
    //        i += 1;
    //        let msg: String = format!("blah{}", i);
    //        (Message::Echo{ payload: msg, ts: Utc::now(), }, server_addr)
    //    }) -> outbound_chan;
    //};

    if let Some(graph) = opts.graph {
        print_graph(&flow, graph);
    }

    flow.run_async().await.unwrap();
}