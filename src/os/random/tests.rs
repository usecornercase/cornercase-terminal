use super::*;

#[test]
fn fills_with_new_bytes_each_time() {
    let (mut one, mut two) = ([0_u8; 32], [0_u8; 32]);
    fill(&mut one).expect("random bytes");
    fill(&mut two).expect("random bytes");

    assert_ne!(one, two);
    assert_ne!(one, [0; 32]);
}
