//! Ely.by's documented Yggdrasil flow. Passwords are used once; tokens use the OS keychain.
//! The published Horizon client cannot consume Ely.by sessions: only its local name is used.
use super::*;

pub const BASE: &str = "https://authserver.ely.by";
pub const ONLINE_INFO: &str = "Ely.by online: your Ely.by name and UUID are used on servers configured to accept Ely.by. Microsoft-only servers require a Microsoft account. Missing game resources can be downloaded from Mojang while your Ely.by session is valid.";

#[derive(serde::Serialize, serde::Deserialize)]
struct Tokens {
    access: String,
    client: String,
}

fn key(uuid: &str) -> String {
    format!("elyby:{uuid}")
}

fn post(base: &str, path: &str, body: Value) -> Result<(u16, Value), String> {
    let url = format!("{base}/auth/{path}");
    // Don't forward a credential-bearing POST to a redirect target.
    let request = crate::proxy::agent(
        ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(Duration::from_secs(20)),
        &url,
    )
    .build()
    .post(&url);
    send(request, Some(Body::Json(body)), "Ely.by")
}

fn error(status: u16, v: &Value) -> String {
    if v.get("errorMessage").and_then(Value::as_str)
        == Some("Account protected with two factor auth.")
    {
        return "Ely.by requires your current 2FA code. Enter it and sign in again.".into();
    }
    match status {
        401 | 403 => "Ely.by refused the credentials or session. Check your login, password and 2FA code, or sign in again.",
        429 => "Too many Ely.by requests. Wait a moment and try again.",
        _ => "Ely.by could not complete the request. Try again later.",
    }.into()
}

fn grant(status: u16, v: &Value) -> Result<(Account, Tokens), String> {
    if status != 200 {
        return Err(error(status, v));
    }
    let bad = "Ely.by returned an invalid profile or session.";
    let p = v.get("selectedProfile").ok_or(bad)?;
    let uuid = str_field(p, "id")
        .ok_or(bad)?
        .replace('-', "")
        .to_lowercase();
    let name = str_field(p, "name").ok_or(bad)?;
    if uuid.len() != 32
        || !uuid.bytes().all(|b| b.is_ascii_hexdigit())
        || !valid_offline_name(&name)
    {
        return Err(bad.into());
    }
    let access = str_field(v, "accessToken")
        .filter(|s| !s.is_empty())
        .ok_or(bad)?;
    let client = str_field(v, "clientToken")
        .filter(|s| !s.is_empty())
        .ok_or(bad)?;
    Ok((
        Account {
            uuid,
            name,
            skin: None,
            cape: None,
        },
        Tokens { access, client },
    ))
}

pub fn sign_in(login: &str, password: &str, code: &str) -> Result<Account, String> {
    let (account, tokens) = authenticate(BASE, login, password, code)?;
    let mut accounts = Accounts::load();
    accounts.add_elyby(store(), account.clone(), &tokens)?;
    accounts.save()?;
    Ok(account)
}

fn authenticate(
    base: &str,
    login: &str,
    password: &str,
    code: &str,
) -> Result<(Account, Tokens), String> {
    if login.trim().is_empty() || password.is_empty() {
        return Err("Enter your Ely.by login and password.".into());
    }
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "Could not generate a client token.")?;
    let client: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let password = if code.trim().is_empty() {
        password.to_owned()
    } else {
        format!("{password}:{}", code.trim())
    };
    let (status, v) = post(
        base,
        "authenticate",
        json!({
            "username": login.trim(), "password": password,
            "clientToken": client, "requestUser": false,
        }),
    )?;
    let (a, t) = grant(status, &v)?;
    if t.client != client {
        return Err("Ely.by returned a different client token.".into());
    }
    Ok((a, t))
}

pub fn restore(store: &dyn SecretStore, account: &Account) -> Result<(), String> {
    restore_at(BASE, store, account)
}

pub(super) fn session(store: &dyn SecretStore, account: &Account) -> Result<Secrets, String> {
    restore(store, account)?;
    let text = store
        .get(&key(&account.uuid))?
        .ok_or("Sign in to Ely.by again.")?;
    let t: Tokens = serde_json::from_str(&text).map_err(|_| "Sign in to Ely.by again.")?;
    Ok(Secrets {
        refresh_token: String::new(),
        minecraft_token: t.access,
        minecraft_expires_at: now() + 3600,
        xuid: None,
        owns: false,
    })
}

fn restore_at(base: &str, store: &dyn SecretStore, account: &Account) -> Result<(), String> {
    let text = store
        .get(&key(&account.uuid))?
        .ok_or("Sign in to Ely.by again.")?;
    let tokens: Tokens = serde_json::from_str(&text).map_err(|_| "Sign in to Ely.by again.")?;
    let (status, v) = post(base, "validate", json!({"accessToken": tokens.access}))?;
    if status == 200 || status == 204 {
        return Ok(());
    }
    if !matches!(status, 400 | 401 | 403) {
        return Err(error(status, &v));
    }
    let (status, v) = post(
        base,
        "refresh",
        json!({
            "accessToken": tokens.access, "clientToken": tokens.client, "requestUser": false,
        }),
    )?;
    let (renewed, new) = grant(status, &v)?;
    if renewed.uuid != account.uuid || renewed.name != account.name || new.client != tokens.client {
        return Err("Your Ely.by profile changed. Sign in again.".into());
    }
    store.set(
        &key(&account.uuid),
        &serde_json::to_string(&new).map_err(|_| "Could not save the Ely.by session.")?,
    )
}

