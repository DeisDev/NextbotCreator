use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use semver::Version;
use serde::Deserialize;
use thiserror::Error;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::APP_VERSION;

pub const REPOSITORY_URL: &str = "https://github.com/DeisDev/NextbotCreator";
pub const ISSUES_URL: &str = "https://github.com/DeisDev/NextbotCreator/issues";
pub const RELEASES_URL: &str = "https://github.com/DeisDev/NextbotCreator/releases";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/DeisDev/NextbotCreator/releases/latest";
const CHECK_COOLDOWN: Duration = Duration::from_secs(60);
const RESPONSE_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate,
    Available { version: Version, url: String },
    NoRelease,
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Could not contact GitHub. Check your connection and try again later. ({0})")]
    Network(#[from] ureq::Error),
    #[error("GitHub's request limit was reached. Please try again later.")]
    RateLimited,
    #[error("GitHub returned HTTP {0}. Please try again later.")]
    Http(u16),
    #[error("GitHub returned an invalid release response: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("The latest release tag is not a valid version: {0}")]
    InvalidVersion(String),
    #[error("The update check could not run. Please try again later.")]
    Worker,
}

#[derive(Debug, Default)]
pub enum UpdateStatus {
    #[default]
    NotChecked,
    Checking,
    Finished(Result<UpdateOutcome, UpdateError>),
}

/// Owns a single background request. Polling and reading status never perform network I/O.
#[derive(Default)]
pub struct UpdateChecker {
    status: UpdateStatus,
    worker: Option<JoinHandle<Result<UpdateOutcome, UpdateError>>>,
    last_started: Option<Instant>,
}

impl UpdateChecker {
    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn can_check(&self) -> bool {
        self.worker.is_none()
            && self
                .last_started
                .is_none_or(|started| started.elapsed() >= CHECK_COOLDOWN)
    }

    pub fn start(&mut self) {
        if !self.can_check() {
            return;
        }
        self.last_started = Some(Instant::now());
        match std::thread::Builder::new()
            .name("update-check".into())
            .spawn(check_latest_release)
        {
            Ok(worker) => {
                self.worker = Some(worker);
                self.status = UpdateStatus::Checking;
            }
            Err(_) => self.status = UpdateStatus::Finished(Err(UpdateError::Worker)),
        }
    }

    /// Returns true when a result becomes available. Never joins an unfinished request.
    pub fn poll(&mut self) -> bool {
        if !self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            return false;
        }
        let result = self.worker.take().unwrap().join();
        self.status = UpdateStatus::Finished(result.unwrap_or(Err(UpdateError::Worker)));
        true
    }
}

pub fn check_latest_release() -> Result<UpdateOutcome, UpdateError> {
    check_release_at(LATEST_RELEASE_API, APP_VERSION)
}

