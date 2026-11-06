//! Local SOCKS5 forwarder for the login WebView2.
//!
//! Chromium can't log in to a SOCKS5 proxy, so while the login window is open
//! we listen on `127.0.0.1:<random>` without auth and relay every CONNECT
//! through the user's proxy (SOCKS5 with username/password, or HTTP(S)
//! `CONNECT` with Basic auth). Host names are passed through unresolved, so
//! DNS stays on the proxy. Dropping the `Forwarder` stops it.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use base64::Engine as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::app::diagnostics;
use crate::net::proxy::{ProxyEndpoint, ProxyScheme};

const SOCKS_VERSION: u8 = 5;
const METHOD_NONE: u8 = 0;
const METHOD_USER_PASS: u8 = 2;
const METHOD_UNACCEPTABLE: u8 = 0xff;
const CMD_CONNECT: u8 = 1;
const ATYP_IPV4: u8 = 1;
const ATYP_DOMAIN: u8 = 3;
const ATYP_IPV6: u8 = 4;
const REPLY_OK: u8 = 0;
const REPLY_FAILURE: u8 = 1;
const REPLY_COMMAND_UNSUPPORTED: u8 = 7;
const MAX_HTTP_HEADER: usize = 16 * 1024;

/// Where the browser wants to go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Domain(String, u16),
    V4(Ipv4Addr, u16),
    V6(Ipv6Addr, u16),
}

impl Target {
    /// `host:port` for an HTTP `CONNECT` line.
    fn authority(&self) -> String {
        match self {
            Target::Domain(host, port) => format!("{host}:{port}"),
            Target::V4(ip, port) => format!("{ip}:{port}"),
            Target::V6(ip, port) => format!("[{ip}]:{port}"),
        }
    }
}

pub struct Upstream {
    pub endpoint: ProxyEndpoint,
    pub password: String,
}

pub struct Forwarder {
    pub port: u16,
    task: JoinHandle<()>,
}

impl Drop for Forwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub async fn start(upstream: Upstream) -> std::io::Result<Forwarder> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let upstream = Arc::new(upstream);
    let task = tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else {
                continue;
            };
            let upstream = upstream.clone();
            tokio::spawn(async move {
                if let Err(error) = serve(client, &upstream).await {
                    diagnostics::warn(format!("[login-proxy] {error}"));
                }
            });
        }
    });
    Ok(Forwarder { port, task })
}

async fn serve(mut client: TcpStream, upstream: &Upstream) -> Result<(), String> {
    let target = accept_socks(&mut client).await?;
    match open_upstream(upstream, &target).await {
        Ok(mut remote) => {
            reply(&mut client, REPLY_OK).await?;
            remote
                .relay(&mut client)
                .await
                .map_err(|e| format!("relay: {e}"))
        }
        Err(error) => {
            let _ = reply(&mut client, REPLY_FAILURE).await;
            Err(format!("{}: {error}", target.authority()))
        }
    }
}

/// Server side of the SOCKS5 handshake with the browser (no auth, CONNECT only).
pub async fn accept_socks<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
) -> Result<Target, String> {
    let io = |e: std::io::Error| format!("socks handshake: {e}");
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).await.map_err(io)?;
    if head[0] != SOCKS_VERSION {
        return Err(format!("unsupported SOCKS version {}", head[0]));
    }
    let mut methods = vec![0u8; head[1] as usize];
    stream.read_exact(&mut methods).await.map_err(io)?;
    if !methods.contains(&METHOD_NONE) {
        stream
            .write_all(&[SOCKS_VERSION, METHOD_UNACCEPTABLE])
            .await
            .map_err(io)?;
        return Err("client offered no unauthenticated method".into());
    }
    stream
        .write_all(&[SOCKS_VERSION, METHOD_NONE])
        .await
        .map_err(io)?;

    let mut request = [0u8; 3];
    stream.read_exact(&mut request).await.map_err(io)?;
    if request[1] != CMD_CONNECT {
        reply(stream, REPLY_COMMAND_UNSUPPORTED).await?;
        return Err(format!("unsupported SOCKS command {}", request[1]));
    }
    read_address(stream).await
}

/// ATYP + address + port, shared by requests and upstream replies.
async fn read_address<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Target, String> {
    let io = |e: std::io::Error| format!("socks address: {e}");
    let atyp = stream.read_u8().await.map_err(io)?;
    let target = match atyp {
        ATYP_IPV4 => {
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await.map_err(io)?;
            Target::V4(ip.into(), stream.read_u16().await.map_err(io)?)
        }
        ATYP_IPV6 => {
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await.map_err(io)?;
            Target::V6(ip.into(), stream.read_u16().await.map_err(io)?)
        }
        ATYP_DOMAIN => {
            let len = stream.read_u8().await.map_err(io)? as usize;
            let mut host = vec![0u8; len];
            stream.read_exact(&mut host).await.map_err(io)?;
            let host = String::from_utf8(host).map_err(|_| "non-UTF-8 host name".to_string())?;
            Target::Domain(host, stream.read_u16().await.map_err(io)?)
        }
        other => return Err(format!("unsupported address type {other}")),
    };
    Ok(target)
}

