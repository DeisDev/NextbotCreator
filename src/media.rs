//! Cancellable media acquisition. Only committed imports leave the private staging folder.
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use url::Url;

use crate::audio::{self, AudioPreview};

pub const MAX_AUDIO_SECONDS: f64 = 3600.0;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_JOB_BYTES: u64 = 768 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct JobContext {
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<String>>,
}

impl JobContext {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("Cancelled.".into())
        } else {
            Ok(())
        }
    }
    pub fn report(&self, value: impl Into<String>) {
        if let Ok(mut progress) = self.progress.lock() {
            *progress = value.into();
        }
    }
    pub fn progress(&self) -> String {
        self.progress
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }
}

pub struct MediaJob<T> {
    pub context: JobContext,
    worker: Option<JoinHandle<Result<T, String>>>,
}

impl<T: Send + 'static> MediaJob<T> {
    pub fn start(work: impl FnOnce(JobContext) -> Result<T, String> + Send + 'static) -> Self {
        let context = JobContext::default();
        let worker_context = context.clone();
        Self {
            context,
            worker: Some(std::thread::spawn(move || work(worker_context))),
        }
    }
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        if !self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            return None;
        }
        Some(
            self.worker.take().unwrap().join().unwrap_or_else(|_| {
                Err("Media task stopped unexpectedly. Please try again.".into())
            }),
        )
    }
}

impl<T> Drop for MediaJob<T> {
    fn drop(&mut self) {
        self.context.cancel();
    }
}

pub struct PreparedMedia {
    pub source: PathBuf,
    pub title: String,
    pub source_url: Option<String>,
    pub audio: Option<AudioPreview>,
    pub image: Option<image::RgbaImage>,
    // Own the workspace until the GUI has copied the accepted asset into its project.
    pub workspace: TempDir,
}

pub fn workspace(portable_root: &Path) -> Result<TempDir, String> {
    let root = portable_root.join(".media-cache");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let portable = portable_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !root.starts_with(&portable) {
        return Err("Media cache must remain inside the portable folder.".into());
    }
    tempfile::Builder::new()
        .prefix("job-")
        .tempdir_in(root)
        .map_err(|error| error.to_string())
}

pub fn parse_url(value: &str) -> Result<Url, String> {
    let value = value.trim();
    if value.len() > 8192 || value.chars().any(char::is_control) {
        return Err("Paste a valid HTTP or HTTPS link.".into());
    }
    let url = Url::parse(value)
        .map_err(|_| "Paste a complete link beginning with https:// or http://.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Only HTTP or HTTPS links without embedded credentials are supported.".into());
    }
    Ok(url)
}

pub fn video_url(value: &str) -> Result<Url, String> {
    let url = parse_url(value)?;
    let host = url.host_str().unwrap_or_default();
    let path = url.path();
    let youtube = matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com"
    ) && ((path == "/watch"
        && url
            .query_pairs()
            .any(|(key, value)| key == "v" && !value.is_empty()))
        || ["/shorts/", "/embed/", "/live/"]
            .iter()
            .any(|prefix| path.starts_with(prefix) && path.len() > prefix.len()));
    let short_youtube = host == "youtu.be" && path.len() > 1;
    let tiktok = matches!(host, "tiktok.com" | "www.tiktok.com" | "m.tiktok.com")
        && (path.contains("/video/") || path.starts_with("/t/"));
    let short_tiktok = matches!(host, "vm.tiktok.com" | "vt.tiktok.com") && path.len() > 1;
    if !youtube && !short_youtube && !tiktok && !short_tiktok {
        return Err(
            "Paste an individual YouTube or TikTok video link (including Shorts or share links)."
                .into(),
        );
    }
    Ok(url)
}

pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_body(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// Follow redirects ourselves so every destination receives the same URL validation.
pub fn download(
    url: &str,
    destination: &Path,
    limit: u64,
    context: &JobContext,
) -> Result<(), String> {
    let agent = http_agent();
    let mut url = parse_url(url)?;
    for _ in 0..=5 {
        context.check()?;
        let mut response = agent
            .get(url.as_str())
            .header(
                "User-Agent",
                concat!("NextbotCreator/", env!("CARGO_PKG_VERSION")),
            )
            .call()
            .map_err(|error| format!("Download failed: {error}"))?;
        let status = response.status().as_u16();
        if [301, 302, 303, 307, 308].contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or("Invalid download redirect.")?;
            url = parse_url(
                url.join(location)
                    .map_err(|error| error.to_string())?
                    .as_str(),
            )?;
            continue;
        }
        if status != 200 {
            return Err(format!(
                "The server returned HTTP {status}. Check that the link is public and points to the media file."
            ));
        }
        let total = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        if total.is_some_and(|size| size > limit) {
            return Err(format!(
                "Download exceeds the {} MiB limit.",
                limit / 1024 / 1024
            ));
        }
        let mut reader = response.body_mut().as_reader();
        let mut file = File::create(destination).map_err(|error| error.to_string())?;
        let mut count = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            context.check()?;
            let read = reader
                .read(&mut buffer)
                .map_err(|error| format!("Download interrupted: {error}"))?;
            if read == 0 {
                break;
            }
            count += read as u64;
            if count > limit {
                return Err(format!(
                    "Download exceeds the {} MiB limit.",
                    limit / 1024 / 1024
                ));
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            context.report(match total.filter(|total| *total > 0) {
                Some(total) => format!(
                    "Downloading: {:.0}% ({:.1} MiB)",
                    count as f64 / total as f64 * 100.0,
                    count as f64 / 1048576.0
                ),
                None => format!("Downloading: {:.1} MiB", count as f64 / 1048576.0),
            });
        }
        if count == 0 || total.is_some_and(|total| total != count) {
            return Err("The download was empty or incomplete.".into());
        }
        return Ok(());
    }
    Err("The link redirected too many times.".into())
}

