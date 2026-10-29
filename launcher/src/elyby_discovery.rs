//! Adapt Ely.by to the native client's existing HORIZON_API_DISCOVERY extension point.
//! The loopback listener serves only a fixed public discovery document, never credentials.
//! Its guard lives until the game process exits, including terminal/wrapped launches.
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

pub const PROVIDER_ENV: &str = "HORIZON_AUTH_PROVIDER";

pub struct Discovery {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn document() -> String {
    document_for(crate::account::elyby::BASE)
}

fn document_for(base: &str) -> String {
    json!({"discovery":{"session":{"endpoints":{
        "join":{"uri":format!("{base}/session/join")},
        "verify":{"uri":format!("{base}/session/hasJoined")}
    }}}})
    .to_string()
}

/// Must run after launch wrappers so configured variables cannot redirect an Ely.by token.
pub fn configure(cmd: &mut Command) -> Result<Option<Discovery>, String> {
    let provider = cmd
        .get_envs()
        .find(|(k, _)| *k == PROVIDER_ENV)
        .and_then(|(_, v)| v)
        .and_then(|v| v.to_str());
    match provider {
        None | Some("microsoft") => return Ok(None),
        Some("elyby") => {}
        _ => return Err("Unknown account provider; refusing to send a session.".into()),
    }
    let (guard, url) = start(document())?;
    cmd.env("HORIZON_API_DISCOVERY", url);
    Ok(Some(guard))
}

fn start(body: String) -> Result<(Discovery, String), String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|_| "Could not start Ely.by session discovery on localhost.")?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| "Could not initialize session discovery.")?;
    let path = format!(
        "/elyby/{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let url = format!("http://{addr}{path}");
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let thread = std::thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
                    let mut request = [0; 2048];
                    let mut n = 0;
                    while n < request.len() && !request[..n].contains(&b'\n') {
                        match stream.read(&mut request[n..]) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => n += count,
                        }
                    }
                    let expected = format!("GET {path} HTTP/1.");
                    let (status, text) = if request[..n].starts_with(expected.as_bytes()) {
                        (200, body.as_str())
                    } else {
                        (404, "")
                    };
                    let _ = write!(stream,"HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len());
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(_) => break,
            }
        }
    });
    Ok((
        Discovery {
            stop,
            thread: Some(thread),
        },
        url,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_local_fixed_and_closed_after_game() {
        let mut cmd = Command::new("game");
        cmd.env(PROVIDER_ENV, "elyby")
            .env("HORIZON_API_DISCOVERY", "https://wrong.example");
        let guard = configure(&mut cmd).unwrap().unwrap();
        let url = cmd
            .get_envs()
            .find(|(k, _)| *k == "HORIZON_API_DISCOVERY")
            .unwrap()
            .1
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(url.starts_with("http://127.0.0.1:"));
        let doc: serde_json::Value = ureq::get(&url).call().unwrap().into_json().unwrap();
        assert_eq!(
            doc["discovery"]["session"]["endpoints"]["join"]["uri"],
            "https://authserver.ely.by/session/join"
        );
        assert_eq!(
            doc["discovery"]["session"]["endpoints"]["verify"]["uri"],
            "https://authserver.ely.by/session/hasJoined"
        );
        assert!(ureq::get(&(url.clone() + "/other")).call().is_err());
        drop(guard);
        assert!(ureq::get(&url).call().is_err());
        let mut msa = Command::new("game");
        assert!(configure(&mut msa).unwrap().is_none());
    }
}
