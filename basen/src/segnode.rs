use chrono::prelude::*;
use crate::helpers::print_graph;
use hydroflow::hydroflow_syntax;
use hydroflow::scheduled::graph::Hydroflow;
use hydroflow::util::{bind_udp_bytes, ipv4_resolve, UdpSink, UdpStream};
use std::net::SocketAddr;
use crate::protocol::*;

pub(crate) async fn run_segnode(outbound: UdpSink, inbound: UdpStream, opts: crate::Opts) {
}