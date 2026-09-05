use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use nextbot_creator::domain::{AudioClip, AudioSlot, AudioTrim, Project};
use nextbot_creator::{audio, converter, generator, media, persistence};

fn serve(responses: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/image", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0_u8; 8192];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(&response);
        }
    });
    (url, worker)
}

fn response(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

#[test]
fn image_download_follows_redirects_sniffs_content_and_keeps_the_original() {
    let root = tempfile::tempdir().unwrap();
    let original = fs::read("assets/app-icon.png").unwrap();
    let (url, server) = serve(vec![b"HTTP/1.1 302 Found\r\nLocation: /actual-image?token=123\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(), response(&original)]);
    let prepared = media::fetch_image(&url, root.path(), &Default::default()).unwrap();
    server.join().unwrap();
    assert_eq!(fs::read(&prepared.source).unwrap(), original);
    assert_eq!(prepared.source.extension().unwrap(), "png");
    assert!(prepared.image.is_some());
    let staging = prepared.workspace.path().to_owned();
    let project = persistence::create_project(root.path(), "Imported image").unwrap();
    let imported = persistence::import_source_asset(
        &project,
        &project.nextbots[0].class_name,
        &prepared.source,
    )
    .unwrap();
    assert!(imported.starts_with(&project.root));
    drop(prepared);
    assert!(!staging.exists());
    assert_eq!(fs::read(imported).unwrap(), original);
}

#[test]
fn image_failures_and_cancelled_jobs_clean_up_without_importing() {
    let root = tempfile::tempdir().unwrap();
    for data in [response(b"<html>Not an image</html>"), b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\nConnection: close\r\n\r\n".to_vec(), b"HTTP/1.1 302 Found\r\nLocation: file:///C:/Windows/win.ini\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()] {
        let (url, server) = serve(vec![data]);
        assert!(media::fetch_image(&url, root.path(), &Default::default()).is_err());
        server.join().unwrap();
        assert_eq!(fs::read_dir(root.path().join(".media-cache")).unwrap().count(), 0);
    }
    let context = media::JobContext::default();
    context.cancel();
    assert!(media::fetch_image("https://example.com/image.png", root.path(), &context).is_err());
    assert_eq!(
        fs::read_dir(root.path().join(".media-cache"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn download_limit_is_enforced_without_a_content_length() {
    let root = tempfile::tempdir().unwrap();
    let (url, server) = serve(vec![
        b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n123456789".to_vec(),
    ]);
    let result = media::download(&url, &root.path().join("download"), 4, &Default::default());
    server.join().unwrap();
    assert!(result.unwrap_err().contains("limit"));
}

#[test]
fn downloaded_gifs_retain_animation_through_addon_generation() {
    use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut gif);
        for color in [Rgba([255, 0, 0, 255]), Rgba([0, 0, 255, 255])] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(16, 16, color),
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    let root = tempfile::tempdir().unwrap();
    let (url, server) = serve(vec![response(&gif)]);
    let prepared = media::fetch_image(&url, root.path(), &Default::default()).unwrap();
    server.join().unwrap();
    let artifact = converter::convert_visual(
        &prepared.source,
        &root.path().join("materials"),
        &root.path().join("icons"),
        "nextbotcreator/test/sprite",
        "npc_test",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(artifact.frame_count, 2);
    assert_eq!(fs::read(&prepared.source).unwrap(), gif);
    let material = fs::read_to_string(artifact.vmt_path).unwrap();
    assert!(material.starts_with(&format!(
        "// This nextbot was created by NextbotCreator {}",
        nextbot_creator::APP_VERSION
    )));
    assert!(material.contains("AnimatedTexture"));
}

#[test]
fn legacy_audio_loads_and_edited_clips_survive_project_relocation() {
    let root = tempfile::tempdir().unwrap();
    let mut project = persistence::create_project(root.path(), "Legacy").unwrap();
    fs::create_dir_all(project.root.join("source_assets")).unwrap();
    fs::write(project.root.join("source_assets/old.wav"), b"original").unwrap();
    let mut legacy = serde_json::to_value(&project).unwrap();
    legacy["format_version"] = 1.into();
    for slot in AudioSlot::ALL {
        legacy["nextbots"][0]["audio"][slot.key()] = serde_json::json!(["source_assets/old.wav"]);
    }
    fs::write(
        project.root.join(nextbot_creator::PROJECT_FILE),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    project = persistence::load_project(&project.root).unwrap();
    for slot in AudioSlot::ALL {
        let clip = &mut slot.get_mut(&mut project.nextbots[0].audio)[0];
        assert_eq!(clip.trim, AudioTrim::default());
        clip.trim = AudioTrim {
            start: 1.25,
            end: Some(2.75),
        };
        clip.source_url = Some("https://youtu.be/test".into());
    }
    persistence::save_project(&project).unwrap();
    let json = fs::read_to_string(project.root.join(nextbot_creator::PROJECT_FILE)).unwrap();
    assert!(!json.contains(&root.path().to_string_lossy().replace('\\', "\\\\")));
    let moved = root.path().join("relocated");
    fs::rename(&project.root, &moved).unwrap();
    let loaded = persistence::load_project(&moved).unwrap();
    for slot in AudioSlot::ALL {
        let clip = &slot.get(&loaded.nextbots[0].audio)[0];
        assert_eq!(clip.source, moved.join("source_assets/old.wav"));
        assert_eq!(
            clip.trim,
            AudioTrim {
                start: 1.25,
                end: Some(2.75)
            }
        );
        assert_eq!(clip.source_url.as_deref(), Some("https://youtu.be/test"));
    }
}

#[test]
fn invalid_trim_times_are_rejected_before_generation() {
    assert!(
        AudioTrim {
            start: 0.0,
            end: Some(0.000001)
        }
        .range(1.0)
        .is_err()
    );
    let mut project = Project::new("Invalid trim", PathBuf::from("unused"));
    for trim in [
        AudioTrim {
            start: -1.0,
            end: None,
        },
        AudioTrim {
            start: f64::NAN,
            end: None,
        },
        AudioTrim {
            start: 2.0,
            end: Some(1.0),
        },
        AudioTrim {
            start: 0.0,
            end: Some(f64::INFINITY),
        },
    ] {
        project.nextbots[0].audio.spawn = vec![AudioClip {
            source: "source.wav".into(),
            trim,
            source_url: None,
        }];
        assert!(generator::validate_project(&project).is_err());
    }
    assert!(
        AudioTrim {
            start: 2.0,
            end: None
        }
        .range(1.0)
        .is_err()
    );
}

#[cfg(windows)]
#[test]
fn cancelling_a_media_process_terminates_its_helpers_and_cleans_staging() {
    let root = tempfile::tempdir().unwrap();
    let workspace = media::workspace(root.path()).unwrap();
    let context = media::JobContext::default();
    let cancellation = context.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        cancellation.cancel();
    });
    let started = Instant::now();
    let result = media::run_process(
        Command::new("powershell.exe").args([
            "-NoProfile",
            "-Command",
            "& powershell.exe -NoProfile -Command 'Start-Sleep -Seconds 30'",
        ]),
        workspace.path(),
        &context,
        Duration::from_secs(15),
    );
    worker.join().unwrap();
    assert_eq!(result.unwrap_err(), "Cancelled.");
    assert!(started.elapsed() < Duration::from_secs(5));
    let path = workspace.path().to_owned();
    drop(workspace);
    assert!(!path.exists());
}

fn smoke_root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("NEXTBOTCREATOR_SMOKE_ROOT")
            .expect("Set NEXTBOTCREATOR_SMOKE_ROOT to the full portable bundle"),
    )
}

#[test]
#[ignore = "Requires bundled FFmpeg; run with NEXTBOTCREATOR_SMOKE_ROOT"]
fn audio_conversion_trimming_and_waveform_smoke() {
    let portable = smoke_root();
    let temp = media::workspace(&portable).unwrap();
    let mut project = persistence::create_project(temp.path(), "Trim smoke").unwrap();
    let source = project.root.join("original.wav");
    let mut writer = hound::WavWriter::create(
        &source,
        hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for index in 0..44100 * 4 {
        writer
            .write_sample(((index / 44100 + 1) * 2000) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();
    let original = fs::read(&source).unwrap();
    let prepared = media::prepare_local_audio(&source, &portable, &Default::default()).unwrap();
    let preview = prepared.audio.as_ref().unwrap();
    assert_eq!(preview.duration, 4.0);
    assert!(preview.peaks.len() <= 2048);
    assert!(preview.peaks.last().unwrap()[1] > preview.peaks.first().unwrap()[1]);
    let trim = AudioTrim {
        start: 1.25,
        end: Some(2.75),
    };
    let imported =
        persistence::import_source_asset(&project, &project.nextbots[0].class_name, &source)
            .unwrap();
    project.nextbots[0].audio.spawn.push(AudioClip {
        source: imported.clone(),
        trim,
        source_url: None,
    });
    persistence::save_project(&project).unwrap();
    let project = persistence::load_project(&project.root).unwrap();
    generator::generate_project(&project, &portable).unwrap();
    let output = project
        .root
        .join("sound/nextbotcreator/trim_smoke/npc_my_nextbot/spawn_01.wav");
    let mut wave = hound::WavReader::open(&output).unwrap();
    assert_eq!(wave.duration(), 66150);
    assert_eq!(wave.samples::<i16>().next().unwrap().unwrap(), 4000);
    assert_eq!(fs::read(&imported).unwrap(), original);
    let mut clip: AudioClip = imported.into();
    clip.trim = AudioTrim {
        start: 5.0,
        end: Some(6.0),
    };
    assert!(
        converter::convert_audio_clip(&clip, &temp.path().join("invalid.wav"), &portable).is_err()
    );
    // Resetting trim restores the complete source, without a new download.
    clip.trim = AudioTrim::default();
    converter::convert_audio_clip(&clip, &temp.path().join("reset.wav"), &portable).unwrap();
    assert_eq!(
        hound::WavReader::open(temp.path().join("reset.wav"))
            .unwrap()
            .duration(),
        176400
    );
}

#[test]
#[ignore = "Requires Windows audio output and bundled FFmpeg"]
fn audio_playback_smoke() {
    let portable = smoke_root();
    let temp = media::workspace(&portable).unwrap();
    let source = temp.path().join("tone.wav");
    assert!(
        Command::new(converter::ffmpeg_path(&portable).unwrap())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-ac",
                "1",
                "-ar",
                "44100"
            ])
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let preview = audio::waveform(&source, &Default::default()).unwrap();
    let mut player = audio::PreviewPlayer::default();
    player
        .play(
            &preview,
            AudioTrim {
                start: 0.25,
                end: Some(1.5),
            },
            0.5,
            0.05,
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(player.active());
    assert!(player.position() > 0.5);
    player.toggle_pause();
    assert!(player.paused());
    player.toggle_pause();
    assert!(!player.paused());
    player.stop();
    assert!(!player.active());
}

#[test]
#[ignore = "Live network smoke; set NEXTBOTCREATOR_SMOKE_URL to a public video"]
fn public_video_download_smoke() {
    let portable = smoke_root();
    let url = std::env::var("NEXTBOTCREATOR_SMOKE_URL").expect("Set NEXTBOTCREATOR_SMOKE_URL");
    let prepared = media::fetch_audio(&url, &portable, &Default::default()).unwrap();
    assert!(prepared.source.is_file());
    assert!(prepared.audio.as_ref().unwrap().duration > 0.0);
    assert_eq!(prepared.source_url.as_deref(), Some(url.as_str()));
}

#[test]
#[ignore = "Downloads the official downloader to a private staging folder"]
fn downloader_update_smoke() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("tools")).unwrap();
    fs::write(root.path().join("tools/yt-dlp.exe"), b"previous version").unwrap();
    let message =
        nextbot_creator::media_tools::update_downloader(root.path(), &Default::default()).unwrap();
    assert!(message.starts_with("Downloader updated to "));
    assert!(
        fs::metadata(root.path().join("tools/yt-dlp.exe"))
            .unwrap()
            .len()
            > 1024 * 1024
    );
    assert!(
        fs::metadata(root.path().join("tools/yt-dlp-third-party-licenses.txt"))
            .unwrap()
            .len()
            > 1024
    );
    assert_eq!(
        fs::read_dir(root.path().join(".media-cache"))
            .unwrap()
            .count(),
        0
    );
}
