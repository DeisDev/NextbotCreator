use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{self, Color32, RichText};

use nextbot_creator::catalog::{PropertySection, PropertySpec, property_catalog};
use nextbot_creator::converter;
use nextbot_creator::domain::{
    ATTACK_ACTIVITIES, BaseVariant, BehaviorPreset, BindTrigger, DAMAGE_TYPES, HookAction,
    HookActionKind, HookEvent, HookRecipe, KillfeedIconMode, Nextbot, POSSESSION_KEYS,
    PossessionAction, PossessionBind, PossessionView, Project, PropertyValue, SpawnTab,
    sanitize_class_name, slugify,
};
use nextbot_creator::generator;
use nextbot_creator::integration::{self, LinkStatus};
use nextbot_creator::persistence::{self, AppSettings};
use nextbot_creator::{APP_NAME, APP_VERSION, PROJECT_FILE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Self::Basic => "Basic",
            Self::Visual => "Visual",
            Self::Audio => "Audio",
            Self::Combat => "Combat",
            Self::Possession => "Possession",
            Self::Events => "Events",
            Self::Advanced => "Advanced",
            Self::Project => "Project",
        }
    }
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

    fn save(&mut self) {
        let Some(project) = &self.project else { return };
        match persistence::save_project(project) {
            Ok(()) => self.set_status("Project saved."),
            Err(error) => self.set_error(error),
        }
    }

    fn generate(&mut self) {
        let Some(project) = &self.project else { return };
        if let Err(error) = persistence::save_project(project) {
            self.set_error(error);
            return;
        }
        match generator::generate_project(project, &self.portable_root) {
            Ok(report) => {
                let mut message = format!("Generated {} files", report.files_written);
                if report.files_removed > 0 {
                    message.push_str(&format!("; removed {} stale files", report.files_removed));
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
            Err(error) => self.set_error(error),
        }
    }

    fn link_or_unlink(&mut self, unlink: bool) {
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
                let project_root = project.root.clone();
                self.selected_bot = 0;
                self.preview = None;
                self.killfeed_preview = None;
                self.project = Some(project);
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
                let project_root = project.root.clone();
                self.project = Some(project);
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
            slot.get_mut(&mut bot.audio).extend(imported);
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
                    ui.add_space(8.0);
                    ui.label(RichText::new(APP_NAME).size(20.0).strong().color(accent()));
                    ui.label(RichText::new(format!("v{APP_VERSION}")).weak());
                    ui.separator();
                    if ui.button("Home").clicked() {
                        self.project = None;
                        self.preview = None;
                        self.killfeed_preview = None;
                    }
                    let launch = ui
                        .add_enabled(
                            self.settings.garrys_mod_root.is_some(),
                            egui::Button::new("Launch GMod"),
                        )
                        .on_hover_text(if self.settings.garrys_mod_root.is_some() {
                            "Start Garry's Mod from the configured installation"
                        } else {
                            "Choose or detect a Garry's Mod installation first"
                        });
                    if launch.clicked() {
                        self.launch_gmod();
                    }
                    if self.project.is_some() {
                        if ui.button("Save").clicked() {
                            self.save();
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Generate addon").strong())
                                    .fill(accent().gamma_multiply(0.45)),
                            )
                            .clicked()
                        {
                            self.generate();
                        }
                        let link_status = self.current_link_status();
                        match link_status {
                            LinkStatus::Linked(_) => {
                                if ui.button("Unlink from GMod").clicked() {
                                    self.link_or_unlink(true);
                                }
                            }
                            LinkStatus::Unlinked => {
                                if ui.button("Link to GMod").clicked() {
                                    self.link_or_unlink(false);
                                }
                            }
                            LinkStatus::Conflict(_) => {
                                ui.colored_label(Color32::YELLOW, "Addon-path conflict");
                            }
                        }
                        if ui.button("Open folder").clicked()
                            && let Some(project) = &self.project
                        {
                            open_in_explorer(&project.root);
                        }
                    }
                });
            });
    }

    fn current_link_status(&self) -> LinkStatus {
        match (&self.project, &self.settings.garrys_mod_root) {
            (Some(project), Some(gmod)) => {
                integration::link_status(gmod, &project.slug, &project.root)
            }
            _ => LinkStatus::Unlinked,
        }
    }

    fn home(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(root, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading(RichText::new("Build a DRGBase NextBot").size(30.0));
                ui.label(
                    RichText::new(
                        "Portable projects, automatic asset conversion, no Lua required.",
                    )
                    .weak(),
                );
                ui.add_space(30.0);
            });
            ui.columns(2, |columns| {
                columns[0].group(|ui| {
                    ui.heading("New project");
                    ui.label("Project name");
                    ui.text_edit_singleline(&mut self.new_project_name);
                    ui.label(RichText::new("Saved under").small().weak());
                    path_label(ui, &self.settings.projects_root);
                    if ui
                        .add_enabled(
                            !self.new_project_name.trim().is_empty(),
                            egui::Button::new("Create project"),
                        )
                        .clicked()
                    {
                        self.create_project();
                    }
                });
                columns[1].group(|ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Recent projects");
                        if ui.small_button("Refresh").clicked() {
                            self.refresh_recent();
                        }
                        if ui.small_button("Open another…").clicked()
                            && let Some(path) = rfd::FileDialog::new().pick_folder()
                        {
                            self.open_project(&path);
                        }
                    });
                    let projects = self.recent_projects.clone();
                    if projects.is_empty() {
                        ui.label(RichText::new("No projects yet.").weak());
                    }
                    for path in projects {
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Project");
                        if ui
                            .selectable_label(false, name)
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            self.open_project(&path);
                        }
                    }
                });
            });
            ui.add_space(24.0);
            ui.group(|ui| {
                ui.heading("Garry's Mod");
                if let Some(path) = &self.settings.garrys_mod_root {
                    path_label(ui, path);
                } else {
                    ui.label("Not detected");
                }
                ui.horizontal(|ui| {
                    if ui.button("Choose folder…").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        match integration::normalize_gmod_root(&path) {
                            Some(path) => {
                                self.settings.garrys_mod_root = Some(path);
                                if let Err(error) = self.settings.save(&self.portable_root) {
                                    self.set_error(error);
                                } else {
                                    self.set_status("Garry's Mod path saved.");
                                }
                            }
                            None => {
                                self.set_error("That folder does not contain garrysmod/addons.")
                            }
                        }
                    }
                    if ui.button("Detect again").clicked() {
                        self.detected_gmod = integration::detect_garrys_mod();
                        if let Some(path) = self.detected_gmod.first().cloned() {
                            self.settings.garrys_mod_root = Some(path);
                            let _ = self.settings.save(&self.portable_root);
                            self.set_status("Garry's Mod detected.");
                        } else {
                            self.set_error(
                                "No Garry's Mod installation was found in Steam libraries.",
                            );
                        }
                    }
                });
            });
        });
    }

    fn project_ui(&mut self, root: &mut egui::Ui) {
        self.bot_sidebar(root);
        egui::Panel::bottom("status_bar")
            .frame(panel_frame())
            .show_inside(root, |ui| {
                let color = if self.status_is_error {
                    Color32::from_rgb(255, 110, 125)
                } else {
                    Color32::from_rgb(150, 180, 165)
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(if self.status_is_error {
                            "Error"
                        } else {
                            "Status"
                        })
                        .strong()
                        .color(color),
                    );
                    ui.label(&self.status);
                    if converter::ffmpeg_path(&self.portable_root).is_none() {
                        ui.separator();
                        ui.colored_label(
                            Color32::YELLOW,
                            "FFmpeg is unavailable; audio generation is disabled.",
                        );
                    }
                });
            });

        egui::CentralPanel::default().show_inside(root, |ui| {
            ui.horizontal(|ui| {
                for page in EditorPage::ALL {
                    if ui
                        .selectable_label(self.page == page, page.label())
                        .clicked()
                    {
                        self.page = page;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.page {
                    EditorPage::Basic => self.basic_editor(ui),
                    EditorPage::Visual => {
                        let context = ui.ctx().clone();
                        self.visual_editor(ui, &context)
                    }
                    EditorPage::Audio => self.audio_editor(ui),
                    EditorPage::Combat => self.combat_editor(ui),
                    EditorPage::Possession => self.possession_editor(ui),
                    EditorPage::Events => self.events_editor(ui),
                    EditorPage::Advanced => self.advanced_editor(ui),
                    EditorPage::Project => self.project_editor(ui),
                });
        });
    }

    fn bot_sidebar(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("bot_sidebar")
            .resizable(false)
            .default_size(210.0)
            .frame(panel_frame())
            .show_inside(root, |ui| {
                ui.heading("NextBots");
                let names = self
                    .project
                    .as_ref()
                    .map(|project| {
                        project
                            .nextbots
                            .iter()
                            .map(|bot| bot.display_name.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for (index, name) in names.iter().enumerate() {
                    if ui
                        .selectable_label(self.selected_bot == index, name)
                        .clicked()
                    {
                        self.selected_bot = index;
                        self.preview = None;
                        self.killfeed_preview = None;
                    }
                }
                ui.add_space(8.0);
                if ui.button("+ Add NextBot").clicked()
                    && let Some(project) = self.project.as_mut()
                {
                    let index = project.nextbots.len() + 1;
                    project.nextbots.push(Nextbot::new(
                        format!("Nextbot {index}"),
                        format!("npc_{}_{}", project.slug, index),
                    ));
                    self.selected_bot = project.nextbots.len() - 1;
                    self.preview = None;
                    self.killfeed_preview = None;
                }
                if ui.button("Duplicate").clicked()
                    && let Some(project) = self.project.as_mut()
                    && let Some(mut duplicate) = project.nextbots.get(self.selected_bot).cloned()
                {
                    duplicate.display_name.push_str(" Copy");
                    duplicate.class_name = format!("{}_copy", duplicate.class_name);
                    project.nextbots.push(duplicate);
                    self.selected_bot = project.nextbots.len() - 1;
                    self.preview = None;
                    self.killfeed_preview = None;
                }
                let can_remove = self
                    .project
                    .as_ref()
                    .is_some_and(|project| project.nextbots.len() > 1);
                if ui
                    .add_enabled(can_remove, egui::Button::new("Remove"))
                    .clicked()
                    && let Some(project) = self.project.as_mut()
                {
                    project.nextbots.remove(self.selected_bot);
                    self.selected_bot = self
                        .selected_bot
                        .min(project.nextbots.len().saturating_sub(1));
                    self.preview = None;
                    self.killfeed_preview = None;
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new("DRGBase required in-game").small().weak());
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
                ui.text_edit_singleline(&mut bot.display_name);
            },
        );
        field_row(
            ui,
            "Class name",
            "Stable lowercase entity identifier.",
            |ui| {
                if ui.text_edit_singleline(&mut bot.class_name).lost_focus() {
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
                    ui.text_edit_singleline(&mut bot.custom_tab_name);
                },
            );
        }
        field_row(ui, "Category", "Defaults to NPCs > Nextbot.", |ui| {
            ui.text_edit_singleline(&mut bot.category);
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
        ui.heading("Quick behavior presets");
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

        ui.add_space(16.0);
        ui.heading("Common DRGBase settings");
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
        ui.label("PNG, JPEG, BMP, TGA, WebP and GIF inputs are scaled with aspect-preserving transparent padding and encoded as DXT5 VTF. GIFs become animated VTFs. Existing VTF/VMT pairs are accepted.");
        if ui.button("Import visual asset…").clicked() {
            self.import_visual(context);
        }
        if let Some((_, texture)) = &self.preview {
            ui.add(egui::Image::new(texture).max_size(egui::vec2(280.0, 280.0)));
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
                if ui
                    .text_edit_singleline(&mut bot.visual.material_name)
                    .lost_focus()
                {
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
        if custom_mode {
            if ui.button("Import custom killfeed image...").clicked() {
                import_custom = true;
            }
            if let Some(source) = &custom_source {
                path_label(ui, source);
            }
        } else if selected.is_none() {
            ui.colored_label(
                Color32::YELLOW,
                "Import a NextBot visual asset to generate its killfeed icon.",
            );
        }
        if mode_changed {
            self.killfeed_preview = None;
        }
        if import_custom {
            self.import_killfeed_icon(context);
        }
        if let Some((_, texture)) = &self.killfeed_preview {
            ui.add(egui::Image::new(texture).max_size(egui::vec2(128.0, 128.0)));
        }
    }

    fn audio_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sounds");
        ui.label("All imported audio is normalized to mono 16-bit PCM WAV at 44.1 kHz for reliable Source playback. Multiple files in a slot are selected randomly through a generated sound script.");
        let slots = [
            AudioSlot::Spawn,
            AudioSlot::Idle,
            AudioSlot::Damage,
            AudioSlot::Death,
            AudioSlot::Downed,
            AudioSlot::Jump,
            AudioSlot::Footsteps,
        ];
        for slot in slots {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong(slot.label());
                    if ui.small_button("Add files…").clicked() {
                        self.import_audio(slot);
                    }
                });
                let Some(bot) = self.selected_bot_mut() else {
                    return;
                };
                let files = slot.get_mut(&mut bot.audio);
                let mut remove = None;
                for (index, file) in files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            file.file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("audio"),
                        );
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    files.remove(index);
                }
            });
        }
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
                    ui.text_edit_singleline(&mut bot.combat.projectile_class);
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
                    ui.text_edit_singleline(&mut view.name);
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
        if ui.button("+ Add view").clicked() {
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
        if ui.button("+ Add bind").clicked() {
            bot.possession_binds.push(PossessionBind {
                key: "IN_ATTACK".into(),
                trigger: BindTrigger::Held,
                action: PossessionAction::PrimaryAttack,
            });
        }
    }

    fn advanced_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("All documented DRGBase properties");
        ui.label("Cataloged from the current DRGBase base, human, sprite, and official template sources. Values are emitted as typed Lua; no free-form code is required.");
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut self.advanced_search);
            if ui.small_button("Clear").clicked() {
                self.advanced_search.clear();
            }
        });
        let query = self.advanced_search.trim().to_ascii_lowercase();
        let Some(bot) = self.selected_bot_mut() else {
            return;
        };
        let catalog = property_catalog();
        for section in PropertySection::ALL {
            let specs = catalog
                .iter()
                .filter(|spec| spec.section == section)
                .filter(|spec| {
                    query.is_empty()
                        || spec.name.to_ascii_lowercase().contains(&query)
                        || spec.label.to_ascii_lowercase().contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>();
            if specs.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(section.label())
                .default_open(matches!(
                    section,
                    PropertySection::Stats | PropertySection::Ai
                ))
                .show(ui, |ui| {
                    render_property_specs(ui, bot, &specs);
                });
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
                        if ui.small_button("−").clicked() { remove_action = Some(action_index); }
                    });
                }
                if let Some(index) = remove_action { recipe.actions.remove(index); }
                if ui.small_button("+ Add action").clicked() { recipe.actions.push(HookAction::default()); }
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
            ui.text_edit_singleline(&mut project.name);
        });
        field_row(
            ui,
            "Folder slug",
            "Stable addon folder and sound namespace.",
            |ui| {
                if ui.text_edit_singleline(&mut project.slug).lost_focus() {
                    project.slug = slugify(&project.slug);
                }
            },
        );
        field_row(ui, "Author", "Written into project metadata.", |ui| {
            ui.text_edit_singleline(&mut project.author);
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
            ui.colored_label(Color32::YELLOW, "No valid Garry's Mod folder selected.");
        }
        if ui.button("Choose Garry's Mod folder…").clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            match integration::normalize_gmod_root(&path) {
                Some(path) => {
                    self.settings.garrys_mod_root = Some(path);
                    if let Err(error) = self.settings.save(&self.portable_root) {
                        self.set_error(error);
                    } else {
                        self.set_status("Garry's Mod path saved.");
                    }
                }
                None => self.set_error("That folder does not contain garrysmod/addons."),
            }
        }
    }
}

