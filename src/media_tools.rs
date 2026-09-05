//! Explicit, checksum-verified updates of the bundled downloader from its official release.
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::media::{JobContext, download, workspace};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    digest: Option<String>,
}

fn release_asset(json: &[u8]) -> Result<(String, String), String> {
    let release: Release = serde_json::from_slice(json).map_err(|error| error.to_string())?;
    if release.draft
        || release.prerelease
        || release.tag_name.is_empty()
        || release.tag_name.len() > 32
        || !release
            .tag_name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return Err("GitHub returned an invalid downloader release.".into());
    }
    let hash = release
        .assets
        .iter()
        .find(|asset| asset.name == "yt-dlp.exe")
        .and_then(|asset| asset.digest.as_deref())
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("The official release did not supply a SHA-256 checksum.")?;
    Ok((release.tag_name, hash.to_ascii_lowercase()))
}

pub fn update_downloader(portable_root: &Path, context: &JobContext) -> Result<String, String> {
    let workspace = workspace(portable_root)?;
    let metadata = workspace.path().join("release.json");
    context.report("Checking the official downloader release...");
    download(
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest",
        &metadata,
        2 * 1024 * 1024,
        context,
    )?;
    let (version, expected) =
        release_asset(&fs::read(metadata).map_err(|error| error.to_string())?)?;
    let staged = workspace.path().join("yt-dlp.exe");
    download(
        &format!("https://github.com/yt-dlp/yt-dlp/releases/download/{version}/yt-dlp.exe"),
        &staged,
        64 * 1024 * 1024,
        context,
    )?;
    context.report("Verifying downloader...");
    verify_hash(&staged, &expected)?;
    // Fetch the notices matching this exact release before changing any installed tool.
    let mut notices = Vec::new();
    for (remote, local) in [
        ("LICENSE", "yt-dlp-license.txt"),
        (
            "THIRD_PARTY_LICENSES.txt",
            "yt-dlp-third-party-licenses.txt",
        ),
    ] {
        let path = workspace.path().join(local);
        download(
            &format!("https://raw.githubusercontent.com/yt-dlp/yt-dlp/{version}/{remote}"),
            &path,
            2 * 1024 * 1024,
            context,
        )?;
        notices.push((path, local));
    }
    context.check()?;
    let portable = portable_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let tools = portable.join("tools");
    if tools.exists()
        && !tools
            .canonicalize()
            .map_err(|error| error.to_string())?
            .starts_with(&portable)
    {
        return Err("The tools folder must remain inside the portable folder.".into());
    }
    fs::create_dir_all(&tools).map_err(|error| error.to_string())?;
    let tools = tools.canonicalize().map_err(|error| error.to_string())?;
    for name in [
        "yt-dlp.exe",
        "yt-dlp-license.txt",
        "yt-dlp-third-party-licenses.txt",
        "yt-dlp-version.txt",
    ] {
        let path = tools.join(name);
        if path.exists()
            && (path.is_dir()
                || !path
                    .canonicalize()
                    .map_err(|error| error.to_string())?
                    .starts_with(&tools))
        {
            return Err("The existing downloader files have an invalid location.".into());
        }
    }
    // Stage on the destination volume and atomically replace the executable. A failed
    // download, checksum, cancellation, or locked executable leaves the old binary intact.
    let mut replacement =
        tempfile::NamedTempFile::new_in(&tools).map_err(|error| error.to_string())?;
    std::io::copy(
        &mut File::open(&staged).map_err(|error| error.to_string())?,
        replacement.as_file_mut(),
    )
    .map_err(|error| error.to_string())?;
    replacement
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    for (source, name) in notices {
        fs::copy(source, tools.join(name)).map_err(|error| error.to_string())?;
    }
    context.check()?;
    replacement
        .persist(tools.join("yt-dlp.exe"))
        .map_err(|error| error.to_string())?;
    fs::write(tools.join("yt-dlp-version.txt"), &version).map_err(|error| error.to_string())?;
    Ok(format!("Downloader updated to {version}."))
}

fn verify_hash(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err(
            "Downloader checksum mismatch. The installed executable was left unchanged.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloader_updates_require_an_official_release_shape_and_hash() {
        let good = serde_json::json!({"tag_name":"2026.08.19","draft":false,"prerelease":false,"assets":[{"name":"yt-dlp.exe","digest":format!("sha256:{}", "ab".repeat(32))}]});
        assert!(release_asset(&serde_json::to_vec(&good).unwrap()).is_ok());
        for tag in ["../bad", "2026.08.19/evil", ""] {
            let mut value = good.clone();
            value["tag_name"] = tag.into();
            assert!(release_asset(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let mut missing_hash = good;
        missing_hash["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(release_asset(&serde_json::to_vec(&missing_hash).unwrap()).is_err());
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), b"incomplete download").unwrap();
        assert!(verify_hash(file.path(), &"ab".repeat(32)).is_err());
    }
}
