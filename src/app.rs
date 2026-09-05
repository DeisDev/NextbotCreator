use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use crate::media_dialog::{MediaDialog, MediaTarget};
use nextbot_creator::catalog::{PropertySection, PropertySpec, property_catalog};
use nextbot_creator::converter;
use nextbot_creator::domain::{
    ATTACK_ACTIVITIES, AudioSlot, BaseVariant, BehaviorPreset, BindTrigger, DAMAGE_TYPES,
    HookAction, HookActionKind, HookEvent, HookRecipe, KillfeedIconMode, Nextbot, POSSESSION_KEYS,
    PossessionAction, PossessionBind, PossessionView, Project, PropertyValue, SpawnTab,
    sanitize_class_name, slugify,
};
use nextbot_creator::generator;
use nextbot_creator::integration::{self, LinkStatus};
use nextbot_creator::persistence::{self, AppSettings};
use nextbot_creator::updates::{UpdateChecker, UpdateOutcome, UpdateStatus};
use nextbot_creator::{APP_NAME, APP_VERSION, PROJECT_FILE};

mod settings;
mod ui;

use settings::SettingsPage;
use ui::{Icon, badge, external_link, icon_button, navigation, search_field, section_heading};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EditorPage {
    Basic,
    Visual,
    Audio,
    Combat,
    Possession,
    Events,
    Advanced,
    Project,
}

impl EditorPage {
    const ALL: [Self; 8] = [
        Self::Basic,
        Self::Visual,
        Self::Audio,
        Self::Combat,
        Self::Possession,
        Self::Events,
        Self::Advanced,
        Self::Project,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Basic => "Overview",
            Self::Visual => "Appearance",
            Self::Audio => "Sounds",
            Self::Combat => "Combat",
            Self::Possession => "Possession",
            Self::Events => "Events",
            Self::Advanced => "Advanced",
            Self::Project => "Project settings",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Basic => Icon::Overview,
            Self::Visual => Icon::Image,
            Self::Audio => Icon::Audio,
            Self::Combat => Icon::Combat,
            Self::Possession => Icon::Game,
            Self::Events => Icon::Events,
            Self::Advanced => Icon::Settings,
            Self::Project => Icon::Folder,
        }
    }
}

#[derive(Clone, Copy)]
enum LeaveAction {
    Home,
    Close,
}

pub struct CreatorApp {
    portable_root: PathBuf,
    settings: AppSettings,
    project: Option<Project>,
    recent_projects: Vec<PathBuf>,
    selected_bot: usize,
    page: EditorPage,
    new_project_name: String,
    status: String,
    status_is_error: bool,
    advanced_search: String,
    detected_gmod: Vec<PathBuf>,
    preview: Option<(PathBuf, egui::TextureHandle)>,
    killfeed_preview: Option<(PathBuf, egui::TextureHandle)>,
    saved_project: Option<Project>,
    generation: Option<JoinHandle<Result<generator::GenerationReport, String>>>,
    generation_started: Option<Instant>,
    leave_action: Option<LeaveAction>,
    allow_close: bool,
    removed_bot: Option<(usize, Nextbot)>,
    bot_search: String,
    audio_search: String,
    ffmpeg_available: bool,
    link_cache: Option<(Instant, LinkStatus)>,
    status_details: bool,
    updates: UpdateChecker,
    show_settings: bool,
    settings_page: SettingsPage,
    update_notice_dismissed: bool,
    media_dialog: Option<MediaDialog>,
    downloader_update: Option<nextbot_creator::media::MediaJob<String>>,
}

