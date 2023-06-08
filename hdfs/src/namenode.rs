use crate::helpers::{print_graph, deserialize_json, serialize_json};
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

    // OK finish writing a sample log
    // read it back as json
    // use one of the operators (lattice_batch?) to trigger processing client intput AFTER loading the log
    // copy KVS code to implement the namespace lookup, return a dummy set of DN
    
    // THEN
    // include DN registration and last-heartbeat tracking
    // filter response based on last timestamp from DN, rather than removing DNs
    // include background replication thread that periodically checks for missing DNs
    write_tmp_log();

    // TODO: should this be organized as multiple flows? As a single flow?
    let mut ns_log = hydroflow_syntax! {
        // hack to buffer requests until the server is ready
        // TODO: can one distinguish between a full batch or a signal? Release requests after a timeout to a busy response?
        //client_inbound = source_stream_serde(inbound) -> map(Result::unwrap) -> batch(100, cli_ready_rx) -> tee();
        init = source_file("ops.log") // TODO collect source files by pattern (start_end.log, start_.log), join consecutively
            -> map(|op| serde_json::from_str::<NSOperation>(&op).unwrap())
            -> enumerate()            // TODO: take start from source LSN
            -> tee();
            
        init[0] -> for_each(|(lsn, op)| println!("@{:?} op: {:?}", lsn, op));
        logops = init[1] -> demux(|(lsn, op), var_args!(nsop, blkop, err)| match op {
            NSOperation::CREATE {..} => nsop.give((lsn, op)),
            NSOperation::ADDBLOCK {..} => blkop.give((lsn, op)),
            NSOperation::SEALBLOCK {..} => blkop.give((lsn, op)),
            NSOperation::COMMIT {..} => nsop.give((lsn, op)),
            NSOperation::EOF {..} => panic!("EOF in log"),
            _ => err.give((lsn, op)),
        });

        logops[nsop]  -> for_each(|(lsn, op)| println!("@{:?} nsop: {:?}", lsn, op));
        logops[blkop] -> for_each(|(lsn, op)| println!("@{:?} blkop: {:?}", lsn, op));
        logops[err]   -> for_each(|(lsn, op)| println!("@{:?} err: {:?}", lsn, op));
    };
    let chkpt_load = ns_log.run_async();

    // idea: client should create and add blocks under a lease, DNs should seal as they're durable
    // a client read should find the longest prefix of blocks that are sealed, and return the locations

    // DELETE THIS
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
            NSOperation::ADDBLOCK { lease, id: offset, .. } => {
                let key = leases.get(&lease.id).unwrap();
                let fileinfo = ns.get_mut(key).unwrap();
                fileinfo.leases.insert(lease.id);
            }
            NSOperation::SEALBLOCK { lease, blkid, len } => {
                let file = leases.get(&lease.id).unwrap();
                let fileinfo = ns.get_mut(file).unwrap();
                fileinfo.leases.remove(&lease.id);
            }
            // TODO update
            _ => panic!("Unexpected op: {:?}", op)
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
        inbound_chan = source_stream_serde(inbound) -> map(Result::unwrap) -> tee();
        outbound_chan = merge() -> dest_sink_serde(outbound);

        in_dn_demux = source_stream_serde(dn_inbound) -> map(Result::unwrap) -> demux(|(msg, addr), var_args!(liveness, leaseops, errs)|
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

fn write_tmp_log() {
    let dingos_yaks = Uuid::new_v4();
    let mut dbg_init = hydroflow_syntax!{
        source_iter(vec![
            NSOperation::CREATE {
                key: "/dingos/yaks".to_string(),
                replication: 3u8,
                lease: Lease {
                    id: dingos_yaks,
                },
            },
            //NSOperation::CREATE {
            //    key: "/qvatbf/yaks".to_string(),
            //    replication: 3u8,
            //    lease: Lease {
            //        id: Uuid::new_v4(),
            //    },
            //},
            NSOperation::ADDBLOCK {
                lease: Lease {
                    id: dingos_yaks,
                },
                blkid: Block {
                    id: 234678u64,
                    stamp: 0u64,
                },
                id: 0,
            },
            NSOperation::CREATE {
                key: "/qvatbf/lnxf".to_string(),
                replication: 3u8,
                lease: Lease {
                    id: Uuid::new_v4(),
                },
            },
            NSOperation::ADDBLOCK {
                lease: Lease {
                    id: dingos_yaks,
                },
                blkid: Block {
                    id: 373894u64,
                    stamp: 0u64,
                },
                id: 1,
            },
            NSOperation::CREATE {
                key: "/dingos/lnxf".to_string(),
                replication: 3u8,
                lease: Lease {
                    id: Uuid::new_v4(),
                },
            },
            NSOperation::SEALBLOCK {
                lease: Lease {
                    id: dingos_yaks,
                },
                blkid: Block {
                    id: 234678u64,
                    stamp: 0u64,
                },
                len: 256 * 1024 * 1024,
            },
            ])
          -> map(serialize_json)
          -> dest_file("ops.log", false);
    };
    dbg_init.run_available();

}
