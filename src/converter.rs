use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

use image::{AnimationDecoder, DynamicImage, GenericImageView, ImageBuffer, Rgba, imageops};
use thiserror::Error;

use crate::{APP_VERSION, domain::VisualSettings};

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("unsupported asset type: {0}")]
    Unsupported(String),
    #[error("failed to decode image: {0}")]
    Image(#[from] image::ImageError),
    #[error("failed to encode Valve texture: {0}")]
    Vtf(#[from] vtf::Error),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "the portable audio converter is missing; expected tools/ffmpeg.exe next to NextbotCreator.exe"
    )]
    FfmpegMissing,
    #[error("audio conversion failed: {0}")]
    FfmpegFailed(String),
    #[error("invalid audio clip: {0}")]
    InvalidAudio(String),
}

#[derive(Debug, Clone)]
pub struct VisualArtifact {
    pub vtf_path: PathBuf,
    pub vmt_path: PathBuf,
    pub icon_path: PathBuf,
    pub frame_count: usize,
    pub aspect_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct KillfeedIconArtifact {
    pub vtf_path: PathBuf,
    pub vmt_path: PathBuf,
}

struct DecodedVisual {
    frames: Vec<DynamicImage>,
    aspect_ratio: f32,
    original_vtf: Option<Vec<u8>>,
}

pub fn convert_visual(
    source: &Path,
    materials_root: &Path,
    entity_icon_root: &Path,
    material_relative: &str,
    class_name: &str,
    settings: &VisualSettings,
) -> Result<VisualArtifact, ConversionError> {
    let output_stem = materials_root.join(material_relative.replace('/', "\\"));
    if let Some(parent) = output_stem.parent() {
        fs::create_dir_all(parent).map_err(|source| ConversionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::create_dir_all(entity_icon_root).map_err(|source| ConversionError::Io {
        path: entity_icon_root.to_path_buf(),
        source,
    })?;
    let vtf_path = output_stem.with_extension("vtf");
    let vmt_path = output_stem.with_extension("vmt");
    let icon_path = entity_icon_root.join(format!("{class_name}.png"));

    let decoded = decode_visual(source)?;
    if decoded.frames.is_empty() {
        return Err(ConversionError::Unsupported(
            "image contains no frames".into(),
        ));
    }

    let size = settings.texture_size.clamp(64, 4096).next_power_of_two();
    let processed = decoded
        .frames
        .into_iter()
        .map(|frame| contain_square(frame, size))
        .collect::<Vec<_>>();
    if let Some(bytes) = decoded.original_vtf {
        fs::write(&vtf_path, bytes).map_err(|source| ConversionError::Io {
            path: vtf_path.clone(),
            source,
        })?;
    } else {
        let bytes = if processed.len() == 1 {
            vtf::create(processed[0].clone(), vtf::ImageFormat::Dxt5)?
        } else {
            let mut builder = vtf::create_animated(vtf::ImageFormat::Dxt5);
            for frame in &processed {
                builder = builder.add_frame(frame.clone())?;
            }
            builder.build()?
        };
        fs::write(&vtf_path, bytes).map_err(|source| ConversionError::Io {
            path: vtf_path.clone(),
            source,
        })?;
    }
    processed[0].save_with_format(&icon_path, image::ImageFormat::Png)?;
    write_vmt(&vmt_path, material_relative, processed.len() > 1, settings)?;

    Ok(VisualArtifact {
        vtf_path,
        vmt_path,
        icon_path,
        frame_count: processed.len(),
        aspect_ratio: decoded.aspect_ratio,
    })
}

pub fn convert_killfeed_icon(
    source: &Path,
    materials_root: &Path,
    material_relative: &str,
) -> Result<KillfeedIconArtifact, ConversionError> {
    let output_stem = materials_root.join(material_relative.replace('/', "\\"));
    if let Some(parent) = output_stem.parent() {
        fs::create_dir_all(parent).map_err(|source| ConversionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let decoded = decode_visual(source)?;
    let frame = decoded
        .frames
        .into_iter()
        .next()
        .ok_or_else(|| ConversionError::Unsupported("image contains no frames".into()))?;
    let processed = contain_square(frame, 128);
    let vtf_path = output_stem.with_extension("vtf");
    let vmt_path = output_stem.with_extension("vmt");
    let bytes = vtf::create(processed, vtf::ImageFormat::Dxt5)?;
    fs::write(&vtf_path, bytes).map_err(|source| ConversionError::Io {
        path: vtf_path.clone(),
        source,
    })?;
    write_vmt(
        &vmt_path,
        material_relative,
        false,
        &VisualSettings::default(),
    )?;
    Ok(KillfeedIconArtifact { vtf_path, vmt_path })
}

fn decode_visual(source: &Path) -> Result<DecodedVisual, ConversionError> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (frames, aspect_ratio, original_vtf) = match extension.as_str() {
        "vtf" => {
            let bytes = fs::read(source).map_err(|source_error| ConversionError::Io {
                path: source.to_path_buf(),
                source: source_error,
            })?;
            let parsed = vtf::from_bytes(&bytes)?;
            let frames = decode_vtf_frames(&parsed)?;
            let ratio = frames[0].width() as f32 / frames[0].height().max(1) as f32;
            (frames, ratio, Some(bytes))
        }
        "vmt" => {
            let sibling_vtf = source.with_extension("vtf");
            if !sibling_vtf.is_file() {
                return Err(ConversionError::Unsupported(
                    "a selected VMT must have a same-named VTF beside it".into(),
                ));
            }
            let bytes = fs::read(&sibling_vtf).map_err(|source| ConversionError::Io {
                path: sibling_vtf.clone(),
                source,
            })?;
            let parsed = vtf::from_bytes(&bytes)?;
            let frames = decode_vtf_frames(&parsed)?;
            let ratio = frames[0].width() as f32 / frames[0].height().max(1) as f32;
            (frames, ratio, Some(bytes))
        }
        "gif" => {
            let file = fs::File::open(source).map_err(|source_error| ConversionError::Io {
                path: source.to_path_buf(),
                source: source_error,
            })?;
            let decoder = image::codecs::gif::GifDecoder::new(BufReader::new(file))?;
            let decoded = decoder.into_frames().collect_frames()?;
            let ratio = decoded
                .first()
                .map(|frame| frame.buffer().width() as f32 / frame.buffer().height().max(1) as f32)
                .unwrap_or(1.0);
            let frames = decoded
                .into_iter()
                .map(|frame| DynamicImage::ImageRgba8(frame.into_buffer()))
                .collect();
            (frames, ratio, None)
        }
        "png" | "jpg" | "jpeg" | "bmp" | "tga" | "webp" => {
            let image = image::open(source)?;
            let ratio = image.width() as f32 / image.height().max(1) as f32;
            (vec![image], ratio, None)
        }
        other => return Err(ConversionError::Unsupported(other.to_owned())),
    };
    if frames.is_empty() {
        return Err(ConversionError::Unsupported(
            "image contains no frames".into(),
        ));
    }

    Ok(DecodedVisual {
        frames,
        aspect_ratio,
        original_vtf,
    })
}

fn decode_vtf_frames(texture: &vtf::vtf::VTF<'_>) -> Result<Vec<DynamicImage>, ConversionError> {
    let frame_count = u32::from(texture.header.frames.max(1));
    (0..frame_count)
        .map(|frame| texture.highres_image.decode(frame).map_err(Into::into))
        .collect()
}

fn contain_square(image: DynamicImage, size: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    let scale = (size as f32 / width.max(1) as f32).min(size as f32 / height.max(1) as f32);
    let resized_width = ((width as f32 * scale).round() as u32).clamp(1, size);
    let resized_height = ((height as f32 * scale).round() as u32).clamp(1, size);
    let resized = image
        .resize_exact(
            resized_width,
            resized_height,
            imageops::FilterType::Lanczos3,
        )
        .to_rgba8();
    let mut canvas = ImageBuffer::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    imageops::overlay(
        &mut canvas,
        &resized,
        ((size - resized_width) / 2) as i64,
        ((size - resized_height) / 2) as i64,
    );
    DynamicImage::ImageRgba8(canvas)
}

fn write_vmt(
    path: &Path,
    material_relative: &str,
    animated: bool,
    settings: &VisualSettings,
) -> Result<(), ConversionError> {
    let shader = if settings.unlit {
        "UnlitGeneric"
    } else {
        "VertexLitGeneric"
    };
    let mut text = format!(
        "// This nextbot was created by NextbotCreator {APP_VERSION}\n\"{shader}\"\n{{\n    \"$basetexture\" \"{}\"\n    \"$vertexcolor\" \"1\"\n    \"$vertexalpha\" \"1\"\n",
        material_relative.replace('\\', "/")
    );
    if settings.translucent {
        text.push_str("    \"$translucent\" \"1\"\n");
    }
    if animated {
        text.push_str(&format!(
            "    \"Proxies\"\n    {{\n        \"AnimatedTexture\"\n        {{\n            \"animatedTextureVar\" \"$basetexture\"\n            \"animatedTextureFrameNumVar\" \"$frame\"\n            \"animatedTextureFrameRate\" \"{}\"\n        }}\n    }}\n",
            settings.frames_per_second.clamp(0.1, 120.0)
        ));
    }
    text.push_str("}\n");
    fs::write(path, text).map_err(|source| ConversionError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub fn ffmpeg_path(portable_root: &Path) -> Option<PathBuf> {
    std::env::var_os("NEXTBOTCREATOR_FFMPEG")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let bundled = portable_root.join("tools").join("ffmpeg.exe");
            bundled.is_file().then_some(bundled)
        })
        .or_else(|| {
            Command::new("ffmpeg")
                .arg("-version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|_| PathBuf::from("ffmpeg"))
        })
}

pub fn convert_audio(
    source: &Path,
    destination: &Path,
    portable_root: &Path,
) -> Result<(), ConversionError> {
    convert_audio_clip(&source.to_path_buf().into(), destination, portable_root)
}

pub fn convert_audio_clip(
    clip: &crate::domain::AudioClip,
    destination: &Path,
    portable_root: &Path,
) -> Result<(), ConversionError> {
    clip.trim
        .validate()
        .map_err(|error| ConversionError::InvalidAudio(error.into()))?;
    let source = &clip.source;
    let ffmpeg = ffmpeg_path(portable_root).ok_or(ConversionError::FfmpegMissing)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| ConversionError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut command = Command::new(ffmpeg);
    command
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
        .arg(source)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "1",
            "-ar",
            "44100",
            "-c:a",
            "pcm_s16le",
        ]);
    if clip.trim.start > 0.0 {
        command.arg("-ss").arg(format!("{:.9}", clip.trim.start));
    }
    if let Some(end) = clip.trim.end {
        command
            .arg("-t")
            .arg(format!("{:.9}", end - clip.trim.start));
    }
    command.arg(destination);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(|source| ConversionError::Io {
        path: destination.to_path_buf(),
        source,
    })?;
    if !output.status.success() {
        return Err(ConversionError::FfmpegFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let wave = hound::WavReader::open(destination)
        .map_err(|error| ConversionError::InvalidAudio(error.to_string()))?;
    let duration = f64::from(wave.duration()) / f64::from(wave.spec().sample_rate);
    if duration <= 0.0
        || clip
            .trim
            .end
            .is_some_and(|end| duration + 0.001 < end - clip.trim.start)
    {
        return Err(ConversionError::InvalidAudio(
            "Trim extends beyond the source audio. Open Preview / trim and adjust it.".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, Frame, RgbaImage, codecs::gif::GifEncoder};

    fn temp_folder(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nextbot_creator_converter_{name}_{}",
            std::process::id()
        ))
    }

    #[test]
    fn image_containment_preserves_square_output() {
        let input = DynamicImage::new_rgba8(400, 200);
        let output = contain_square(input, 512);
        assert_eq!(output.dimensions(), (512, 512));
    }

    #[test]
    fn gif_conversion_creates_animated_vtf_and_proxy() {
        let root = temp_folder("gif");
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        let gif_path = root.join("animated.gif");
        {
            let file = fs::File::create(&gif_path).unwrap();
            let mut encoder = GifEncoder::new(file);
            let frames = [Rgba([255, 0, 255, 255]), Rgba([0, 0, 0, 255])]
                .into_iter()
                .map(|color| {
                    Frame::from_parts(
                        RgbaImage::from_pixel(8, 4, color),
                        0,
                        0,
                        Delay::from_numer_denom_ms(100, 1),
                    )
                });
            encoder.encode_frames(frames).unwrap();
        }
        let settings = VisualSettings {
            texture_size: 64,
            frames_per_second: 12.5,
            ..VisualSettings::default()
        };
        let artifact = convert_visual(
            &gif_path,
            &root.join("materials"),
            &root.join("materials/entities"),
            "nextbotcreator/test/npc_test",
            "npc_test",
            &settings,
        )
        .unwrap();
        assert_eq!(artifact.frame_count, 2);
        let vtf_bytes = fs::read(&artifact.vtf_path).unwrap();
        assert_eq!(vtf::from_bytes(&vtf_bytes).unwrap().header.frames, 2);
        let vmt = fs::read_to_string(&artifact.vmt_path).unwrap();
        assert!(vmt.contains("AnimatedTexture"));
        assert!(vmt.contains("12.5"));

        let copied = convert_visual(
            &artifact.vtf_path,
            &root.join("copied/materials"),
            &root.join("copied/materials/entities"),
            "nextbotcreator/test/npc_copy",
            "npc_copy",
            &settings,
        )
        .unwrap();
        assert_eq!(copied.frame_count, 2);
        assert!(
            fs::read_to_string(copied.vmt_path)
                .unwrap()
                .contains("AnimatedTexture")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn killfeed_conversion_uses_a_static_128_pixel_valve_texture() {
        let root = temp_folder("killfeed");
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        let source = root.join("icon.png");
        RgbaImage::from_pixel(20, 10, Rgba([255, 0, 255, 255]))
            .save(&source)
            .unwrap();
        let artifact = convert_killfeed_icon(
            &source,
            &root.join("materials"),
            "nextbotcreator/test/npc_test_killfeed",
        )
        .unwrap();
        let texture_bytes = fs::read(&artifact.vtf_path).unwrap();
        let texture = vtf::from_bytes(&texture_bytes).unwrap();
        let decoded = texture.highres_image.decode(0).unwrap();
        assert_eq!(decoded.dimensions(), (128, 128));
        assert_eq!(texture.header.frames, 1);
        assert!(
            !fs::read_to_string(artifact.vmt_path)
                .unwrap()
                .contains("AnimatedTexture")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