impl CreatorApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&context.egui_ctx);
        let portable_root = persistence::portable_root();
        let mut settings = AppSettings::load_or_default(&portable_root);
        let detected_gmod = integration::detect_garrys_mod();
        if settings
            .garrys_mod_root
            .as_ref()
            .and_then(|path| integration::normalize_gmod_root(path))
            .is_none()
        {
            settings.garrys_mod_root = detected_gmod.first().cloned();
        }
        let recent_projects = settings.available_projects();
        let ffmpeg_available = converter::ffmpeg_path(&portable_root).is_some();
        let mut updates = UpdateChecker::default();
        if settings.check_for_updates_on_startup {
            updates.start();
        }
        Self {
            portable_root,
            settings,
            project: None,
            recent_projects,
            selected_bot: 0,
            page: EditorPage::Basic,
            new_project_name: "My Nextbot Project".into(),
            status: "Ready".into(),
            status_is_error: false,
            advanced_search: String::new(),
            detected_gmod,
            preview: None,
            killfeed_preview: None,
            saved_project: None,
            generation: None,
            generation_started: None,
            leave_action: None,
            allow_close: false,
            removed_bot: None,
            bot_search: String::new(),
            audio_search: String::new(),
            ffmpeg_available,
            link_cache: None,
            status_details: false,
            updates,
            show_settings: false,
            settings_page: SettingsPage::General,
            update_notice_dismissed: false,
            media_dialog: None,
            downloader_update: None,
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_is_error = false;
    }

    fn set_error(&mut self, error: impl std::fmt::Display) {
        self.status = error.to_string();
        self.status_is_error = true;
    }

    fn save(&mut self) -> bool {
        let Some(project) = &self.project else {
            return true;
        };
        match persistence::save_project(project) {
            Ok(()) => {
                self.saved_project = Some(project.clone());
                self.set_status("Project saved.");
                true
            }
            Err(error) => {
                self.set_error(error);
                false
            }
        }
    }

    fn is_dirty(&self) -> bool {
        self.project != self.saved_project
    }

    fn generate(&mut self) {
        if self.media_dialog.is_some()
            || self.generation.is_some()
            || self.project.is_none()
            || !self.save()
        {
            return;
        }
        let project = self.project.as_ref().unwrap().clone();
        let root = self.portable_root.clone();
        self.generation_started = Some(Instant::now());
        self.generation = Some(std::thread::spawn(move || {
            generator::generate_project(&project, &root).map_err(|error| error.to_string())
        }));
        self.set_status("Generating addon and converting assets...");
    }

    fn poll_generation(&mut self, context: &egui::Context) {
        if self
            .generation
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            let result = self.generation.take().unwrap().join();
            self.generation_started = None;
            self.link_cache = None;
            match result {
                Ok(Ok(report)) => {
                    let mut message = format!("Generated {} files", report.files_written);
                    if report.files_removed > 0 {
                        message
                            .push_str(&format!("; removed {} stale files", report.files_removed));
                    }
                    if !report.warnings.is_empty() {
                        message.push_str(&format!(
                            ". {} warning(s): {}",
                            report.warnings.len(),
                            report.warnings.join(" | ")
                        ));
                    }
                    self.set_status(message);
                }
                Ok(Err(error)) => self.set_error(error),
                Err(_) => self.set_error(
                    "Generation stopped unexpectedly. Your project is saved; try generating again.",
                ),
            }
        }
        if self.generation.is_some() {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn request_leave(&mut self, action: LeaveAction, context: &egui::Context) {
        if self.downloader_update.is_some() {
            self.set_status("Finish or cancel the downloader update before closing.");
        } else if self.media_dialog.is_some() {
            self.set_status("Finish or cancel the media import before closing the project.");
        } else if self.generation.is_some() {
            self.set_status("Finish generating before closing the project.");
        } else if self.is_dirty() {
            self.leave_action = Some(action);
        } else {
            self.leave_project(action, context);
        }
    }

    fn leave_project(&mut self, action: LeaveAction, context: &egui::Context) {
        self.leave_action = None;
        match action {
            LeaveAction::Close => {
                self.allow_close = true;
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            LeaveAction::Home => {
                self.show_settings = false;
                self.project = None;
                self.saved_project = None;
                self.preview = None;
                self.killfeed_preview = None;
                self.removed_bot = None;
                self.link_cache = None;
            }
        }
    }

    fn leave_dialog(&mut self, context: &egui::Context) {
        let Some(action) = self.leave_action else {
            return;
        };
        let response = egui::Modal::new(egui::Id::new("unsaved_changes")).show(context, |ui| {
            ui.set_width(400.0);
            ui.heading("Save your changes?");
            ui.label("This project has unsaved edits.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button("Save and continue")).clicked() && self.save() {
                    self.leave_project(action, context);
                }
                if ui.button("Discard edits").clicked() {
                    self.leave_project(action, context);
                }
                if ui.button("Cancel").clicked() {
                    self.leave_action = None;
                }
            });
            if self.status_is_error {
                ui.colored_label(error_color(), &self.status);
            }
        });
        if response.should_close() {
            self.leave_action = None;
        }
    }

    fn link_or_unlink(&mut self, unlink: bool) {
        self.link_cache = None;
        let (Some(project), Some(gmod)) = (&self.project, &self.settings.garrys_mod_root) else {
            self.set_error("Choose a valid Garry's Mod folder first.");
            return;
        };
        let result = if unlink {
            integration::remove_junction(gmod, &project.slug, &project.root).map(|_| PathBuf::new())
        } else {
            integration::create_junction(gmod, &project.slug, &project.root)
        };
        match result {
            Ok(path) if unlink => self.set_status("Project unlinked from Garry's Mod."),
            Ok(path) => self.set_status(format!("Project linked at {}", path.display())),
            Err(error) => self.set_error(error),
        }
    }

    fn launch_gmod(&mut self) {
        let Some(gmod_root) = self.settings.garrys_mod_root.clone() else {
            self.set_error("Choose or detect a Garry's Mod folder first.");
            return;
        };
        match integration::launch_garrys_mod(&gmod_root) {
            Ok(executable) => self.set_status(format!(
                "Launched Garry's Mod from {}",
                executable.display()
            )),
            Err(error) => self.set_error(error),
        }
    }

    fn create_project(&mut self) {
        match persistence::create_project(
            &self.settings.projects_root,
            self.new_project_name.trim(),
        ) {
            Ok(project) => {
                if project.nextbots.is_empty() {
                    self.set_error("This project contains no NextBots.");
                    return;
                }
                let project_root = project.root.clone();
                self.selected_bot = 0;
                self.preview = None;
                self.killfeed_preview = None;
                self.saved_project = Some(project.clone());
                self.project = Some(project);
                self.page = EditorPage::Basic;
                self.bot_search.clear();
                self.audio_search.clear();
                self.removed_bot = None;
                self.link_cache = None;
                self.settings.remember_project(&project_root);
                self.refresh_recent();
                match self.settings.save(&self.portable_root) {
                    Ok(()) => self.set_status("Project created."),
                    Err(error) => self.set_error(format!(
                        "Project created, but its recent-project entry could not be saved: {error}"
                    )),
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn open_project(&mut self, path: &Path) {
        match persistence::load_project(path) {
            Ok(project) => {
                if project.nextbots.is_empty() {
                    self.set_error("This project contains no NextBots.");
                    return;
                }
                let project_root = project.root.clone();
                self.saved_project = Some(project.clone());
                self.project = Some(project);
                self.page = EditorPage::Basic;
                self.bot_search.clear();
                self.audio_search.clear();
                self.removed_bot = None;
                self.link_cache = None;
                self.selected_bot = 0;
                self.preview = None;
                self.killfeed_preview = None;
                self.settings.remember_project(&project_root);
                self.refresh_recent();
                match self.settings.save(&self.portable_root) {
                    Ok(()) => self.set_status("Project opened."),
                    Err(error) => self.set_error(format!(
                        "Project opened, but its recent-project entry could not be saved: {error}"
                    )),
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn refresh_recent(&mut self) {
        self.recent_projects = self.settings.available_projects();
    }

    fn import_visual(&mut self, context: &egui::Context) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let Some(class_name) = project
            .nextbots
            .get(self.selected_bot)
            .map(|bot| bot.class_name.clone())
        else {
            return;
        };
        let Some(source) = rfd::FileDialog::new()
            .add_filter(
                "Visual assets",
                &[
                    "png", "jpg", "jpeg", "bmp", "tga", "webp", "gif", "vtf", "vmt",
                ],
            )
            .pick_file()
        else {
            return;
        };
        match persistence::import_source_asset(project, &class_name, &source) {
            Ok(imported) => {
                let aspect_ratio = visual_dimensions(&imported)
                    .map(|(width, height)| width as f32 / height.max(1) as f32);
                let Some(bot) = project.nextbots.get_mut(self.selected_bot) else {
                    return;
                };
                bot.visual.source = Some(imported.clone());
                bot.visual.material_name = source
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .map(slugify)
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "nextbot".into());
                if let Some(aspect_ratio) = aspect_ratio.filter(|value| value.is_finite()) {
                    bot.visual.width = (bot.visual.height * aspect_ratio).clamp(1.0, 4096.0);
                }
                self.preview = load_preview(context, &imported).map(|texture| (imported, texture));
                self.set_status("Visual asset imported into the project.");
            }
            Err(error) => self.set_error(error),
        }
    }

    fn open_media(&mut self, target: MediaTarget) {
        if self.downloader_update.is_some() {
            self.set_status("Wait for the downloader update to finish.");
            return;
        }
        if self.generation.is_some() || self.media_dialog.is_some() {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let Some(bot) = project.nextbots.get(self.selected_bot) else {
            return;
        };
        let mut dialog = MediaDialog::new(target, project.root.clone(), bot.class_name.clone());
        if let MediaTarget::Audio {
            slot,
            index: Some(index),
        } = target
        {
            let Some(clip) = slot.get(&bot.audio).get(index) else {
                return;
            };
            dialog.edit(clip.clone(), &self.portable_root);
        }
        self.media_dialog = Some(dialog);
    }

    fn show_media(&mut self, context: &egui::Context) {
        let Some(mut dialog) = self.media_dialog.take() else {
            return;
        };
        if dialog.show(context, &self.portable_root) {
            match self.accept_media(&dialog) {
                Ok(()) => {
                    self.preview = None;
                    self.killfeed_preview = None;
                    self.set_status("Media updated. The original source is preserved; conversion and trimming are applied when generating.");
                    return;
                }
                Err(error) => dialog.error = error,
            }
        }
        if !dialog.closed {
            self.media_dialog = Some(dialog);
        }
    }

    fn accept_media(&mut self, dialog: &MediaDialog) -> Result<(), String> {
        let project = self
            .project
            .as_mut()
            .ok_or("The project is no longer open.")?;
        if project.root != dialog.project_root {
            return Err("The active project changed. Reopen the import dialog.".into());
        }
        let index = project
            .nextbots
            .iter()
            .position(|bot| bot.class_name == dialog.bot_class)
            .ok_or("The selected NextBot is no longer available.")?;
        let prepared = dialog
            .prepared
            .as_ref()
            .ok_or("Media has not finished downloading.")?;
        if let Some(audio) = &prepared.audio {
            dialog.trim.range(audio.duration).map_err(str::to_owned)?;
        }
        if let MediaTarget::Audio {
            slot,
            index: Some(clip_index),
        } = dialog.target
        {
            let clip = slot
                .get_mut(&mut project.nextbots[index].audio)
                .get_mut(clip_index)
                .ok_or("This clip was removed.")?;
            if Some(&*clip) != dialog.original_clip.as_ref() {
                return Err("This clip changed. Reopen the trim editor.".into());
            }
            clip.trim = dialog.trim;
            return Ok(());
        }
        let source = persistence::import_source_asset(project, &dialog.bot_class, &prepared.source)
            .map_err(|error| error.to_string())?;
        let bot = &mut project.nextbots[index];
        match dialog.target {
            MediaTarget::Visual => {
                if let Some((width, height)) = visual_dimensions(&source) {
                    bot.visual.width = (bot.visual.height * width as f32 / height.max(1) as f32)
                        .clamp(1.0, 4096.0);
                }
                bot.visual.source = Some(source);
            }
            MediaTarget::Killfeed => {
                bot.visual.killfeed_icon.source = Some(source);
                bot.visual.killfeed_icon.mode = KillfeedIconMode::CustomImage;
            }
            MediaTarget::Audio { slot, .. } => {
                slot.get_mut(&mut bot.audio)
                    .push(nextbot_creator::domain::AudioClip {
                        source,
                        trim: dialog.trim,
                        source_url: prepared.source_url.clone(),
                    });
            }
        }
        Ok(())
    }

    fn import_audio(&mut self, slot: AudioSlot) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let Some(class_name) = project
            .nextbots
            .get(self.selected_bot)
            .map(|bot| bot.class_name.clone())
        else {
            return;
        };
        let Some(sources) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "ogg", "flac", "m4a", "aac", "wma"])
            .pick_files()
        else {
            return;
        };
        let mut imported = Vec::new();
        for source in sources {
            match persistence::import_source_asset(project, &class_name, &source) {
                Ok(path) => imported.push(path),
                Err(error) => {
                    self.set_error(error);
                    return;
                }
            }
        }
        if let Some(bot) = project.nextbots.get_mut(self.selected_bot) {
            slot.get_mut(&mut bot.audio)
                .extend(imported.into_iter().map(Into::into));
        }
        self.set_status(
            "Audio imported. It will be normalized to mono 44.1 kHz PCM WAV when generated.",
        );
    }

    fn import_killfeed_icon(&mut self, context: &egui::Context) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let Some(class_name) = project
            .nextbots
            .get(self.selected_bot)
            .map(|bot| bot.class_name.clone())
        else {
            return;
        };
        let Some(source) = rfd::FileDialog::new()
            .add_filter(
                "Killfeed image",
                &[
                    "png", "jpg", "jpeg", "bmp", "tga", "webp", "gif", "vtf", "vmt",
                ],
            )
            .pick_file()
        else {
            return;
        };
        match persistence::import_source_asset(project, &class_name, &source) {
            Ok(imported) => {
                let Some(bot) = project.nextbots.get_mut(self.selected_bot) else {
                    return;
                };
                bot.visual.killfeed_icon.mode = KillfeedIconMode::CustomImage;
                bot.visual.killfeed_icon.source = Some(imported.clone());
                self.killfeed_preview =
                    load_preview_named(context, &imported, "killfeed_icon_preview")
                        .map(|texture| (imported, texture));
                self.set_status("Custom killfeed icon imported into the project.");
            }
            Err(error) => self.set_error(error),
        }
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top_bar")
            .frame(panel_frame())
            .show_inside(root, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    for (x, y, color) in [
                        (0.0, 0.0, accent()),
                        (11.0, 0.0, Color32::BLACK),
                        (0.0, 11.0, Color32::BLACK),
                        (11.0, 11.0, accent()),
                    ] {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.min + egui::vec2(x, y),
                                egui::vec2(11.0, 11.0),
                            ),
                            0,
                            color,
                        );
                    }
                    ui.label(RichText::new(APP_NAME).size(17.0).strong());
                    ui.label(RichText::new(format!("v{APP_VERSION}")).small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                icon_button(Icon::Settings, "Settings")
                                    .selected(self.show_settings),
                            )
                            .on_hover_text("Application preferences, game installation, and tools")
                            .clicked()
                        {
                            self.show_settings = !self.show_settings;
                        }
                    });
                });
            });
    }

    fn project_commands(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("project_commands")
            .frame(panel_frame().fill(ui::SURFACE))
            .show_inside(root, |ui| {
                ui.horizontal(|ui| {
                    let busy = self.generation.is_some();
                    ui.add_enabled_ui(!busy, |ui| {
                        let menu =
                            egui::containers::menu::MenuButton::from_button(egui::Button::new((
                                "Project",
                                egui::Atom::custom(egui::Id::new("project_chevron"), [14.0, 14.0]),
                            )))
                            .ui(ui, |ui| {
                                if ui
                                    .add(icon_button(Icon::Folder, "Open project folder").quiet())
                                    .clicked()
                                {
                                    if let Some(project) = &self.project {
                                        open_in_explorer(&project.root);
                                    }
                                    ui.close();
                                }
                                match self.current_link_status() {
                                    LinkStatus::Linked(_) => {
                                        if ui.button("Unlink from Garry's Mod").clicked() {
                                            self.link_or_unlink(true);
                                            ui.close();
                                        }
                                    }
                                    LinkStatus::Unlinked => {
                                        if ui
                                        .add_enabled(
                                            self.settings.garrys_mod_root.is_some(),
                                            egui::Button::new("Link to Garry's Mod"),
                                        )
                                        .on_disabled_hover_text(
                                            "Choose a Garry's Mod installation in Settings first.",
                                        )
                                        .clicked()
                                    {
                                        self.link_or_unlink(false);
                                        ui.close();
                                    }
                                    }
                                    LinkStatus::Conflict(path) => {
                                        ui.colored_label(
                                            error_color(),
                                            "Addon path is already occupied",
                                        )
                                        .on_hover_text(path.display().to_string());
                                    }
                                }
                                ui.separator();
                                if ui
                                    .add(icon_button(Icon::Settings, "Project settings").quiet())
                                    .clicked()
                                {
                                    self.page = EditorPage::Project;
                                    ui.close();
                                }
                                if ui
                                    .add(icon_button(Icon::Back, "Back to projects").quiet())
                                    .clicked()
                                {
                                    self.request_leave(LeaveAction::Home, ui.ctx());
                                    ui.close();
                                }
                            })
                            .0;
                        let rect = egui::Rect::from_center_size(
                            egui::pos2(
                                menu.rect.right() - ui.spacing().button_padding.x - 7.0,
                                menu.rect.center().y,
                            ),
                            egui::vec2(14.0, 14.0),
                        );
                        Icon::ChevronDown.paint(ui, rect, ui.style().interact(&menu).text_color());
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !busy,
                                icon_button(
                                    Icon::Generate,
                                    if busy {
                                        "Generating..."
                                    } else {
                                        "Generate addon"
                                    },
                                )
                                .primary(),
                            )
                            .on_hover_text("Save and generate all NextBots (Ctrl+G)")
                            .clicked()
                        {
                            self.generate();
                        }
                        if ui
                            .add_enabled(!busy, icon_button(Icon::Save, "Save"))
                            .on_hover_text("Save project (Ctrl+S)")
                            .clicked()
                        {
                            self.save();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                self.settings.garrys_mod_root.is_some(),
                                icon_button(Icon::Play, "Launch game").quiet(),
                            )
                            .on_hover_text("Launch the configured Garry's Mod installation")
                            .on_disabled_hover_text(
                                "Choose a Garry's Mod installation in Settings first.",
                            )
                            .clicked()
                        {
                            self.launch_gmod();
                        }
                        badge(
                            ui,
                            if self.is_dirty() {
                                Icon::Info
                            } else {
                                Icon::Check
                            },
                            if self.is_dirty() { "Unsaved" } else { "Saved" },
                            if self.is_dirty() {
                                ui::WARNING
                            } else {
                                ui::MUTED
                            },
                        );
                        if let Some(project) = &self.project {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&project.name).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(&project.name);
                                },
                            );
                        }
                    });
                });
            });
    }

    fn current_link_status(&mut self) -> LinkStatus {
        if let Some((checked, status)) = &self.link_cache
            && checked.elapsed() < Duration::from_secs(3)
        {
            return status.clone();
        }
        let status = match (&self.project, &self.settings.garrys_mod_root) {
            (Some(project), Some(gmod)) => {
                integration::link_status(gmod, &project.slug, &project.root)
            }
            _ => LinkStatus::Unlinked,
        };
        self.link_cache = Some((Instant::now(), status.clone()));
        status
    }

    fn update_notice(&mut self, root: &mut egui::Ui) {
        if self.update_notice_dismissed {
            return;
        }
        if let UpdateStatus::Finished(Ok(UpdateOutcome::Available { version, url })) =
            self.updates.status()
        {
            egui::Panel::top("update_notice")
                .frame(panel_frame())
                .show_inside(root, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            accent(),
                            format!("NextbotCreator {version} is available"),
                        );
                        external_link(ui, "View release", url);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Dismiss").clicked() {
                                self.update_notice_dismissed = true;
                            }
                        });
                    });
                });
        }
    }

    fn status_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .frame(panel_frame().inner_margin(egui::Margin::symmetric(14, 4)))
            .show_inside(root, |ui| {
                ui.spacing_mut().interact_size.y = 24.0;
                ui.horizontal(|ui| {
                    let color = if self.status_is_error {
                        error_color()
                    } else {
                        ui::SUCCESS
                    };
                    if self.generation_started.is_some() {
                        ui.spinner();
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        (if self.status_is_error {
                            Icon::Warning
                        } else {
                            Icon::Check
                        })
                        .paint(ui, rect, color);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Details").clicked() {
                            self.status_details = !self.status_details;
                        }
                        if !self.ffmpeg_available
                            && ui
                                .add(icon_button(Icon::Warning, "Media tools").small().quiet())
                                .on_hover_text(
                                    "FFmpeg is unavailable. Open Media tools in Settings.",
                                )
                                .clicked()
                        {
                            self.settings_page = SettingsPage::Media;
                            self.show_settings = true;
                        }
                        if self.removed_bot.is_some()
                            && ui
                                .add_enabled(
                                    self.generation.is_none(),
                                    icon_button(Icon::Back, "Undo remove").small().quiet(),
                                )
                                .clicked()
                            && let Some((index, mut bot)) = self.removed_bot.take()
                            && let Some(project) = &mut self.project
                        {
                            bot.class_name = project.unique_class_name(&bot.class_name);
                            let index = index.min(project.nextbots.len());
                            project.nextbots.insert(index, bot);
                            self.select_bot(index);
                        }
                        let message = self.generation_started.map_or_else(
                            || self.status.clone(),
                            |started| {
                                format!(
                                    "Generating addon... {:.0}s",
                                    started.elapsed().as_secs_f32()
                                )
                            },
                        );
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(message).color(
                                    if self.status_is_error {
                                        error_color()
                                    } else {
                                        ui::MUTED
                                    },
                                ))
                                .truncate(),
                            )
                            .on_hover_text(&self.status);
                        });
                    });
                });
            });
        if self.status_details {
            egui::Window::new("Activity details")
                .open(&mut self.status_details)
                .default_width(560.0)
                .show(root.ctx(), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.label(&self.status);
                        });
                    if ui.add(icon_button(Icon::Copy, "Copy details")).clicked() {
                        ui.ctx().copy_text(self.status.clone());
                    }
                });
        }
    }

    fn home(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui::BACKGROUND).inner_margin(28))
            .show_inside(root, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(1120.0));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Projects").size(28.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(icon_button(Icon::Folder, "Open project...")).clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_folder()
                            {
                                self.open_project(&path);
                            }
                        });
                    });
                    ui.label(RichText::new("Create a project or continue working on your NextBots.").weak());
                    ui.add_space(20.0);
                    if ui.available_width() >= 780.0 {
                        let width = ui.available_width();
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(width * 0.56, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                self.recent_projects_ui(ui);
                            });
                            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                self.new_project_ui(ui);
                            });
                        });
                    } else {
                        self.new_project_ui(ui);
                        ui.add_space(18.0);
                        self.recent_projects_ui(ui);
                    }
                    ui.add_space(20.0);
                    card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        section_heading(ui, Icon::Game, "Garry's Mod", "");
                        ui.horizontal_wrapped(|ui| {
                            if self.settings.garrys_mod_root.is_some() {
                                badge(ui, Icon::Check, "Installation selected", ui::SUCCESS);
                                if ui.add(icon_button(Icon::Play, "Launch game")).clicked() {
                                    self.launch_gmod();
                                }
                            } else {
                                badge(ui, Icon::Warning, "Setup needed", ui::WARNING);
                            }
                            if ui.add(icon_button(Icon::Settings, "Game settings").quiet()).clicked() {
                                self.show_settings = true;
                                self.settings_page = SettingsPage::General;
                            }
                        });
                        if let Some(path) = &self.settings.garrys_mod_root {
                            path_label(ui, path);
                        } else {
                            ui.label(RichText::new("Select your installation in Settings to launch the game and link projects.").weak());
                        }
                    });
                });
            });
    }

    fn new_project_ui(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(
                ui,
                Icon::Plus,
                "New project",
                "Keep related NextBots together in one project.",
            );
            ui.label("Project name");
            let name = ui.add(
                egui::TextEdit::singleline(&mut self.new_project_name)
                    .hint_text("Project name")
                    .margin(egui::Margin::symmetric(8, 7))
                    .desired_width(ui.available_width()),
            );
            let create_on_enter =
                name.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            ui.add_space(8.0);
            ui.label(RichText::new("Save location").small().weak());
            path_label(ui, &self.settings.projects_root);
            ui.add_space(12.0);
            let valid = !self.new_project_name.trim().is_empty();
            if ui
                .add_enabled(valid, icon_button(Icon::Plus, "Create project").primary())
                .clicked()
                || (create_on_enter && valid)
            {
                self.create_project();
            }
        });
    }

    fn recent_projects_ui(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.heading("Recent projects");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(icon_button(Icon::Refresh, "Refresh").small().quiet())
                        .clicked()
                    {
                        self.refresh_recent();
                    }
                });
            });
            ui.label(
                RichText::new(format!("{} available", self.recent_projects.len()))
                    .small()
                    .weak(),
            );
            ui.add_space(10.0);
            let projects = self.recent_projects.clone();
            if projects.is_empty() {
                ui.add_space(20.0);
                ui.strong("Your projects will appear here");
                ui.label(
                    RichText::new("Create your first project, or open an existing project folder.")
                        .weak(),
                );
                ui.add_space(24.0);
            }
            for (index, path) in projects.iter().enumerate() {
                if index > 0 {
                    ui.separator();
                }
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Project");
                if navigation(ui, Icon::Folder, name, false)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    self.open_project(path);
                }
                ui.add(
                    egui::Label::new(RichText::new(path.display().to_string()).small().weak())
                        .truncate(),
                )
                .on_hover_text(path.display().to_string());
            }
        });
    }

    fn project_ui(&mut self, root: &mut egui::Ui) {
        self.project_commands(root);
        if self.project.is_none() {
            self.home(root);
            return;
        }
        self.bot_sidebar(root);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui::BACKGROUND).inner_margin(24))
            .show_inside(root, |ui| {
                let project = self.project.as_ref().unwrap();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&project.name).small().weak());
                    if self.page != EditorPage::Project {
                        ui.label(RichText::new("/").weak());
                        ui.add(
                            egui::Label::new(
                                RichText::new(&project.nextbots[self.selected_bot].display_name)
                                    .small(),
                            )
                            .truncate(),
                        );
                    }
                });
                ui.label(RichText::new(self.page.label()).size(28.0).strong());
                ui.label(RichText::new(page_description(self.page)).weak());
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                let project_key = project.root.clone();
                egui::ScrollArea::vertical()
                    .id_salt(("editor", project_key, self.selected_bot, self.page))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(920.0));
                        ui.add_enabled_ui(self.generation.is_none(), |ui| {
                            card_frame().show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.push_id((self.selected_bot, self.page), |ui| match self.page {
                                    EditorPage::Basic => self.basic_editor(ui),
                                    EditorPage::Visual => {
                                        let context = ui.ctx().clone();
                                        self.visual_editor(ui, &context);
                                    }
                                    EditorPage::Audio => self.audio_editor(ui),
                                    EditorPage::Combat => self.combat_editor(ui),
                                    EditorPage::Possession => self.possession_editor(ui),
                                    EditorPage::Events => self.events_editor(ui),
                                    EditorPage::Advanced => self.advanced_editor(ui),
                                    EditorPage::Project => self.project_editor(ui),
                                });
                            });
                        });
                        ui.add_space(24.0);
                    });
            });
    }

    fn select_bot(&mut self, index: usize) {
        self.selected_bot = index;
        self.preview = None;
        self.killfeed_preview = None;
        if self.page == EditorPage::Project {
            self.page = EditorPage::Basic;
        }
    }

    fn bot_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("bot_sidebar")
            .resizable(false)
            .default_size(226.0)
            .frame(panel_frame())
            .show_inside(root, |ui| {
                let busy = self.generation.is_some();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("NEXTBOTS").small().strong().weak());
                    ui.label(
                        RichText::new(self.project.as_ref().unwrap().nextbots.len().to_string())
                            .small()
                            .color(accent()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(!busy, icon_button(Icon::Plus, "Add").small())
                            .clicked()
                            && let Some(project) = &mut self.project
                        {
                            let index = project.nextbots.len() + 1;
                            let class = project
                                .unique_class_name(&format!("npc_{}_{}", project.slug, index));
                            project
                                .nextbots
                                .push(Nextbot::new(format!("NextBot {index}"), class));
                            self.bot_search.clear();
                            self.select_bot(index - 1);
                        }
                    });
                });
                search_field(
                    ui,
                    &mut self.bot_search,
                    "Find a NextBot...",
                    egui::Id::new("bot_search"),
                );
                let query = self.bot_search.trim().to_lowercase();
                let names: Vec<_> = self
                    .project
                    .as_ref()
                    .unwrap()
                    .nextbots
                    .iter()
                    .enumerate()
                    .filter(|(_, bot)| {
                        query.is_empty()
                            || bot.display_name.to_lowercase().contains(&query)
                            || bot.class_name.contains(&query)
                    })
                    .map(|(i, bot)| (i, bot.display_name.clone(), bot.class_name.clone()))
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("bot_list")
                    .max_height((ui.available_height() * 0.18).clamp(70.0, 150.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if names.is_empty() {
                            ui.label(RichText::new("No matching NextBots").small().weak());
                        }
                        for (index, name, class) in names {
                            if ui
                                .add_sized(
                                    [ui.available_width(), 34.0],
                                    egui::Button::selectable(
                                        self.selected_bot == index,
                                        RichText::new(name),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(class)
                                .clicked()
                            {
                                self.select_bot(index);
                            }
                        }
                    });
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add(icon_button(Icon::Copy, "Duplicate").small().quiet())
                            .on_hover_text("Copy the selected NextBot with a unique class name")
                            .clicked()
                            && let Some(project) = &mut self.project
                        {
                            let mut bot = project.nextbots[self.selected_bot].clone();
                            bot.display_name.push_str(" Copy");
                            bot.class_name =
                                project.unique_class_name(&format!("{}_copy", bot.class_name));
                            project.nextbots.push(bot);
                            let index = project.nextbots.len() - 1;
                            self.bot_search.clear();
                            self.select_bot(index);
                        }
                        if ui
                            .add_enabled(
                                self.project.as_ref().unwrap().nextbots.len() > 1,
                                icon_button(Icon::Trash, "Remove").small().quiet(),
                            )
                            .clicked()
                            && let Some(project) = &mut self.project
                        {
                            let index = self.selected_bot;
                            self.removed_bot = Some((index, project.nextbots.remove(index)));
                            let selected = index.min(project.nextbots.len() - 1);
                            self.select_bot(selected);
                            self.set_status("NextBot removed. Use Undo remove to restore it.");
                        }
                    });
                });
                ui.add_space(12.0);
                ui.separator();
                ui.label(RichText::new("EDIT NEXTBOT").small().strong().weak());
                egui::ScrollArea::vertical()
                    .id_salt("navigation")
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for page in EditorPage::ALL {
                            if page == EditorPage::Project {
                                ui.add_space(10.0);
                                ui.separator();
                                ui.label(RichText::new("PROJECT").small().strong().weak());
                            }
                            if navigation(ui, page.icon(), page.label(), self.page == page)
                                .clicked()
                            {
                                self.page = page;
                            }
                        }
                    });
            });
    }

    fn selected_bot_mut(&mut self) -> Option<&mut Nextbot> {
        self.project.as_mut()?.nextbots.get_mut(self.selected_bot)
    }

    fn basic_editor(&mut self, ui: &mut egui::Ui) {
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        ui.heading("Identity & spawnmenu");
        field_row(
            ui,
            "Display name",
            "The name shown in the spawnmenu.",
            |ui| {
                text_edit(ui, &mut bot.display_name);
            },
        );
        field_row(
            ui,
            "Class name",
            "Stable lowercase entity identifier.",
            |ui| {
                if text_edit(ui, &mut bot.class_name).lost_focus() {
                    bot.class_name = sanitize_class_name(&bot.class_name);
                }
            },
        );
        field_row(
            ui,
            "Description",
            "Project-facing notes for this NextBot.",
            |ui| {
                ui.text_edit_multiline(&mut bot.description);
            },
        );
        field_row(
            ui,
            "DRGBase variant",
            "Sprite for image NextBots; Human for weapon-ready humanoids.",
            |ui| {
                egui::ComboBox::from_id_salt("base_variant")
                    .selected_text(bot.base.label())
                    .show_ui(ui, |ui| {
                        for value in BaseVariant::ALL {
                            ui.selectable_value(&mut bot.base, value, value.label());
                        }
                    });
            },
        );
        field_row(ui, "Spawn tab", "Top-level spawnmenu location.", |ui| {
            egui::ComboBox::from_id_salt("spawn_tab")
                .selected_text(bot.spawn_tab.label())
                .show_ui(ui, |ui| {
                    for value in SpawnTab::ALL {
                        let label = value.label();
                        ui.selectable_value(&mut bot.spawn_tab, value, label);
                    }
                });
        });
        if matches!(bot.spawn_tab, SpawnTab::Custom) {
            field_row(
                ui,
                "Custom tab",
                "Name of the generated top-level tab.",
                |ui| {
                    text_edit(ui, &mut bot.custom_tab_name);
                },
            );
        }
        field_row(ui, "Category", "Defaults to NPCs > Nextbot.", |ui| {
            text_edit(ui, &mut bot.category);
        });
        field_row(
            ui,
            "Admin only",
            "Restrict spawnmenu spawning to admins.",
            |ui| {
                ui.checkbox(&mut bot.admin_only, "Require admin");
            },
        );

        ui.add_space(16.0);
        ui.heading("Behavior");
        ui.horizontal_wrapped(|ui| {
            for preset in BehaviorPreset::ALL {
                if ui
                    .button(preset.label())
                    .on_hover_text(preset.description())
                    .clicked()
                {
                    bot.apply_behavior_preset(preset);
                }
            }
        });
        ui.label(
            RichText::new(
                "Presets update related DRGBase, combat, movement, and event settings. Every value remains editable.",
            )
            .small()
            .weak(),
        );

        ui.add_space(10.0);
        field_row(
            ui,
            "Other NextBots",
            "Keep multiple hunters focused on their enemies without fighting each other.",
            |ui| {
                ui.checkbox(&mut bot.ignore_nextbots, "Ignore other NextBots");
            },
        );
        ui.add_space(16.0);
        ui.heading("Common settings");
        let specs = property_catalog()
            .into_iter()
            .filter(|spec| spec.basic)
            .collect::<Vec<_>>();
        render_property_specs(ui, bot, &specs);
    }

    fn visual_editor(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let selected = self
            .project
            .as_ref()
            .and_then(|project| project.nextbots.get(self.selected_bot))
            .and_then(|bot| bot.visual.source.clone());
        let killfeed_selected = self
            .project
            .as_ref()
            .and_then(|project| project.nextbots.get(self.selected_bot))
            .and_then(|bot| match bot.visual.killfeed_icon.mode {
                KillfeedIconMode::NextbotSprite => bot.visual.source.clone(),
                KillfeedIconMode::CustomImage => bot.visual.killfeed_icon.source.clone(),
            });
        if self.preview.as_ref().map(|(path, _)| path) != selected.as_ref() {
            self.preview = selected.as_ref().and_then(|path| {
                load_preview(context, path).map(|texture| (path.clone(), texture))
            });
        }
        if self.killfeed_preview.as_ref().map(|(path, _)| path) != killfeed_selected.as_ref() {
            self.killfeed_preview = killfeed_selected.as_ref().and_then(|path| {
                load_preview_named(context, path, "killfeed_icon_preview")
                    .map(|texture| (path.clone(), texture))
            });
        }
        ui.heading("Model & texture");
        ui.label(RichText::new("Import an image, GIF, or VTF/VMT pair. GIFs animate in-game; image proportions are preserved.").weak());
        ui.horizontal_wrapped(|ui| {
            if ui
                .add(icon_button(Icon::Image, "Import visual asset..."))
                .clicked()
            {
                self.import_visual(context);
            }
            if ui
                .add(icon_button(Icon::Download, "Paste image URL..."))
                .clicked()
            {
                self.open_media(MediaTarget::Visual);
            }
        });
        if let Some((_, texture)) = &self.preview {
            card_frame().show(ui, |ui| {
                ui.add(egui::Image::new(texture).max_size(egui::vec2(240.0, 240.0)));
            });
        }
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        if let Some(source) = &bot.visual.source {
            path_label(ui, source);
        }
        field_row(
            ui,
            "Material name",
            "Generated material filename for this NextBot.",
            |ui| {
                if text_edit(ui, &mut bot.visual.material_name).lost_focus() {
                    bot.visual.material_name = slugify(&bot.visual.material_name);
                }
            },
        );
        field_row(
            ui,
            "Texture size",
            "Power-of-two square canvas used for Source.",
            |ui| {
                egui::ComboBox::from_id_salt("texture_size")
                    .selected_text(format!("{} px", bot.visual.texture_size))
                    .show_ui(ui, |ui| {
                        for size in [256, 512, 1024, 2048, 4096] {
                            ui.selectable_value(
                                &mut bot.visual.texture_size,
                                size,
                                format!("{size} px"),
                            );
                        }
                    });
            },
        );
        field_row(
            ui,
            "Animation FPS",
            "AnimatedTexture playback rate for GIF inputs.",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut bot.visual.frames_per_second)
                        .range(0.1..=120.0)
                        .speed(0.25),
                );
            },
        );
        field_row(
            ui,
            "World size",
            "Sprite width and height in Source units.",
            |ui| {
                ui.add(egui::DragValue::new(&mut bot.visual.width).range(1.0..=4096.0));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut bot.visual.height).range(1.0..=4096.0));
            },
        );
        field_row(
            ui,
            "Vertical offset",
            "Moves the sprite above or below the collision center.",
            |ui| {
                ui.add(egui::DragValue::new(&mut bot.visual.vertical_offset));
            },
        );
        field_row(
            ui,
            "Material",
            "Unlit is the usual nextbot look; lit follows map lighting.",
            |ui| {
                ui.checkbox(&mut bot.visual.unlit, "Unlit");
                ui.checkbox(&mut bot.visual.translucent, "Transparency");
            },
        );

        ui.add_space(16.0);
        ui.heading("Killfeed icon");
        ui.label(
            "Generate a static 128 px Valve material from the first frame of the NextBot sprite, or choose a separate image.",
        );
        let previous_mode = bot.visual.killfeed_icon.mode;
        field_row(
            ui,
            "Image source",
            "The icon displayed when this NextBot gets a kill.",
            |ui| {
                ui.radio_value(
                    &mut bot.visual.killfeed_icon.mode,
                    KillfeedIconMode::NextbotSprite,
                    "Use NextBot sprite",
                );
                ui.radio_value(
                    &mut bot.visual.killfeed_icon.mode,
                    KillfeedIconMode::CustomImage,
                    "Use custom image",
                );
            },
        );
        let mode_changed = previous_mode != bot.visual.killfeed_icon.mode;
        let custom_mode = matches!(bot.visual.killfeed_icon.mode, KillfeedIconMode::CustomImage);
        let custom_source = bot.visual.killfeed_icon.source.clone();
        let mut import_custom = false;
        let mut import_custom_url = false;
        if custom_mode {
            if ui.button("Import custom killfeed image...").clicked() {
                import_custom = true;
            }
            if ui.button("Paste killfeed image URL...").clicked() {
                import_custom_url = true;
            }
            if let Some(source) = &custom_source {
                path_label(ui, source);
            }
        } else if selected.is_none() {
            ui.colored_label(
                ui::WARNING,
                "Import a NextBot visual asset to generate its killfeed icon.",
            );
        }
        if mode_changed {
            self.killfeed_preview = None;
        }
        if import_custom {
            self.import_killfeed_icon(context);
        }
        if import_custom_url {
            self.open_media(MediaTarget::Killfeed);
        }
        if let Some((_, texture)) = &self.killfeed_preview {
            ui.add(egui::Image::new(texture).max_size(egui::vec2(128.0, 128.0)));
        }
    }

    fn audio_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width() - 80.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    search_field(
                        ui,
                        &mut self.audio_search,
                        "Find a sound type...",
                        egui::Id::new("audio_search"),
                    );
                },
            );
            if ui
                .add_enabled(
                    !self.audio_search.is_empty(),
                    icon_button(Icon::Close, "Clear").small().quiet(),
                )
                .clicked()
            {
                self.audio_search.clear();
            }
        });
        ui.label(RichText::new("Add several clips to a sound type for random variations. Imported audio is converted automatically.").small().weak());
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Playback settings").show(ui, |ui| {
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        ui.add_space(12.0);
        field_row(
            ui,
            "Loop idle sounds",
            "Replay the idle sound slot continuously without the normal delay between clips.",
            |ui| {
                ui.checkbox(&mut bot.audio.idle_loop, "Continuous idle audio");
            },
        );
        if bot.audio.idle_loop {
            ui.label(
                RichText::new(
                    "The advanced Idle sound delay value is preserved but ignored while looping is enabled.",
                )
                .small()
                .weak(),
            );
        }
        field_row(
            ui,
            "Volume",
            "Sound-script volume from near-silent to full.",
            |ui| {
                ui.add(egui::Slider::new(&mut bot.audio.volume, 0.01..=1.0));
            },
        );
        field_row(ui, "Pitch", "100 is the original pitch.", |ui| {
            ui.add(egui::Slider::new(&mut bot.audio.pitch, 1..=255));
        });
        field_row(
            ui,
            "Sound level",
            "Approximate audible level in dB.",
            |ui| {
                ui.add(egui::Slider::new(&mut bot.audio.sound_level, 20..=180).suffix(" dB"));
            },
        );

        });
        ui.add_space(6.0);
        let query = self.audio_search.trim().to_lowercase();
        let mut shown = 0;
        for slot in AudioSlot::ALL {
            if !query.is_empty()
                && !slot.label().to_lowercase().contains(&query)
                && !slot.description().to_lowercase().contains(&query)
            {
                continue;
            }
            shown += 1;
            ui.push_id(slot, |ui| {
                card_frame().inner_margin(12).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.strong(slot.label());
                        let count = self
                            .project
                            .as_ref()
                            .map(|project| {
                                slot.get(&project.nextbots[self.selected_bot].audio).len()
                            })
                            .unwrap_or(0);
                        ui.label(RichText::new(format!("{count} clips")).small().weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(icon_button(Icon::Plus, "Add files").small())
                                .clicked()
                            {
                                self.import_audio(slot);
                            }
                            if ui
                                .add(icon_button(Icon::Download, "Paste link...").small())
                                .clicked()
                            {
                                self.open_media(MediaTarget::Audio { slot, index: None });
                            }
                        });
                    });
                    ui.label(RichText::new(slot.description()).small().weak());
                    let Some(bot) = self.selected_bot_mut() else {
                        return;
                    };
                    let files = slot.get_mut(&mut bot.audio);
                    let mut remove = None;
                    let mut edit = None;
                    for (index, file) in files.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("Remove").clicked() {
                                            remove = Some(index);
                                        }
                                        if ui
                                            .add(icon_button(Icon::Play, "Preview / trim").small())
                                            .clicked()
                                        {
                                            edit = Some(index);
                                        }
                                        if file.trim != Default::default() {
                                            ui.small("Trimmed");
                                        }
                                        ui.add(
                                            egui::Label::new(
                                                file.source
                                                    .file_name()
                                                    .and_then(|name| name.to_str())
                                                    .unwrap_or("audio"),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(file.source.display().to_string());
                                    },
                                );
                            });
                        });
                    }
                    if let Some(index) = remove {
                        files.remove(index);
                    }
                    if remove.is_none()
                        && let Some(index) = edit
                    {
                        self.open_media(MediaTarget::Audio {
                            slot,
                            index: Some(index),
                        });
                    }
                });
            });
        }
        if shown == 0 {
            ui.label("No sound types match your search.");
        }
    }

    fn combat_editor(&mut self, ui: &mut egui::Ui) {
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        ui.heading("Melee attack");
        ui.checkbox(&mut bot.combat.melee_enabled, "Generate melee behavior");
        ui.add_enabled_ui(bot.combat.melee_enabled, |ui| {
            field_row(ui, "Damage", "Random inclusive damage range.", |ui| {
                ui.add(
                    egui::DragValue::new(&mut bot.combat.melee_damage_min).range(0.0..=1_000_000.0),
                );
                ui.label("to");
                ui.add(
                    egui::DragValue::new(&mut bot.combat.melee_damage_max).range(0.0..=1_000_000.0),
                );
            });
            field_row(
                ui,
                "Damage type",
                "Whitelisted Source damage constant.",
                |ui| {
                    egui::ComboBox::from_id_salt("damage_type")
                        .selected_text(&bot.combat.melee_damage_type)
                        .show_ui(ui, |ui| {
                            for value in DAMAGE_TYPES {
                                ui.selectable_value(
                                    &mut bot.combat.melee_damage_type,
                                    (*value).into(),
                                    *value,
                                );
                            }
                        });
                },
            );
            field_row(
                ui,
                "Hit delay",
                "Seconds from attack start to damage.",
                |ui| {
                    ui.add(egui::DragValue::new(&mut bot.combat.melee_delay).range(0.0..=30.0));
                },
            );
            field_row(ui, "Animation", "Source activity constant.", |ui| {
                activity_selector(ui, "melee_animation", &mut bot.combat.melee_animation);
            });
        });
        ui.add_space(16.0);
        ui.heading("Ranged attack");
        ui.checkbox(
            &mut bot.combat.ranged_enabled,
            "Generate projectile behavior",
        );
        ui.add_enabled_ui(bot.combat.ranged_enabled, |ui| {
            field_row(
                ui,
                "Projectile model/class",
                "A .mdl path creates a DRGBase projectile; an entity class spawns that projectile.",
                |ui| {
                    text_edit(ui, &mut bot.combat.projectile_class);
                },
            );
            field_row(
                ui,
                "Damage",
                "Contact damage for generated model projectiles.",
                |ui| {
                    ui.add(
                        egui::DragValue::new(&mut bot.combat.ranged_damage).range(0.0..=100000.0),
                    );
                },
            );
            field_row(ui, "Speed", "Projectile launch speed.", |ui| {
                ui.add(egui::DragValue::new(&mut bot.combat.ranged_speed).range(1.0..=100000.0));
            });
            field_row(ui, "Cooldown", "Wait after firing.", |ui| {
                ui.add(egui::DragValue::new(&mut bot.combat.ranged_cooldown).range(0.0..=120.0));
            });
            field_row(ui, "Animation", "Source activity constant.", |ui| {
                activity_selector(ui, "ranged_animation", &mut bot.combat.ranged_animation);
            });
        });
        ui.add_space(16.0);
        ui.heading("Behavior recipes");
        ui.checkbox(&mut bot.hooks.patrol_when_idle, "Patrol while idle");
        if bot.hooks.patrol_when_idle {
            field_row(ui, "Patrol radius", "Random patrol point radius.", |ui| {
                ui.add(egui::DragValue::new(&mut bot.hooks.patrol_radius).range(0.0..=100000.0));
            });
            field_row(
                ui,
                "Patrol wait",
                "Random wait range at patrol points.",
                |ui| {
                    ui.add(egui::DragValue::new(&mut bot.hooks.patrol_wait_min).range(0.0..=600.0));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut bot.hooks.patrol_wait_max).range(0.0..=600.0));
                },
            );
        }
        ui.checkbox(
            &mut bot.hooks.spot_damage_attacker,
            "Spot entities that damage this NextBot",
        );
        ui.checkbox(
            &mut bot.hooks.remove_on_death,
            "Remove immediately on death",
        );
    }

    fn possession_editor(&mut self, ui: &mut egui::Ui) {
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        ui.heading("Camera views");
        ui.label("Players cycle through these while possessing the NextBot.");
        let mut remove_view = None;
        for (index, view) in bot.possession_views.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    text_edit(ui, &mut view.name);
                    if ui.small_button("Remove").clicked() {
                        remove_view = Some(index);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Offset");
                    for value in &mut view.offset {
                        ui.add(egui::DragValue::new(value).speed(0.5));
                    }
                    ui.label("Distance");
                    ui.add(egui::DragValue::new(&mut view.distance).range(0.0..=10000.0));
                    ui.checkbox(&mut view.eye_position, "Eye position");
                });
            });
        }
        if let Some(index) = remove_view {
            bot.possession_views.remove(index);
        }
        if ui.add(icon_button(Icon::Plus, "Add view")).clicked() {
            bot.possession_views.push(PossessionView::default());
        }

        ui.add_space(16.0);
        ui.heading("Input binds");
        let mut remove_bind = None;
        for (index, bind) in bot.possession_binds.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("bind_key", index))
                    .selected_text(&bind.key)
                    .show_ui(ui, |ui| {
                        for key in POSSESSION_KEYS {
                            ui.selectable_value(&mut bind.key, (*key).into(), *key);
                        }
                    });
                egui::ComboBox::from_id_salt(("bind_trigger", index))
                    .selected_text(trigger_label(&bind.trigger))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut bind.trigger, BindTrigger::Pressed, "Pressed");
                        ui.selectable_value(&mut bind.trigger, BindTrigger::Held, "Held");
                        ui.selectable_value(&mut bind.trigger, BindTrigger::Released, "Released");
                    });
                egui::ComboBox::from_id_salt(("bind_action", index))
                    .selected_text(action_label(&bind.action))
                    .show_ui(ui, |ui| {
                        for action in possession_actions() {
                            let label = action_label(&action);
                            ui.selectable_value(&mut bind.action, action, label);
                        }
                    });
                if ui.small_button("Remove").clicked() {
                    remove_bind = Some(index);
                }
            });
        }
        if let Some(index) = remove_bind {
            bot.possession_binds.remove(index);
        }
        if ui.add(icon_button(Icon::Plus, "Add bind")).clicked() {
            bot.possession_binds.push(PossessionBind {
                key: "IN_ATTACK".into(),
                trigger: BindTrigger::Held,
                action: PossessionAction::PrimaryAttack,
            });
        }
    }

    fn advanced_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width() - 80.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    search_field(
                        ui,
                        &mut self.advanced_search,
                        "Search settings, descriptions, or sections...",
                        egui::Id::new("advanced_search"),
                    );
                },
            );
            if ui
                .add_enabled(
                    !self.advanced_search.is_empty(),
                    icon_button(Icon::Close, "Clear").small().quiet(),
                )
                .clicked()
            {
                self.advanced_search.clear();
            }
        });
        let query = self.advanced_search.trim().to_ascii_lowercase();
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        let catalog = property_catalog();
        let mut matches = 0;
        for section in PropertySection::ALL {
            let specs = catalog
                .iter()
                .filter(|spec| spec.section == section)
                .filter(|spec| {
                    query.is_empty()
                        || spec.name.to_ascii_lowercase().contains(&query)
                        || spec.label.to_ascii_lowercase().contains(&query)
                        || spec.help.to_ascii_lowercase().contains(&query)
                        || spec.section.label().to_ascii_lowercase().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>();
            if specs.is_empty() {
                continue;
            }
            matches += specs.len();
            egui::CollapsingHeader::new(format!("{}  ·  {}", section.label(), specs.len()))
                .id_salt(section.label())
                .open((!query.is_empty()).then_some(true))
                .default_open(matches!(
                    section,
                    PropertySection::Stats | PropertySection::Ai
                ))
                .show(ui, |ui| {
                    render_property_specs(ui, bot, &specs);
                });
        }
        if matches == 0 {
            ui.label("No settings match. Try a broader term, such as speed or sound.");
        }
    }

    fn events_editor(&mut self, ui: &mut egui::Ui) {
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        ui.heading("Documented DRGBase lifecycle events");
        ui.label("Attach ordered, code-free actions to any hook from DRGBase's official NextBot template. Combat and patrol recipes are merged with actions configured here.");
        let mut remove_recipe = None;
        for (recipe_index, recipe) in bot.hook_recipes.iter_mut().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("hook_event", recipe_index))
                        .selected_text(recipe.event.label())
                        .show_ui(ui, |ui| {
                            for event in HookEvent::ALL {
                                ui.selectable_value(&mut recipe.event, event, event.label());
                            }
                        });
                    if ui.small_button("Remove event").clicked() { remove_recipe = Some(recipe_index); }
                });
                if recipe.event.is_client() {
                    ui.label(RichText::new("Client event: sound actions are supported; server-only actions are skipped during generation.").small().weak());
                }
                let mut remove_action = None;
                for (action_index, action) in recipe.actions.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("hook_action", recipe_index, action_index))
                            .selected_text(action.kind.label())
                            .show_ui(ui, |ui| {
                                for kind in HookActionKind::ALL {
                                    ui.selectable_value(&mut action.kind, kind, kind.label());
                                }
                            });
                        if action.kind.uses_value() {
                            ui.add(egui::DragValue::new(&mut action.value).speed(0.1));
                            ui.label(match action.kind {
                                HookActionKind::Wait => "seconds",
                                HookActionKind::AddRandomPatrol => "units",
                                HookActionKind::Heal => "health",
                                _ => "",
                            });
                        }
                        if ui.add(icon_button(Icon::Trash, "Remove").small()).clicked() { remove_action = Some(action_index); }
                    });
                }
                if let Some(index) = remove_action { recipe.actions.remove(index); }
                if ui.add(icon_button(Icon::Plus, "Add action").small()).clicked() { recipe.actions.push(HookAction::default()); }
            });
        }
        if let Some(index) = remove_recipe {
            bot.hook_recipes.remove(index);
        }
        if ui.button("+ Add event recipe").clicked() {
            bot.hook_recipes.push(HookRecipe::default());
        }
    }

    fn project_editor(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        ui.heading("Project");
        field_row(ui, "Name", "Addon title.", |ui| {
            text_edit(ui, &mut project.name);
        });
        field_row(
            ui,
            "Folder slug",
            "Stable addon folder and sound namespace.",
            |ui| {
                if text_edit(ui, &mut project.slug).lost_focus() {
                    project.slug = slugify(&project.slug);
                }
            },
        );
        field_row(ui, "Author", "Written into project metadata.", |ui| {
            text_edit(ui, &mut project.author);
        });
        field_row(
            ui,
            "Project folder",
            "This folder is junctioned into garrysmod/addons.",
            |ui| {
                path_label(ui, &project.root);
            },
        );
        field_row(ui, "Project file", "Portable JSON source of truth.", |ui| {
            ui.monospace(PROJECT_FILE);
        });
        ui.add_space(16.0);
        ui.heading("Garry's Mod integration");
        if let Some(gmod) = &self.settings.garrys_mod_root {
            path_label(ui, gmod);
        } else {
            ui.colored_label(ui::WARNING, "No valid Garry's Mod folder selected.");
        }
        if ui
            .add(icon_button(Icon::Settings, "Open application settings"))
            .clicked()
        {
            self.show_settings = true;
            self.settings_page = SettingsPage::General;
        }
    }
}

