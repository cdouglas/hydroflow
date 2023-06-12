use chrono::Utc;
use clap::{Parser, ValueEnum};
use client::run_client;
use hydroflow::{tokio, hydroflow_syntax};
use hydroflow::util::{bind_udp_bytes, ipv4_resolve, connect_tcp_bytes, bind_tcp_bytes};
use keynode::run_keynode;
use segnode::run_segnode;
use tokio::time;
use tokio_stream::wrappers::IntervalStream;
use uuid::Uuid;
use std::collections::HashSet;
use std::net::SocketAddr;
use crate::protocol::*;

mod client;
mod helpers;
mod protocol;
mod keynode;
mod segnode;

#[derive(Clone, ValueEnum, Debug)]
enum Role {
    Keynode,
    Segnode,
    Client,
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

#[hydroflow::main]
async fn main() {
    const KEYNODE_SERVER_ADDR: &str = "127.0.0.55:4345";

    futures::join!(segment_node(KEYNODE_SERVER_ADDR), key_node(KEYNODE_SERVER_ADDR));
}

async fn key_node(keynode_server_addr: &'static str) {
    let (sn_outbound, sn_inbound, _sn_addr) = bind_tcp_bytes(ipv4_resolve(keynode_server_addr).unwrap()).await;

    let mut df = hydroflow_syntax! {
        segnode_in = source_stream_serde(sn_inbound) -> map(|m| m.unwrap()) -> tee();
        //segnode_out = union() -> dest_sink_serde(sn_outbound);
        segnode_out = dest_sink_serde(sn_outbound);
        segnode_in[1]
            -> for_each(|(m, a): (SKRequest, SocketAddr)| println!("{}: SN {:?} from {:?}", Utc::now(), m, a));

        segnode_demux = segnode_in[0]
            -> demux(|(sn_req, addr), var_args!(heartbeat, errs)|
                    match sn_req {
                        //SKRequest::Register {id, ..}    => register.give((key, addr)),
                        SKRequest::Heartbeat { id, blocks, } => heartbeat.give((id, blocks, addr, hydroflow::lattices::Max::new(Utc::now()))),
                        //_ => errs.give((sn_req, addr)),
                        _ => errs.give((sn_req, addr)),
                    }
                );
        heartbeats = segnode_demux[heartbeat] -> tee();
        sn_last_contact = heartbeats[0]
          -> map(|(id, _, addr, last_contact)| (id, (addr, last_contact)))
          -> tee();

        sn_last_contact[0]
          -> for_each(|(id, (addr, last_contact))| println!("{}: LC {:?} at {:?} from {:?}", Utc::now(), id, last_contact, addr));

        sn_last_contact[1]
            -> map(|(_, (addr, _))| (SKResponse::Heartbeat { }, addr))
            -> segnode_out;

        // block -> Vec<SegmentNodeID>
        block_map = heartbeats[1]
          -> flat_map(|(id, blocks, _, _): (SegmentNodeID, Vec<Block>, _, _)| blocks.into_iter().map(move |block| (block, id.clone())))
          -> fold_keyed(HashSet::new, |acc: &mut HashSet<SegmentNodeID>, id| {
                acc.insert(id);
                acc.to_owned()
            })
          -> tee();

        segnode_demux[errs]
          -> for_each(|(msg, addr)| println!("Unexpected SN message type: {:?} from {:?}", msg, addr));

        block_map[1] -> null();
    };

    df.run_async().await.unwrap();
}

async fn segment_node(keynode_server_addr: &'static str) {
    let (kn_outbound, kn_inbound) = connect_tcp_bytes();
    
    let sn_id = Uuid::parse_str("454147e2-ef1c-4a2f-bcbc-a9a774a4bb62").unwrap();
    let sn_id = SegmentNodeID { id: sn_id, }; //Uuid::new_v4(), };
    let kn_addr: SocketAddr = ipv4_resolve(keynode_server_addr).unwrap();

    let hb_interval_stream = IntervalStream::new(time::interval(time::Duration::from_secs(1)));
    let mut flow = hydroflow_syntax! {
        canned_blocks = source_iter(vec![
            (kn_addr, Block { pool: "2023874_0".to_owned(), id: 2348980u64 }),
            (kn_addr, Block { pool: "2023874_0".to_owned(), id: 2348985u64 }),
        ]);
        // each hb, should include all the blocks not-yet reported in the KN epoch
        // join w/ KN pool to determine to which KN the blocks should be reported

        hb_timer = source_stream(hb_interval_stream)
            -> map(|_| (kn_addr, ()))
            -> [0]hb_report;
        canned_blocks
            -> [1]hb_report;
        hb_report = join::<'tick, 'static>()
            -> fold_keyed::<'tick>(Vec::new,
                |acc: &mut Vec<Block>, (_, blk): ((), Block)| {
                    acc.push(blk);
                    //acc.to_owned() // !#! not necessary for _keyed operators; not moved into the fold closure
                })
            -> map(|(addr, blk): (SocketAddr, Vec<Block>)| (SKRequest::Heartbeat {id: sn_id.clone(), blocks: blk }, addr))
            -> inspect(|(m, a)| println!("{}: HB {:?} to {:?}", Utc::now(), m, a))
            -> dest_sink_serde(kn_outbound);

        kn_demux = source_stream_serde(kn_inbound)
            -> map(Result::unwrap)
            -> demux(|(kn_resp, addr), var_args!(heartbeat, errs)|
                match kn_resp {
                    SKResponse::Heartbeat { /* other things */ } => heartbeat.give(addr),
                    _ => errs.give(()),
                }
            );

        kn_demux[heartbeat]
          -> for_each(|addr| println!("{}: HB response from {:?}", Utc::now(), addr));

        kn_demux[errs]
            -> null();




        // // Define shared inbound and outbound channels
        // inbound_chan = source_stream_serde(cl_inbound)
        //     -> map(|udp_msg| udp_msg.unwrap()) /* -> tee() */; // commented out since we only use this once in the client template

        // // Print all messages for debugging purposes
        // inbound_chan[1]
        //     -> for_each(|(m, a): (SKResponse, SocketAddr)| println!("{}: Got {:?} from {:?}", Utc::now(), m, a));

        //// take stdin and send to server as an Message::Echo
        //source_stdin() -> map(|l| (Message::Echo{ payload: l.unwrap(), ts: Utc::now(), }, server_addr) )
        //    -> outbound_chan;
    };

    flow.run_async().await;
}