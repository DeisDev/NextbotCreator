//! Shared desktop controls and resolution-independent line icons.
//!
//! Icons use a 24-unit canvas and are painted inside native egui buttons so
//! keyboard focus, disabled states, tooltips, and accessibility labels remain intact.

use eframe::egui::{self, Atom, Color32, Rect, RichText, Stroke, Widget};

pub(super) const BACKGROUND: Color32 = Color32::from_rgb(20, 22, 27);
pub(super) const SURFACE: Color32 = Color32::from_rgb(28, 31, 38);
pub(super) const BORDER: Color32 = Color32::from_rgb(48, 52, 63);
pub(super) const MUTED: Color32 = Color32::from_rgb(163, 170, 184);
pub(super) const SUCCESS: Color32 = Color32::from_rgb(137, 207, 173);
pub(super) const WARNING: Color32 = Color32::from_rgb(235, 194, 126);
pub(super) const PRIMARY_TEXT: Color32 = Color32::from_rgb(26, 19, 29);

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Settings,
    Folder,
    Plus,
    Copy,
    Trash,
    Save,
    Generate,
    Play,
    Back,
    ChevronDown,
    Overview,
    Image,
    Audio,
    Combat,
    Game,
    Events,
    Download,
    Refresh,
    External,
    Info,
    Check,
    Warning,
    Search,
    Close,
}

