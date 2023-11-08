# 2023-10-30

OK. You rewrote everything to use this weird pattern. What now?

## Adding info/create

OK. Got all the exhaust into one place.

Now. This is calling out for a request ID and unique IDs for each stage/request, but you can IGNORE THAT and use the ClientID.

Given the ClientID, try to merge all the PartialResults into a HashMap. Because why not, I guess.

Start with INFO. 

# 2023-10-29

## Not now.

Should clientIDs be visible, so users can use partitioned namespaces? Rather
than assuming that each client ID corresponds to a process that emits a sequence
of requests, can we distribute the namespace across endpoints? This would admit
the possibility of partitioning a program without losing sequentiality, at least
beyond its spec.  Messages arriving from different endpoints in the same
namespace could declare points of order/coordination by advancing the epoch?

Definitely reinvinting the wheel here, but I'm not sure what the wheel is.

## Better design? At least it doesn't feel like throwaway

OK. Let's just safe all pending requests to a relation, join later.  we could
also join after each step, yielding a subsequent step.  this looks a lot like
_eddies_, creating an address bus and... we're combining the stage lattices of
evita raced with priority queue from eddies.

Don't have time to do this, but write it down, anwyay.

Start w/ union of prev (w/ state machine) and next (w/ init state machine).

Blindly generate new actions from each state machine. If it's a CKResponse, queue
it and check for conflicts. If it's empty, then queue it for the next tick.

Actions are routed to modules (NSLookup, INodeLookup, etc.). Dependencies are
handled within the SM.

As internal responses to actions accumulate, route them to the originating SM.

## Current notes

req_merge: union of all the exhaust produced by queries
- join with requests
- demux and resolve to responses


### Repeating pattern?

req_(mapping) = union() -> tee()
req_(mapping)[0] -> map(PartialResult) -> req_merge; // empty result
req_(mapping)[1] -> LHS of equijoin;                 // resolve: non-empty result

(mapping) -> map(id, (payload)) -> [0]resp_(mapping);
tick_reqs[k] -> [1]resp_(mapping);

resp_(mapping) = join() // (id, ((payload), (req, _)))
  -> tee();

resp_(mapping)[0]
  -> map(PartialResult) -> req_merge // non-empty result

resp_(mapping)[1]
  -> demux(req.type)

resp_(mapping)[create]
  -> req_(mapping2)

This retains the 1:1 id: access mapping, but if multiple clients make the same request, we don't need to repeat them...
Leave as-is for now, but explore whether the empty result can be joined with the query across all requests
- query could differ by type... possible for read-only queries to share, conflicts to be detected within the pipeline?


# 2023-10-23

Working on restoring INFO
- add key->inode inode->block
- report err for empty key, handle empty file, empty host list for blocks

Create a sort/merge of all exhaust from an operator
- there's some grammar that should, for all histories, produce a response
- if it is not exhaustive, then the result is either partial for that tick or it's a logic error

e.g., info
- equijoin w/ key->inode
  - FNF if failed
- equijoin w/ inode->(seq, block)
  - empty file if length = 0
- equijoin w/ block->host
  - missing replica if failed
- equijoin w/ host->last_contact
  - missing replica if failed
(etc.)



# 2023-10-22

Missing left/right outer join
- alternative: if the join didn't match on the lhs/rhs, produce this instead
- simulate by merging result
- simulate w/ cross-product (gross)

How to ensure that, within a single tick, exactly one create succeeds for a given key?

# 2023-09-29

## WTF was I doing?

Rewrite implementation of the heartbeat/liveness handling (with assoc default keys) without using lattice types

latless.rs


## WTF am I doing?

✓ fix the lattice implmementation
○ finish the non-lattice implementation
○ chain replication
  ○ recv list of nodes (whatever, just return everything for now)
  ○ 

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
