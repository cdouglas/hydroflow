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
