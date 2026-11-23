use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::ErrorKind,
    net::{Ipv4Addr, UdpSocket},
    time::{Duration, Instant},
};
use url::Url;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Nearby {
    id: String,
    pub name: String,
    pub address: String,
}

fn announcement(bytes: &[u8]) -> Option<Nearby> {
    let reply: Nearby = serde_json::from_slice(bytes).ok()?;
    let url = Url::parse(&reply.address).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && !reply.id.is_empty())
    .then_some(reply)
}

pub fn nearby() -> Result<Vec<Nearby>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    socket
        .send_to(b"Who is JellyfinServer?", (Ipv4Addr::BROADCAST, 7359))
        .context("Sending the local Jellyfin discovery request")?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut found = BTreeMap::new();
    let mut bytes = [0; 4096];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if left.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(left))?;
        match socket.recv_from(&mut bytes) {
            Ok((count, _)) => {
                if let Some(info) = announcement(&bytes[..count]) {
                    found.entry(info.id.clone()).or_insert(info);
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => break,
            Err(e) => return Err(e.into()),
        }
    }
    let mut found: Vec<_> = found.into_values().collect();
    found.sort_by_cached_key(|s| s.name.to_lowercase());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_advertised_base_path_and_ignores_unusable_addresses() {
        let packet = |url: &str| {
            serde_json::to_vec(&serde_json::json!({"Id":"fixture","Name":"Home","Address":url}))
                .unwrap()
        };
        assert_eq!(
            announcement(&packet("http://media.local:8096/jellyfin"))
                .unwrap()
                .name,
            "Home"
        );
        for url in [
            "file:///C:/media",
            "https://user@media.local",
            "http://media.local/?token=1",
        ] {
            assert!(announcement(&packet(url)).is_none());
        }
    }
}
