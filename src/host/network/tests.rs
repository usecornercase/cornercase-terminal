use super::*;

mod the_source_address {
    use super::*;

    #[test]
    fn is_the_one_this_computer_would_send_from() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);

        assert_eq!(source(IpAddr::V4(Ipv4Addr::UNSPECIFIED), loopback), Some(loopback));
    }

    #[test]
    fn is_none_without_a_way_there() {
        assert_eq!(source(IpAddr::V4(Ipv4Addr::UNSPECIFIED), IpAddr::V6(TEST_NET_6)), None);
    }

    #[test]
    fn of_the_default_route_is_the_one_toward_a_far_address_of_each_family() {
        let v4 = source(IpAddr::V4(Ipv4Addr::UNSPECIFIED), IpAddr::V4(TEST_NET_4));
        let v6 = source(IpAddr::V6(Ipv6Addr::UNSPECIFIED), IpAddr::V6(TEST_NET_6));

        assert_eq!(default_route(), Route { v4, v6 });
    }

    #[test]
    fn of_the_default_route_is_never_unspecified() {
        let route = default_route();

        assert!(route.v4.is_none_or(|ip| ip.is_ipv4() && !ip.is_unspecified()), "{route:?}");
        assert!(route.v6.is_none_or(|ip| ip.is_ipv6() && !ip.is_unspecified()), "{route:?}");
    }
}

mod eligibility {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::ten("10.0.0.1", Some(Reach::Lan))]
    #[case::sixteen("172.16.0.1", Some(Reach::Lan))]
    #[case::thirty_one("172.31.255.255", Some(Reach::Lan))]
    #[case::home("192.168.1.20", Some(Reach::Lan))]
    #[case::tailscale_first("100.64.0.1", Some(Reach::Tailscale))]
    #[case::tailscale_last("100.127.255.254", Some(Reach::Tailscale))]
    #[case::tailscale_v6("fd7a:115c:a1e0::1", Some(Reach::Tailscale))]
    #[case::past_tailscale("100.128.0.1", None)]
    #[case::past_private("172.32.0.1", None)]
    #[case::loopback("127.0.0.1", None)]
    #[case::link_local("169.254.1.1", None)]
    #[case::public("8.8.8.8", None)]
    #[case::wildcard("0.0.0.0", None)]
    #[case::loopback_v6("::1", None)]
    #[case::link_local_v6("fe80::1", None)]
    #[case::other_ula("fd00::1", None)]
    #[case::mapped("::ffff:192.168.1.1", None)]
    fn follows_the_bridge(#[case] ip: &str, #[case] expected: Option<Reach>) {
        assert_eq!(eligible(ip.parse().expect("an address")), expected);
    }

    #[test]
    fn sorting_puts_the_lan_first_then_by_address_and_drops_duplicates() {
        let ips = listed("100.64.0.2, 192.168.1.20, 10.0.0.1, 192.168.1.20, 8.8.8.8, fd7a:115c:a1e0::1");

        let sorted: Vec<String> = sorted(ips).iter().map(ToString::to_string).collect();

        assert_eq!(sorted, ["10.0.0.1", "192.168.1.20", "100.64.0.2", "fd7a:115c:a1e0::1"]);
    }

    #[test]
    fn a_list_skips_bad_entries_and_keeps_loopback() {
        let ips: Vec<String> = listed("127.0.0.1, ::1,bad").iter().map(ToString::to_string).collect();

        assert_eq!(ips, ["127.0.0.1", "::1"]);
    }
}
