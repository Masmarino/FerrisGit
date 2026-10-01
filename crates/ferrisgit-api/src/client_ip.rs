//! Which address a per-IP throttle (login, registration, activation) keys on.
//!
//! By default it is the TCP peer. Behind a reverse proxy (the Traefik ingress in production) the peer is always the
//! proxy, which would put every client in one bucket. `TRUSTED_PROXY_CIDRS` lists the networks the proxy connects
//! from, and the `X-Forwarded-For` header is only read for a peer inside one of them. Anybody else can send that
//! header too, and it is ignored.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// `address/prefix` or a bare `address` (a /32 or /128). Host bits are masked off (`10.2.0.5/16` is `10.2.0.0/16`).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let (address, prefix) = match raw.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (raw, None),
        };
        let address: IpAddr = address
            .parse()
            .map_err(|_| "not an IP address".to_string())?;
        let max = if address.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(prefix) => prefix
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("the prefix length must be a number from 0 to {max}"))?,
            None => max,
        };
        Ok(Self {
            network: mask(address, prefix),
            prefix,
        })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        // Different families never match (`mask` keeps the family).
        ip.is_ipv4() == self.network.is_ipv4() && mask(ip, self.prefix) == self.network
    }
}

fn mask(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            let keep = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(prefix))
            };
            IpAddr::V4(Ipv4Addr::from(bits & keep))
        }
        IpAddr::V6(v6) => {
            let bits = u128::from(v6);
            let keep = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(prefix))
            };
            IpAddr::V6(Ipv6Addr::from(bits & keep))
        }
    }
}

pub fn parse_cidr_list(raw: &str) -> Result<Vec<Cidr>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            Cidr::parse(entry).map_err(|reason| format!("invalid entry {entry:?} ({reason})"))
        })
        .collect()
}

fn parse_forwarded_entry(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    if let Ok(ip) = entry.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Ok(socket) = entry.parse::<std::net::SocketAddr>() {
        return Some(socket.ip());
    }
    entry.strip_prefix('[')?.strip_suffix(']')?.parse().ok()
}

/// The address to key the throttle on. `peer` is the TCP peer. It is `None` when the server was not started with
/// connect info, which leaves one shared bucket: degraded, but never absent. `forwarded_for` is the joined value of
/// the `X-Forwarded-For` header(s).
///
/// With no trusted network, or a peer outside them, the peer is the client and the header is ignored (anyone can
/// send it). For a trusted peer the header is read from its right end, since each proxy appends the address it saw.
/// Trusted hops are skipped and the first untrusted address is the client. When nothing is usable (no header, only
/// trusted hops, or an unparseable element before an untrusted address), it falls back to the peer, never to a
/// forged value.
/// IPv6 addresses are keyed by their /64 (a client owns a whole /64 and can rotate inside it), IPv4-mapped ones as IPv4.
pub fn resolve_client_ip(
    peer: Option<IpAddr>,
    forwarded_for: Option<&str>,
    trusted: &[Cidr],
) -> IpAddr {
    let Some(peer) = peer.map(|ip| ip.to_canonical()) else {
        return IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    };
    let is_trusted = |ip: IpAddr| trusted.iter().any(|cidr| cidr.contains(ip));
    let client = if is_trusted(peer) {
        client_from_header(forwarded_for.unwrap_or(""), &is_trusted).unwrap_or(peer)
    } else {
        peer
    };
    bucket(client)
}

fn client_from_header(header: &str, is_trusted: &impl Fn(IpAddr) -> bool) -> Option<IpAddr> {
    for entry in header
        .rsplit(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        let ip = parse_forwarded_entry(entry)?.to_canonical();
        if !is_trusted(ip) {
            return Some(ip);
        }
    }
    None
}

