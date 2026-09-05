use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use nextbot_creator::audio::PreviewPlayer;
use nextbot_creator::domain::{AudioClip, AudioSlot, AudioTrim};
use nextbot_creator::media::{self, MediaJob, PreparedMedia};

#[derive(Clone, Copy)]
pub enum MediaTarget {
    Visual,
    Killfeed,
    Audio {
        slot: AudioSlot,
        index: Option<usize>,
    },
}

pub struct MediaDialog {
    pub target: MediaTarget,
    pub project_root: PathBuf,
    pub bot_class: String,
    pub prepared: Option<PreparedMedia>,
    pub trim: AudioTrim,
    pub error: String,
    pub closed: bool,
    pub original_clip: Option<AudioClip>,
    url: String,
    job: Option<MediaJob<PreparedMedia>>,
    cancelling: bool,
    texture: Option<egui::TextureHandle>,
    player: PreviewPlayer,
    cursor: f64,
    preview_volume: f32,
    loop_preview: bool,
    zoom: f64,
    view_start: f64,
    dragged_handle: Option<bool>,
}

impl MediaDialog {
    pub fn new(target: MediaTarget, project_root: PathBuf, bot_class: String) -> Self {
        Self {
            target,
            project_root,
            bot_class,
            prepared: None,
            trim: AudioTrim::default(),
            error: String::new(),
            closed: false,
            original_clip: None,
            url: String::new(),
            job: None,
            cancelling: false,
            texture: None,
            player: PreviewPlayer::default(),
            cursor: 0.0,
            preview_volume: 0.7,
            loop_preview: false,
            zoom: 1.0,
            view_start: 0.0,
            dragged_handle: None,
        }
    }

    pub fn edit(&mut self, clip: AudioClip, portable_root: &Path) {
        self.trim = clip.trim;
        self.cursor = clip.trim.start;
        let source = clip.source.clone();
        let root = portable_root.to_owned();
        self.original_clip = Some(clip);
        self.job = Some(MediaJob::start(move |context| {
            media::prepare_local_audio(&source, &root, &context)
        }));
    }

    fn cancel(&mut self) {
        self.player.stop();
        if let Some(job) = &self.job {
            job.context.cancel();
            self.cancelling = true;
        } else {
            self.closed = true;
        }
    }

    /// Returns true only when the user accepts the prepared asset or trim.
    pub fn show(&mut self, context: &egui::Context, portable_root: &Path) -> bool {
        if let Some(result) = self.job.as_mut().and_then(MediaJob::poll) {
            self.job = None;
            if self.cancelling {
                self.closed = true;
                return false;
            }
            match result {
                Ok(prepared) => {
                    if let Some(image) = &prepared.image {
                        self.texture = Some(context.load_texture(
                            "url_image_preview",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    if let Some(audio) = &prepared.audio {
                        self.error = self
                            .trim
                            .range(audio.duration)
                            .err()
                            .unwrap_or_default()
                            .to_owned();
                    }
                    self.prepared = Some(prepared);
                }
                Err(error) => self.error = error,
            }
        }
        let mut accept = false;
        let response = egui::Modal::new(egui::Id::new("media_import")).show(context, |ui| {
            ui.set_width(640.0_f32.min(context.content_rect().width() - 60.0));
            ui.heading(match self.target { MediaTarget::Audio { .. } => "Audio preview & trim", _ => "Import image from URL" });
            ui.add_space(8.0);
            if let Some(job) = &self.job {
                ui.horizontal(|ui| { ui.spinner(); ui.label(if self.cancelling { "Cancelling...".into() } else { job.context.progress() }); });
                if ui.add_enabled(!self.cancelling, egui::Button::new("Cancel")).clicked() { self.cancel(); }
                context.request_repaint_after(Duration::from_millis(75));
            } else if self.prepared.is_some() {
                ui.label(RichText::new(&self.prepared.as_ref().unwrap().title).strong());
                if let Some(texture) = &self.texture {
                    ui.add(egui::Image::new(texture).max_size(egui::vec2(360.0, 280.0)));
                    ui.label("The original image or GIF is kept in your project and converted automatically when generating.");
                }
                if self.prepared.as_ref().unwrap().audio.is_some() { self.trim_editor(ui); }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let valid = self.prepared.as_ref().unwrap().audio.as_ref().is_none_or(|audio| self.trim.range(audio.duration).is_ok());
                    if ui.add_enabled(valid, egui::Button::new(if matches!(self.target, MediaTarget::Audio { .. }) { "Use clip" } else { "Use image" })).clicked() {
                        self.player.stop(); accept = true;
                    }
                    if ui.button("Cancel").clicked() { self.cancel(); }
                });
            } else {
                let audio = matches!(self.target, MediaTarget::Audio { .. });
                if self.original_clip.is_some() {
                    if ui.button("Retry audio preview").clicked() {
                        self.error.clear();
                        self.edit(self.original_clip.clone().unwrap(), portable_root);
                    }
                } else {
                    ui.label(if audio { "Paste a public YouTube or TikTok video link." } else { "Paste an image address from your browser (Copy image address)." });
                    let input = ui.add(egui::TextEdit::singleline(&mut self.url).hint_text("https://...").desired_width(f32::INFINITY));
                    if input.gained_focus() { self.error.clear(); }
                    ui.label(RichText::new(if audio { "Individual videos up to 60 minutes / 256 MiB. Includes Shorts and TikTok share links." } else { "PNG, JPEG, GIF, WebP, BMP, or TGA. Up to 32 MiB and 8192 × 8192 pixels." }).small().weak());
                    let enter = input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(!self.url.trim().is_empty(), egui::Button::new("Download")).clicked() || enter {
                        let validation = if audio { media::video_url(&self.url) } else { media::parse_url(&self.url) };
                        match validation {
                            Err(error) => self.error = error,
                            Ok(url) => {
                                self.error.clear();
                                let root = portable_root.to_owned();
                                self.job = Some(MediaJob::start(move |context| {
                                    if audio { media::fetch_audio(url.as_str(), &root, &context) }
                                    else { media::fetch_image(url.as_str(), &root, &context) }
                                }));
                            }
                        }
                    }
                }
                if ui.button("Cancel").clicked() { self.cancel(); }
            }
            if !self.error.is_empty() {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| { ui.colored_label(Color32::from_rgb(242, 126, 126), &self.error); });
            }
        });
        if response.should_close() {
            self.cancel();
        }
        accept
    }

