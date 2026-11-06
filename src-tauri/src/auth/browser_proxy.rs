//! Proxy flags for the login WebView2. It must leave through the same route
//! as the Rust client. Chromium can't authenticate to a SOCKS5 proxy (and
//! would prompt for HTTP proxy auth), so a proxy with a login goes through
//! the local forwarder instead (`super::forwarder`).

use crate::net::proxy::{NetworkSettings, ProxyEndpoint, ProxyScheme};

/// Tauri's own default WebView2 flags; `additional_browser_args` replaces them,
/// so they are repeated here.
const DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

#[derive(Debug, PartialEq, Eq)]
pub enum BrowserRoute {
    /// Direct mode: ignore any system proxy, like the Rust client does.
    Direct,
    /// A proxy Chromium can use as-is (no credentials).
    Proxy(String),
    /// A proxy with a login: run the forwarder and point Chromium at it.
    Forwarded(ProxyEndpoint),
}

pub fn route(settings: &NetworkSettings) -> Result<BrowserRoute, String> {
    let Some(endpoint) = settings.active_proxy() else {
        return Ok(BrowserRoute::Direct);
    };
    endpoint.validate()?;
    if endpoint.has_credentials() {
        return Ok(BrowserRoute::Forwarded(endpoint.clone()));
    }
    // Chromium's `socks5://` already resolves host names on the proxy.
    let scheme = match endpoint.scheme {
        ProxyScheme::Socks5 => "socks5",
        ProxyScheme::Http => "http",
        ProxyScheme::Https => "https",
    };
    let url = endpoint.url()?;
    let authority = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
    Ok(BrowserRoute::Proxy(format!("{scheme}://{authority}")))
}

/// Full `additional_browser_args` for the login window.
pub fn args(proxy: Option<&str>) -> String {
    match proxy {
        Some(proxy) => format!("{DEFAULT_ARGS} --proxy-server={proxy}"),
        None => format!("{DEFAULT_ARGS} --no-proxy-server"),
    }
}

pub fn forwarder_proxy(port: u16) -> String {
    format!("socks5://127.0.0.1:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::proxy::NetMode;

    fn settings(scheme: ProxyScheme, username: &str) -> NetworkSettings {
        NetworkSettings {
            mode: NetMode::Proxy,
            proxy: ProxyEndpoint {
                scheme,
                host: "10.0.0.1".into(),
                port: 1080,
                username: username.into(),
            },
        }
    }

    #[test]
    fn direct_mode_disables_the_system_proxy() {
        assert_eq!(route(&NetworkSettings::default()), Ok(BrowserRoute::Direct));
        assert!(args(None).ends_with(" --no-proxy-server"));
    }

    #[test]
    fn proxies_without_login_are_passed_to_chromium() {
        assert_eq!(
            route(&settings(ProxyScheme::Socks5, "")),
            Ok(BrowserRoute::Proxy("socks5://10.0.0.1:1080".into()))
        );
        assert_eq!(
            route(&settings(ProxyScheme::Http, "")),
            Ok(BrowserRoute::Proxy("http://10.0.0.1:1080".into()))
        );
        assert_eq!(
            route(&settings(ProxyScheme::Https, "")),
            Ok(BrowserRoute::Proxy("https://10.0.0.1:1080".into()))
        );
    }

    #[test]
    fn proxies_with_login_go_through_the_forwarder() {
        for scheme in [ProxyScheme::Socks5, ProxyScheme::Http, ProxyScheme::Https] {
            assert!(matches!(
                route(&settings(scheme, "user")),
                Ok(BrowserRoute::Forwarded(_))
            ));
        }
        assert_eq!(forwarder_proxy(5555), "socks5://127.0.0.1:5555");
    }

    #[test]
    fn args_keep_tauri_defaults() {
        let args = args(Some("socks5://127.0.0.1:1"));
        assert!(args.starts_with(DEFAULT_ARGS));
        assert!(args.ends_with(" --proxy-server=socks5://127.0.0.1:1"));
    }
}