fn bucket(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => mask(IpAddr::V6(v6), 64),
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn cidrs(raw: &str) -> Vec<Cidr> {
        parse_cidr_list(raw).unwrap()
    }

    #[test]
    fn a_cidr_contains_the_addresses_of_its_network_only() {
        let net = Cidr::parse("10.2.0.0/16").unwrap();
        assert!(net.contains(ip("10.2.0.1")) && net.contains(ip("10.2.255.255")));
        assert!(
            !net.contains(ip("10.3.0.1"))
                && !net.contains(ip("10.1.255.255"))
                && !net.contains(ip("192.168.0.1"))
        );
        assert!(
            !net.contains(ip("::1")),
            "an IPv4 network never contains an IPv6 address"
        );
        let v6 = Cidr::parse("fd00:1234::/32").unwrap();
        assert!(
            v6.contains(ip("fd00:1234:ffff::1"))
                && !v6.contains(ip("fd00:1235::1"))
                && !v6.contains(ip("10.2.0.1"))
        );
    }

    #[test]
    fn prefix_edges_and_bare_addresses() {
        assert!(
            Cidr::parse("0.0.0.0/0")
                .unwrap()
                .contains(ip("203.0.113.9"))
        );
        assert!(Cidr::parse("::/0").unwrap().contains(ip("2001:db8::1")));
        let host = Cidr::parse("192.168.1.7").unwrap();
        assert!(host.contains(ip("192.168.1.7")) && !host.contains(ip("192.168.1.8")));
        let host6 = Cidr::parse("::1").unwrap();
        assert!(host6.contains(ip("::1")) && !host6.contains(ip("::2")));
        assert!(Cidr::parse("10.0.0.1/32").unwrap().contains(ip("10.0.0.1")));
        assert_eq!(
            Cidr::parse("10.2.0.5/16").unwrap(),
            Cidr::parse("10.2.0.0/16").unwrap(),
            "host bits are masked"
        );
    }

    #[test]
    fn an_ipv4_mapped_ipv6_address_is_matched_as_ipv4() {
        assert!(
            Cidr::parse("10.2.0.0/16")
                .unwrap()
                .contains(ip("::ffff:10.2.0.4"))
        );
    }

    #[test]
    fn invalid_cidrs_are_refused() {
        for bad in [
            "",
            "abc",
            "10.2.0.0/33",
            "10.2.0.0/-1",
            "10.2.0.0/x",
            "::/129",
            "10.2.0/16",
            "10.2.0.0/16/1",
            "10.2.0.0/",
            "/16",
            "10.2.0.256/16",
            " ",
        ] {
            assert!(Cidr::parse(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_cidr_list_is_comma_separated_and_may_be_empty() {
        assert!(parse_cidr_list("").unwrap().is_empty());
        assert!(parse_cidr_list("  ").unwrap().is_empty());
        assert_eq!(
            parse_cidr_list("10.2.0.0/16, fd00::/8 ,127.0.0.1")
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            parse_cidr_list("10.2.0.0/16,").unwrap().len(),
            1,
            "a trailing comma is harmless"
        );
    }

    #[test]
    fn a_bad_entry_names_itself_in_the_error() {
        let error = parse_cidr_list("10.2.0.0/16, nonsense").unwrap_err();
        assert!(error.contains("nonsense"), "{error}");
        assert!(
            parse_cidr_list("10.2.0.0/99")
                .unwrap_err()
                .contains("10.2.0.0/99")
        );
    }

    #[test]
    fn without_trusted_proxies_the_peer_is_the_client_and_the_header_is_ignored() {
        assert_eq!(
            resolve_client_ip(Some(ip("203.0.113.5")), Some("1.2.3.4"), &[]),
            ip("203.0.113.5")
        );
        assert_eq!(
            resolve_client_ip(Some(ip("203.0.113.5")), None, &[]),
            ip("203.0.113.5")
        );
    }

    #[test]
    fn an_untrusted_peer_cannot_spoof_its_address() {
        let trusted = cidrs("10.2.0.0/16");
        assert_eq!(
            resolve_client_ip(Some(ip("203.0.113.5")), Some("1.2.3.4"), &trusted),
            ip("203.0.113.5")
        );
        assert_eq!(
            resolve_client_ip(Some(ip("10.3.0.1")), Some("10.2.0.9"), &trusted),
            ip("10.3.0.1")
        );
    }

    #[test]
    fn a_trusted_peer_gives_the_rightmost_untrusted_address_of_the_header() {
        let trusted = cidrs("10.2.0.0/16");
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some("198.51.100.4"), &trusted),
            ip("198.51.100.4")
        );
        // The client can put anything on the left: the proxy appended the real address on the right.
        assert_eq!(
            resolve_client_ip(
                Some(ip("10.2.0.7")),
                Some("9.9.9.9, 198.51.100.4"),
                &trusted
            ),
            ip("198.51.100.4")
        );
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some(" 198.51.100.4 "), &trusted),
            ip("198.51.100.4")
        );
    }

    #[test]
    fn several_trusted_hops_are_skipped_from_the_right() {
        let trusted = cidrs("10.2.0.0/16, 172.16.0.0/12");
        assert_eq!(
            resolve_client_ip(
                Some(ip("10.2.0.7")),
                Some("198.51.100.4, 172.16.3.3, 10.2.0.9"),
                &trusted
            ),
            ip("198.51.100.4")
        );
        // An untrusted hop in the middle is the first from the right: everything left of it is unverified.
        assert_eq!(
            resolve_client_ip(
                Some(ip("10.2.0.7")),
                Some("1.1.1.1, 198.51.100.4, 10.2.0.9"),
                &trusted
            ),
            ip("198.51.100.4")
        );
    }

    #[test]
    fn a_missing_empty_or_all_trusted_header_falls_back_to_the_peer() {
        let trusted = cidrs("10.2.0.0/16");
        let peer = ip("10.2.0.7");
        assert_eq!(resolve_client_ip(Some(peer), None, &trusted), peer);
        assert_eq!(resolve_client_ip(Some(peer), Some(""), &trusted), peer);
        assert_eq!(resolve_client_ip(Some(peer), Some(" , "), &trusted), peer);
        assert_eq!(
            resolve_client_ip(Some(peer), Some("10.2.0.8, 10.2.0.9"), &trusted),
            peer
        );
    }

    #[test]
    fn a_garbage_header_falls_back_to_the_peer_never_to_a_forged_address() {
        let trusted = cidrs("10.2.0.0/16");
        let peer = ip("10.2.0.7");
        for garbage in [
            "not-an-ip",
            "198.51.100.4, garbage",
            "198.51.100.999",
            "unknown",
            "198.51.100.4;evil",
            "\u{0}",
        ] {
            assert_eq!(
                resolve_client_ip(Some(peer), Some(garbage), &trusted),
                peer,
                "{garbage:?}"
            );
        }
    }

    #[test]
    fn ports_and_brackets_in_the_header_are_tolerated() {
        let trusted = cidrs("10.2.0.0/16");
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some("198.51.100.4:5678"), &trusted),
            ip("198.51.100.4")
        );
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some("[2001:db8::7]:443"), &trusted),
            ip("2001:db8::"),
            "IPv6 folded to /64"
        );
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some("[2001:db8::7]"), &trusted),
            ip("2001:db8::")
        );
    }

    #[test]
    fn ipv6_clients_are_keyed_by_their_slash_64() {
        let a = resolve_client_ip(Some(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")), None, &[]);
        let b = resolve_client_ip(Some(ip("2001:db8:1:2:1111:2222:3333:4444")), None, &[]);
        let other = resolve_client_ip(Some(ip("2001:db8:1:3::1")), None, &[]);
        assert_eq!(a, b, "rotating inside a /64 must not buy a fresh bucket");
        assert_eq!(a, ip("2001:db8:1:2::"));
        assert_ne!(a, other);
        let trusted = cidrs("10.2.0.0/16");
        assert_eq!(
            resolve_client_ip(Some(ip("10.2.0.7")), Some("2001:db8:1:2:aaaa::1"), &trusted),
            a
        );
        let trusted6 = cidrs("fd00::/8");
        assert_eq!(
            resolve_client_ip(Some(ip("fd00::5")), Some("198.51.100.4"), &trusted6),
            ip("198.51.100.4")
        );
    }

    #[test]
    fn an_ipv4_mapped_peer_is_treated_as_its_ipv4_address() {
        let trusted = cidrs("10.2.0.0/16");
        assert_eq!(
            resolve_client_ip(Some(ip("::ffff:10.2.0.7")), Some("198.51.100.4"), &trusted),
            ip("198.51.100.4"),
            "the mapped proxy is trusted"
        );
        assert_eq!(
            resolve_client_ip(Some(ip("::ffff:203.0.113.5")), None, &[]),
            ip("203.0.113.5")
        );
    }

    #[test]
    fn no_peer_means_the_shared_unspecified_bucket() {
        assert_eq!(
            resolve_client_ip(None, Some("198.51.100.4"), &cidrs("10.2.0.0/16")),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
        assert_eq!(
            resolve_client_ip(None, None, &[]),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
    }
}
