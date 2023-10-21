use crate::helpers::print_graph;
use crate::Opts;
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::util::connect_tcp_bytes;
use uuid::Uuid;
use std::net::SocketAddr;
use crate::protocol::*;

#[allow(dead_code)]
pub(crate) async fn run_client(keynode_client_addr: SocketAddr, opts: Opts) {
    println!("Connecting to {:?}", keynode_client_addr);

    let (outbound, inbound) = connect_tcp_bytes();
    let client_id = ClientID { id: Uuid::new_v4() };

    let mut flow = hydroflow_syntax! {
        outbound_chan = // union() ->  // commented out since we only use this once in the client template
            dest_sink_serde(outbound);

        kn_inbound = source_stream_serde(inbound)
            -> map(|udp_msg| udp_msg.unwrap())
            -> demux(|(kn_resp, addr), var_args!(info, errs)|
                match kn_resp {
                    CKResponse::Info{ key, blocks } => info.give((key, blocks, addr)),
                    _ => errs.give((kn_resp, addr)),
                }
            );

        kn_inbound[info]
            -> inspect(|(k, _, _): &(String, Vec<LocatedBlock>, SocketAddr)| println!("{}: INFO {:?}", Utc::now(), k))
            -> flat_map(|(_, b, _): (String, Vec<LocatedBlock>, SocketAddr)| b.into_iter().map(move |lb| lb))
            -> for_each(|b: LocatedBlock| println!("{}:      {:?} {:?}", Utc::now(), b.block.id, b.locations));

        kn_inbound[errs]
            -> for_each(|(m, a): (CKResponse, SocketAddr)| println!("{}: {:?} ERR {:?}", Utc::now(), a, m));
            

        // every line a request to open(line). FFS
        source_stdin()
            -> map(|key| (CKRequest::Info{ id: client_id.clone(), key: key.unwrap() }, keynode_client_addr) )
            -> outbound_chan;
    };

    if let Some(graph) = &opts.graph {
        print_graph(&flow, graph);
    }

    flow.run_async().await.unwrap();
}