impl eframe::App for CreatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.poll_generation(&context);
        if let Some(result) = self
            .downloader_update
            .as_mut()
            .and_then(nextbot_creator::media::MediaJob::poll)
        {
            self.downloader_update = None;
            match result {
                Ok(message) => self.set_status(message),
                Err(error) => self.set_error(error),
            }
        }
        if self.downloader_update.is_some() {
            context.request_repaint_after(Duration::from_millis(100));
        }
        if self.updates.poll() {
            self.update_notice_dismissed = false;
        }
        if matches!(self.updates.status(), UpdateStatus::Checking) || self.show_settings {
            context.request_repaint_after(Duration::from_millis(250));
        }
        if context.input(|input| input.viewport().close_requested())
            && !self.allow_close
            && (self.is_dirty()
                || self.generation.is_some()
                || self.media_dialog.is_some()
                || self.downloader_update.is_some())
        {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_leave(LeaveAction::Close, &context);
        }
        if self.leave_action.is_none() && self.generation.is_none() && self.media_dialog.is_none() {
            if context.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
                self.save();
            }
            if context.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::G)) {
                self.generate();
            }
            if self.project.is_some()
                && context.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::F))
            {
                self.show_settings = false;
                self.page = EditorPage::Advanced;
                context.memory_mut(|memory| memory.request_focus(egui::Id::new("advanced_search")));
            }
        }
        self.top_bar(ui);
        self.update_notice(ui);
        self.status_bar(ui);
        if self.show_settings {
            self.settings_ui(ui);
        } else if self.project.is_none() {
            self.home(ui);
        } else {
            self.project_ui(ui);
        }
        self.leave_dialog(&context);
        self.show_media(&context);
    }
}

