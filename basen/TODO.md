# 2023-09-29

## WTF was I doing?



## WTF am I doing?



# 2023-07-04

Achieve for today: Client lookup and request to segment node

OK... so is the keymap a versioned key to a set of blocks, or a key with a versioned set of blocks?

Will EVERY mapping need to be demux'd by the operation?

Why the unit type?
        `block_map = lattice_join::<'tick, 'static, SetUnionHashSet<()>, BlockSetLattice>()`

How does this merge back into a request at the end of the pipeline?

How do I retain information through this pipeline i.e., the client ID and other information?

Ask Lucky/Mingwei about move and flat_map in block_map thing

# Joe 2023-06-14

## hydro
- worked w/ Lucky on the impl (walk through)
- building the pipeline we discussed last week for client requests
- next week: chain replication

# Lucky 2023-06-13

Point: how to wrap things into Lattice types
type LastContactMapLattice = DomPair<Max<DateTime<Utc>>, Point<SocketAddr>>;
type LastContactMapLattice = DomPair<Max<DateTime<Utc>>, SetUnionHashSet<SocketAddr>>;
- `SUHS`: things w/ the same time will include multiple socket addresses
- looks at LHS first, if change then obliterate RHS; if LHS equal then try to merge RHS
- `Point` crashes

join: implementing set_union lattice, but lattice_join is allowing you to specify exactly the lattice you intend
- lj a generalization of regular join

# Questions for Lucky

1. hb_report: in fold_keyed, wtf is going on references? Why did this differ from the example: https://hydro.run/docs/hydroflow/syntax/surface_ops_gen#fold_keyed ?

keyed + non-keyed version
- keyed/non-keyed version; type of reduce is different (shouldn't be, but is)
- non-keyed: moved, but doesn't work w/ hash tables & other machinery underneath
- now, get passed mutable value, fold/reduce new data, and that's it (nothing to return)+
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

when `fold_keyed` is static, then it reschedules itself; still has data, but if nobody is interested then you don't need to run again (already been seen)
- if someone asks for it, it's retained, it'll show up
- means: are external events feeding into the subgraph
- bug? re-triggers if something happens everywhere

3. Currently going crazy instead of triggering a response
- right now, whenever _any_ event triggers a flow to run, everything runs, not just the subgraph


4. Why is `Cargo.lock` checked in?
- Good for CI, and for binaries. Libraries can leave it out, let it bind to the project that includes them

# Lucky 2023-06-12

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
