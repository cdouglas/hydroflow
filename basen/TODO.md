# Questions for Joe

# Questions for Lucky

1. hb_report: in fold_keyed, wtf is going on references? Why did this differ from the example: https://hydro.run/docs/hydroflow/syntax/surface_ops_gen#fold_keyed ?
2.  The following duplicates and repeats the `canned_blocks` map for each HB. Why?
        hb_timer = source_stream(hb_stream)
            -> map(|_| (kn_addr, ()))
            -> [0]hb_report;
        canned_blocks
            -> map(|b| (kn_addr, b))
            // -> multiset_delta()          // `!#!` no effect
            -> [1]hb_report;
        hb_report = join::<'tick,'static>() // `!#!` changing this to <'static,'static> has no effect? <'static,'tick> and <'tick,'tick> emits nothing?
            // -> multiset_delta()          // `!#!` no duplicates, but still reports all blocks, not new blocks
            -> fold_keyed(Vec::new,
                |acc: &mut Vec<Block>, (_, blk): ((), Block)| {
                    acc.push(blk);
                    acc.to_owned()          // `!#!` why is .to_owned() necessary?
                })
            -> map(|(addr, blk): (SocketAddr, Vec<Block>)| (SKRequest::Heartbeat {id: sn_id.clone(), blocks: blk }, addr))
            -> tee();
3. Currently going crazy instead of triggering a response

#

Versioned lattice type: how to delete things
- override w/ up-to-date version
- not a tombstone; uses a VC/node... 

bugfix: some persisted things can spin, rebase to ensure it's not that


# Random BASE'n

RPC:
- framing
- add headers to messages s.t. appropriate context and tokens are retrieved
- flow control + backpressure?

Do SegmentNodes handle a single device/spindle? Or multiple?
- per fault tolerance discussion, probably small w/ separate aggregate type

When a SN removes/loses blocks... needs to change collections? Or if each transducer is a device, all/nothing?