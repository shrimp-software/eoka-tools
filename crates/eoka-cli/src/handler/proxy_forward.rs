use base64::Engine;
use std::net::SocketAddr;
#[cfg(any(target_os = "linux", test))]
use std::net::{IpAddr, Ipv4Addr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use url::Url;

pub struct ProxyForwarder {
    server: String,
    task: JoinHandle<()>,
}

impl ProxyForwarder {
    pub async fn start(upstream: &str, username: &str, password: &str) -> std::io::Result<Self> {
        let parsed = Url::parse(upstream).map_err(invalid_input)?;
        let host = parsed
            .host_str()
            .ok_or_else(|| invalid_input("proxy host missing"))?;
        let port = parsed
            .port()
            .ok_or_else(|| invalid_input("proxy port missing"))?;
        let upstream_addr = format!("{host}:{port}");
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        let auth_header = format!("Basic {credentials}");
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr()?;
        #[cfg(not(target_os = "linux"))]
        eprintln!("[eoka] proxy forwarder cannot verify local callers on this platform");
        let task = tokio::spawn(async move {
            while let Ok((mut client, peer)) = listener.accept().await {
                let upstream_addr = upstream_addr.clone();
                let auth_header = auth_header.clone();
                tokio::spawn(async move {
                    if !peer_is_allowed(peer, local_addr).await {
                        eprintln!("[eoka] proxy forwarder refused a connection from another user");
                        let _ = client
                            .write_all(b"HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n")
                            .await;
                        return;
                    }
                    let _ = handle_client(client, upstream_addr, auth_header).await;
                });
            }
        });
        Ok(Self {
            server: format!("http://{local_addr}"),
            task,
        })
    }

    pub fn server(&self) -> &str {
        &self.server
    }
}

impl Drop for ProxyForwarder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(target_os = "linux")]
async fn peer_is_allowed(peer: SocketAddr, listener: SocketAddr) -> bool {
    let expected = unsafe { libc::geteuid() };
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        if let Ok(table) = tokio::fs::read_to_string("/proc/net/tcp").await {
            if let Some(owner) = socket_owner(&table, peer, listener) {
                return owner == expected;
            }
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
async fn peer_is_allowed(_peer: SocketAddr, _listener: SocketAddr) -> bool {
    true
}

#[cfg(any(target_os = "linux", test))]
fn parse_proc_address(field: &str) -> Option<SocketAddr> {
    let (address, port) = field.split_once(':')?;
    let address = u32::from_str_radix(address, 16).ok()?;
    let port = u16::from_str_radix(port, 16).ok()?;
    Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::from(address.to_ne_bytes())),
        port,
    ))
}

#[cfg(any(target_os = "linux", test))]
fn socket_owner(table: &str, peer: SocketAddr, listener: SocketAddr) -> Option<u32> {
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let matches = parse_proc_address(fields.get(1)?)? == peer
            && parse_proc_address(fields.get(2)?)? == listener;
        if matches {
            fields.get(7)?.parse().ok()
        } else {
            None
        }
    })
}

async fn handle_client(
    mut client: TcpStream,
    upstream_addr: String,
    auth_header: String,
) -> std::io::Result<()> {
    let Some(request) = read_header(&mut client).await? else {
        return Ok(());
    };
    let request = String::from_utf8_lossy(&request);
    let mut upstream = TcpStream::connect(upstream_addr).await?;
    let forwarded = add_proxy_auth(&request, &auth_header);
    upstream.write_all(forwarded.as_bytes()).await?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

async fn read_header(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 1];
    while bytes.len() < 64 * 1024 {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Ok(Some(bytes))
            };
        }
        bytes.push(buffer[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(Some(bytes));
        }
    }
    Err(invalid_input("proxy request header too large"))
}

fn add_proxy_auth(request: &str, auth_header: &str) -> String {
    let mut lines = request.split("\r\n");
    let mut forwarded = String::new();
    let mut tunnel = false;
    if let Some(first) = lines.next() {
        tunnel = first
            .get(..8)
            .is_some_and(|method| method.eq_ignore_ascii_case("CONNECT "));
        forwarded.push_str(first);
        forwarded.push_str("\r\n");
    }
    for line in lines {
        if line.is_empty() {
            break;
        }
        let lower = line
            .split_once(':')
            .map(|(name, _)| name.trim().to_ascii_lowercase());
        if matches!(
            lower.as_deref(),
            Some("proxy-authorization" | "proxy-connection")
        ) || (!tunnel && matches!(lower.as_deref(), Some("connection" | "keep-alive")))
        {
            continue;
        }
        forwarded.push_str(line);
        forwarded.push_str("\r\n");
    }
    forwarded.push_str("Proxy-Authorization: ");
    forwarded.push_str(auth_header);
    forwarded.push_str(if tunnel {
        "\r\nProxy-Connection: Keep-Alive\r\n\r\n"
    } else {
        "\r\nProxy-Connection: close\r\nConnection: close\r\n\r\n"
    });
    forwarded
}

