use super::{UnixStream, peer_pid};

mod the_peer_pid {
    use super::*;

    #[test]
    fn of_a_pair_is_this_process() {
        let (ours, _theirs) = UnixStream::pair().expect("a pair");
        let pid = i32::try_from(std::process::id()).expect("a pid");
        assert_eq!(peer_pid(&ours).expect("the peer's pid"), pid);
    }
}
