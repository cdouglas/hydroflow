use clap::{Parser, ValueEnum};
use client::run_client;
use hydroflow::tokio;
use hydroflow::util::{bind_udp_bytes, ipv4_resolve};
use std::net::SocketAddr;

mod client;
mod datanode;
mod helpers;
mod logger;
mod namenode;
mod protocol;

#[derive(Clone, ValueEnum, Debug)]
enum Role {
    Client,
    NameNode,
    DataNode,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum GraphType {
    Mermaid,
    Dot,
}

#[derive(Parser, Debug)]
struct Opts {
    #[clap(value_enum, long)]
    role: Role,
    // #[clap(long)]
    #[clap(long, value_parser = ipv4_resolve)]
    addr: Option<SocketAddr>,
    // #[clap(long)]
    #[clap(long, value_parser = ipv4_resolve)]
    server_addr: Option<SocketAddr>,
    #[clap(value_enum, long)]
    graph: Option<GraphType>,
}

#[tokio::main]
async fn main() {
    // parse command line arguments
    let opts = Opts::parse();
    // if no addr was provided, we ask the OS to assign a local port by passing in "localhost:0"
    let addr = opts
        .addr
        .unwrap_or_else(|| ipv4_resolve("localhost:0").unwrap());

    // allocate `outbound` sink and `inbound` stream
    let (outbound, inbound, addr) = bind_udp_bytes(addr).await;
    println!("Listening on {:?}", addr);

    match opts.role {
        Role::NameNode => {
            // bind DN port TODO: from config
            let (dn_outbound, dn_inbound, dn_addr) = bind_udp_bytes(addr).await;
            namenode::run_server(outbound, inbound, opts).await;
        }
        Role::DataNode => {
            datanode::run_server(outbound, inbound, opts).await;
        }
        Role::Client => {
            run_client(outbound, inbound, opts).await;
        }
    }
}
