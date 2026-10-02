use reqwest::Url;
use url::Host;

/// Match parsed hosts, following Shardline's OIDC endpoint validation pattern.
pub(super) fn is_loopback(endpoint: &Url) -> bool {
    match endpoint.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}