fn render_property_specs(ui: &mut egui::Ui, bot: &mut Nextbot, specs: &[PropertySpec]) {
    for spec in specs {
        let Some(value) = bot.properties.get_mut(spec.name) else {
            continue;
        };
        field_row(ui, spec.label, spec.help, |ui| {
            render_property_value(ui, spec, value)
        });
    }
}

fn render_property_value(ui: &mut egui::Ui, spec: &PropertySpec, value: &mut PropertyValue) {
    match value {
        PropertyValue::Bool(value) => {
            ui.checkbox(value, "Enabled");
        }
        PropertyValue::Number(value) => {
            ui.add(egui::DragValue::new(value).speed(0.1));
        }
        PropertyValue::Integer(value) => {
            ui.add(egui::DragValue::new(value));
        }
        PropertyValue::Text(value) => {
            text_edit(ui, value);
        }
        PropertyValue::StringList(values) => render_string_list(ui, values),
        PropertyValue::IntegerList(values) => render_integer_list(ui, values),
        PropertyValue::Vector(values) | PropertyValue::Angle(values) => {
            for value in values {
                ui.add(egui::DragValue::new(value).speed(0.1));
            }
        }
        PropertyValue::Choice(value) => {
            egui::ComboBox::from_id_salt(("property", spec.name))
                .selected_text(value.as_str())
                .show_ui(ui, |ui| {
                    for choice in spec.choices {
                        ui.selectable_value(value, (*choice).to_owned(), *choice);
                    }
                });
        }
    }
}

