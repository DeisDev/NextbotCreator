use super::ui::{self, Icon, badge, external_link, icon_button, navigation, section_heading};
use super::{
    CreatorApp, accent, card_frame, error_color, open_in_explorer, panel_frame, path_label,
};
use eframe::egui::{self, RichText};
use nextbot_creator::APP_VERSION;
use nextbot_creator::updates::{self, UpdateOutcome, UpdateStatus};
use nextbot_creator::{converter, integration};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum SettingsPage {
    General,
    Media,
    Updates,
    About,
}

impl SettingsPage {
    const ALL: [Self; 4] = [Self::General, Self::Media, Self::Updates, Self::About];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Media => "Media tools",
            Self::Updates => "Updates",
            Self::About => "About",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::General => Icon::Settings,
            Self::Media => Icon::Audio,
            Self::Updates => Icon::Download,
            Self::About => Icon::Info,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "Connect your game and find your project files.",
            Self::Media => "Manage the tools used to import and convert media.",
            Self::Updates => "Choose when to check for new versions of NextbotCreator.",
            Self::About => "Application information, support, and licenses.",
        }
    }
}

impl CreatorApp {
    pub(super) fn settings_ui(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("settings_navigation")
            .resizable(false)
            .default_size(226.0)
            .frame(panel_frame())
            .show_inside(root, |ui| {
                ui.add_space(6.0);
                let back_label = if self.project.is_some() {
                    "Back to editor"
                } else {
                    "Back to projects"
                };
                if ui
                    .add(icon_button(Icon::Back, back_label).quiet())
                    .clicked()
                {
                    self.show_settings = false;
                }
                ui.add_space(18.0);
                ui.label(
                    RichText::new("APPLICATION")
                        .small()
                        .strong()
                        .color(ui::MUTED),
                );
                ui.add_space(4.0);
                for page in SettingsPage::ALL {
                    if navigation(ui, page.icon(), page.label(), self.settings_page == page)
                        .clicked()
                    {
                        self.settings_page = page;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(format!("NextbotCreator {APP_VERSION}"))
                            .small()
                            .weak(),
                    );
                    ui.separator();
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui::BACKGROUND).inner_margin(28))
            .show_inside(root, |ui| {
                ui.label(RichText::new("Settings").small().color(ui::MUTED));
                ui.label(
                    RichText::new(self.settings_page.label())
                        .size(28.0)
                        .strong(),
                );
                ui.label(RichText::new(self.settings_page.description()).color(ui::MUTED));
                ui.add_space(20.0);
                egui::ScrollArea::vertical()
                    .id_salt(("application_settings", self.settings_page))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(860.0));
                        match self.settings_page {
                            SettingsPage::General => {
                                self.gmod_settings(ui);
                                ui.add_space(18.0);
                                self.storage_settings(ui);
                            }
                            SettingsPage::Media => self.media_tool_settings(ui),
                            SettingsPage::Updates => self.update_settings(ui),
                            SettingsPage::About => self.about_settings(ui),
                        }
                        ui.add_space(24.0);
                    });
            });
    }

    fn update_settings(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(ui, Icon::Download, "Application updates", "Download new versions from GitHub releases.");
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("NextbotCreator {APP_VERSION}"));
                badge(ui, Icon::Info, "Installed version", ui::MUTED);
            });
            ui.add_space(8.0);
            match self.updates.status() {
                UpdateStatus::NotChecked => { ui.label(RichText::new("Check for updates to see if a newer version is available.").weak()); }
                UpdateStatus::Checking => {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Checking GitHub releases..."); });
                }
                UpdateStatus::Finished(Ok(UpdateOutcome::UpToDate)) => {
                    badge(ui, Icon::Check, "You're up to date", ui::SUCCESS);
                }
                UpdateStatus::Finished(Ok(UpdateOutcome::NoRelease)) => {
                    ui.label("No public stable release is available yet, or the repository is unavailable.");
                }
                UpdateStatus::Finished(Ok(UpdateOutcome::Available { version, url })) => {
                    badge(ui, Icon::Download, &format!("Version {version} available"), accent());
                    external_link(ui, "View release and download", url);
                }
                UpdateStatus::Finished(Err(error)) => {
                    badge(ui, Icon::Warning, "Unable to check for updates", error_color());
                    ui.label("Couldn't reach GitHub. Check your connection and try again.");
                    egui::CollapsingHeader::new("Error details").show(ui, |ui| {
                        ui.label(RichText::new(error.to_string()).small().weak());
                    });
                }
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.updates.can_check(), icon_button(Icon::Refresh, "Check for updates"))
                    .on_disabled_hover_text("A check is running or was made within the last minute. Please wait before trying again.")
                    .clicked()
                {
                    self.updates.start();
                }
                external_link(ui, "All releases", updates::RELEASES_URL);
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            let previous = self.settings.check_for_updates_on_startup;
            if ui.checkbox(&mut self.settings.check_for_updates_on_startup, "Check for updates at startup").changed() {
                match self.settings.save(&self.portable_root) {
                    Ok(()) => {
                        self.set_status("Update preference saved.");
                        if self.settings.check_for_updates_on_startup { self.updates.start(); }
                    }
                    Err(error) => {
                        self.settings.check_for_updates_on_startup = previous;
                        self.set_error(error);
                    }
                }
            }
            ui.label(RichText::new("Checks public GitHub releases. You choose when to download and install an update.").small().weak());
        });
    }

    fn gmod_settings(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(
                ui,
                Icon::Game,
                "Garry's Mod",
                "Choose the installation used to launch the game and link your projects.",
            );
            if let Some(path) = &self.settings.garrys_mod_root {
                badge(ui, Icon::Check, "Installation selected", ui::SUCCESS);
                ui.add_space(6.0);
                ui.label(RichText::new("Installation folder").small().weak());
                path_label(ui, path);
            } else {
                badge(ui, Icon::Warning, "Setup needed", ui::WARNING);
                ui.label("Choose your Garry's Mod folder, or search your Steam libraries.");
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(icon_button(Icon::Folder, "Choose folder..."))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Choose your Garry's Mod installation")
                        .pick_folder()
                {
                    match integration::normalize_gmod_root(&path) {
                        Some(path) => self.save_gmod_root(path),
                        None => self.set_error("That folder does not contain garrysmod/addons."),
                    }
                }
                if ui.add(icon_button(Icon::Search, "Auto-detect")).clicked() {
                    self.detected_gmod = integration::detect_garrys_mod();
                    if let Some(path) = self.detected_gmod.first().cloned() {
                        self.save_gmod_root(path);
                    } else {
                        self.set_error("No Garry's Mod installation was found in Steam libraries.");
                    }
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.label(
                RichText::new("Link each project from the Project menu in the editor.")
                    .small()
                    .weak(),
            );
        });
    }

    fn storage_settings(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(
                ui,
                Icon::Folder,
                "Project storage",
                "New projects are saved in this folder.",
            );
            path_label(ui, &self.settings.projects_root);
            ui.add_space(8.0);
            if ui
                .add(icon_button(Icon::Folder, "Open projects folder"))
                .clicked()
            {
                open_in_explorer(&self.settings.projects_root);
            }
        });
    }

    fn save_gmod_root(&mut self, path: PathBuf) {
        let previous = self.settings.garrys_mod_root.replace(path);
        match self.settings.save(&self.portable_root) {
            Ok(()) => {
                self.link_cache = None;
                self.set_status("Garry's Mod path saved.");
            }
            Err(error) => {
                self.settings.garrys_mod_root = previous;
                self.set_error(error);
            }
        }
    }

    fn media_tool_settings(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(ui, Icon::Audio, "Audio conversion", "FFmpeg converts imported audio for Garry's Mod.");
            if self.ffmpeg_available {
                badge(ui, Icon::Check, "FFmpeg available", ui::SUCCESS);
            } else {
                badge(ui, Icon::Warning, "FFmpeg unavailable", ui::WARNING);
                ui.label("Use the full portable download, or place FFmpeg in the tools folder beside the application.");
            }
            ui.add_space(8.0);
            if ui.add(icon_button(Icon::Refresh, "Refresh tool status")).clicked() {
                self.ffmpeg_available = converter::ffmpeg_path(&self.portable_root).is_some();
            }
        });
        ui.add_space(18.0);
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(ui, Icon::Download, "Media downloader", "Used for audio imports from YouTube and TikTok.");
            ui.label("If video imports stop working, update the bundled downloader and try the import again.");
            ui.add_space(10.0);
            if let Some(job) = &self.downloader_update {
                ui.horizontal_wrapped(|ui| { ui.spinner(); ui.label(job.context.progress()); });
                if ui.add(icon_button(Icon::Close, "Cancel update")).clicked() { job.context.cancel(); }
            } else if ui.add_enabled(self.media_dialog.is_none(), icon_button(Icon::Refresh, "Update downloader"))
                .on_disabled_hover_text("Finish or cancel the current media import first.").clicked()
            {
                let root = self.portable_root.clone();
                self.downloader_update = Some(nextbot_creator::media::MediaJob::start(move |context| {
                    nextbot_creator::media_tools::update_downloader(&root, &context)
                }));
            }
        });
    }

    fn about_settings(&mut self, ui: &mut egui::Ui) {
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(
                ui,
                Icon::Info,
                "NextbotCreator",
                "Create DRGBase NextBots for Garry's Mod.",
            );
            ui.label(format!("Version {APP_VERSION}"));
            ui.label(
                RichText::new("Portable for Windows 10 / 11. Licensed under GPL-3.0-only.").weak(),
            );
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                external_link(ui, "GitHub repository", updates::REPOSITORY_URL);
                external_link(ui, "Report an issue", updates::ISSUES_URL);
            });
        });
        ui.add_space(18.0);
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_heading(
                ui,
                Icon::Folder,
                "Application folder",
                "Settings and bundled tools stay beside the executable.",
            );
            path_label(ui, &self.portable_root);
            if ui
                .add(icon_button(Icon::Folder, "Open application folder"))
                .clicked()
            {
                open_in_explorer(&self.portable_root);
            }
            ui.add_space(6.0);
            egui::CollapsingHeader::new("Third-party notices").show(ui, |ui| {
                ui.label(include_str!("../../THIRD_PARTY_NOTICES.txt"));
            });
        });
    }
}