impl Icon {
    pub(super) fn paint(self, ui: &egui::Ui, rect: Rect, color: Color32) {
        let scale = rect.width().min(rect.height()) / 24.0;
        let point = |x: f32, y: f32| rect.min + egui::vec2(x, y) * scale;
        let stroke = Stroke::new(1.8 * scale, color);
        let line = |points: &[[f32; 2]]| {
            ui.painter().add(egui::Shape::line(
                points.iter().map(|[x, y]| point(*x, *y)).collect(),
                stroke,
            ));
        };
        let circle = |x, y, radius| {
            ui.painter()
                .circle_stroke(point(x, y), radius * scale, stroke);
        };
        let box_outline = |x, y, width, height| {
            ui.painter().rect_stroke(
                Rect::from_min_size(point(x, y), egui::vec2(width, height) * scale),
                2.0 * scale,
                stroke,
                egui::StrokeKind::Middle,
            );
        };
        match self {
            Self::Settings => {
                for (y, x) in [(5.0, 8.0), (12.0, 16.0), (19.0, 10.0)] {
                    line(&[[3.0, y], [x - 2.0, y]]);
                    line(&[[x + 2.0, y], [21.0, y]]);
                    circle(x, y, 2.0);
                }
            }
            Self::Folder => line(&[
                [3.0, 20.0],
                [3.0, 5.0],
                [9.0, 5.0],
                [12.0, 8.0],
                [21.0, 8.0],
                [21.0, 20.0],
                [3.0, 20.0],
            ]),
            Self::Plus => {
                line(&[[12.0, 5.0], [12.0, 19.0]]);
                line(&[[5.0, 12.0], [19.0, 12.0]]);
            }
            Self::Copy => {
                box_outline(8.0, 8.0, 13.0, 13.0);
                line(&[
                    [16.0, 5.0],
                    [16.0, 3.0],
                    [3.0, 3.0],
                    [3.0, 16.0],
                    [5.0, 16.0],
                ]);
            }
            Self::Trash => {
                line(&[[3.0, 6.0], [21.0, 6.0]]);
                line(&[[9.0, 6.0], [9.0, 3.0], [15.0, 3.0], [15.0, 6.0]]);
                line(&[[5.0, 6.0], [6.0, 21.0], [18.0, 21.0], [19.0, 6.0]]);
                line(&[[10.0, 10.0], [10.0, 17.0]]);
                line(&[[14.0, 10.0], [14.0, 17.0]]);
            }
            Self::Save => {
                line(&[
                    [20.0, 21.0],
                    [3.0, 21.0],
                    [3.0, 3.0],
                    [17.0, 3.0],
                    [21.0, 7.0],
                    [21.0, 21.0],
                    [20.0, 21.0],
                ]);
                box_outline(7.0, 13.0, 10.0, 8.0);
                line(&[[7.0, 3.0], [7.0, 8.0], [15.0, 8.0], [15.0, 3.0]]);
            }
            Self::Generate => {
                line(&[
                    [12.0, 2.0],
                    [3.0, 7.0],
                    [12.0, 12.0],
                    [21.0, 7.0],
                    [12.0, 2.0],
                ]);
                line(&[
                    [3.0, 7.0],
                    [3.0, 17.0],
                    [12.0, 22.0],
                    [21.0, 17.0],
                    [21.0, 7.0],
                ]);
                line(&[[12.0, 12.0], [12.0, 22.0]]);
            }
            Self::Play => line(&[[7.0, 3.0], [21.0, 12.0], [7.0, 21.0], [7.0, 3.0]]),
            Self::Back => {
                line(&[[11.0, 5.0], [4.0, 12.0], [11.0, 19.0]]);
                line(&[[4.0, 12.0], [21.0, 12.0]]);
            }
            Self::ChevronDown => line(&[[6.0, 9.0], [12.0, 15.0], [18.0, 9.0]]),
            Self::Overview => {
                box_outline(3.0, 3.0, 7.0, 10.0);
                box_outline(14.0, 3.0, 7.0, 6.0);
                box_outline(3.0, 17.0, 7.0, 4.0);
                box_outline(14.0, 13.0, 7.0, 8.0);
            }
            Self::Image => {
                box_outline(3.0, 3.0, 18.0, 18.0);
                circle(8.0, 8.0, 2.0);
                line(&[
                    [3.0, 18.0],
                    [9.0, 12.0],
                    [13.0, 16.0],
                    [17.0, 11.0],
                    [21.0, 15.0],
                ]);
            }
            Self::Audio => {
                for (x, y) in [
                    (4.0, 9.0),
                    (8.0, 5.0),
                    (12.0, 2.0),
                    (16.0, 6.0),
                    (20.0, 9.0),
                ] {
                    line(&[[x, y], [x, 24.0 - y]]);
                }
            }
            Self::Combat => {
                circle(12.0, 12.0, 8.0);
                circle(12.0, 12.0, 3.0);
                for points in [
                    [[12.0, 1.0], [12.0, 6.0]],
                    [[12.0, 18.0], [12.0, 23.0]],
                    [[1.0, 12.0], [6.0, 12.0]],
                    [[18.0, 12.0], [23.0, 12.0]],
                ] {
                    line(&points);
                }
            }
            Self::Game => {
                line(&[
                    [7.0, 6.0],
                    [17.0, 6.0],
                    [20.0, 9.0],
                    [22.0, 18.0],
                    [19.0, 20.0],
                    [15.0, 16.0],
                    [9.0, 16.0],
                    [5.0, 20.0],
                    [2.0, 18.0],
                    [4.0, 9.0],
                    [7.0, 6.0],
                ]);
                line(&[[8.0, 9.0], [8.0, 14.0]]);
                line(&[[5.5, 11.5], [10.5, 11.5]]);
                circle(16.0, 10.0, 0.7);
                circle(18.0, 13.0, 0.7);
            }
            Self::Events => line(&[
                [13.0, 2.0],
                [4.0, 14.0],
                [11.0, 14.0],
                [10.0, 22.0],
                [20.0, 9.0],
                [13.0, 9.0],
                [13.0, 2.0],
            ]),
            Self::Download => {
                line(&[[12.0, 2.0], [12.0, 16.0]]);
                line(&[[7.0, 11.0], [12.0, 16.0], [17.0, 11.0]]);
                line(&[[3.0, 16.0], [3.0, 21.0], [21.0, 21.0], [21.0, 16.0]]);
            }
            Self::Refresh => {
                line(&[
                    [20.0, 10.0],
                    [18.0, 5.0],
                    [12.0, 3.0],
                    [6.0, 5.0],
                    [3.0, 9.0],
                ]);
                line(&[[20.0, 3.0], [20.0, 10.0], [13.0, 10.0]]);
                line(&[
                    [4.0, 14.0],
                    [6.0, 19.0],
                    [12.0, 21.0],
                    [18.0, 19.0],
                    [21.0, 15.0],
                ]);
                line(&[[4.0, 21.0], [4.0, 14.0], [11.0, 14.0]]);
            }
            Self::External => {
                line(&[[14.0, 3.0], [21.0, 3.0], [21.0, 10.0]]);
                line(&[[21.0, 3.0], [10.0, 14.0]]);
                line(&[
                    [10.0, 3.0],
                    [3.0, 3.0],
                    [3.0, 21.0],
                    [21.0, 21.0],
                    [21.0, 14.0],
                ]);
            }
            Self::Info | Self::Warning => {
                if matches!(self, Self::Info) {
                    circle(12.0, 12.0, 9.0);
                    line(&[[12.0, 11.0], [12.0, 17.0]]);
                    circle(12.0, 7.0, 0.5);
                } else {
                    line(&[[12.0, 3.0], [22.0, 21.0], [2.0, 21.0], [12.0, 3.0]]);
                    line(&[[12.0, 9.0], [12.0, 14.0]]);
                    circle(12.0, 17.5, 0.5);
                }
            }
            Self::Check => line(&[[4.0, 12.0], [9.0, 17.0], [20.0, 6.0]]),
            Self::Search => {
                circle(10.0, 10.0, 7.0);
                line(&[[15.0, 15.0], [21.0, 21.0]]);
            }
            Self::Close => {
                line(&[[6.0, 6.0], [18.0, 18.0]]);
                line(&[[6.0, 18.0], [18.0, 6.0]]);
            }
        }
    }
}

