use super::{CreatorApp, accent, card_frame, error_color, path_label};
use eframe::egui::{self, Color32, RichText};
use nextbot_creator::APP_VERSION;
use nextbot_creator::integration;
use nextbot_creator::updates::{self, UpdateOutcome, UpdateStatus};
use std::path::PathBuf;

impl CreatorApp {
    pub(super) fn settings_ui(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 21, 27))
                    .inner_margin(28),
            )
            .show_inside(root, |ui| {
                let back_label = if self.project.is_some() {
                    "Back to editor"
                } else {
                    "Back to projects"
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚙ Settings").size(28.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(back_label).clicked() {
                            self.show_settings = false;
                        }
                        ui.hyperlink_to("↗ Issues", updates::ISSUES_URL)
                            .on_hover_text("Report a bug or request a feature on GitHub");
                        ui.hyperlink_to("↗ GitHub", updates::REPOSITORY_URL)
                            .on_hover_text("Open the NextbotCreator repository");
                    });
                });
                ui.label(
                    RichText::new("Application preferences, game connection, and tools.").weak(),
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .id_salt("application_settings")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(920.0));
                        card_frame().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.update_settings(ui);
                        });
                        ui.add_space(16.0);
                        card_frame().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.gmod_settings(ui);
                        });
                        ui.add_space(16.0);
                        card_frame().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.media_tool_settings(ui);
                        });
                        ui.add_space(24.0);
                    });
            });
    }

    fn update_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("Updates").strong());
            ui.label(
                RichText::new(format!("Installed version: {APP_VERSION}"))
                    .small()
                    .weak(),
            );
        });
        let previous = self.settings.check_for_updates_on_startup;
        if ui
            .checkbox(
                &mut self.settings.check_for_updates_on_startup,
                "Check for updates at startup",
            )
            .changed()
        {
            match self.settings.save(&self.portable_root) {
                Ok(()) => {
                    self.set_status("Update preference saved.");
                    if self.settings.check_for_updates_on_startup {
                        self.updates.start();
                    }
                }
                Err(error) => {
                    self.settings.check_for_updates_on_startup = previous;
                    self.set_error(error);
                }
            }
        }
        ui.label(
            RichText::new("Checks public GitHub releases. Updates are downloaded manually.")
                .small()
                .weak(),
        );
        match self.updates.status() {
            UpdateStatus::NotChecked => {
                ui.label("No update check yet.");
            }
            UpdateStatus::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking GitHub...");
                });
            }
            UpdateStatus::Finished(Ok(UpdateOutcome::UpToDate)) => {
                ui.label("You're up to date.");
            }
            UpdateStatus::Finished(Ok(UpdateOutcome::NoRelease)) => {
                ui.label(
                    "No public stable release is available yet, or the repository is unavailable.",
                );
            }
            UpdateStatus::Finished(Ok(UpdateOutcome::Available { version, url })) => {
                ui.colored_label(accent(), format!("Version {version} is available."));
                ui.hyperlink_to("View release and download ↗", url);
            }
            UpdateStatus::Finished(Err(error)) => {
                ui.colored_label(error_color(), error.to_string());
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.updates.can_check(),
                    egui::Button::new("Check for updates"),
                )
                .on_disabled_hover_text(
                    "Checks are limited to once per minute. Please wait before trying again.",
                )
                .clicked()
            {
                self.updates.start();
            }
            ui.hyperlink_to("All releases ↗", updates::RELEASES_URL);
        });
    }

    fn gmod_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new("Garry's Mod").strong());
        ui.label(
            RichText::new("Choose the installation used to launch the game and link projects.")
                .small()
                .weak(),
        );
        if let Some(path) = &self.settings.garrys_mod_root {
            path_label(ui, path);
        } else {
            ui.label("Not detected");
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Choose folder…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                match integration::normalize_gmod_root(&path) {
                    Some(path) => self.save_gmod_root(path),
                    None => self.set_error("That folder does not contain garrysmod/addons."),
                }
            }
            if ui.button("Detect again").clicked() {
                self.detected_gmod = integration::detect_garrys_mod();
                if let Some(path) = self.detected_gmod.first().cloned() {
                    self.save_gmod_root(path);
                } else {
                    self.set_error("No Garry's Mod installation was found in Steam libraries.");
                }
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
        ui.heading(RichText::new("Media download tools").strong());
        ui.label(
            RichText::new("Update the downloader if YouTube or TikTok imports stop working.")
                .small()
                .weak(),
        );
        if let Some(job) = &self.downloader_update {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(job.context.progress());
            });
            if ui.button("Cancel downloader update").clicked() {
                job.context.cancel();
            }
        } else if ui
            .add_enabled(
                self.media_dialog.is_none(),
                egui::Button::new("Update downloader"),
            )
            .on_disabled_hover_text("Finish or cancel the current media import first.")
            .clicked()
        {
            let root = self.portable_root.clone();
            self.downloader_update =
                Some(nextbot_creator::media::MediaJob::start(move |context| {
                    nextbot_creator::media_tools::update_downloader(&root, &context)
                }));
        }
    }
}
