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