fn render_string_list(ui: &mut egui::Ui, values: &mut Vec<String>) {
    ui.vertical(|ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().text_edit_width =
                    (ui.available_width() - 100.0).clamp(80.0, 360.0);
                text_edit(ui, value);
                if ui.add(icon_button(Icon::Trash, "Remove").small()).clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.add(icon_button(Icon::Plus, "Add").small()).clicked() {
            values.push(String::new());
        }
    });
}

fn render_integer_list(ui: &mut egui::Ui, values: &mut Vec<i64>) {
    ui.horizontal_wrapped(|ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            ui.add(egui::DragValue::new(value));
            if ui.add(icon_button(Icon::Trash, "Remove").small()).clicked() {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.add(icon_button(Icon::Plus, "Add").small()).clicked() {
            values.push(0);
        }
    });
}

fn field_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    help: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.push_id(label, |ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        let width = ui.available_width();
        let result = if width < 520.0 {
            ui.vertical(|ui| {
                ui.strong(label).on_hover_text(help);
                let result = ui.horizontal_wrapped(add_contents).inner;
                if !help.is_empty() {
                    ui.label(RichText::new(help).small().weak());
                }
                result
            })
            .inner
        } else {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(166.0, 30.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.set_min_width(166.0);
                        ui.add_space(5.0);
                        ui.strong(label).on_hover_text(help);
                    },
                );
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().text_edit_width = ui.available_width().min(360.0);
                    let result = ui.horizontal_wrapped(add_contents).inner;
                    if !help.is_empty() {
                        ui.label(RichText::new(help).small().weak());
                    }
                    result
                })
                .inner
            })
            .inner
        };
        ui.add_space(7.0);
        result
    })
    .inner
}

