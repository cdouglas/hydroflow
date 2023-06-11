use crate::helpers::print_graph;
use crate::Opts;
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::util::{UdpSink, UdpStream};
use uuid::Uuid;
use std::net::SocketAddr;
use crate::protocol::*;

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

    let client_id = ClientID { id: Uuid::new_v4() };
    let mut flow = hydroflow_syntax! {
        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(inbound) -> map(|udp_msg| udp_msg.unwrap()) /* -> tee() */; // commented out since we only use this once in the client template

        outbound_chan = // union() ->  // commented out since we only use this once in the client template
            dest_sink_serde(outbound);

        // Print all messages for debugging purposes
        inbound_chan[1]
            -> for_each(|(m, a): (CKResponse, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        // take stdin and send to server as an Message::Echo
        source_stdin()
            -> map(|key| (CKRequest::Info{ id: client_id.clone(), key: key.unwrap() }, server_addr) )
            -> outbound_chan;
    };

    if let Some(graph) = opts.graph {
        print_graph(&flow, graph);
    }

    flow.run_async().await.unwrap();
}
