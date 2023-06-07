use crate::GraphType;
use hydroflow::scheduled::graph::Hydroflow;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use tokio_util::codec::LinesCodecError;

pub fn print_graph(flow: &Hydroflow, graph: GraphType) {
    let serde_graph = flow
        .meta_graph()
        .expect("No graph found, maybe failed to parse.");
    match graph {
        GraphType::Mermaid => {
            println!("{}", serde_graph.to_mermaid());
        }
        GraphType::Dot => {
            println!("{}", serde_graph.to_dot())
        }
    }
}

// shamelessly copied from echo_json
pub fn serialize_json<T>(msg: T) -> String
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    json!(msg).to_string()
}

pub fn deserialize_json<T>(msg: Result<(String, SocketAddr), LinesCodecError>) -> (T, SocketAddr)
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    let (m, a) = msg.unwrap();
    (serde_json::from_str(&(m)).unwrap(), a)
}