fn text_edit(ui: &mut egui::Ui, value: &mut String) -> egui::Response {
    ui.add(egui::TextEdit::singleline(value).margin(egui::Margin::symmetric(8, 7)))
}

fn path_label(ui: &mut egui::Ui, path: &Path) {
    let full_path = path.display().to_string();
    ui.add(
        egui::Label::new(RichText::new(full_path.clone()).monospace())
            .truncate()
            .selectable(true),
    )
    .on_hover_text(full_path);
}

fn activity_selector(ui: &mut egui::Ui, id: &'static str, value: &mut String) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.as_str())
        .show_ui(ui, |ui| {
            for activity in ATTACK_ACTIVITIES {
                ui.selectable_value(value, (*activity).into(), *activity);
            }
        });
}

fn possession_actions() -> [PossessionAction; 6] {
    [
        PossessionAction::PrimaryAttack,
        PossessionAction::SecondaryAttack,
        PossessionAction::Reload,
        PossessionAction::Jump,
        PossessionAction::ToggleCrouch,
        PossessionAction::PlaySpawnSound,
    ]
}

fn trigger_label(trigger: &BindTrigger) -> &'static str {
    match trigger {
        BindTrigger::Pressed => "Pressed",
        BindTrigger::Held => "Held",
        BindTrigger::Released => "Released",
    }
}