pub fn fetch_image(
    value: &str,
    portable_root: &Path,
    context: &JobContext,
) -> Result<PreparedMedia, String> {
    let url = parse_url(value)?;
    let workspace = workspace(portable_root)?;
    let temporary = workspace.path().join("download");
    download(url.as_str(), &temporary, MAX_IMAGE_BYTES, context)?;
    context.report("Checking image...");
    context.check()?;
    let mut reader = image::ImageReader::open(&temporary)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let format = reader.format().or_else(|| url.path().to_ascii_lowercase().ends_with(".tga").then_some(image::ImageFormat::Tga))
        .ok_or("This link returned a webpage or an unsupported image. Use Copy image address in your browser.")?;
    reader.set_format(format);
    let extension = match format {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Bmp => "bmp",
        image::ImageFormat::Tga => "tga",
        _ => return Err("Use a PNG, JPEG, GIF, WebP, BMP, or TGA image link.".into()),
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| format!("Invalid or oversized image: {error}"))?;
    context.check()?;
    let image = decoded.thumbnail(512, 512).to_rgba8();
    let name = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .unwrap_or("image");
    let stem = crate::domain::slugify(
        Path::new(name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("image"),
    );
    let source = workspace.path().join(format!(
        "image_{}.{extension}",
        if stem.is_empty() {
            "image"
        } else {
            &stem[..stem.len().min(80)]
        }
    ));
    fs::rename(temporary, &source).map_err(|error| error.to_string())?;
    Ok(PreparedMedia {
        source,
        title: "Image from URL".into(),
        source_url: Some(url.into()),
        audio: None,
        image: Some(image),
        workspace,
    })
}

pub fn fetch_audio(
    value: &str,
    portable_root: &Path,
    context: &JobContext,
) -> Result<PreparedMedia, String> {
    let url = video_url(value)?;
    let tools = portable_root.join("tools");
    let downloader = tools.join("yt-dlp.exe");
    let deno = tools.join("deno.exe");
    if !downloader.is_file() || !deno.is_file() {
        return Err("Download tools are missing. Use the full portable release, which includes yt-dlp and Deno in tools/.".into());
    }
    let workspace = workspace(portable_root)?;
    let mut command = Command::new(downloader);
    command
        .args([
            "--ignore-config",
            "--no-plugin-dirs",
            "--no-update",
            "--no-playlist",
            "--playlist-items",
            "1",
            "--no-cache-dir",
            "--no-js-runtimes",
            "--js-runtimes",
        ])
        .arg(format!("deno:{}", deno.display()))
        .arg("--ffmpeg-location")
        .arg(&tools)
        .args([
            "--no-remote-components",
            "--socket-timeout",
            "10",
            "--retries",
            "2",
            "--fragment-retries",
            "2",
            "--max-filesize",
            "256M",
            "--match-filters",
            "!is_live & duration <= 3600",
            "--format",
            "bestaudio/best",
            "--write-info-json",
            "--newline",
            "--progress-template",
            "download:Downloading audio: %(progress._percent_str)s (%(progress._speed_str)s)",
            "--output",
        ])
        .arg(workspace.path().join("source.%(ext)s"))
        .arg("--")
        .arg(url.as_str());
    context.report("Finding audio...");
    run_process(
        &mut command,
        workspace.path(),
        context,
        Duration::from_secs(600),
    )?;
    context.check()?;
    let metadata: serde_json::Value = fs::read(workspace.path().join("source.info.json")).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("No downloadable audio found. Use a public video up to 60 minutes long; live streams and restricted videos are unsupported.")?;
    let source = fs::read_dir(workspace.path())
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_stem().is_some_and(|stem| stem == "source")
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        [
                            "webm", "mp4", "m4a", "mp3", "ogg", "opus", "wav", "flac", "aac", "mka",
                        ]
                        .contains(&extension)
                    })
        })
        .ok_or(
            "The video did not provide a supported audio stream. Try updating the download tools.",
        )?;
    let source = source.canonicalize().map_err(|error| error.to_string())?;
    if !source.starts_with(workspace.path()) {
        return Err("Invalid downloaded file location.".into());
    }
    let title: String = metadata["title"]
        .as_str()
        .unwrap_or("Downloaded audio")
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let stem = crate::domain::slugify(&title);
    let renamed = workspace.path().join(format!(
        "audio_{}.{}",
        if stem.is_empty() {
            "clip"
        } else {
            &stem[..stem.len().min(80)]
        },
        source.extension().unwrap().to_string_lossy()
    ));
    fs::rename(&source, &renamed).map_err(|error| error.to_string())?;
    let source = if renamed
        .extension()
        .is_some_and(|extension| extension == "mp4")
    {
        // TikTok sometimes supplies a combined stream. Retain its original audio
        // losslessly and let the staging folder dispose of the video after import.
        let extracted = renamed.with_extension("mka");
        let ffmpeg = crate::converter::ffmpeg_path(portable_root)
            .ok_or("FFmpeg is missing from the portable tools folder.")?;
        context.report("Extracting original audio...");
        run_process(
            Command::new(ffmpeg)
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-y",
                    "-protocol_whitelist",
                    "file,pipe",
                    "-i",
                ])
                .arg(&renamed)
                .args(["-map", "0:a:0", "-vn", "-c:a", "copy"])
                .arg(&extracted),
            workspace.path(),
            context,
            Duration::from_secs(120),
        )?;
        extracted
    } else {
        renamed
    };
    let audio = audio::prepare_preview(&source, portable_root, workspace.path(), context)?;
    Ok(PreparedMedia {
        source,
        title,
        source_url: Some(url.into()),
        audio: Some(audio),
        image: None,
        workspace,
    })
}

pub fn prepare_local_audio(
    source: &Path,
    portable_root: &Path,
    context: &JobContext,
) -> Result<PreparedMedia, String> {
    let workspace = workspace(portable_root)?;
    let audio = audio::prepare_preview(source, portable_root, workspace.path(), context)?;
    let title = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(PreparedMedia {
        source: source.to_owned(),
        title,
        source_url: None,
        audio: Some(audio),
        image: None,
        workspace,
    })
}