pub(super) struct IconButton<'a> {
    button: egui::Button<'a>,
    icon: Icon,
    primary: bool,
    selected: bool,
}

pub(super) fn icon_button(icon: Icon, label: &str) -> IconButton<'_> {
    IconButton {
        button: egui::Button::new((Atom::custom(egui::Id::new("icon"), [16.0, 16.0]), label)),
        icon,
        primary: false,
        selected: false,
    }
}

impl IconButton<'_> {
    pub(super) fn primary(mut self) -> Self {
        self.primary = true;
        self.button = self.button.fill(super::accent()).stroke(Stroke::NONE);
        self
    }

    pub(super) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self.button = self.button.selected(selected);
        self
    }

    pub(super) fn quiet(mut self) -> Self {
        self.button = self.button.frame_when_inactive(self.selected);
        self
    }

    pub(super) fn small(mut self) -> Self {
        self.button = self.button.small();
        self
    }
}

impl Widget for IconButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            if self.primary {
                ui.visuals_mut().override_text_color = Some(PRIMARY_TEXT);
            }
            let result = self.button.atom_ui(ui);
            if let Some(rect) = result.rect(egui::Id::new("icon")) {
                let color = if self.primary {
                    PRIMARY_TEXT
                } else if self.selected {
                    ui.visuals().selection.stroke.color
                } else {
                    ui.style().interact(&result.response).text_color()
                };
                self.icon.paint(ui, rect, color);
            }
            result.response
        })
        .inner
    }
}

pub(super) fn navigation(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    selected: bool,
) -> egui::Response {
    let mut button = icon_button(icon, label).selected(selected).quiet();
    button.button = button.button.right_text(()).truncate().min_size(egui::vec2(
        ui.available_width(),
        if ui.ctx().content_rect().height() < 700.0 {
            30.0
        } else {
            36.0
        },
    ));
    let response = ui.add(button);
    if selected {
        let marker = Rect::from_center_size(
            egui::pos2(response.rect.left() + 2.0, response.rect.center().y),
            egui::vec2(3.0, 18.0),
        );
        ui.painter().rect_filled(marker, 2.0, super::accent());
    }
    response
}

pub(super) fn badge(ui: &mut egui::Ui, icon: Icon, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.10))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 14.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                icon.paint(ui, rect, color);
                ui.label(RichText::new(label).size(12.0).color(color));
            });
        });
}

pub(super) fn section_heading(ui: &mut egui::Ui, icon: Icon, title: &str, description: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
        icon.paint(ui, rect, MUTED);
        ui.heading(title);
    });
    if !description.is_empty() {
        ui.label(RichText::new(description).color(MUTED));
    }
    ui.add_space(8.0);
}

pub(super) fn external_link(ui: &mut egui::Ui, label: &str, url: &str) {
    if ui
        .add(icon_button(Icon::External, label).quiet())
        .on_hover_text(url)
        .clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

pub(super) fn search_field(ui: &mut egui::Ui, text: &mut String, hint: &str, id: egui::Id) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 24.0;
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                Icon::Search.paint(ui, rect, MUTED);
                ui.add(
                    egui::TextEdit::singleline(text)
                        .id(id)
                        .hint_text(hint)
                        .frame(egui::Frame::NONE)
                        .desired_width(ui.available_width()),
                );
            });
        });
}
