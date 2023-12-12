use hydroflow::hydroflow_syntax;

#[allow(non_snake_case)]
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
    let (R_send, R_recv) = hydroflow::util::unbounded_channel::<((usize, usize, usize), i32)>();
    let (T_send, T_recv) = hydroflow::util::unbounded_channel::<((usize, usize, usize), i32)>();
    let mut df = hydroflow_syntax! {
        Tin = source_stream(T_recv);
        Rin = source_stream(R_recv);
        // filter a > 2, project id, x
        dT = Tin
            -> filter(|((_, a, _), _)| a > &2)
            -> map(|((id, _, x), n)| (id, x, n))
            -> tee();

        // filter s > 5, project id, y
        dR = Rin
            -> filter(|((_, s, _), _)| s > &5)
            -> map(|((id, _, y), m)| (id, y, m))
            -> tee();

        // count unique relational tuples from dT, generate tuples with multiplicities
        T = dT
            -> map(|(id, x, n): (usize, usize, i32)| ((id, x), n))
            -> fold_keyed::<'static, (usize, usize), i32>(|| 0, |old: &mut i32, n: i32| *old += n) 
            -> defer_tick()
            //-> inspect(|t| println!("T: {:?}", t))
            -> map(|((id, x), n)| (id, (x, n)))
            -> [0]t_join;
        // count unique relational tuples from dR, generate tuples with multiplicities
        R = dR
            -> map(|(id, y, m): (usize, usize, i32)| ((id, y), m))
            -> fold_keyed::<'static, (usize, usize), i32>(|| 0i32, |old: &mut i32, m: i32| *old += m) 
            -> defer_tick()
            //-> inspect(|t| println!("R: {:?}", t))
            -> map(|((id, y), m)| (id, (y, m)))
            -> [1]r_join;

        // ΔT × ΔR
        dT -> map(|(id, x, n)| (id, (x, n))) -> [0]diffjoin;
        dR -> map(|(id, y, m)| (id, (y, m))) -> [1]diffjoin;
        diffjoin = join()
            //-> inspect(|t| println!("d_join: {:?}", t))
            -> o_stream;
        // T × ΔR
        dR -> map(|(id, y, m)| (id, (y, m))) -> [1]t_join;
        t_join = join()
            //-> inspect(|t| println!("t_join: {:?}", t))
            -> o_stream;

        // ΔT × R
        dT -> map(|(k, x, n)| (k, (x, n))) -> [0]r_join;
        r_join = join()
            //-> inspect(|t| println!("r_join: {:?}", t))
            -> o_stream;

        // ΔT × ΔR + ΔT × R + T × ΔR ; new tuples from ΔT, ΔR
        o_stream = union()
            -> map(|(id, ((x, n), (y, m)))| ((id, x, y), n * m))
            -> reduce_keyed::<'tick, (usize, usize, usize), i32>(|accum: &mut i32, m: i32| *accum += m)
            -> map(|((id, x, y), m)| (id, x, y, m))
            -> tee();

        // I -> z^-1 ; accumulated state, delayed 1 tick
        last_stream = o_stream
            -> map(|(id, x, y, m)| ((id, x, y), m))
            -> reduce_keyed::<'static, (usize, usize, usize), i32>(|accum: &mut i32, m: i32| *accum += m)
            -> defer_tick();
        // send to output if this tuple was not in last_output
        o_stream
            -> map(|(id, x, y, m)| ((id, x, y), m))
            //-> inspect(|t| println!("ostream: {:?}", t))
            -> [0]loj;
        last_stream 
            //-> inspect(|t| println!("last_stream: {:?}", t))  
            -> [1]loj;
        loj = import!("left_outer_join.hf");
        // implment H operator: emit delta only if changing from positive to negative or vice versa
        loj -> map(|((id, x, y), (current, last))| {
                let last = last.unwrap_or(0);
                let m = current + last;
                if last > 0 && m <= 0 {
                    ((id, x, y), -1)
                } else if last <= 0 && m > 0 {
                    ((id, x, y), 1)
                } else {
                    ((id, x, y), 0)
                }
            })
            -> filter(|(_, m) | m != &0)
            -> for_each(|t| println!("==> Output delta: {:?}", t));
    };
    df.run_available();

    println!("\nStart tick 1");

    R_send.send(((0, 9, 10), 1)).unwrap();
    R_send.send(((1, 0, 1), 1)).unwrap();
    T_send.send(((0, 5, 6), 1)).unwrap();
    T_send.send(((1, 0, 1), 1)).unwrap();
    df.run_tick();

    println!("\nStart tick 2");

    R_send.send(((0, 9, 10), 1)).unwrap();
    R_send.send(((1, 0, 1), 1)).unwrap();
    T_send.send(((0, 5, 6), 1)).unwrap();
    T_send.send(((1, 0, 1), 1)).unwrap();
    df.run_tick();

    println!("\nStart tick 3");

    R_send.send(((0, 9, 10), -1)).unwrap();
    R_send.send(((1, 0, 1), 1)).unwrap();
    T_send.send(((0, 5, 6), 1)).unwrap();
    T_send.send(((1, 0, 1), 1)).unwrap();
    df.run_tick();

    println!("\nStart tick 4");
    R_send.send(((0, 9, 10), -1)).unwrap();
    R_send.send(((1, 0, 1), 1)).unwrap();
    T_send.send(((0, 5, 6), 1)).unwrap();
    T_send.send(((1, 0, 1), 1)).unwrap();
    df.run_tick();

    // println!(
    //     "{}",
    //     df.meta_graph()
    //         .expect("No graph found, maybe failed to parse.")
    //         .to_mermaid(&Default::default())
    // );
}

#[test]
fn test() {
    main();
}