/// Best effort server revocation, followed by unconditional local removal.
pub fn sign_out(uuid: &str) -> Result<(), String> {
    let store = store();
    if let Ok(Some(text)) = store.get(&key(uuid)) {
        if let Ok(t) = serde_json::from_str::<Tokens>(&text) {
            let _ = post(
                BASE,
                "invalidate",
                json!({"accessToken": t.access, "clientToken": t.client}),
            );
        }
    }
    let mut accounts = Accounts::load();
    store.delete(&key(uuid))?;
    accounts.elyby.retain(|a| a.uuid != uuid);
    if accounts.elyby_selected.as_deref() == Some(uuid) {
        accounts.elyby_selected = None;
    }
    accounts.save()
}

impl Accounts {
    pub fn selected_elyby(&self) -> Option<&Account> {
        let uuid = self.elyby_selected.as_ref()?;
        self.elyby.iter().find(|a| &a.uuid == uuid)
    }

    fn add_elyby(&mut self, store: &dyn SecretStore, a: Account, t: &Tokens) -> Result<(), String> {
        store.set(
            &key(&a.uuid),
            &serde_json::to_string(t).map_err(|_| "Could not save the Ely.by session.")?,
        )?;
        self.elyby.retain(|old| old.uuid != a.uuid);
        self.elyby_selected = Some(a.uuid.clone());
        self.selected = None;
        self.elyby.push(a);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    const UUID: &str = "069a79f444e94726a5befca90e38aaf5";

    fn mock(steps: Vec<(&'static str, u16)>) -> (String, std::thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut bodies = Vec::new();
            for (path, status) in steps {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with(&format!("POST /auth/{path} ")));
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
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
                reader.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap();
                let response = if path == "validate" {
                    String::new()
                } else {
                    json!({"accessToken": "renewed-mock-token", "clientToken": body["clientToken"],
                        "selectedProfile": {"id": UUID, "name": "Builder"}})
                    .to_string()
                };
                bodies.push(body);
                write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
            bodies
        });
        (url, handle)
    }

    #[test]
    fn authenticates_with_2fa_and_keeps_secrets_out_of_metadata() {
        let (url, thread) = mock(vec![("authenticate", 200)]);
        let (a, t) =
            authenticate(&url, " user@example.test ", "mock-password", " 123456 ").unwrap();
        let requests = thread.join().unwrap();
        assert_eq!(requests[0]["username"], "user@example.test");
        assert_eq!(requests[0]["password"], "mock-password:123456");
        let store = MemoryStore::default();
        let mut accounts = Accounts::default();
        accounts.add_elyby(&store, a, &t).unwrap();
        let metadata = serde_json::to_string(&accounts).unwrap();
        assert!(!metadata.contains("token") && !metadata.contains("password"));
        assert_eq!(
            accounts.name_arg(),
            None,
            "online identity comes from the private session file"
        );
        assert!(
            !accounts.has_owner(&store),
            "Ely.by must not grant Mojang downloads"
        );
        accounts.play_as(&store, Player::GameName).unwrap();
        assert!(accounts.selected_elyby().is_none());
    }

    #[test]
    fn validates_then_renews_a_refused_token() {
        let (url, thread) = mock(vec![
            ("authenticate", 200),
            ("validate", 401),
            ("refresh", 200),
        ]);
        let (a, t) = authenticate(&url, "Builder", "mock-password", "").unwrap();
        let store = MemoryStore::default();
        Accounts::default()
            .add_elyby(&store, a.clone(), &t)
            .unwrap();
        restore_at(&url, &store, &a).unwrap();
        let requests = thread.join().unwrap();
        assert_eq!(requests[1]["accessToken"], "renewed-mock-token");
        assert_eq!(requests[2]["clientToken"], t.client);
        assert!(store.get(&key(UUID)).unwrap().is_some());
    }

    #[test]
    fn refused_refresh_does_not_become_a_valid_session() {
        let (url, thread) = mock(vec![
            ("authenticate", 200),
            ("validate", 401),
            ("refresh", 403),
        ]);
        let (a, t) = authenticate(&url, "Builder", "mock-password", "").unwrap();
        let store = MemoryStore::default();
        Accounts::default()
            .add_elyby(&store, a.clone(), &t)
            .unwrap();
        let error = restore_at(&url, &store, &a).unwrap_err();
        assert!(!error.contains(&t.access) && !error.contains(&t.client));
        thread.join().unwrap();
    }

    #[test]
    fn rejects_invalid_profiles_and_redacts_server_errors() {
        for v in [
            json!({}),
            json!({"accessToken":"a", "clientToken":"c", "selectedProfile":{"id":"bad", "name":"Builder"}}),
        ] {
            assert!(grant(200, &v).is_err());
        }
        assert!(!error(403, &json!({"errorMessage":"mock-secret"})).contains("mock-secret"));
        assert!(error(
            401,
            &json!({"errorMessage":"Account protected with two factor auth."})
        )
        .contains("2FA"));
    }

    #[test]
    fn microsoft_and_elyby_secrets_have_separate_namespaces() {
        let store = MemoryStore::default();
        store.set(UUID, "microsoft-mock").unwrap();
        let (a,t) = grant(200, &json!({"accessToken":"ely-mock", "clientToken":"c", "selectedProfile":{"id":UUID,"name":"Builder"}})).unwrap();
        let mut accounts = Accounts::default();
        accounts.add_elyby(&store, a, &t).unwrap();
        assert_eq!(store.get(UUID).unwrap().as_deref(), Some("microsoft-mock"));
        assert_eq!(accounts.player(), Player::ElyBy(UUID.into()));
        accounts.play_as(&store, Player::GameName).unwrap();
        assert_eq!(accounts.player(), Player::GameName);
    }
}