    fn trim_editor(&mut self, ui: &mut egui::Ui) {
        let audio = self.prepared.as_ref().unwrap().audio.as_ref().unwrap();
        let duration = audio.duration;
        let old_trim = self.trim;
        let mut end = self.trim.end.unwrap_or(duration);
        ui.label(format!(
            "Source: {} • Selection: {}",
            timestamp(duration),
            timestamp((end - self.trim.start).max(0.0))
        ));
        ui.horizontal(|ui| {
            ui.label("Start (s)");
            ui.add(
                egui::DragValue::new(&mut self.trim.start)
                    .speed(0.01)
                    .max_decimals(3)
                    .range(0.0..=duration),
            );
            ui.label("End (s)");
            if ui
                .add(
                    egui::DragValue::new(&mut end)
                        .speed(0.01)
                        .max_decimals(3)
                        .range(0.0..=duration),
                )
                .changed()
            {
                self.trim.end = Some(end);
            }
            if ui.button("Reset trim").clicked() {
                self.trim = AudioTrim::default();
                end = duration;
                self.cursor = 0.0;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Zoom");
            ui.add(
                egui::Slider::new(&mut self.zoom, 1.0..=64.0)
                    .logarithmic(true)
                    .show_value(false),
            );
            let width = duration / self.zoom;
            self.view_start = self.view_start.clamp(0.0, duration - width);
            if self.zoom > 1.0 {
                ui.add(
                    egui::Slider::new(&mut self.view_start, 0.0..=duration - width)
                        .text("Position")
                        .show_value(false),
                );
            }
        });
        let view_width = duration / self.zoom;
        let view_end = self.view_start + view_width;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 140.0),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 5.0, Color32::from_rgb(19, 22, 28));
        let x_for =
            |time: f64| rect.left() + ((time - self.view_start) / view_width) as f32 * rect.width();
        let start_x = x_for(self.trim.start);
        let end_x = x_for(end);
        let selection = egui::Rect::from_min_max(
            egui::pos2(start_x.max(rect.left()), rect.top()),
            egui::pos2(
                end_x.min(rect.right()).max(start_x.max(rect.left())),
                rect.bottom(),
            ),
        )
        .intersect(rect);
        painter.rect_filled(selection, 0.0, Color32::from_rgb(34, 53, 69));
        for (index, peak) in audio.peaks.iter().enumerate() {
            let time = index as f64 / audio.peaks.len() as f64 * duration;
            if time < self.view_start || time > view_end {
                continue;
            }
            let x = x_for(time);
            painter.line_segment(
                [
                    egui::pos2(x, rect.center().y - peak[1] * 58.0),
                    egui::pos2(x, rect.center().y - peak[0] * 58.0),
                ],
                (1.0, Color32::from_rgb(119, 181, 203)),
            );
        }
        for x in [start_x, end_x] {
            if x >= rect.left() && x <= rect.right() {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    (3.0, Color32::from_rgb(226, 176, 99)),
                );
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        egui::pos2(x, rect.top() + 8.0),
                        egui::vec2(10.0, 16.0),
                    ),
                    2.0,
                    Color32::from_rgb(226, 176, 99),
                );
            }
        }
        if self.player.active() {
            self.cursor = self.player.position().min(end);
        }
        let cursor_x = x_for(self.cursor);
        painter.line_segment(
            [
                egui::pos2(cursor_x, rect.top()),
                egui::pos2(cursor_x, rect.bottom()),
            ],
            (1.0, Color32::WHITE),
        );
        if response.drag_started()
            && let Some(origin) = ui.input(|input| input.pointer.press_origin())
        {
            self.dragged_handle =
                if (origin.x - start_x).abs() <= 14.0 || (origin.x - end_x).abs() <= 14.0 {
                    Some((origin.x - start_x).abs() <= (origin.x - end_x).abs())
                } else {
                    None
                };
        }
        if (response.clicked() || response.dragged())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let time = (self.view_start
                + f64::from((pointer.x - rect.left()) / rect.width()) * view_width)
                .clamp(0.0, duration);
            match self.dragged_handle.filter(|_| response.dragged()) {
                Some(true) => self.trim.start = time.min(end - 0.001).max(0.0),
                Some(false) => {
                    self.trim.end = Some(time.max(self.trim.start + 0.001).min(duration));
                }
                None => {
                    self.player.stop();
                    self.loop_preview = false;
                    self.cursor = time;
                }
            }
        }
        if response.drag_stopped() {
            self.dragged_handle = None;
        }
        ui.horizontal(|ui| {
            ui.small(timestamp(self.view_start));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small(timestamp(view_end));
            });
        });
        ui.label(RichText::new("Drag the gold handles to trim. Click the waveform to seek. The original audio is preserved.").small().weak());
        if self.trim != old_trim {
            self.player.stop();
            self.loop_preview = false;
            self.cursor = self.trim.start;
            self.error.clear();
        }
        let valid = self.trim.range(duration);
        if let Err(error) = valid {
            ui.colored_label(Color32::YELLOW, error);
        }
        ui.horizontal(|ui| {
            let active = self.player.active();
            let label = if active && !self.player.paused() {
                "Pause"
            } else if active {
                "Resume"
            } else {
                "Play selection"
            };
            if ui
                .add_enabled(valid.is_ok(), egui::Button::new(label))
                .clicked()
            {
                if active {
                    self.player.toggle_pause();
                } else {
                    if self.cursor >= end || self.cursor < self.trim.start {
                        self.cursor = self.trim.start;
                    }
                    if let Err(error) =
                        self.player
                            .play(audio, self.trim, self.cursor, self.preview_volume)
                    {
                        self.error = error;
                        self.loop_preview = false;
                    }
                }
            }
            if ui.button("Stop").clicked() {
                self.player.stop();
                self.loop_preview = false;
                self.cursor = self.trim.start;
            }
            ui.checkbox(&mut self.loop_preview, "Loop preview");
            if ui
                .add(egui::Slider::new(&mut self.preview_volume, 0.0..=1.0).text("Preview volume"))
                .changed()
            {
                self.player.set_volume(self.preview_volume);
            }
        });
        if self.loop_preview
            && !self.player.active()
            && valid.is_ok()
            && let Err(error) =
                self.player
                    .play(audio, self.trim, self.trim.start, self.preview_volume)
        {
            self.error = error;
            self.loop_preview = false;
        }
        if self.player.active() || self.loop_preview {
            ui.ctx().request_repaint_after(Duration::from_millis(30));
        }
    }
}

impl Drop for MediaDialog {
    fn drop(&mut self) {
        // Release the decoder's file handle before the staging directory is removed on Windows.
        self.player.stop();
    }
}

fn timestamp(seconds: f64) -> String {
    let milliseconds = (seconds.max(0.0) * 1000.0).round() as u64;
    format!(
        "{}:{:02}.{:03}",
        milliseconds / 60000,
        milliseconds / 1000 % 60,
        milliseconds % 1000
    )
}