fn invalid_input(error: impl ToString) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_address(address: &str) -> String {
        let address: SocketAddr = address.parse().unwrap();
        let IpAddr::V4(ip) = address.ip() else {
            panic!("expected IPv4");
        };
        format!(
            "{:08X}:{:04X}",
            u32::from_ne_bytes(ip.octets()),
            address.port()
        )
    }

    fn proc_row(local: &str, remote: &str, uid: u32) -> String {
        format!(
            "   0: {} {} 01 00000000:00000000 00:00000000 00000000 {uid:>5}        0 1234 1 0000000000000000 20 4 30 10 -1\n",
            proc_address(local),
            proc_address(remote)
        )
    }

    #[test]
    fn socket_owner_reads_the_callers_end_of_the_exact_connection() {
        let peer = "127.0.0.1:50000".parse().unwrap();
        let listener = "127.0.0.1:40000".parse().unwrap();
        let mut table = String::from("  sl  local_address rem_address st\n");
        table += &proc_row("127.0.0.1:40000", "127.0.0.1:50000", 1000);
        table += &proc_row("127.0.0.1:50000", "127.0.0.1:9999", 1000);
        table += &proc_row("127.0.0.2:50000", "127.0.0.1:40000", 1000);
        table += &proc_row("127.0.0.1:50000", "127.0.0.1:40000", 524288);
        assert_eq!(socket_owner(&table, peer, listener), Some(524288));
    }

    #[test]
    fn unknown_connections_have_no_owner() {
        let peer = "127.0.0.1:50000".parse().unwrap();
        let listener = "127.0.0.1:40000".parse().unwrap();
        let table = format!(
            "header\n{}garbage row\n",
            proc_row("127.0.0.1:40000", "127.0.0.1:50000", 1000)
        );
        assert_eq!(socket_owner(&table, peer, listener), None);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn a_connection_from_this_user_is_allowed_and_a_stranger_is_not() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(listener_addr).await.unwrap();
        let peer = client.local_addr().unwrap();
        assert!(peer_is_allowed(peer, listener_addr).await);
        let unknown = "127.0.0.1:1".parse().unwrap();
        assert!(!peer_is_allowed(unknown, listener_addr).await);
    }

    #[test]
    fn adds_proxy_authorization_and_preserves_request() {
        let request =
            "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nUser-Agent: test\r\n\r\n";
        let forwarded = add_proxy_auth(request, "Basic abc123");

        assert!(forwarded.starts_with("CONNECT example.com:443 HTTP/1.1\r\n"));
        assert!(forwarded.contains("Host: example.com:443\r\n"));
        assert!(forwarded.contains("User-Agent: test\r\n"));
        assert!(forwarded.contains("Proxy-Authorization: Basic abc123\r\n"));
        assert!(forwarded.contains("Proxy-Connection: Keep-Alive\r\n"));
        assert!(forwarded.ends_with("\r\n\r\n"));
    }

    #[test]
    fn replaces_existing_proxy_authorization() {
        let request = "GET http://example.com/ HTTP/1.1\r\nProxy-Authorization: Basic old\r\nProxy-Connection: close\r\nHost: example.com\r\n\r\n";
        let forwarded = add_proxy_auth(request, "Basic new");

        assert!(!forwarded.contains("Basic old"));
        assert!(forwarded.contains("Proxy-Authorization: Basic new\r\n"));
        assert_eq!(forwarded.matches("Proxy-Connection").count(), 1);
    }

    #[test]
    fn plain_http_requests_close_so_every_request_carries_auth() {
        let request = "GET http://example.com/ HTTP/1.1\r\nConnection: keep-alive\r\nKeep-Alive: 300\r\nHost: example.com\r\n\r\n";
        let forwarded = add_proxy_auth(request, "Basic new");
        assert!(forwarded.contains("Proxy-Authorization: Basic new\r\n"));
        assert!(forwarded.contains("\r\nConnection: close\r\n"));
        assert!(!forwarded.contains("keep-alive"));
        assert!(!forwarded.contains("Keep-Alive"));
    }

    #[tokio::test]
    async fn plain_http_post_body_reaches_upstream_before_any_response() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            read_header(&mut stream).await.unwrap();
            let mut body = [0_u8; 5];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"hello");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let forwarder = ProxyForwarder::start(&format!("http://{upstream_addr}"), "user", "pass")
            .await
            .unwrap();
        let local = Url::parse(forwarder.server()).unwrap();
        let mut client = TcpStream::connect(format!(
            "{}:{}",
            local.host_str().unwrap(),
            local.port().unwrap()
        ))
        .await
        .unwrap();
        client
            .write_all(b"POST http://example.test/ HTTP/1.1\r\nHost: example.test\r\nContent-Length: 5\r\n\r\nhello")
            .await
            .unwrap();
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(5), read_header(&mut client))
                .await
                .expect("POST through the forwarder must not deadlock")
                .unwrap()
                .unwrap();
        assert!(response.starts_with(b"HTTP/1.1 200 OK"));
    }

    #[tokio::test]
    async fn forwards_connect_with_auth_and_tunnels_bytes() {
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (mut stream, _) = upstream.accept().await.unwrap();
            let request = read_header(&mut stream).await.unwrap().unwrap();
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("CONNECT example.com:443 HTTP/1.1\r\n"));
            assert!(request.contains("Proxy-Authorization: Basic dXNlcjpwYXNz\r\n"));
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut payload = [0_u8; 4];
            stream.read_exact(&mut payload).await.unwrap();
            assert_eq!(&payload, b"ping");
            stream.write_all(b"pong").await.unwrap();
        });

        let forwarder = ProxyForwarder::start(&format!("http://{upstream_addr}"), "user", "pass")
            .await
            .unwrap();
        let local = Url::parse(forwarder.server()).unwrap();
        let local_addr = format!("{}:{}", local.host_str().unwrap(), local.port().unwrap());
        let mut client = TcpStream::connect(local_addr).await.unwrap();
        client
            .write_all(b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n")
            .await
            .unwrap();
        let response = read_header(&mut client).await.unwrap().unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 Connection Established\r\n"));
        client.write_all(b"ping").await.unwrap();
        let mut reply = [0_u8; 4];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"pong");

        upstream_task.await.unwrap();
    }
}
