//! Signed GitHub Releases channel for this edition. Official game updates stay independent.
use crate::backend::{self, Release};
use serde_json::Value;

fn get(url: &str) -> Result<ureq::Response, String> {
    crate::proxy::agent(
        ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)),
        url,
    )
    .build()
    .get(url)
    .call()
    .map_err(|e| match e {
        ureq::Error::Status(404, _) => "Update channel answered 404".into(),
        _ => "Could not reach the signed Ely.by update channel.".into(),
    })
}

pub fn repository() -> Option<&'static str> {
    option_env!("HORIZON_ELYBY_UPDATE_REPO").filter(|s| valid_repository(s))
}
fn valid_repository(repo: &str) -> bool {
    let parts: Vec<_> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !matches!(*p, "." | "..")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}

pub fn check() -> Result<Option<Release>, String> {
    let Some(repo) = repository() else {
        return Ok(None);
    };
    let value: Value = match get(&format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    )) {
        Ok(r) => r
            .into_json()
            .map_err(|_| "Could not read the Ely.by update channel.")?,
        Err(e) if e.contains("answered 404") => {
            return Err(
                "No Ely.by launcher releases have been published to this channel yet.".into(),
            );
        }
        Err(e) => return Err(e),
    };
    let tag = value["tag_name"].as_str().ok_or("The update has no tag.")?;
    if !tag.starts_with("launcher@") || !tag.contains("-elyby.") {
        return Err("This is not an Ely.by launcher release.".into());
    }
    if backend::compare_versions(tag, &format!("launcher@{}", backend::VERSION)).is_le() {
        return Ok(None);
    }
    let name = backend::LAUNCHER_ASSET.ok_or("No launcher update for this system.")?;
    let a = value["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(name)))
        .ok_or_else(|| format!("The Ely.by update has no {name}."))?;
    let url = a["browser_download_url"]
        .as_str()
        .filter(|u| u.starts_with("https://github.com/"))
        .ok_or("Invalid update URL.")?;
    let r = Release {
        tag: tag.into(),
        name: value["name"].as_str().unwrap_or(tag).into(),
        body: value["body"].as_str().unwrap_or("").into(),
        date: value["published_at"].as_str().unwrap_or("").into(),
        asset: Some((
            url.into(),
            a["size"].as_u64().ok_or("Missing update size.")?,
        )),
        prerelease: false,
        nightly: None,
    };
    digest(&r)?;
    Ok(Some(r))
}

fn verified_digest(tag: &str, asset: &str, sums: &[u8], sig: &str) -> Result<String, String> {
    let keys = horizon_release_sign::parse_keys(include_str!("../elyby-update-key.txt"))?;
    horizon_release_sign::signed_digest(tag, asset, Some(sums), Some(sig), &keys)
}

fn digest(r: &Release) -> Result<String, String> {
    let repo = repository().ok_or("No signed Ely.by update channel is configured.")?;
    let root = format!("https://github.com/{repo}/releases/download/{}", r.tag);
    let sums = get(&format!("{root}/SHA256SUMS"))?
        .into_string()
        .map_err(|_| "Could not read update checksums.")?;
    let sig = get(&format!("{root}/SHA256SUMS.sig"))?
        .into_string()
        .map_err(|_| "Could not read the update signature.")?;
    verified_digest(
        &r.tag,
        backend::LAUNCHER_ASSET.unwrap_or_default(),
        sums.as_bytes(),
        &sig,
    )
}

pub fn install(r: &Release, progress: impl Fn(f32)) -> Result<(), String> {
    let hash = digest(r)?;
    let (url, size) = r.asset.as_ref().ok_or("No update archive.")?;
    let work = backend::data_dir().join(".elyby-launcher-update");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let archive = work.join("download");
    backend::download(url, *size, &hash, &archive, &progress)?;
    let unpacked = work.join("unpacked");
    if unpacked.exists() {
        std::fs::remove_dir_all(&unpacked).map_err(|e| e.to_string())?;
    }
    backend::unpack(&archive, url, &unpacked)?;
    let name = if cfg!(windows) {
        "horizon-launcher.exe"
    } else {
        "horizon-launcher"
    };
    let binary = backend::find_file(&unpacked, name).ok_or("The update contains no launcher.")?;
    backend::replace_binary(&binary, backend::exe_path()).map_err(|e| e.to_string())?;
    progress(1.);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_pinned_signature_accepts_only_its_exact_tag_and_checksums() {
        let sums = include_bytes!("../tests/fixtures/elyby-update.SHA256SUMS");
        let sig = include_str!("../tests/fixtures/elyby-update.SHA256SUMS.sig");
        let tag = "launcher@0.0.0-elyby.test";
        assert_eq!(
            verified_digest(tag, "test.zip", sums, sig).unwrap(),
            "ab".repeat(32)
        );
        assert!(verified_digest("launcher@0.0.0-elyby.other", "test.zip", sums, sig).is_err());
        assert!(verified_digest(tag, "test.zip", b"tampered", sig).is_err());
    }

    #[test]
    fn untrusted_updates_and_invalid_repositories_are_refused() {
        assert!(valid_repository("user/horizon-elyby"));
        for bad in ["https://github.com/x/y", "u/../r", "u/r?x=y", ""] {
            assert!(!valid_repository(bad));
        }
        let (key, _) = horizon_release_sign::SigningKey::generate().unwrap();
        let tag = "launcher@0.2.24-elyby.99";
        let sums = format!("{}  test.zip\n", "ab".repeat(32));
        assert!(verified_digest(
            tag,
            "test.zip",
            sums.as_bytes(),
            &key.sign(tag, sums.as_bytes())
        )
        .is_err());
        assert!(verified_digest(tag, "test.zip", sums.as_bytes(), "").is_err());
        assert!(
            backend::compare_versions("launcher@0.2.24-elyby.3", "launcher@0.2.24-elyby.2").is_gt()
        );
    }
}