/// Poll the child while readers drain both pipes. No shell, inherited stdin, or UI-thread wait.
pub fn run_process(
    command: &mut Command,
    directory: &Path,
    context: &JobContext,
    timeout: Duration,
) -> Result<String, String> {
    context.check()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(directory)
        .env("TEMP", directory)
        .env("TMP", directory)
        .env("DENO_NO_UPDATE_CHECK", "1")
        .env("DENO_NO_PROMPT", "1")
        .env("DENO_NO_PACKAGE_JSON", "1")
        .env("NO_COLOR", "1")
        .env("DENO_DIR", directory.join("deno-cache"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start media tool: {error}"))?;
    #[cfg(windows)]
    let process_tree = match ProcessTree::attach(&child) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let progress = context.clone();
    let output_reader = std::thread::spawn(move || read_output(stdout, Some(progress)));
    let error_reader = std::thread::spawn(move || read_output(stderr, None));
    let started = Instant::now();
    let result = loop {
        if let Err(error) = context.check() {
            break Err(error);
        }
        if started.elapsed() > timeout {
            break Err("The media task timed out. Please try again.".into());
        }
        let size: u64 = fs::read_dir(directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| entry.metadata().ok())
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .sum();
        if size > MAX_JOB_BYTES {
            break Err("Media exceeds the working file size limit.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(75)),
            Err(error) => break Err(error.to_string()),
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    // Closing the Windows job also terminates downloader helpers (Deno / FFmpeg).
    #[cfg(windows)]
    drop(process_tree);
    let _ = child.wait();
    let output = output_reader.join().unwrap_or_default();
    let errors = error_reader.join().unwrap_or_default();
    let status = result?;
    if !status.success() {
        return Err(format!("Media tool failed. {}", errors.trim()));
    }
    context.check()?;
    Ok(output)
}

#[cfg(windows)]
struct ProcessTree(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // The owned handle is closed in Drop. The OS copies the initialized limits structure.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let tree = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                return Err(format!(
                    "Could not supervise the media process: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(tree)
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        // This is the sole owner of the job handle.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn read_output(reader: impl Read, progress: Option<JobContext>) -> String {
    let mut reader = BufReader::new(reader);
    let mut retained = Vec::new();
    loop {
        // Bound individual lines as well as retained diagnostics.
        let mut line = Vec::new();
        match reader.by_ref().take(8192).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if let Some(progress) = &progress {
            let line = String::from_utf8_lossy(&line);
            if line.starts_with("Downloading audio:") {
                progress.report(line.trim().to_owned());
            }
        }
        retained.extend(line);
        if retained.len() > 64 * 1024 {
            retained.drain(..retained.len() - 64 * 1024);
        }
    }
    String::from_utf8_lossy(&retained).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_are_validated_before_downloading_or_starting_a_tool() {
        for value in [
            "file:///C:/secret",
            "https://user:pass@example.com/a",
            "--exec=bad",
            "https://example.com/\n",
        ] {
            // Leading/trailing whitespace is intentionally accepted for clipboard pastes.
            if value.ends_with('\n') {
                continue;
            }
            assert!(parse_url(value).is_err());
        }
        for value in [
            "https://youtube.com.evil.test/watch?v=x",
            "https://youtube.com/playlist?list=x",
            "https://tiktok.com/@person",
            "https://example.com/video",
        ] {
            assert!(video_url(value).is_err());
        }
        for value in [
            "https://www.youtube.com/watch?v=abc&list=x",
            "https://youtu.be/abc",
            "https://youtube.com/shorts/abc",
            "https://vm.tiktok.com/abc/",
            "https://www.tiktok.com/@name/video/123",
        ] {
            assert!(video_url(value).is_ok(), "{value}");
        }
    }
}
