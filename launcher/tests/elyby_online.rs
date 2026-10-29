//! Run the actual native game's auth module against a Yggdrasil-shaped local server.
//! This does not rebuild or emulate the engine and never uses a real account or remote token.
#[allow(dead_code)]
#[rustfmt::skip]
#[path = "../../crates/horizon-client-render/src/auth.rs"]
mod native_auth;

#[test]
fn native_game_sends_the_elyby_identity_to_its_discovered_session_service() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let mut doc: serde_json::Value =
        serde_json::from_str(&horizon_launcher::elyby_discovery::document()).unwrap();
    let endpoints = &mut doc["discovery"]["session"]["endpoints"];
    assert_eq!(
        endpoints["join"]["uri"],
        "https://authserver.ely.by/session/join"
    );
    endpoints["join"]["uri"] = format!("{base}/session/join").into();
    endpoints["verify"]["uri"] = format!("{base}/session/hasJoined").into();
    let thread = std::thread::spawn(move || {
        for expected in [
            "GET /discovery ",
            "POST /session/join ",
            "GET /session/hasJoined?",
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut read = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            read.read_line(&mut line).unwrap();
            assert!(line.starts_with(expected), "{line}");
            let first = line.clone();
            let mut length = 0;
            loop {
                line.clear();
                read.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            read.read_exact(&mut body).unwrap();
            let body = if first.starts_with("POST") {
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["accessToken"], "mock-ely-token");
                assert_eq!(body["selectedProfile"], "069a79f444e94726a5befca90e38aaf5");
                assert_eq!(body["serverId"], "mock-server-hash");
                String::new()
            } else if first.starts_with("GET /discovery ") {
                doc.to_string()
            } else {
                r#"{"id":"069a79f444e94726a5befca90e38aaf5","properties":[]}"#.into()
            };
            let status = if first.starts_with("POST") { 204 } else { 200 };
            write!(stream,"HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    std::env::set_var("HORIZON_API_DISCOVERY", format!("{base}/discovery"));
    let session=native_auth::AuthSession::parse(r#"{"name":"Builder","uuid":"069a79f444e94726a5befca90e38aaf5","access_token":"mock-ely-token","user_type":"msa","auth_provider":"elyby"}"#).unwrap();
    native_auth::join_server(&session, "mock-server-hash").unwrap();
    assert!(native_auth::has_joined("Builder", "mock-server-hash", None)
        .unwrap()
        .is_some());
    thread.join().unwrap();
}
