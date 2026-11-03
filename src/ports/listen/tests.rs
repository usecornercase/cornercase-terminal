use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener};

use rstest::rstest;

use super::{Listener, listeners, listening_sockets, owned_listeners, owner_pid_table, socket_fdinfo, tcp_listener};
use crate::test_util;

fn me() -> i32 {
    i32::try_from(std::process::id()).expect("a pid")
}

mod discovery {
    use super::*;

    #[rstest]
    #[case::ipv4_loopback("127.0.0.1:0")]
    #[case::ipv6_loopback("[::1]:0")]
    fn finds_a_socket_listening_on_an_ephemeral_port(#[case] at: &str) {
        let socket = TcpListener::bind(at).expect("listen");
        let local = socket.local_addr().expect("its address");

        let found = listeners(&[me()]);

        let wanted = Listener { pid: me(), port: local.port(), address: local.ip() };
        assert!(found.contains(&wanted), "{wanted:?} not in {found:?}");
    }

    #[test]
    fn a_socket_that_stopped_listening_is_gone() {
        let socket = TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = socket.local_addr().expect("its address").port();
        drop(socket);

        assert!(listeners(&[me()]).iter().all(|listener| listener.port != port));
    }

    #[test]
    fn keeps_only_the_listeners_of_the_pids_it_is_given() {
        let _socket = TcpListener::bind("127.0.0.1:0").expect("listen");

        let found = listeners(&[me()]);

        assert!(!found.is_empty() && found.iter().all(|listener| listener.pid == me()), "{found:?}");
    }

    #[test]
    fn a_process_that_is_gone_has_no_listeners() {
        assert_eq!(listeners(&[test_util::exited_pid()]), Vec::new());
    }
}

mod owner_pid_rows {
    use super::*;
    use owner_pid_table::{IPV4, IPV6, LISTEN, ROWS, Row};

    const ESTABLISHED: u32 = 5;

    fn table(layout: &Row, rows: &[(u32, &[u8], u16, u32)]) -> Vec<u8> {
        let mut table = vec![0_u8; ROWS + layout.size * rows.len()];
        table[..4].copy_from_slice(&u32::try_from(rows.len()).expect("a count").to_ne_bytes());
        for (i, &(state, address, port, pid)) in rows.iter().enumerate() {
            let row = &mut table[ROWS + i * layout.size..][..layout.size];
            row[layout.state..][..4].copy_from_slice(&state.to_ne_bytes());
            row[layout.address..][..address.len()].copy_from_slice(address);
            row[layout.port..][..2].copy_from_slice(&port.to_be_bytes());
            row[layout.pid..][..4].copy_from_slice(&pid.to_ne_bytes());
        }
        table
    }

    fn listener(pid: i32, port: u16, address: IpAddr) -> Listener {
        Listener { pid, port, address }
    }

    #[test]
    fn reads_each_ipv4_listener_with_the_pid_that_owns_it() {
        let rows = table(&IPV4, &[(LISTEN, &[127, 0, 0, 1], 5173, 4242), (LISTEN, &[0, 0, 0, 0], 3000, 70_000)]);
        let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let any = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        assert_eq!(owned_listeners(&rows, &IPV4), [listener(4242, 5173, localhost), listener(70_000, 3000, any)]);
    }

    #[test]
    fn reads_ipv6_and_turns_a_mapped_ipv4_address_into_ipv4() {
        let (loopback, mapped) = (Ipv6Addr::LOCALHOST.octets(), Ipv4Addr::LOCALHOST.to_ipv6_mapped().octets());
        let rows = table(&IPV6, &[(LISTEN, &loopback, 8080, 8), (LISTEN, &mapped, 8082, 12)]);
        let localhost = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let mapped = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(owned_listeners(&rows, &IPV6), [listener(8, 8080, localhost), listener(12, 8082, mapped)]);
    }

    #[test]
    fn skips_a_row_that_is_not_listening() {
        let rows = table(&IPV4, &[(ESTABLISHED, &[127, 0, 0, 1], 5173, 4242)]);
        assert_eq!(owned_listeners(&rows, &IPV4), Vec::new());
    }

    #[test]
    fn reads_no_further_than_its_count_or_its_bytes() {
        let rows = table(&IPV4, &[(LISTEN, &[127, 0, 0, 1], 5173, 4242), (LISTEN, &[127, 0, 0, 1], 5174, 4242)]);
        let mut fewer = rows.clone();
        fewer[..4].copy_from_slice(&1_u32.to_ne_bytes());
        let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(owned_listeners(&fewer, &IPV4), [listener(4242, 5173, localhost)]);
        assert_eq!(owned_listeners(&rows[..rows.len() - 1], &IPV4), [listener(4242, 5173, localhost)]);
        assert_eq!(owned_listeners(&rows[..3], &IPV4), Vec::new());
    }
}

mod proc_net_tcp {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0 100 0 0 10 0
   1: 0100007F:1F91 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 4343 1 0 20 4 30 10 -1
   2: 00000000:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4444 1 0 100 0 0 10 0
";

    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5151 1 0 100 0 0 10 0
   1: 0000000000000000FFFF00000100007F:1F92 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5252 1 0 100 0 0 10 0
";

    #[test]
    fn keeps_the_listening_sockets_with_their_inode_address_and_port() {
        let localhost = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let any = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        assert_eq!(listening_sockets(TCP), [(4242, localhost, 8080), (4444, any, 3000)]);
    }

    #[test]
    fn reads_ipv6_and_turns_a_mapped_ipv4_address_into_ipv4() {
        let localhost = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let mapped = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(listening_sockets(TCP6), [(5151, localhost, 8080), (5252, mapped, 8082)]);
    }

    #[test]
    fn skips_lines_it_cannot_read() {
        assert_eq!(listening_sockets("header\n   0: nonsense 0A\n\n"), Vec::new());
    }
}

mod socket_info {
    use super::*;
    use socket_fdinfo::{ADDRESS, FAMILIES, KIND, LISTENING, PORT, SIZE, STATE, TCP};

    fn info(families: u8, address: [u8; 16], state: i32) -> Vec<u8> {
        let mut info = vec![0_u8; SIZE];
        info[KIND..KIND + 4].copy_from_slice(&TCP.to_ne_bytes());
        info[STATE..STATE + 4].copy_from_slice(&state.to_ne_bytes());
        info[PORT..PORT + 2].copy_from_slice(&5173_u16.to_be_bytes());
        info[FAMILIES] = families;
        info[ADDRESS..ADDRESS + 16].copy_from_slice(&address);
        info
    }

    #[test]
    fn reads_an_ipv4_listener() {
        let mut address = [0_u8; 16];
        address[12..].copy_from_slice(&[127, 0, 0, 1]);
        let found = tcp_listener(&info(1, address, LISTENING));
        assert_eq!(found, Some((IpAddr::V4(Ipv4Addr::LOCALHOST), 5173)));
    }

    #[test]
    fn a_dual_stack_socket_reads_as_ipv6() {
        assert_eq!(tcp_listener(&info(3, [0; 16], LISTENING)), Some((IpAddr::V6(Ipv6Addr::UNSPECIFIED), 5173)));
    }

    #[test]
    fn a_connected_socket_or_a_short_buffer_is_no_listener() {
        assert_eq!(tcp_listener(&info(1, [0; 16], 4)), None);
        assert_eq!(tcp_listener(&info(1, [0; 16], LISTENING)[..SIZE - 1]), None);
    }
}
