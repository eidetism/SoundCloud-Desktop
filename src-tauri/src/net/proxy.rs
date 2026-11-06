//! Network mode settings and proxy URL building (pure logic, no I/O).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetMode {
    #[default]
    Direct,
    Proxy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyScheme {
    /// SOCKS5 with DNS resolved by the proxy (`socks5h://`): local DNS never
    /// sees SoundCloud hostnames, so DNS-level blocking doesn't apply.
    #[default]
    Socks5,
    Http,
    Https,
}

impl ProxyScheme {
    fn url_scheme(self) -> &'static str {
        match self {
            ProxyScheme::Socks5 => "socks5h",
            ProxyScheme::Http => "http",
            ProxyScheme::Https => "https",
        }
    }
}

/// Proxy endpoint without the password (that lives in the Credential Manager).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProxyEndpoint {
    pub scheme: ProxyScheme,
    pub host: String,
    pub port: u16,
    pub username: String,
}

/// The endpoint is kept even in Direct mode so switching back restores it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NetworkSettings {
    pub mode: NetMode,
    pub proxy: ProxyEndpoint,
}

impl NetworkSettings {
    /// The proxy to route through, or `None` for a direct connection.
    pub fn active_proxy(&self) -> Option<&ProxyEndpoint> {
        (self.mode == NetMode::Proxy).then_some(&self.proxy)
    }
}

impl ProxyEndpoint {
    pub fn validate(&self) -> Result<(), String> {
        let host = self.host.trim();
        if host.is_empty() {
            return Err("proxy host is empty".into());
        }
        if host.contains(['/', '@', '?', '#', ' ']) || host.contains("://") {
            return Err("proxy host must be a bare host name or IP address".into());
        }
        if self.port == 0 {
            return Err("proxy port must be between 1 and 65535".into());
        }
        Ok(())
    }

    /// Proxy URL without credentials, e.g. `socks5h://10.0.0.1:1080`.
    /// Credentials are attached separately through `reqwest::Proxy::basic_auth`,
    /// which percent-encodes them correctly.
    pub fn url(&self) -> Result<String, String> {
        self.validate()?;
        let host = self.host.trim();
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        Ok(format!(
            "{}://{host}:{}",
            self.scheme.url_scheme(),
            self.port
        ))
    }

    pub fn has_credentials(&self) -> bool {
        !self.username.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(scheme: ProxyScheme, host: &str, port: u16) -> ProxyEndpoint {
        ProxyEndpoint {
            scheme,
            host: host.into(),
            port,
            username: String::new(),
        }
    }

    #[test]
    fn socks5_resolves_dns_on_the_proxy() {
        let url = endpoint(ProxyScheme::Socks5, "10.0.0.1", 1080)
            .url()
            .unwrap();
        assert_eq!(url, "socks5h://10.0.0.1:1080");
    }

    #[test]
    fn http_and_https_schemes() {
        assert_eq!(
            endpoint(ProxyScheme::Http, "proxy.local", 3128)
                .url()
                .unwrap(),
            "http://proxy.local:3128"
        );
        assert_eq!(
            endpoint(ProxyScheme::Https, "proxy.local", 443)
                .url()
                .unwrap(),
            "https://proxy.local:443"
        );
    }

    #[test]
    fn ipv6_hosts_get_brackets_once() {
        assert_eq!(
            endpoint(ProxyScheme::Socks5, "::1", 1080).url().unwrap(),
            "socks5h://[::1]:1080"
        );
        assert_eq!(
            endpoint(ProxyScheme::Socks5, "[::1]", 1080).url().unwrap(),
            "socks5h://[::1]:1080"
        );
    }

    #[test]
    fn host_is_trimmed() {
        assert_eq!(
            endpoint(ProxyScheme::Http, "  proxy.local ", 8080)
                .url()
                .unwrap(),
            "http://proxy.local:8080"
        );
    }

    #[test]
    fn invalid_endpoints_are_rejected() {
        for (host, port) in [
            ("", 1080),
            ("   ", 1080),
            ("socks5://10.0.0.1", 1080),
            ("user@10.0.0.1", 1080),
            ("10.0.0.1/path", 1080),
            ("10.0.0.1", 0),
        ] {
            assert!(
                endpoint(ProxyScheme::Socks5, host, port).url().is_err(),
                "{host}:{port}"
            );
        }
    }

    #[test]
    fn only_proxy_mode_routes_through_the_proxy() {
        let mut settings = NetworkSettings {
            proxy: endpoint(ProxyScheme::Socks5, "10.0.0.1", 1080),
            ..Default::default()
        };
        assert!(settings.active_proxy().is_none());
        settings.mode = NetMode::Proxy;
        assert_eq!(settings.active_proxy().unwrap().host, "10.0.0.1");
    }

    #[test]
    fn settings_serialize_in_camel_case_without_password() {
        let settings = NetworkSettings {
            mode: NetMode::Proxy,
            proxy: ProxyEndpoint {
                scheme: ProxyScheme::Socks5,
                host: "h".into(),
                port: 1,
                username: "u".into(),
            },
        };
        assert_eq!(
            serde_json::to_string(&settings).unwrap(),
            r#"{"mode":"proxy","proxy":{"scheme":"socks5","host":"h","port":1,"username":"u"}}"#
        );
    }
}