impl eframe::App for CreatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.top_bar(ui);
        if self.project.is_none() {
            egui::Panel::bottom("home_status")
                .frame(panel_frame())
                .show_inside(ui, |ui| {
                    let color = if self.status_is_error {
                        Color32::from_rgb(255, 110, 125)
                    } else {
                        Color32::GRAY
                    };
                    ui.colored_label(color, &self.status);
                });
            self.home(ui);
        } else {
            self.project_ui(ui);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AudioSlot {
    Spawn,
    Idle,
    Damage,
    Death,
    Downed,
    Jump,
    Footsteps,
}

impl AudioSlot {
    fn label(self) -> &'static str {
        match self {
            Self::Spawn => "Spawn",
            Self::Idle => "Idle",
            Self::Damage => "Damage",
            Self::Death => "Death",
            Self::Downed => "Downed",
            Self::Jump => "Jump",
            Self::Footsteps => "Footsteps",
        }
    }

    fn get_mut(self, audio: &mut nextbot_creator::domain::AudioSettings) -> &mut Vec<PathBuf> {
        match self {
            Self::Spawn => &mut audio.spawn,
            Self::Idle => &mut audio.idle,
            Self::Damage => &mut audio.damage,
            Self::Death => &mut audio.death,
            Self::Downed => &mut audio.downed,
            Self::Jump => &mut audio.jump,
            Self::Footsteps => &mut audio.footsteps,
        }
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
            ui.text_edit_singleline(value);
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
                ui.text_edit_singleline(value);
                if ui.small_button("−").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.small_button("+ Add").clicked() {
            values.push(String::new());
        }
    });
}