fn action_label(action: &PossessionAction) -> &'static str {
    match action {
        PossessionAction::PrimaryAttack => "Primary attack",
        PossessionAction::SecondaryAttack => "Secondary attack",
        PossessionAction::Reload => "Reload",
        PossessionAction::Jump => "Jump",
        PossessionAction::ToggleCrouch => "Toggle crouch",
        PossessionAction::PlaySpawnSound => "Play spawn sound",
    }
}

fn load_preview(context: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    load_preview_named(context, path, "visual_preview")
}

fn load_preview_named(
    context: &egui::Context,
    path: &Path,
    texture_name: &'static str,
) -> Option<egui::TextureHandle> {
    let image = load_visual_image(path)?;
    let rgba = image.thumbnail(512, 512).to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(context.load_texture(
        texture_name,
        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}

fn visual_dimensions(path: &Path) -> Option<(u32, u32)> {
    use image::GenericImageView;
    Some(load_visual_image(path)?.dimensions())
}

fn load_visual_image(path: &Path) -> Option<image::DynamicImage> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(if extension == "vtf" {
        let bytes = std::fs::read(path).ok()?;
        vtf::from_bytes(&bytes).ok()?.highres_image.decode(0).ok()?
    } else if extension == "vmt" {
        let bytes = std::fs::read(path.with_extension("vtf")).ok()?;
        vtf::from_bytes(&bytes).ok()?.highres_image.decode(0).ok()?
    } else {
        image::open(path).ok()?
    })
}

fn configure_theme(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = ui::BACKGROUND;
    visuals.window_fill = ui::SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(19, 21, 27);
    visuals.faint_bg_color = Color32::from_rgb(33, 36, 44);
    visuals.weak_text_color = Some(ui::MUTED);
    visuals.selection.bg_fill = Color32::from_rgb(57, 43, 64);
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::from_rgb(243, 193, 235));
    visuals.hyperlink_color = accent();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, ui::BORDER);
    visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(228, 232, 240);
    visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(213, 219, 230);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 40, 49);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 40, 49);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(62, 67, 81));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(47, 51, 63);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(47, 51, 63);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent().gamma_multiply(0.65));
    visuals.widgets.active.bg_fill = Color32::from_rgb(77, 46, 78);
    for widgets in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widgets.corner_radius = egui::CornerRadius::same(6);
    }
    context.set_visuals(visuals);
    context.global_style_mut(|style| {
        style.animation_time = 0.10;
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 8.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.text_edit_width = 270.0;
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
    });
}

