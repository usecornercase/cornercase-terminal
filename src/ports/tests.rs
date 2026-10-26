use std::net::{IpAddr, TcpListener};

use rstest::rstest;

use super::listen::Listener;
use super::{Attribution, Pane, Place, Places, Via, host, merge, scan};
use crate::test_util::{Family, Running, TempDir, wait_until};

fn me() -> i32 {
    i32::try_from(std::process::id()).expect("a pid")
}

fn ip(text: &str) -> IpAddr {
    text.parse().expect("an address")
}

fn pane(id: u64, shell: i32, workspace: u64) -> Pane {
    Pane { id, shell, workspace, agent: None }
}

mod merging {
    use super::*;

    #[test]
    fn the_ipv4_and_ipv6_sockets_of_one_program_on_one_port_are_one_row() {
        let listener = |address: &str| Listener { pid: 7, port: 5173, address: ip(address) };
        let merged = merge(vec![listener("::"), listener("0.0.0.0"), listener("::")]);
        assert_eq!(merged.into_iter().collect::<Vec<_>>(), [((7, 5173), vec![ip("0.0.0.0"), ip("::")])]);
    }

    #[test]
    fn two_programs_on_one_port_stay_apart() {
        let listener = |pid: i32| Listener { pid, port: 3000, address: ip("127.0.0.1") };
        assert_eq!(merge(vec![listener(8), listener(9)]).len(), 2);
    }
}

mod urls {
    use super::*;

    #[rstest]
    #[case::every_ipv4_address(&["0.0.0.0"], "localhost")]
    #[case::every_address(&["0.0.0.0", "::"], "localhost")]
    #[case::ipv4_loopback(&["127.0.0.1"], "localhost")]
    #[case::ipv4_and_ipv6_loopback(&["127.0.0.1", "::1"], "localhost")]
    #[case::only_ipv6_loopback(&["::1"], "[::1]")]
    #[case::a_lan_address(&["192.168.1.20"], "192.168.1.20")]
    #[case::an_ipv6_address(&["fd00::1"], "[fd00::1]")]
    fn the_host_is_localhost_unless_the_program_listens_somewhere_else(
        #[case] addresses: &[&str],
        #[case] wanted: &str,
    ) {
        let addresses: Vec<IpAddr> = addresses.iter().map(|a| ip(a)).collect();
        let url = crate::host::open::Browse { ip: host(&addresses), port: 80 }.url();
        assert_eq!(url, format!("http://{wanted}:80"));
    }
}

mod attribution {
    use super::*;

    #[test]
    fn a_program_a_pane_started_belongs_to_its_workspace() {
        let family = Family::new();
        let places = Places { panes: vec![pane(4, family.pid(), 2)], ..Places::default() };
        let place = Attribution::new(&places).place(family.grandchild().expect("a grandchild"));
        assert_eq!(place, Place { workspace: Some(2), pane: Some(4), via: Some(Via::Pane), agent: None });
    }

    #[test]
    fn a_program_an_agent_started_is_marked_with_the_agent() {
        let family = Family::new();
        let agent = Some(("claude".to_string(), family.child().expect("a child")));
        let places = Places { panes: vec![Pane { agent, ..pane(4, family.pid(), 2) }], ..Places::default() };
        let attribution = Attribution::new(&places);
        let place = attribution.place(family.grandchild().expect("a grandchild"));
        assert_eq!((place.via, place.agent.as_deref()), (Some(Via::Agent), Some("claude")));
        assert_eq!(attribution.place(family.pid()).via, Some(Via::Pane));
    }

    #[test]
    fn a_program_working_in_a_worktree_belongs_to_its_workspace_unless_a_pane_started_it() {
        let (worktree, other) = (TempDir::new(), TempDir::new());
        let sleeper = Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(worktree.path()));
        wait_until("the sleeper works in the worktree", || {
            crate::process::cwd(sleeper.pid()).is_some_and(|cwd| cwd.starts_with(worktree.path()))
        });
        let worktrees = vec![(worktree.path().to_path_buf(), 3), (other.path().to_path_buf(), 5)];
        let places = Places { worktrees, ..Places::default() };
        let place = Attribution::new(&places).place(sleeper.pid());
        assert_eq!(place, Place { workspace: Some(3), via: Some(Via::Cwd), ..Place::default() });

        let places = Places { panes: vec![pane(9, me(), 5)], ..places };
        assert_eq!(Attribution::new(&places).place(sleeper.pid()).via, Some(Via::Pane));
    }

    #[test]
    fn a_program_in_no_listed_folder_and_no_pane_belongs_to_no_workspace() {
        let folder = TempDir::new();
        let sleeper = Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(folder.path()));
        assert_eq!(Attribution::new(&Places::default()).place(sleeper.pid()), Place::default());
    }
}

mod scanning {
    use super::*;

    #[test]
    fn lists_a_port_of_the_workspace_asked_with_its_url() {
        let socket = TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = socket.local_addr().expect("its address").port();
        let places = Places { panes: vec![pane(1, me(), 6)], ..Places::default() };

        let found = scan(&places, Some(6));

        let ours = found.iter().find(|p| p.number == port).expect("our port");
        assert_eq!((ours.url(), ours.pid, ours.place.pane), (format!("http://localhost:{port}"), me(), Some(1)));
        assert!(scan(&places, Some(7)).iter().all(|p| p.number != port));
    }

    #[test]
    fn leaves_out_the_server() {
        let socket = TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = socket.local_addr().expect("its address").port();
        let places = Places { server: me(), ..Places::default() };
        assert!(scan(&places, None).iter().all(|p| p.number != port));
    }
}