fn check_release_at(endpoint: &str, current: &str) -> Result<UpdateOutcome, UpdateError> {
    let agent = ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .new_agent();
    let mut response = agent
        .get(endpoint)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("NextbotCreator/", env!("CARGO_PKG_VERSION")),
        )
        .header("X-GitHub-Api-Version", "2026-03-10")
        .call()?;
    match response.status().as_u16() {
        200 => {
            let body = response
                .body_mut()
                .with_config()
                .limit(RESPONSE_LIMIT)
                .read_to_string()?;
            parse_release(&body, current)
        }
        404 => Ok(UpdateOutcome::NoRelease),
        403 | 429 => Err(UpdateError::RateLimited),
        status => Err(UpdateError::Http(status)),
    }
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn parse_release(body: &str, current: &str) -> Result<UpdateOutcome, UpdateError> {
    let release: GitHubRelease = serde_json::from_str(body)?;
    if release.draft || release.prerelease {
        return Ok(UpdateOutcome::NoRelease);
    }
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| UpdateError::InvalidVersion(release.tag_name.clone()))?;
    if !version.pre.is_empty() {
        return Ok(UpdateOutcome::NoRelease);
    }
    let current =
        Version::parse(current).map_err(|_| UpdateError::InvalidVersion(current.to_owned()))?;
    // Build metadata does not affect SemVer precedence.
    if version.cmp_precedence(&current).is_gt() {
        Ok(UpdateOutcome::Available {
            version,
            // Construct the link from a validated SemVer tag on our fixed repository.
            // Never open a server-supplied URL or execute anything from a release.
            url: format!("{RELEASES_URL}/tag/{}", release.tag_name),
        })
    } else {
        Ok(UpdateOutcome::UpToDate)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    fn release(tag: &str) -> String {
        serde_json::json!({"tag_name": tag, "draft": false, "prerelease": false}).to_string()
    }

    #[test]
    fn versions_follow_semver_including_multi_digit_and_build_metadata() {
        for tag in ["v0.10.0", "0.10.0", "v1.0.0"] {
            assert!(matches!(
                parse_release(&release(tag), "0.9.0").unwrap(),
                UpdateOutcome::Available { .. }
            ));
        }
        for tag in ["v0.8.0", "v0.9.0", "0.9.0+other"] {
            assert_eq!(
                parse_release(&release(tag), "0.9.0+local").unwrap(),
                UpdateOutcome::UpToDate
            );
        }
        assert!(matches!(
            parse_release(&release("v1.0.0"), "1.0.0-rc.1").unwrap(),
            UpdateOutcome::Available { .. }
        ));
    }

    #[test]
    fn unpublished_prerelease_and_invalid_tags_are_not_offered() {
        for flag in ["draft", "prerelease"] {
            let mut body = serde_json::from_str::<serde_json::Value>(&release("v9.0.0")).unwrap();
            body[flag] = true.into();
            assert_eq!(
                parse_release(&body.to_string(), "0.6.0").unwrap(),
                UpdateOutcome::NoRelease
            );
        }
        assert_eq!(
            parse_release(&release("v9.0.0-beta.1"), "0.6.0").unwrap(),
            UpdateOutcome::NoRelease
        );
        for tag in ["latest", "1.2", "../../other", "v1.0.0?redirect=evil"] {
            assert!(matches!(
                parse_release(&release(tag), "0.6.0"),
                Err(UpdateError::InvalidVersion(_))
            ));
        }
        assert!(parse_release("{}", "0.6.0").is_err());
        assert!(parse_release("not json", "0.6.0").is_err());
    }

    #[test]
    fn release_links_stay_on_the_project_repository() {
        let body = r#"{"tag_name":"v9.0.0","draft":false,"prerelease":false,"html_url":"https://example.com"}"#;
        let UpdateOutcome::Available { url, .. } = parse_release(body, "0.6.0").unwrap() else {
            panic!("missing update")
        };
        assert_eq!(
            url,
            "https://github.com/DeisDev/NextbotCreator/releases/tag/v9.0.0"
        );
    }

    // Exercise the real HTTP client against a local fixture, without depending on GitHub.
    fn serve(status: u16, body: &str) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/latest", listener.local_addr().unwrap());
        let body = body.to_owned();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                request.extend_from_slice(&buffer[..count]);
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            String::from_utf8(request).unwrap()
        });
        (endpoint, server)
    }

    #[test]
    fn http_handles_success_missing_releases_rate_limits_and_bad_responses() {
        for status in [200, 404, 403, 429, 500, 302] {
            let (endpoint, server) = serve(status, &release("v9.0.0"));
            let result = check_release_at(&endpoint, "0.6.0");
            match status {
                200 => assert!(matches!(result, Ok(UpdateOutcome::Available { .. }))),
                404 => assert_eq!(result.unwrap(), UpdateOutcome::NoRelease),
                403 | 429 => assert!(matches!(result, Err(UpdateError::RateLimited))),
                _ => assert!(matches!(result, Err(UpdateError::Http(code)) if code == status)),
            }
            let request = server.join().unwrap().to_ascii_lowercase();
            assert!(request.contains("user-agent: nextbotcreator/"));
            assert!(request.contains("accept: application/vnd.github+json"));
            assert!(!request.contains("authorization:"));
        }
        let (endpoint, server) = serve(200, "invalid json");
        assert!(matches!(
            check_release_at(&endpoint, "0.6.0"),
            Err(UpdateError::InvalidResponse(_))
        ));
        server.join().unwrap();
        let (endpoint, server) = serve(200, &"x".repeat(RESPONSE_LIMIT as usize + 1));
        assert!(check_release_at(&endpoint, "0.6.0").is_err());
        server.join().unwrap();
    }

    #[test]
    fn polling_is_nonblocking_and_duplicate_checks_are_suppressed() {
        let (send, receive) = std::sync::mpsc::channel();
        let mut checker = UpdateChecker {
            status: UpdateStatus::Checking,
            worker: Some(std::thread::spawn(move || {
                receive.recv().unwrap();
                Ok(UpdateOutcome::UpToDate)
            })),
            last_started: Some(Instant::now()),
        };
        assert!(!checker.poll());
        assert!(!checker.can_check());
        checker.start();
        send.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !checker.poll() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(matches!(
            checker.status(),
            UpdateStatus::Finished(Ok(UpdateOutcome::UpToDate))
        ));
        assert!(!checker.can_check());
        checker.last_started = Some(Instant::now() - CHECK_COOLDOWN);
        assert!(checker.can_check());
    }

    #[test]
    #[ignore = "requires access to the public GitHub API"]
    fn github_update_check_smoke() {
        println!("GitHub update check: {:?}", check_latest_release().unwrap());
    }
}
