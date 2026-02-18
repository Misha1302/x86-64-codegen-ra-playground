use analysis::{compute_liveness, compute_live_intervals};
use ir::examples;

#[test]
fn liveness_smoke() {
    let f = examples::trace().unwrap();
    let l = compute_liveness(&f).unwrap();
    assert!(l.per_block.contains_key(&f.entry));
}

#[test]
fn intervals_sorted() {
    let f = examples::basicblock().unwrap();
    let li = compute_live_intervals(&f).unwrap();
    for w in li.intervals.windows(2) {
        assert!(w[0].start <= w[1].start);
    }
}
