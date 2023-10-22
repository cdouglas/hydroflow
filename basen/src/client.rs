use crate::helpers::print_graph;
use crate::Opts;
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::util::connect_tcp_bytes;
use uuid::Uuid;
use std::net::SocketAddr;
use crate::protocol::*;

#[allow(dead_code)]
pub(crate) async fn run_client(ck_addr: SocketAddr, opts: Opts) {
    println!("Connecting to {:?}", ck_addr);

    let (outbound, inbound) = connect_tcp_bytes();
    let client_id = ClientID { id: Uuid::new_v4() };

    let mut flow = hydroflow_syntax! {
        outbound_chan = union()
            -> dest_sink_serde(outbound);

        kn_inbound = source_stream_serde(inbound)
            -> map(|udp_msg| udp_msg.unwrap())
            -> demux(|(kn_resp, addr), var_args!(create, info, errs)|
                match kn_resp {
                    CKResponse::Info{ key, blocks } => info.give((key, blocks, addr)),
                    CKResponse::Create { klease } => create.give((klease, addr)),
                    _ => errs.give((kn_resp, addr)),
                }
            );

        kn_inbound[info]
            -> inspect(|(k, _, _): &(String, Vec<LocatedBlock>, SocketAddr)| println!("{}: INFO {:?}", Utc::now(), k))
            -> flat_map(|(_, b, _): (String, Vec<LocatedBlock>, SocketAddr)| b.into_iter().map(move |lb| lb))
            -> for_each(|b: LocatedBlock| println!("{}:      {:?} {:?}", Utc::now(), b.block.id, b.locations));

        kn_inbound[create]
            -> for_each(|(kl, a): (KeyLease, SocketAddr)| println!("{}: {:?} CREATE {:?}", Utc::now(), a, kl));

        kn_inbound[errs]
            -> for_each(|(m, a): (CKResponse, SocketAddr)| println!("{}: {:?} ERR {:?}", Utc::now(), a, m));
            

        // TODO awful.
        repl = source_stdin()
            -> map(Result::unwrap)
            -> map(move |line| line.trim().split(' ').map(str::to_owned).collect())
            // filter out empty lines?
            -> demux(|argv: Vec<String>, var_args!(create, info, errs)|
                match argv[0].to_lowercase().as_str() {
                    "create" => create.give(argv[1].to_owned()),
                    "info" => info.give(argv[1].to_owned()),
                    _ => errs.give(()),
                }
            );

        repl[create]
            -> map(|key: String| (CKRequest::Create{ id: client_id.clone(), key }, ck_addr) )
            -> outbound_chan;

        //repl[addblock]
        //    -> map(|key: String| (CKRequest::AddBlock{ id: client_id.clone(), key }, ck_addr) )
        //    -> outbound_chan;

        repl[info]
            //-> map(|argv: &[&str]| (CKRequest::Info{ id: client_id.clone(), key: argv[1].to_string() }, ck_addr) )
            -> map(|key: String| (CKRequest::Info{ id: client_id.clone(), key }, ck_addr) )
            -> outbound_chan;

        repl[errs]
            -> for_each(|_| println!("{}: ERR", Utc::now()));

            //-> map(|key| (CKRequest::Info{ id: client_id.clone(), key: key.unwrap() }, ck_addr) )
            //-> outbound_chan;
    };

    if let Some(graph) = &opts.graph {
        print_graph(&flow, graph);
    }

    flow.run_async().await.unwrap();
}
