use ir::{examples, interp::Interpreter};

#[test]
fn interp_basicblock() {
    let f = examples::basicblock().unwrap();
    let it = Interpreter::default();
    let r = it.eval_i64(&f, &[2, 3, 4]).unwrap();
    // (2+3)*4 + 7 + (2*4) + (3*4) = 5*4+7+8+12=47
    assert_eq!(r, 47);
}

#[test]
fn interp_trace() {
    let f = examples::trace().unwrap();
    let it = Interpreter::default();
    assert_eq!(it.eval_i64(&f, &[10, 7]).unwrap(), 10);
    assert_eq!(it.eval_i64(&f, &[3, 9]).unwrap(), 9);
}