fn page_description(page: EditorPage) -> &'static str {
    match page {
        EditorPage::Basic => "Give your NextBot an identity and choose how it behaves.",
        EditorPage::Visual => "Shape its appearance, animation, and killfeed icon.",
        EditorPage::Audio => "Give each moment its own sound.",
        EditorPage::Combat => "Tune melee attacks, projectiles, and combat behavior.",
        EditorPage::Possession => "Configure player control, camera views, and actions.",
        EditorPage::Events => "Build custom reactions with code-free action recipes.",
        EditorPage::Advanced => {
            "Find and fine-tune every documented DRGBase setting. Ctrl+F to search."
        }
        EditorPage::Project => "Manage project details and your Garry's Mod connection.",
    }
}

fn accent() -> Color32 {
    Color32::from_rgb(219, 128, 207)
}

fn error_color() -> Color32 {
    Color32::from_rgb(246, 131, 145)
}

fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        RichText::new(label)
            .strong()
            .color(Color32::from_rgb(23, 17, 26)),
    )
    .fill(accent())
    .stroke(egui::Stroke::NONE)
}

fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(ui::SURFACE)
        .stroke(egui::Stroke::new(1.0, ui::BORDER))
        .corner_radius(8)
        .inner_margin(20)
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(24, 26, 32))
        .inner_margin(egui::Margin::symmetric(14, 10))
}

fn open_in_explorer(path: &Path) {
    #[cfg(windows)]
    {
        let _ = Command::new("explorer.exe").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_for_test(name: &str) -> CreatorApp {
        let root = std::env::temp_dir().join(format!("nbc_ui_{name}_{}", std::process::id()));
        let project = Project::new("UI test", root.clone());
        CreatorApp {
            portable_root: root.clone(),
            settings: AppSettings {
                projects_root: root,
                garrys_mod_root: None,
                recent_projects: Vec::new(),
                check_for_updates_on_startup: false,
            },
            saved_project: Some(project.clone()),
            project: Some(project),
            recent_projects: Vec::new(),
            selected_bot: 0,
            page: EditorPage::Basic,
            new_project_name: String::new(),
            status: String::new(),
            status_is_error: false,
            advanced_search: String::new(),
            detected_gmod: Vec::new(),
            preview: None,
            killfeed_preview: None,
            generation: None,
            generation_started: None,
            leave_action: None,
            allow_close: false,
            removed_bot: None,
            bot_search: String::new(),
            audio_search: String::new(),
            ffmpeg_available: false,
            link_cache: None,
            status_details: false,
            updates: UpdateChecker::default(),
            show_settings: false,
            settings_page: SettingsPage::General,
            update_notice_dismissed: false,
            media_dialog: None,
            downloader_update: None,
        }
    }

    #[test]
    fn leaving_an_edited_project_requires_a_save_or_explicit_discard() {
        let mut app = app_for_test("leave");
        let context = egui::Context::default();
        app.project.as_mut().unwrap().name = "Unsaved".into();
        app.request_leave(LeaveAction::Home, &context);
        assert!(app.project.is_some());
        assert!(app.leave_action.is_some());
        app.leave_action = None; // Cancel keeps the edits.
        assert!(app.is_dirty());
        app.leave_project(LeaveAction::Home, &context); // Explicit discard.
        assert!(app.project.is_none());
        assert!(!app.is_dirty());
    }

    #[test]
    fn accepting_a_trim_updates_only_the_original_clip_and_rejects_stale_edits() {
        let mut app = app_for_test("trim_target");
        let original: nextbot_creator::domain::AudioClip = PathBuf::from("original.wav").into();
        let project = app.project.as_mut().unwrap();
        project.nextbots[0].audio.spawn.push(original.clone());
        let mut dialog = MediaDialog::new(
            MediaTarget::Audio {
                slot: AudioSlot::Spawn,
                index: Some(0),
            },
            project.root.clone(),
            project.nextbots[0].class_name.clone(),
        );
        dialog.original_clip = Some(original.clone());
        dialog.trim = nextbot_creator::domain::AudioTrim {
            start: 1.0,
            end: Some(2.0),
        };
        dialog.prepared = Some(nextbot_creator::media::PreparedMedia {
            source: original.source.clone(),
            title: "Original".into(),
            source_url: None,
            audio: Some(nextbot_creator::audio::AudioPreview {
                path: PathBuf::from("unused.wav"),
                duration: 4.0,
                peaks: vec![[0.0, 0.1]],
            }),
            image: None,
            workspace: tempfile::tempdir().unwrap(),
        });
        app.accept_media(&dialog).unwrap();
        let clip = &app.project.as_ref().unwrap().nextbots[0].audio.spawn[0];
        assert_eq!(clip.source, original.source);
        assert_eq!(clip.trim, dialog.trim);
        assert!(app.accept_media(&dialog).is_err());
        assert!(app.is_dirty());
    }

    #[test]
    fn failed_save_preserves_edits_and_the_last_saved_state() {
        let mut app = app_for_test("failed_save");
        let root = app.portable_root.clone();
        std::fs::write(&root, b"A file blocks this project directory").unwrap();
        app.project.as_mut().unwrap().name = "Keep this edit".into();
        assert!(!app.save());
        assert!(app.status_is_error);
        assert!(app.is_dirty());
        assert_eq!(app.project.as_ref().unwrap().name, "Keep this edit");
        assert_eq!(app.saved_project.as_ref().unwrap().name, "UI test");
        std::fs::remove_file(root).unwrap();
    }

    #[test]
    fn background_generation_saves_and_prevents_duplicate_jobs_or_closing() {
        let mut app = app_for_test("generation");
        let root = app.portable_root.clone();
        let context = egui::Context::default();
        app.project.as_mut().unwrap().name = "Updated".into();
        app.generate();
        assert!(app.generation.is_some());
        assert!(!app.is_dirty());
        let started = app.generation_started;
        app.generate();
        assert_eq!(app.generation_started, started);
        app.request_leave(LeaveAction::Close, &context);
        assert!(app.project.is_some());
        assert!(!app.allow_close);
        let timeout = Instant::now();
        while app.generation.is_some() && timeout.elapsed() < Duration::from_secs(10) {
            app.poll_generation(&context);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.generation.is_none());
        assert!(!app.status_is_error, "{}", app.status);
        assert!(
            root.join("lua/entities/npc_my_nextbot/shared.lua")
                .is_file()
        );
        assert_eq!(persistence::load_project(&root).unwrap().name, "Updated");
        std::fs::remove_dir_all(root).unwrap();
    }
}