async fn reply<S: AsyncWrite + Unpin>(stream: &mut S, code: u8) -> Result<(), String> {
    stream
        .write_all(&[SOCKS_VERSION, code, 0, ATYP_IPV4, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| format!("socks reply: {e}"))
}

enum Remote {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl Remote {
    async fn relay(&mut self, client: &mut TcpStream) -> std::io::Result<()> {
        match self {
            Remote::Plain(stream) => tokio::io::copy_bidirectional(client, stream).await,
            Remote::Tls(stream) => tokio::io::copy_bidirectional(client, stream.as_mut()).await,
        }
        .map(|_| ())
    }
}

async fn open_upstream(upstream: &Upstream, target: &Target) -> Result<Remote, String> {
    let endpoint = &upstream.endpoint;
    let host = endpoint
        .host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let mut tcp = TcpStream::connect((host, endpoint.port))
        .await
        .map_err(|e| format!("connect to proxy: {e}"))?;
    tcp.set_nodelay(true).ok();
    match endpoint.scheme {
        ProxyScheme::Socks5 => {
            socks_connect(&mut tcp, &endpoint.username, &upstream.password, target).await?;
            Ok(Remote::Plain(tcp))
        }
        ProxyScheme::Http => {
            http_connect(&mut tcp, &endpoint.username, &upstream.password, target).await?;
            Ok(Remote::Plain(tcp))
        }
        ProxyScheme::Https => {
            let mut tls = tls_to_proxy(tcp, host).await?;
            http_connect(&mut tls, &endpoint.username, &upstream.password, target).await?;
            Ok(Remote::Tls(Box::new(tls)))
        }
    }
}

async fn tls_to_proxy(
    tcp: TcpStream,
    host: &str,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, String> {
    use rustls_platform_verifier::ConfigVerifierExt as _;

    let config =
        rustls::ClientConfig::with_platform_verifier().map_err(|e| format!("tls config: {e}"))?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| format!("proxy host name: {e}"))?;
    tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|e| format!("tls to proxy: {e}"))
}

/// Client side of SOCKS5 to the upstream proxy, with username/password auth.
pub async fn socks_connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    username: &str,
    password: &str,
    target: &Target,
) -> Result<(), String> {
    let io = |e: std::io::Error| format!("proxy socks: {e}");
    stream
        .write_all(&[SOCKS_VERSION, 2, METHOD_NONE, METHOD_USER_PASS])
        .await
        .map_err(io)?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await.map_err(io)?;
    match choice[1] {
        METHOD_NONE => {}
        METHOD_USER_PASS => {
            let (user, pass) = (username.as_bytes(), password.as_bytes());
            if user.len() > 255 || pass.len() > 255 {
                return Err("proxy login or password is longer than 255 bytes".into());
            }
            let mut auth = vec![1, user.len() as u8];
            auth.extend_from_slice(user);
            auth.push(pass.len() as u8);
            auth.extend_from_slice(pass);
            stream.write_all(&auth).await.map_err(io)?;
            let mut status = [0u8; 2];
            stream.read_exact(&mut status).await.map_err(io)?;
            if status[1] != 0 {
                return Err("proxy rejected the login or password".into());
            }
        }
        _ => return Err("proxy accepts none of our auth methods".into()),
    }

    let mut request = vec![SOCKS_VERSION, CMD_CONNECT, 0];
    match target {
        Target::Domain(host, port) => {
            let host = host.as_bytes();
            if host.len() > 255 {
                return Err("host name too long".into());
            }
            request.extend([ATYP_DOMAIN, host.len() as u8]);
            request.extend_from_slice(host);
            request.extend(port.to_be_bytes());
        }
        Target::V4(ip, port) => {
            request.push(ATYP_IPV4);
            request.extend(ip.octets());
            request.extend(port.to_be_bytes());
        }
        Target::V6(ip, port) => {
            request.push(ATYP_IPV6);
            request.extend(ip.octets());
            request.extend(port.to_be_bytes());
        }
    }
    stream.write_all(&request).await.map_err(io)?;

    let mut head = [0u8; 3];
    stream.read_exact(&mut head).await.map_err(io)?;
    if head[1] != REPLY_OK {
        return Err(format!(
            "proxy refused the connection (SOCKS reply {})",
            head[1]
        ));
    }
    read_address(stream).await.map(|_| ())
}