fn render_integer_list(ui: &mut egui::Ui, values: &mut Vec<i64>) {
    ui.horizontal_wrapped(|ui| {
        let mut remove = None;
        for (index, value) in values.iter_mut().enumerate() {
            ui.add(egui::DragValue::new(value));
            if ui.small_button("−").clicked() {
                remove = Some(index);
            }
        }
        if let Some(index) = remove {
            values.remove(index);
        }
        if ui.small_button("+").clicked() {
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
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(180.0);
            ui.strong(label).on_hover_text(help);
            ui.label(RichText::new(help).small().weak());
        });
        ui.vertical(|ui| add_contents(ui)).inner
    })
    .inner
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
    let rgba = image.to_rgba8();
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
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(18, 19, 23);
    visuals.window_fill = Color32::from_rgb(22, 23, 28);
    visuals.extreme_bg_color = Color32::from_rgb(10, 11, 14);
    visuals.selection.bg_fill = accent().gamma_multiply(0.55);
    visuals.hyperlink_color = accent();
    visuals.widgets.active.bg_fill = accent().gamma_multiply(0.65);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(62, 38, 65);
    context.set_visuals(visuals);
    context.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

fn accent() -> Color32 {
    Color32::from_rgb(235, 30, 210)
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(14, 15, 18))
        .inner_margin(egui::Margin::symmetric(8, 7))
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
