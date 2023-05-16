use crate::helpers::print_graph;
use crate::logger::{Logger, NSOperation};
use crate::protocol::{Block, DNRequest, Lease, NSRequest, NSResponse};
use chrono::prelude::*;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{bind_udp_bytes, ipv4_resolve, UdpSink, UdpStream};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use uuid::Uuid;

struct FileInfo {
    blocks: Vec<Block>,
    block_locations: HashMap<Block, Vec<SocketAddr>>,
    leases: HashSet<Uuid>,
    replication: u8,
    len: u64,
}

pub(crate) async fn run_server(outbound: UdpSink, inbound: UdpStream, opts: crate::Opts) {
    println!("Server live!");

    let mut oplog = Logger::new("todo_id_from_cfg");
    let mut ns: BTreeMap<String, FileInfo> = BTreeMap::new();
    let mut leases: HashMap<Uuid, String> = HashMap::new();
    while let Some(op) = oplog.next() {
        match op {
            NSOperation::CREATE {
                key,
                replication,
                lease,
            } => {
                leases.insert(lease.id, key.to_string());
                let mut fileinfo = FileInfo {
                    blocks: Vec::new(),
                    block_locations: HashMap::new(),
                    leases: HashSet::new(),
                    replication,
                    len: 0u64,
                };
                fileinfo.leases.insert(lease.id);
                if let Some(_x) = ns.insert(key.to_string(), fileinfo) {
                    panic!("Key already exists: {:?}", &key);
                }
            }
            NSOperation::ADDBLOCK { lease, offset, .. } => {
                let key = leases.get(&lease.id).unwrap();
                let fileinfo = ns.get_mut(key).unwrap();
                fileinfo.leases.insert(lease.id);
            }
            NSOperation::SEAL_BLOCK { lease, blkid, len } => {
                let file = leases.get(&lease.id).unwrap();
                let fileinfo = ns.get_mut(file).unwrap();
                fileinfo.leases.remove(&lease.id);
            }
        }
    }
    println!("Loaded namespace from log: ({} keys)", ns.len());

    let mut blocks: HashMap<Block, Vec<SocketAddr>> = HashMap::new();
    let (dn_outbound, dn_inbound, dn_addr) =
        bind_udp_bytes(ipv4_resolve("localhost:4345").unwrap()).await;

    // open leases need to be recovered from datanodes/clients reconnecting

    let mut logflow: Hydroflow = hydroflow_syntax! {
        // recv events from the main flow
        // log them
        // ACK the main flow when events are persisted
    };

    let mut flow: Hydroflow = hydroflow_syntax! {
        // Define shared inbound and outbound channels
        inbound_chan = source_stream_serde(inbound) -> tee();
        outbound_chan = merge() -> dest_sink_serde(outbound);

        in_dn_demux = source_stream_serde(dn_inbound) -> demux(|(msg, addr), var_args!(liveness, leaseops, errs)|
                match msg {
                    DNRequest::Heartbeat { id, clientaddr } => liveness.give((id, clientaddr, addr)),
                    DNRequest::BlockReport { blocks } => leaseops.give((blocks, addr)),
                    _ => errs.give((msg, addr)),
                }
            );
        // TODO
        in_dn_demux[liveness] -> for_each(|(id, clientaddr, addr)| println!("Received heartbeat from {:?} @{:?} from {:?}", id, clientaddr, addr));
        in_dn_demux[leaseops] -> for_each(|(blocks, addr)| println!("Received blocks {:?} from {:?}", blocks, addr));
        in_dn_demux[errs] -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));


        // Print all messages for debugging purposes
        inbound_chan[1]
            -> for_each(|(m, a): (NSRequest, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        // Demux and destructure the inbound messages into separate streams
        inbound_demuxed = inbound_chan[0]
            ->  demux(|(msg, addr), var_args!(nsops, leaseops, errs)|
                    match msg {
                        NSRequest::Create {key, replication, ..} => nsops.give((key, replication, addr)),
                        NSRequest::AddBlock { lease, .. } => leaseops.give((lease, addr)),
                        _ => errs.give((msg, addr)),
                    }
                );

        // namespace operation
        inbound_demuxed[nsops]
            -> map(|(key, replication, addr)| (NSResponse::Error { message: "yaks".to_string(), }, addr) ) -> [0]outbound_chan;

        // lease operation
        inbound_demuxed[leaseops]
            -> map(|(lease, addr)| (NSResponse::Error { message: "yaks".to_string(), }, addr) ) -> [1]outbound_chan;

        // Respond to Heartbeat messages
        //inbound_demuxed[heartbeat] -> map(|addr| (Message::HeartbeatAck, addr)) -> [2]outbound_chan;

        // Print unexpected messages
        inbound_demuxed[errs]
            -> for_each(|(msg, addr)| println!("Received unexpected message type: {:?} from {:?}", msg, addr));

        // timeouts
    };

    if let Some(graph) = opts.graph {
        print_graph(&flow, graph);
    }

    // run the server
    flow.run_async().await;
}
