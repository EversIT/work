//! Outbound webhook safety. Automations let project admins make the *server* send HTTP requests,
//! so the destination must be a public host: no loopback, private, link-local (cloud metadata),
//! carrier-grade NAT or other internal ranges. The check runs on the resolved addresses and the
//! connection is pinned to the checked address, which defeats DNS rebinding. Redirects are not
//! followed, because they could point at an internal address after the check.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::Url;

pub fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || o[0] == 0                                   // 0.0.0.0/8 "this network"
        || (o[0] == 100 && (64..128).contains(&o[1]))  // 100.64.0.0/10 carrier-grade NAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 192 && o[1] == 0 && o[2] == 2)     // documentation
        || (o[0] == 198 && o[1] == 51 && o[2] == 100)  // documentation
        || (o[0] == 203 && o[1] == 0 && o[2] == 113)   // documentation
        || (o[0] == 198 && (18..20).contains(&o[1]))   // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // reserved / future use
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            // IPv4 embedded in IPv6 (mapped ::ffff:a.b.c.d and NAT64 64:ff9b::/96) takes the IPv4 rules.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let seg = v6.segments();
            if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0] {
                let [a, b] = seg[6..8] else { return false };
                return is_public_v4(Ipv4Addr::new(
                    (a >> 8) as u8,
                    a as u8,
                    (b >> 8) as u8,
                    b as u8,
                ));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00          // fc00::/7 unique local
                || (seg[0] & 0xffc0) == 0xfe80          // fe80::/10 link-local
                || (seg[0] == 0x2001 && seg[1] == 0x0db8) // documentation
                || v6 == Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("invalid webhook url")]
    InvalidUrl,
    #[error("webhook url must use https")]
    InsecureScheme,
    #[error("webhook target is not a public address")]
    NotPublic,
    #[error("could not resolve the webhook host")]
    Resolve,
    #[error("webhook request failed: {0}")]
    Request(String),
}

/// Static validation, done when an automation is saved.
pub fn validate_url(raw: &str, allow_http: bool) -> Result<Url, WebhookError> {
    if raw.len() > 2000 {
        return Err(WebhookError::InvalidUrl);
    }
    let url = Url::parse(raw).map_err(|_| WebhookError::InvalidUrl)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(WebhookError::InvalidUrl); // no credentials in URLs
    }
    match url.scheme() {
        "https" => {}
        "http" if allow_http => {}
        _ => return Err(WebhookError::InsecureScheme),
    }
    if url.host_str().is_none() {
        return Err(WebhookError::InvalidUrl);
    }
    Ok(url)
}

/// Resolve the host and pick an address that passes the public-address check.
pub async fn resolve_checked(url: &Url, allow_private: bool) -> Result<SocketAddr, WebhookError> {
    let port = url
        .port_or_known_default()
        .ok_or(WebhookError::InvalidUrl)?;
    let addrs: Vec<SocketAddr> = match url.host().ok_or(WebhookError::InvalidUrl)? {
        // IP literals are checked as they are; there is nothing to resolve.
        url::Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        url::Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        url::Host::Domain(d) => tokio::net::lookup_host((d, port))
            .await
            .map_err(|_| WebhookError::Resolve)?
            .collect(),
    };
    if addrs.is_empty() {
        return Err(WebhookError::Resolve);
    }
    // Every record must be public, so a host cannot hide an internal address among public ones.
    if !allow_private && addrs.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(WebhookError::NotPublic);
    }
    Ok(addrs[0])
}

pub struct Delivery<'a> {
    pub url: &'a str,
    pub body: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
    pub allow_http: bool,
    pub allow_private: bool,
}

/// POST a JSON body. Returns the HTTP status. The response body is not read.
pub async fn deliver(d: Delivery<'_>) -> Result<u16, WebhookError> {
    let url = validate_url(d.url, d.allow_http)?;
    let addr = resolve_checked(&url, d.allow_private).await?;
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent("work-webhook/1");
    if let Some(url::Host::Domain(host)) = url.host() {
        builder = builder.resolve(host, addr); // pin the connection to the address we checked
    }
    let client = builder
        .build()
        .map_err(|e| WebhookError::Request(e.to_string()))?;
    let mut req = client
        .post(url)
        .header("content-type", "application/json")
        .body(d.body);
    for (k, v) in d.headers {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| WebhookError::Request(e.without_url().to_string()))?;
    Ok(resp.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn internal_addresses_are_blocked() {
        for bad in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.2.1",
            "198.51.100.7",
            "203.0.113.9",
            "198.18.0.1",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad} must be blocked");
        }
    }

    #[test]
    fn public_addresses_pass() {
        for good in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "172.32.0.1",
            "100.63.255.255",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(is_public_ip(ip(good)), "{good} must be allowed");
        }
    }

    #[test]
    fn url_validation() {
        assert!(validate_url("https://hooks.example.com/x", false).is_ok());
        assert!(matches!(
            validate_url("http://hooks.example.com/x", false),
            Err(WebhookError::InsecureScheme)
        ));
        assert!(validate_url("http://hooks.example.com/x", true).is_ok());
        assert!(
            validate_url("https://user:pw@hooks.example.com/", false).is_err(),
            "credentials in the URL"
        );
        assert!(validate_url("file:///etc/passwd", true).is_err());
        assert!(validate_url("gopher://example.com", true).is_err());
        assert!(validate_url("not a url", false).is_err());
        assert!(validate_url("https://", false).is_err());
    }

    #[tokio::test]
    async fn ip_literals_and_localhost_are_refused_at_resolution() {
        for u in [
            "https://127.0.0.1/",
            "https://[::1]/",
            "https://169.254.169.254/latest/meta-data",
            "https://localhost/",
            "https://10.0.0.5:8443/",
        ] {
            let url = validate_url(u, false).unwrap();
            assert!(
                matches!(
                    resolve_checked(&url, false).await,
                    Err(WebhookError::NotPublic)
                ),
                "{u}"
            );
        }
        // The escape hatch for local development is explicit.
        let url = validate_url("https://127.0.0.1/", false).unwrap();
        assert!(resolve_checked(&url, true).await.is_ok());
    }
}
