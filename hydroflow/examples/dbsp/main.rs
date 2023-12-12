use hydroflow::hydroflow_syntax;

pub fn main() {
    // dT(x, id) :- Tin(...), a > 2;
    // dR(id, y) :- Rin(...), s > 5;
    // R@t+1 :- R@t;
    // R@t+1 :- dR;
    // T@t+1 :- T@t;
    // T@t+1 :- dT;
    // o_stream :- dR, dT;
    // o_stream :- dR, T;
    // o_stream :- R, dT;
    let (R_send, R_recv) = hydroflow::util::unbounded_channel::<(usize, usize)>();
    let (T_send, T_recv) = hydroflow::util::unbounded_channel::<(usize, usize)>();
    let mut df = hydroflow_syntax! {
        Tin = source_stream(T_recv);
        Rin = source_stream(R_recv);
        dT = Tin -> filter(|t| t.1 > 2) -> map(|t| (t.0, t.1)) -> tee();
        dR = Rin -> filter(|t| t.1 > 5) -> map(|t| (t.0, t.1)) -> tee();
        T = persist();
        dT -> defer_tick() -> T;
        R = persist();
        dR -> defer_tick() -> R;
        o_stream = union() -> tee();

        dT -> [0]diffjoin;
        dR -> [1]diffjoin;
        diffjoin = join() -> o_stream;

        T -> [0]t_join;
        dR -> [1]t_join;
        t_join = join() -> o_stream;

        R -> [0]r_join;
        dT -> [1]r_join;
        r_join = join() -> o_stream;

        // HACKY implementation of DBSP's multiset H operator as applied to sets
        last_output = o_stream -> unique() -> defer_tick();
        // send to output if this tuple was not in last_output
        o_stream -> [pos]output;
        last_output -> [neg]output;
        output = difference()
            // -> filter(0->1 change in multiplicity)
            -> for_each(|t| println!("Output delta: {:?}", t));
    };
    df.run_available();

    println!("A");

    R_send.send((0, 10)).unwrap();
    R_send.send((1, 1)).unwrap();
    T_send.send((0, 6)).unwrap();
    T_send.send((1, 1)).unwrap();
    df.run_available();

    println!("A");

    R_send.send((0, 10)).unwrap();
    R_send.send((1, 1)).unwrap();
    T_send.send((0, 6)).unwrap();
    T_send.send((1, 1)).unwrap();
    df.run_available();

    println!("B");
}

#[test]
fn test() {
    main();
}
