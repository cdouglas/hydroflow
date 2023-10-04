key, (cli, blocks)
------------------
1   | ((100, a:10), {a, b})
2   | ((101, b:11), {a, c})
3   | ((102, c:12), {b, d})

-> flat_map(...)

block, {(cli, key)}
-----------------
a, {(100, 1)}
b, {(100, 1)}
a, {(101, 2)}
c, {(101, 2)}
b, {(102, 3)}
d, {(102, 3)}

block, {(cli, key)}
-----------------
a, {(100, 1), (101, 2)}
b, {(100, 1), (102, 3)}
c, {(101, 2)}
d, {(102, 3)}



DESIRED:
cli, key, {(block, {socketaddr1, socketaddr2}), ...}
@100 1, {(1, {127.0.0.1:10, b:11}), 2, {a:10}}

ALTERNATIVE FLAT REPRESENTATION:
table with dups: [cli, key, block, socketaddr]


HAVE:
(_, (Pair(SetUnion{(cli1, key1), (cli2, key2)}, SetUnion{block1, block2}), DomPair({_},{socketaddr} )))


(_, (Pair { a: SetUnion({((ClientID { id: 547f1aff-a535-4b77-9c86-fae8b4de573c }, 127.0.0.1:4344), "dingo")}), b: SetUnion({Block { pool: "2023874_0", id: 2348985 }, Block { pool: "2023874_0", id: 2348980 }}) }, DomPair { key: Max(2023-07-07T00:19:55.330847703Z), val: SetUnion({127.0.0.1:59802}) }))

(_, (Pair { a: SetUnion({((ClientID { id: 547f1aff-a535-4b77-9c86-fae8b4de573c }, 127.0.0.1:4344), "dingo")}), b: SetUnion({Block { pool: "2023874_0", id: 2348980 }}) }, DomPair { key: Max(2023-07-07T00:19:55.330484431Z), val: SetUnion({127.0.0.1:59788}) }))

-> fold_keyed(MapUnion::<keytype, SetUnion<valtype>::new(), |accum, e| {
       accum.merge(MapUnion::<keytype, SetUnion<valtype>::new(e));
       accum
   })

-> map(|(block, set)| MapUnion::<keytype, SetUnion<valtype>>::from((block, set)))
-> lattice_fold()



Base data: A->{B}
Result data: B->{A}


## Ideas for optimizing nesting/iteration/protocol design

Nesting/Grouping prior to transmission
- whether over a network, or over a local edge!
- vs cost of unnesting
- how to make this less clumsy in the program syntax
- Implication on failures: nested provides an "atomic packet" at app level, flattened does not.
- Note on many-to-many relationship:
   - (X, Y) could be (X, {Y}) or ({X} Y) and we may want to go fluidly between the alternate nestings
   - E.g. (client, state) on ingress is per-client: (client, {state}) 
    - then internally for batching we want per-resource: ({client}, state)
    - then to egress we return to per-client: (client, {state}).

By-value (stateless) vs By-reference (stateful) protocols (David Chu I worked on this in his thesis)
- Do we copy the big payload in each message?
- Does the sender send only an ID of the payload, and recipient requests on demand?
- Implication on failures, blast radius, etc.

Cursors: For big payloads, handling lazy responses/iteration
- cursors into larger-than-expected(?) responses
- who manages the cursor/state of the response?
- e.g., recv 1M blocks, different payload/encoding than 3 blocks (continuation vs compress/nest)
- Implication on failures, blast radius, etc.
 
Refinement on cursors: Handling lazy logical state? 
  - position in the log? computation+other state
  - return not just results, but code to the client? retain at the server?
  - Implication on failures, blast radius, etc.

Reach goal: How does this interact with coding theory at the network layer?
  - Eg fountain codes
  - Note that relational normalization is a form of compression/coding
    - There have a been a few papers on this

Backpressure
- Binding the resolution of all outstanding queries to a tick may create large spikes in latency
  - e.g., one very large/complex request can hold back replies to others
  - how can we add preemption, or at least fairness/bounded latency, to the system?
    - only if we can ensure that the queries we pass out of that tick are not related and/or will cause this query to abort/retry
- Implication on failures, blast radius, etc.