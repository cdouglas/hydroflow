use std::collections::HashMap;
use std::time::Duration;

use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
// completely broken... hope not in a way that matters
//use tokio::sync::mpsc::UnboundedSender;
//use tracing_subscriber::prelude::*;

#[test]
pub fn test_echo() {
    // An edge in the input data = a pair of `usize` vertex IDs.
    let (lines_send, lines_recv) = hydroflow::util::unbounded_channel::<String>();

    //use tokio::io::{AsyncBufReadExt, BufReader};
    // use tokio_stream::wrappers::LinesStream;
    // let stdin_lines = LinesStream::new(BufReader::new(tokio::io::stdin()).lines());
    let stdout_lines = tokio::io::stdout();

    let mut df: Hydroflow = hydroflow_syntax! {
        recv_stream(lines_recv) -> map(|line| line + "\n") -> send_async(stdout_lines);
    };

    println!(
        "{}",
        df.serde_graph()
            .expect("No graph found, maybe failed to parse.")
            .to_mermaid()
    );
    df.run_available();

    lines_send.send("Hello".to_owned()).unwrap();
    lines_send.send("World".to_owned()).unwrap();
    df.run_available();

    lines_send.send("Hello".to_owned()).unwrap();
    lines_send.send("World".to_owned()).unwrap();
    df.run_available();

    // Allow background thread to catch up.
    std::thread::sleep(Duration::from_secs(1));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NodeId {
    name: String,
}

#[derive(Clone, Debug)]
struct NodeStart {
    id: NodeId,
    clock: VC,
    peers: Vec<NodeId>,
}

impl NodeStart {
    fn empty(id: &'static str, peer_names: Vec<&'static str>) -> Self {
        Self::new(id, VC::new(), peer_names)
    }
    fn new(id: &'static str, clock: VC, peer_names: Vec<&'static str>) -> Self {
        NodeStart {
            id: NodeId { name: id.to_string()},
            clock,
            peers: peer_names.into_iter().map(|p| NodeId { name: p.to_string() }).collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct VC {
    clock: HashMap<String, u64>,
}

impl VC {
    pub fn new() -> Self {
        VC {
            clock: HashMap::new()
        }
    }
}

#[derive(Clone, Debug)]
enum Pkt<T> {
    // request catchup beyond <known>
    HELLO {
        src: NodeId,
        dst: NodeId,
        known: VC,
    },
    // 
    BROADCAST {
        src: NodeId,
        seq: VC,
        data: T,
    },
}

#[test]
pub fn book_example() {
    let (pairs_send, pairs_recv) = hydroflow::util::unbounded_channel::<(usize, usize)>();

    let mut flow = hydroflow_syntax! {
        // inputs: the origin vertex (node 0) and stream of input edges
        origin = recv_iter(vec![0]);
        stream_of_edges = recv_stream(pairs_recv);
        reached_vertices = merge();
        origin -> [0]reached_vertices;

        // the join
        my_join_tee = join() -> map(|(_src, ((), dst))| dst) -> tee();
        reached_vertices -> map(|v| (v, ())) -> [0]my_join_tee;
        stream_of_edges -> [1]my_join_tee;

        // the loop and the output
        my_join_tee[0] -> [1]reached_vertices;
        my_join_tee[1] -> for_each(|x| println!("Reached: {}", x));
    };

    println!(
        "{}",
        flow.serde_graph()
            .expect("No graph found, maybe failed to parse.")
            .to_mermaid()
    );
    pairs_send.send((0, 1)).unwrap();
    pairs_send.send((2, 4)).unwrap();
    pairs_send.send((3, 4)).unwrap();
    pairs_send.send((1, 2)).unwrap();
    pairs_send.send((0, 3)).unwrap();
    pairs_send.send((0, 3)).unwrap();
    flow.run_available();
}

#[test]
pub fn test_all_to_all_unif() {
    // channel for adding nodes to the flow
    let (node_send, node_recv) = hydroflow::util::unbounded_channel::<NodeStart>();
    // channel for messages on the network, entering the flow
    let (df_send, df_recv) = hydroflow::util::unbounded_channel::<Pkt<String>>();
    let mut df = hydroflow_syntax! {
        // active nodes
        nodes = merge();

        // stream of nodes into the system
        init = join() -> for_each(|(dstId, (clock, peer))| {
            println!("DEBUG ({:?}, ({:?}, {:?}))", dstId, clock, peer);
        });
        new_node = recv_stream(node_recv) -> tee();
        // add self
        new_node[0] -> map(|n| { println!("{:?} joined", n); (n.id, n.clock) }) -> [0]nodes;
        // join peers against active nodes
        nodes -> [0]init;
        new_node[1] -> flat_map(move |hello| hello.peers.into_iter().map(move |dst| (dst, hello.id.clone())))
                    -> [1]init;
        //init -> for_each(|(dstId, (clock, peer))| {
        //    println!("DEBUG {:?}", x);
        //});
        
        // messages passed between nodes (can also introduce traffic from handle outside)
        //ntwk = recv_stream(df_recv) -> tee();

        //// messages in the system
        //messages = merge();

        //// HELLO
        //hello_msg = join();
        //messages -> [0]hello_msg;
        //ntwk[0] -> filter_map(|h: Pkt<String>| {
        //    match h {
        //        Pkt::HELLO { src, dst, known } => Some(((), (src, known))),
        //        _ => None
        //    }
        //}) -> [1]hello_msg;

        //hello_msg -> for_each(|h| {
        //    println!("HELLO to {:?}", h);
        //});

        // BROADCAST
        //bc_msg = join();
        //ntwk[1] -> filter_map(|h: Pkt<String>| {
        //    match h {
        //        Pkt::BROADCAST { src, seq, data } => Some(((), (src, seq))),
        //        _ => None
        //    }
        //}) -> for_each(|h: ((), (NodeId, VC))| {
        //    println!("HELLO to {:?}", h.1.0);
        //});

    };
    node_send.send(NodeStart::empty("server", vec!["a", "b", "c"])).unwrap();
    node_send.send(NodeStart::empty("a", vec!["server"])).unwrap();
    node_send.send(NodeStart::empty("b", vec!["server"])).unwrap();
    node_send.send(NodeStart::empty("c", vec!["server"])).unwrap();
    df_send.send(Pkt::HELLO {
        src: NodeId { name: "a".to_string(), },
        dst: NodeId { name: "b".to_string(), },
        known: VC { clock: HashMap::new(), }
    }).unwrap();

    df.run_available();
}