/// HTTP `CONNECT` with Basic proxy auth; succeeds on a 2xx answer.
pub async fn http_connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    username: &str,
    password: &str,
    target: &Target,
) -> Result<(), String> {
    let io = |e: std::io::Error| format!("proxy http: {e}");
    let authority = target.authority();
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    let request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Authorization: Basic {credentials}\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.map_err(io)?;

    // Read byte by byte up to the blank line so no tunnel data is consumed.
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= MAX_HTTP_HEADER {
            return Err("proxy answer header too large".into());
        }
        header.push(stream.read_u8().await.map_err(io)?);
    }
    let status_line = String::from_utf8_lossy(&header);
    let status_line = status_line.lines().next().unwrap_or_default();
    match status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
    {
        Some(code) if (200..300).contains(&code) => Ok(()),
        Some(407) => Err("proxy rejected the login or password (HTTP 407)".into()),
        _ => Err(format!("proxy refused CONNECT: {status_line}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn accepts_a_browser_connect_by_domain() {
        let (mut browser, mut server) = duplex(1024);
        let serve = tokio::spawn(async move { accept_socks(&mut server).await });
        browser.write_all(&[5, 1, 0]).await.unwrap();
        let mut choice = [0u8; 2];
        browser.read_exact(&mut choice).await.unwrap();
        assert_eq!(choice, [5, 0]);
        let host = b"api-v2.soundcloud.com";
        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host);
        request.extend(443u16.to_be_bytes());
        browser.write_all(&request).await.unwrap();
        assert_eq!(
            serve.await.unwrap().unwrap(),
            Target::Domain("api-v2.soundcloud.com".into(), 443)
        );
    }

    #[tokio::test]
    async fn rejects_a_browser_that_insists_on_auth() {
        let (mut browser, mut server) = duplex(64);
        let serve = tokio::spawn(async move { accept_socks(&mut server).await });
        browser.write_all(&[5, 1, 2]).await.unwrap();
        let mut choice = [0u8; 2];
        browser.read_exact(&mut choice).await.unwrap();
        assert_eq!(choice, [5, 0xff]);
        assert!(serve.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn socks_upstream_sends_login_and_domain() {
        let (mut ours, mut proxy) = duplex(1024);
        let target = Target::Domain("i1.sndcdn.com".into(), 443);
        let client =
            tokio::spawn(
                async move { socks_connect(&mut ours, "user", "p@ss:word", &target).await },
            );

        let mut greeting = [0u8; 4];
        proxy.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting, [5, 2, 0, 2]);
        proxy.write_all(&[5, 2]).await.unwrap();

        let mut auth = vec![0u8; 3 + 4 + 9];
        proxy.read_exact(&mut auth).await.unwrap();
        assert_eq!(&auth[..2], &[1, 4]);
        assert_eq!(&auth[2..6], b"user");
        assert_eq!(auth[6], 9);
        assert_eq!(&auth[7..], b"p@ss:word");
        proxy.write_all(&[1, 0]).await.unwrap();

        let mut request = vec![0u8; 5 + 13 + 2];
        proxy.read_exact(&mut request).await.unwrap();
        assert_eq!(&request[..5], &[5, 1, 0, 3, 13]);
        assert_eq!(&request[5..18], b"i1.sndcdn.com");
        assert_eq!(&request[18..], &443u16.to_be_bytes());
        proxy
            .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();

        assert_eq!(client.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn socks_upstream_reports_a_wrong_password() {
        let (mut ours, mut proxy) = duplex(1024);
        let target = Target::Domain("x".into(), 443);
        let client =
            tokio::spawn(async move { socks_connect(&mut ours, "u", "bad", &target).await });
        let mut greeting = [0u8; 4];
        proxy.read_exact(&mut greeting).await.unwrap();
        proxy.write_all(&[5, 2]).await.unwrap();
        let mut auth = [0u8; 7];
        proxy.read_exact(&mut auth).await.unwrap();
        proxy.write_all(&[1, 1]).await.unwrap();
        assert!(client.await.unwrap().unwrap_err().contains("rejected"));
    }

    #[tokio::test]
    async fn http_connect_sends_basic_auth_and_accepts_200() {
        let (mut ours, mut proxy) = duplex(4096);
        let target = Target::Domain("soundcloud.com".into(), 443);
        let client =
            tokio::spawn(async move { http_connect(&mut ours, "user", "pass", &target).await });
        let mut buf = vec![0u8; 512];
        let n = proxy.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("CONNECT soundcloud.com:443 HTTP/1.1\r\n"));
        assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
        proxy
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        assert_eq!(client.await.unwrap(), Ok(()));
    }

    #[tokio::test]
    async fn http_connect_reports_407() {
        let (mut ours, mut proxy) = duplex(4096);
        let target = Target::V6(Ipv6Addr::LOCALHOST, 443);
        let client = tokio::spawn(async move { http_connect(&mut ours, "u", "p", &target).await });
        let mut buf = vec![0u8; 512];
        let n = proxy.read(&mut buf).await.unwrap();
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("CONNECT [::1]:443 "));
        proxy
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
            .await
            .unwrap();
        assert!(client.await.unwrap().unwrap_err().contains("407"));
    }
}
