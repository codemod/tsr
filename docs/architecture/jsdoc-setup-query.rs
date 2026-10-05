use tsr_core::{Arena, jsdoc_setup::meter};
fn main() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "/** ordinary documentation */\nlet a;");
    let (id, docs) = parsed.jsdoc.iter().next().expect("one eager entry");
    let bytes = arena.allocated_bytes();
    let before = meter::snapshot(126);
    let owner = meter::OwnerGuard::enter(126);
    let first = parsed.jsdoc.get(id);
    let second = parsed.jsdoc.get(id);
    assert!(std::ptr::eq(first, docs));
    assert!(std::ptr::eq(second, first));
    assert_eq!(arena.allocated_bytes(), bytes);
    let after = meter::snapshot(126);
    drop(owner);
    assert_eq!(before.allocation_requests, after.allocation_requests);
    assert_eq!(before.live_requested_bytes, after.live_requested_bytes);
}
