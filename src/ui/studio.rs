//! PalmVI workspace navigation. Project actions use the same jobs and dialogs as the profile.
use super::*;
use egui::{pos2, Rect};
use serde::{Deserialize, Serialize};

mod appearance;
use appearance::Artwork;
pub(crate) use appearance::{Appearance, MIDNIGHT};

#[derive(Default)]
pub(crate) struct Studio {
    pub(super) settings_tab: usize,
    library_filter: usize,
    discover_az: bool,
    library_sort: LibrarySort,
    artwork: Artwork,
    specs_open: bool,
    history: history::History,
}
mod history;
mod quick;
#[cfg(test)]
pub(super) mod tests;
pub(super) mod widgets;

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
enum LibrarySort {
    #[default]
    Recent,
    Name,
    HighestFps,
    LowestFps,
}

impl LibrarySort {
    fn label(self) -> &'static str {
        match self {
            Self::Recent => "Recent",
            Self::Name => "A–Z",
            Self::HighestFps => "Highest FPS",
            Self::LowestFps => "Lowest FPS",
        }
    }
}

struct LibraryColumns {
    status: f32,
    last_played: f32,
    fps: f32,
}

impl LibraryColumns {
    fn new(width: f32) -> Self {
        let fps = width - 232.0;
        let last_played = fps - 125.0;
        let status = (last_played * 0.71).min(last_played - 96.0);
        Self { status, last_played, fps }
    }
}

fn view_name(view: View) -> &'static str {
    match view {
        View::Projects => "Project profile",
        View::Settings => "Global settings",
        View::Library => "Library",
        View::Discover => "Discover",
        View::Activity => "Activity",
        View::System => "Your system",
    }
}

impl App {
    pub(super) fn draw_workspace(&mut self, ctx: &egui::Context) {
        self.saved.appearance.apply(ctx);
        self.studio.artwork.prepare(
            ctx,
            &self.saved.appearance,
            self.screenshot_projects.is_some(),
        );
        let screen = ctx.screen_rect();
        let background = ctx.layer_painter(egui::LayerId::background());
        background.rect_filled(screen, 0.0, theme::bg());
        let rect = screen;
        if self.saved.appearance.slideshow()
            && self.screenshot_projects.is_none()
            && self.slides.ready(ctx)
        {
            theme::set_wall_glass(None);
            self.slides
                .paint_in(&background, rect, ctx.input(|i| i.time) as f32, theme::bg());
            if !self.any_job_running() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        } else {
            self.studio.artwork.wall(&background, rect);
        }

        egui::SidePanel::left("workspace-rail")
            .exact_width(if ctx.screen_rect().width() < 1150.0 {
                208.0
            } else {
                224.0
            })
            .resizable(false)
            .frame(Frame::none().inner_margin(Margin {
                left: 17.0,
                right: 17.0,
                top: 24.0,
                bottom: 3.0,
            }))
            .show(ctx, |ui| {
                let rail = Rect::from_min_max(
                    screen.min,
                    pos2(ui.max_rect().right() + 17.0, screen.bottom()),
                );
                ui.painter()
                    .add(theme::glass_surface(rail, 0.0, theme::deep()));
                self.workspace_rail(ui, ctx);
            });
        egui::TopBottomPanel::top("workspace-header")
            .exact_height(60.0)
            .frame(Frame::none().inner_margin(Margin::symmetric(22.0, 0.0)))
            .show(ctx, |ui| {
                ui.painter().add(theme::glass_surface(
                    ui.max_rect().expand2(vec2(22.0, 0.0)),
                    0.0,
                    theme::deep(),
                ));
                ui.horizontal_centered(|ui| {
                    let group = match self.view {
                        View::Projects => "Project profile",
                        View::Settings => "Global settings",
                        _ => "Workspace",
                    };
                    theme::icon(
                        ui,
                        if self.view == View::Settings {
                            glyph::SETTINGS
                        } else {
                            '\u{E8F1}'
                        },
                        14.0,
                        theme::muted(),
                    );
                    ui.label(RichText::new(group).size(13.0).color(theme::muted()));
                    if self.view != View::Projects {
                        ui.label("/");
                        ui.label(
                            RichText::new(if self.view == View::Settings {
                                [
                                    "Files & storage",
                                    "Graphics",
                                    "Launch defaults",
                                    "Appearance & sharing",
                                    "Updates & credits",
                                ][self.studio.settings_tab.min(4)]
                            } else {
                                view_name(self.view)
                            })
                            .font(theme::strong(13.0)),
                        );
                    }
                    if self.view == View::Projects {
                        if let Some(project) = self.selected_project() {
                            ui.label(" / ");
                            let max_name = (ui.available_width() - 406.0).max(28.0);
                            let name = self.display_name(&project);
                            let galley = theme::one_line(
                                ui,
                                &name,
                                theme::strong(13.0),
                                theme::white(),
                                max_name,
                            );
                            ui.label(galley).on_hover_text(name);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let private = self.screenshot_projects.is_some();
                        if widgets::sharing_button(ui, private).clicked() {
                            self.screenshot_projects = (!private)
                                .then(|| self.all_projects().into_iter().map(|p| p.repo).collect());
                            self.studio.history.close();
                        }
                        ui.add_space(12.0);
                        if self.hardware.header(ui) {
                            self.studio.specs_open = true;
                        }
                    });
                })
            });
        egui::CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                Frame::none()
                    .inner_margin(Margin::symmetric(
                        if rect.width() < 840.0 {
                            16.0
                        } else if self.view == View::Library || self.view == View::Discover {
                            34.0
                        } else {
                            (rect.width() * 0.048).min(62.0)
                        },
                        26.0,
                    ))
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        match self.view {
                            View::Projects => self.page(ui, ctx, self.selected_project()),
                            View::Library | View::Discover => self.catalog(ui, ctx),
                            View::Settings => self.settings_page(ui, ctx),
                            View::Activity => self.activity_page(ui),
                            View::System => {
                                egui::ScrollArea::vertical().show(ui, |ui| self.hardware.show(ui));
                            }
                        }
                    });
            });
        if self.studio.specs_open {
            egui::Window::new("Your system")
                .id(egui::Id::new("header-hardware"))
                .open(&mut self.studio.specs_open)
                .collapsible(false)
                .resizable(false)
                .default_width(370.0)
                .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| self.hardware.show(ui));
        }
    }

    pub(super) fn display_name(&self, p: &Project) -> String {
        self.screenshot_alias(&p.repo)
            .map_or_else(|| p.name.clone(), |a| a.0)
    }
    fn select_profile(&mut self, repo: &str) {
        if self.view != View::Projects {
            self.shown = None;
        }
        self.saved.selected = Some(repo.to_owned());
        self.view = View::Projects;
    }
    fn pin_project(&mut self, repo: &str) {
        if self.saved.pinned.iter().any(|r| r == repo) {
            self.saved.pinned.retain(|r| r != repo);
        } else {
            self.saved.pinned.push(repo.to_owned());
            if let Some(p) = self
                .all_projects()
                .into_iter()
                .find(|p| p.repo == repo && !p.custom)
            {
                if !self.saved.known_projects.iter().any(|q| q.repo == repo) {
                    self.saved.known_projects.push(p);
                }
            }
        }
    }

    fn workspace_rail(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            widgets::palm(ui);
            ui.label(
                RichText::new("Palm")
                    .font(theme::deco(35.0))
                    .color(theme::white()),
            );
            ui.label(
                RichText::new("VI")
                    .font(theme::deco(35.0))
                    .color(theme::pink()),
            );
        });
        ui.add_space(4.0);
        if ui
            .add_enabled_ui(self.screenshot_projects.is_none(), |ui| {
                ui.add_sized(
                    [ui.available_width(), 37.0],
                    egui::Button::new(
                        RichText::new("+   Import Repo / ZIP")
                            .font(theme::strong(14.0))
                            .color(theme::action_text()),
                    )
                    .fill(theme::pink())
                    .rounding(7.0),
                )
            })
            .inner
            .clicked()
        {
            self.open_add_repo();
        }
        ui.add_space(7.0);
        theme::caps(ui, "Workspace", 10.0, theme::muted());
        for (view, icon) in [
            (View::Library, '\u{E8F1}'),
            (View::Discover, '\u{E707}'),
            (View::Activity, '\u{E9EC}'),
        ] {
            let active =
                self.view == view || (view == View::Library && self.view == View::Projects);
            let count = if view == View::Library {
                Some(self.library_projects().len())
            } else if view == View::Activity {
                Some(
                    self.project_jobs
                        .0
                        .values()
                        .filter(|j| j.lock().unwrap().running)
                        .count(),
                )
            } else {
                None
            };
            if widgets::nav_item(ui, view_name(view), icon, active, count).clicked() {
                self.view = view;
            }
        }
        let projects = self.all_projects();
        let mut recent: Vec<_> = projects
            .iter()
            .filter_map(|p| {
                self.state_of(&p.repo)?
                    .activity?
                    .last_run
                    .map(|time| (time, p))
            })
            .collect();
        recent.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
        recent.dedup_by(|a, b| a.1.repo == b.1.repo);
        let last = recent.first().map(|(_, p)| (*p).clone());
        let footer = if last.is_some() { 179.0 } else { 83.0 };
        let height = (ui.available_height() - footer).max(50.0);
        let list_start = ui.painter().add(egui::Shape::Noop);
        let out = egui::ScrollArea::vertical()
            .id_salt("rail-projects")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    theme::caps(ui, "Pinned projects", 10.0, theme::muted());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.link("View all").clicked() {
                            self.view = View::Library;
                            self.studio.library_filter = 1;
                        }
                    });
                });
                let pins: Vec<_> = self
                    .saved
                    .pinned
                    .iter()
                    .filter_map(|r| projects.iter().find(|p| p.repo == *r))
                    .collect();
                if pins.is_empty() {
                    note(ui, "Pin a project in Library.");
                }
                for p in pins {
                    self.rail_project(ui, p, None);
                }
                ui.add_space(7.0);
                theme::caps(ui, "Recently played", 10.0, theme::muted());
                for (time, p) in recent.iter().take(8) {
                    let played = crate::build_activity::local_time(*time);
                    self.rail_project(ui, p, played.as_deref());
                }
            });
        widgets::fade_edges(ui, list_start, out.inner_rect, 23.0, 30.0);
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let icon_size = vec2(24.0, 22.0);
                let icon_gap = 8.0;
                let ok = self.files_ok();
                theme::dot(ui, if ok { theme::good() } else { theme::bad() });
                let folder = if self.screenshot_projects.is_some() {
                    "Hidden in sharing mode"
                } else {
                    &self.saved.game_dir
                };
                let label_width =
                    (ui.available_width() - 2.0 * icon_size.x - icon_gap - 6.0).max(24.0);
                if ui
                    .add_sized(
                        vec2(label_width, icon_size.y),
                        egui::Label::new(if ok {
                            "Game files ready"
                        } else {
                            "Choose game folder"
                        })
                        .truncate()
                        .sense(Sense::click()),
                    )
                    .on_hover_text(folder)
                    .clicked()
                    && self.screenshot_projects.is_none()
                {
                    self.pick_game_folder();
                }
                let info = ui
                    .add(
                        egui::Button::new(RichText::new('\u{E946}').font(theme::glyphs(14.0)))
                            .min_size(icon_size)
                            .frame(false),
                    )
                    .on_hover_ui(|ui| self.status_legend(ui));
                egui::popup_below_widget(
                    ui,
                    info.id,
                    &info,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| self.status_legend(ui),
                );
                if info.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(info.id));
                }
                ui.add_space(icon_gap - ui.spacing().item_spacing.x);
                let available = matches!(self.updater.state, crate::updater::State::Available(_));
                let ready = !matches!(
                    self.updater.state,
                    crate::updater::State::Checking | crate::updater::State::Downloading
                );
                let refresh =
                    RichText::new(glyph::SYNC)
                        .font(theme::glyphs(14.0))
                        .color(if available {
                            theme::warn()
                        } else {
                            theme::muted()
                        });
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(refresh).min_size(icon_size).frame(false),
                    )
                    .on_hover_text(if available {
                        "Launcher update available"
                    } else {
                        "Check for launcher updates"
                    })
                    .clicked()
                {
                    if !available {
                        self.updater.check(ctx);
                    }
                    self.open_launcher_updates(ctx);
                }
            });
            let utilities = ui.horizontal(|ui| {
                let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                for (view, icon) in [
                    (View::System, '\u{E7F4}'),
                    (View::Settings, glyph::SETTINGS),
                ] {
                    if widgets::utility(ui, w, icon, view_name(view), self.view == view).clicked() {
                        self.view = view;
                    }
                }
            });
            let row = utilities.response.rect;
            ui.painter().hline(
                row.x_range(),
                row.bottom() + ui.spacing().item_spacing.y * 0.5,
                Stroke::new(1.0_f32, theme::line()),
            );
            if let Some(last) = last {
                let (r, button) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 91.0), Sense::click());
                let artwork_start = ui.painter().add(egui::Shape::Noop);
                self.studio.artwork.thumbnail(ui.painter(), r, &last.repo);
                widgets::fade_edges(ui, artwork_start, r, 17.0, 53.0);
                let paint = ui.painter_at(r);
                paint.text(
                    r.min + vec2(8.0, 5.0),
                    egui::Align2::LEFT_TOP,
                    "LAST SESSION",
                    theme::strong(10.0),
                    Color32::WHITE,
                );
                paint.text(
                    r.left_bottom() + vec2(8.0, -24.0),
                    egui::Align2::LEFT_BOTTOM,
                    self.display_name(&last),
                    theme::strong(15.0),
                    Color32::WHITE,
                );
                paint.text(
                    r.left_bottom() + vec2(8.0, -5.0),
                    egui::Align2::LEFT_BOTTOM,
                    if last.runs_standalone() || !self.files_ok() {
                        "Open project  ›"
                    } else {
                        "Choose save  ›"
                    },
                    theme::regular(12.0),
                    Color32::WHITE,
                );
                if button.clicked() {
                    self.select_profile(&last.repo);
                    if self.blocking_job(&last.repo, false).is_none()
                        && !last.runs_standalone()
                        && self.files_ok()
                    {
                        self.request_play(ctx, &last);
                    }
                }
            }
        });
    }

    fn status_legend(&self, ui: &mut Ui) {
        ui.strong("Project status");
        for (label, color) in [
            ("Installed", theme::good()),
            ("Never run", self.saved.appearance.never()),
            ("Updated, untested", self.saved.appearance.untested()),
            ("Update available", theme::warn()),
        ] {
            ui.horizontal(|ui| {
                theme::dot(ui, color);
                ui.label(label);
            });
        }
    }
    pub(super) fn project_status(&self, p: &Project) -> (String, Color32) {
        if let Some((text, c)) = self.row_status(p) {
            if text == "Installed" {
                if let Some(a) = self
                    .state_of(&p.repo)
                    .and_then(|s| s.activity)
                    .filter(|a| a.pending_label().is_some())
                {
                    return if a.new_install {
                        (
                            "Installed · never run".into(),
                            self.saved.appearance.never(),
                        )
                    } else {
                        (
                            "Updated · untested".into(),
                            self.saved.appearance.untested(),
                        )
                    };
                }
            }
            return (text, c);
        }
        ("Available".into(), theme::muted())
    }

    fn rail_project(&mut self, ui: &mut Ui, p: &Project, played: Option<&str>) {
        let (r, b) = ui.allocate_exact_size(vec2(ui.available_width(), 43.0), Sense::click());
        if b.hovered() {
            ui.painter().rect_filled(r, 6.0, theme::raised());
        }
        self.studio.artwork.thumbnail(
            ui.painter(),
            Rect::from_min_size(r.min + vec2(3.0, 5.0), vec2(33.0, 32.0)),
            &p.repo,
        );
        let x = r.left() + 43.0;
        let (status, color) = self.project_status(p);
        let dot_x = r.right() - ui.spacing().scroll.bar_width.max(6.0) - 10.0;
        let name = theme::one_line(
            ui,
            &self.display_name(p),
            theme::strong(14.0),
            theme::white(),
            (dot_x - x - 9.0).max(20.0),
        );
        ui.painter()
            .galley(pos2(x, r.top() + 4.0), name, theme::white());
        ui.painter().text(
            pos2(x, r.top() + 24.0),
            egui::Align2::LEFT_TOP,
            played.unwrap_or(&status),
            theme::regular(10.5),
            theme::muted(),
        );
        if status != "Installed" && status != "Available" {
            ui.painter()
                .circle_filled(pos2(dot_x, r.center().y), 3.5, color);
        }
        if b.clicked() {
            self.select_profile(&p.repo);
        }
    }

    fn library_projects(&self) -> Vec<Project> {
        let mut seen = std::collections::HashSet::new();
        self.all_projects()
            .into_iter()
            .filter(|p| {
                (p.custom
                    || self.saved.pinned.contains(&p.repo)
                    || self.state_of(&p.repo).is_some_and(|s| s.installed))
                    && seen.insert(p.repo.clone())
            })
            .collect()
    }

    fn catalog(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let discover = self.view == View::Discover;
        theme::title(
            ui,
            if discover {
                "Discover"
            } else {
                "Project library"
            },
            46.0,
            ui.available_width(),
        );
        note(
            ui,
            if discover {
                "Browse community repositories from PortWatch."
            } else {
                "Select a project to manage it, or press Play to choose a save."
            },
        );
        let catalog = self.projects.lock().unwrap().clone();
        let mut rows = if discover {
            catalog
                .as_ref()
                .and_then(|l| l.as_ref().ok())
                .cloned()
                .unwrap_or_default()
        } else {
            self.library_projects()
        };
        let pins = rows
            .iter()
            .filter(|p| self.saved.pinned.contains(&p.repo))
            .count();
        let updates = rows.iter().filter(|p| self.update_for(p).is_some()).count();
        ui.add_space(17.0);
        ui.horizontal_wrapped(|ui| {
            if discover {
                theme::badge(ui, "PortWatch", theme::lilac());
                note(ui, &format!("{} community projects", rows.len()));
                if theme::icon_button(
                    ui,
                    Look::Ghost,
                    Size::Small,
                    glyph::REFRESH,
                    "Reload PortWatch",
                )
                .clicked()
                {
                    self.refresh_projects(ctx);
                }
            } else {
                let filters = [
                    (0, format!("All   {}", rows.len())),
                    (1, format!("Pinned   {pins}")),
                    (2, format!("Updates   {updates}")),
                ];
                if let Some(i) = widgets::tabs(ui, &filters, self.studio.library_filter) {
                    self.studio.library_filter = i;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if discover {
                    if ui.button(if self.studio.discover_az { "A–Z" } else { "Recent" })
                        .on_hover_text("Change sort order").clicked() {
                        self.studio.discover_az = !self.studio.discover_az;
                    }
                    egui::ComboBox::from_id_salt("catalog-gpu")
                        .width(76.0)
                        .selected_text(["All GPUs", "NVIDIA", "AMD"][self.saved.gpu_filter.min(2)])
                        .show_ui(ui, |ui| {
                            for (i, label) in ["All GPUs", "NVIDIA", "AMD"].iter().enumerate() {
                                ui.selectable_value(&mut self.saved.gpu_filter, i, *label);
                            }
                        });
                } else {
                    egui::ComboBox::from_id_salt("library-sort")
                        .width(103.0).selected_text(self.studio.library_sort.label())
                        .show_ui(ui, |ui| {
                            for sort in [LibrarySort::Recent, LibrarySort::Name, LibrarySort::HighestFps, LibrarySort::LowestFps] {
                                ui.selectable_value(&mut self.studio.library_sort, sort, sort.label());
                            }
                            ui.separator();
                            fps_filter(ui, &mut self.saved.include_exact_60_fps);
                        }).response.on_hover_text("Sort projects");
                }
                if self.screenshot_projects.is_none() {
                    theme::field(
                        ui,
                        &mut self.search,
                        "Search projects…",
                        ui.available_width().clamp(105.0, 220.0),
                        Some(glyph::SEARCH),
                    );
                }
            });
        });
        let query = self.search.to_lowercase();
        rows.retain(|p| {
            let filter = self.studio.library_filter;
            let gpu = match self.saved.gpu_filter {
                1 => "nvidia",
                2 => "amd",
                _ => "",
            };
            (discover
                || filter == 0
                || (filter == 1 && self.saved.pinned.contains(&p.repo))
                || (filter == 2 && self.update_for(p).is_some()))
                && (!discover
                    || gpu.is_empty()
                    || p.custom
                    || p.gpu.iter().any(|g| g.eq_ignore_ascii_case(gpu)))
                && (self.screenshot_projects.is_some()
                    || p.name.to_lowercase().contains(&query)
                    || p.repo.to_lowercase().contains(&query))
        });
        ui.add_space(7.0);
        if discover {
            if self.studio.discover_az {
                rows.sort_by_cached_key(|p| self.display_name(p).to_lowercase());
            }
            match catalog {
                None => {
                    ui.spinner();
                }
                Some(Err(error)) => wrapped(
                    ui,
                    if self.screenshot_projects.is_some() {
                        "PortWatch unavailable"
                    } else {
                        &error
                    },
                    theme::warn(),
                ),
                _ => {}
            }
            self.discover_grid(ui, &rows);
            return;
        }
        let averages = self.saved.play_stats.averages(self.saved.include_exact_60_fps);
        match self.studio.library_sort {
            LibrarySort::Recent => rows.sort_by_cached_key(|p| {
                std::cmp::Reverse(self.state_of(&p.repo).and_then(|s| s.activity).and_then(|a| a.last_run))
            }),
            LibrarySort::Name => rows.sort_by_cached_key(|p| self.display_name(p).to_lowercase()),
            direction => rows.sort_by(|a, b| {
                use std::cmp::Ordering;
                match (averages.get(&project_key(&a.repo)), averages.get(&project_key(&b.repo))) {
                    (Some(x), Some(y)) if direction == LibrarySort::HighestFps => y.total_cmp(x),
                    (Some(x), Some(y)) => x.total_cmp(y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
            }),
        }
        theme::glass_frame(ui, 13.0, 1.0, theme::surface(), |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
            let w = r.width();
            let columns = LibraryColumns::new(w);
            let fps_header = Rect::from_min_size(r.min + vec2(columns.fps, 0.0), vec2(64.0, r.height()));
            if ui.interact(fps_header, ui.id().with("sort-fps"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Sort by your average FPS").clicked() {
                self.studio.library_sort = if self.studio.library_sort == LibrarySort::HighestFps {
                    LibrarySort::LowestFps
                } else {
                    LibrarySort::HighestFps
                };
            }
            let fps_title = match self.studio.library_sort {
                LibrarySort::HighestFps => "AVG FPS ↓",
                LibrarySort::LowestFps => "AVG FPS ↑",
                _ => "AVG FPS",
            };
            for (x, label) in [
                (12.0, "PROJECT"),
                (columns.status, "STATUS"),
                (columns.last_played, "LAST PLAYED"),
                (columns.fps, fps_title),
                (w - 158.0, "PIN"),
                (w - 110.0, "ACTIONS"),
            ] {
                ui.painter().text(
                    pos2(r.left() + x, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    theme::strong(10.0),
                    theme::muted(),
                );
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            if rows.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    note(ui, "No matching projects.");
                    if ui.button("Browse Discover").clicked() {
                        self.view = View::Discover;
                    }
                    ui.add_space(16.0);
                });
            }
            egui::ScrollArea::vertical()
                .id_salt((
                    "library",
                    self.studio.library_filter,
                    &query,
                    self.studio.library_sort,
                ))
                .auto_shrink([false, true])
                .show_rows(ui, 65.0, rows.len(), |ui, range| {
                    for i in range {
                        let p = &rows[i];
                        let fps = averages.get(&project_key(&p.repo)).copied();
                        ui.push_id(&p.repo, |ui| self.library_row(ui, ctx, p, fps));
                    }
                });
        });
    }
    fn library_row(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project, fps: Option<f64>) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 65.0), Sense::hover());
        let columns = LibraryColumns::new(r.width());
        let x = r.left();
        let y = r.center().y;
        let play = Rect::from_center_size(pos2(r.right() - 32.0, y), vec2(32.0, 32.0));
        let pin = Rect::from_center_size(pos2(r.right() - 148.0, y), vec2(29.0, 30.0));
        let open = Rect::from_min_max(r.min, pos2(pin.left(), r.bottom()));
        let row = ui
            .interact(open, ui.id().with("open"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if row.hovered() {
            ui.painter().rect_filled(open, 0.0, theme::raised());
        }
        ui.painter()
            .hline(r.x_range(), r.top(), Stroke::new(1.0_f32, theme::line()));
        self.studio.artwork.thumbnail(
            ui.painter(),
            Rect::from_min_size(r.min + vec2(12.0, 10.0), vec2(57.0, 45.0)),
            &p.repo,
        );
        let name = theme::one_line(
            ui,
            &self.display_name(p),
            theme::strong(17.0),
            theme::white(),
            (columns.status - 92.0).max(25.0),
        );
        ui.painter()
            .galley(pos2(x + 80.0, r.top() + 12.0), name, theme::white());
        ui.painter().text(
            pos2(x + 80.0, r.top() + 37.0),
            egui::Align2::LEFT_TOP,
            if p.archive_exe.is_some() {
                "ZIP build"
            } else if p.local_exe.is_some() {
                "Local program"
            } else {
                "Git repository"
            },
            theme::regular(12.0),
            theme::muted(),
        );
        let (label, color) = self.project_status(p);
        ui.painter()
            .circle_filled(pos2(x + columns.status + 4.0, y), 3.5, color);
        ui.painter().text(
            pos2(x + columns.status + 14.0, y),
            egui::Align2::LEFT_CENTER,
            label,
            theme::regular(12.0),
            color,
        );
        let state = self.state_of(&p.repo);
        let last = state
            .as_ref()
            .and_then(|s| s.activity.as_ref())
            .and_then(|a| a.last_run)
            .and_then(crate::build_activity::local_time)
            .unwrap_or_else(|| "—".into());
        ui.painter().text(
            pos2(x + columns.last_played, y),
            egui::Align2::LEFT_CENTER,
            last,
            theme::regular(12.0),
            theme::muted(),
        );
        let badge = Rect::from_center_size(pos2(x + columns.fps + 21.0, y), vec2(42.0, 33.0));
        let ink = if fps.is_some() { theme::pink() } else { theme::muted() };
        ui.painter().rect(badge, 10.0, theme::raised(), Stroke::new(1.0_f32, theme::line()));
        ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER,
            fps.map_or_else(|| "—".into(), |v| format!("{v:.1}")), theme::strong(12.0), ink);
        if row.clicked() {
            self.select_profile(&p.repo);
        }
        let pinned = self.saved.pinned.contains(&p.repo);
        let b = ui
            .interact(pin, ui.id().with("pin"), Sense::click())
            .on_hover_text(if pinned {
                "Unpin project"
            } else {
                "Pin project"
            });
        ui.painter().text(
            pin.center(),
            egui::Align2::CENTER_CENTER,
            if pinned { "★" } else { "☆" },
            theme::regular(21.0),
            if pinned {
                theme::warn()
            } else {
                theme::muted()
            },
        );
        if b.clicked() {
            self.pin_project(&p.repo);
        }
        let installed = state.as_ref().is_some_and(|s| s.installed)
            || (state.as_ref().is_none_or(|s| s.checking.is_some())
                && (p.archive_program().is_some_and(|exe| exe.is_file())
                    || (p.is_git() && project_installed(&p.repo))));
        let running = self
            .project_jobs
            .get(&p.repo)
            .is_some_and(|job| job.lock().unwrap().running);
        if p.is_git() && !running && (!installed || self.update_for(p).is_some()) {
            let updating = installed;
            let label = if updating { "Update" } else { "Install" };
            let blocked = self.blocking_job(&p.repo, true).is_some();
            let tools_ready = self.tools.as_ref().is_some_and(|tools| tools.ready());
            let enabled = !blocked && (updating || tools_ready);
            let right = if updating {
                play.left() - 6.0
            } else {
                play.right()
            };
            let rect = Rect::from_center_size(pos2(right - 32.0, y), vec2(64.0, 32.0));
            let button = ui.interact(
                rect,
                ui.id().with("install-update"),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            let color = if !enabled {
                theme::muted()
            } else if updating {
                theme::warn()
            } else {
                theme::lilac()
            };
            ui.painter().rect(
                rect,
                6.0,
                theme::mix(
                    theme::deep(),
                    color,
                    if button.hovered() { 0.23 } else { 0.10 },
                ),
                Stroke::new(1.0_f32, theme::mix(theme::line(), color, 0.4)),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                theme::strong(13.0),
                color,
            );
            button.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label)
            });
            let tip = if blocked {
                "Another job must finish first"
            } else if !enabled {
                "Set up build tools in Global settings"
            } else if updating {
                "Update this project"
            } else {
                "Install this project"
            };
            let button = button.on_hover_text(tip);
            if enabled
                && button
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
            {
                self.start(ctx, p.clone(), Kind::Install);
            }
        }
        if !installed {
            return;
        }
        let can_play = self.blocking_job(&p.repo, false).is_none()
            && if p.runs_standalone() {
                p.archive_exe.is_some()
                    || crate::rust_install(&p.repo).is_some_and(|r| r.executable.is_some())
            } else {
                self.files_ok()
            };
        let b = ui.interact(
            play,
            ui.id().with("play"),
            if can_play {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        ui.painter().rect_filled(
            play,
            6.0,
            if can_play {
                theme::mix(theme::deep(), theme::lilac(), 0.15)
            } else {
                theme::deep()
            },
        );
        ui.painter().text(
            play.center(),
            egui::Align2::CENTER_CENTER,
            glyph::PLAY,
            theme::glyphs(15.0),
            if can_play {
                theme::lilac()
            } else {
                theme::muted()
            },
        );
        if b.on_hover_text(if can_play {
            "Play project"
        } else {
            "Open the project to see launch requirements"
        })
        .clicked()
        {
            self.select_profile(&p.repo);
            self.request_play(ctx, p);
        }
    }
    fn discover_grid(&mut self, ui: &mut Ui, rows: &[Project]) {
        let columns = (ui.available_width() / 240.0).floor().clamp(1.0, 3.0) as usize;
        egui::ScrollArea::vertical()
            .id_salt("discover-grid")
            .show(ui, |ui| {
                for chunk in rows.chunks(columns) {
                    ui.columns(columns, |cols| {
                        for (col, p) in cols.iter_mut().zip(chunk) {
                            col.push_id(&p.repo, |ui| {
                                theme::card(ui, 12.0, |ui| {
                                    let (r, _) = ui.allocate_exact_size(
                                        vec2(ui.available_width(), 80.0),
                                        Sense::hover(),
                                    );
                                    self.studio.artwork.thumbnail(ui.painter(), r, &p.repo);
                                    ui.strong(self.display_name(p));
                                    let mut meta = gpu_text(&p.gpu);
                                    if p.verified {
                                        meta.push_str("  ·  Verified");
                                    }
                                    if let Some(f) = p.fps {
                                        meta.push_str(&format!("  ·  {f:.0} FPS"));
                                    }
                                    note(ui, &meta);
                                    if ui.button("View project").clicked() {
                                        self.select_profile(&p.repo);
                                    }
                                });
                            });
                        }
                    });
                }
            });
    }

    pub(super) fn settings_workspace(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::title(ui, "Global settings", 44.0, ui.available_width());
        note(
            ui,
            "Defaults for all projects. Individual profiles can override them.",
        );
        ui.add_space(12.0);
        if self.screenshot_projects.is_some() {
            note(ui, "Settings are hidden in sharing mode.");
            return;
        }
        ui.horizontal_top(|ui| {
            let side = if ui.available_width() > 880.0 {
                200.0
            } else {
                150.0
            };
            ui.allocate_ui_with_layout(vec2(side, 0.0), Layout::top_down(Align::Min), |ui| {
                theme::glass_frame(ui, 10.0, 5.0, theme::deep(), |ui| {
                    ui.set_width(side - 12.0);
                    for (i, label) in [
                        "Files & storage",
                        "Graphics",
                        "Launch defaults",
                        "Appearance & sharing",
                        "Updates & credits",
                    ]
                    .iter()
                    .enumerate()
                    {
                        let (r, b) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 39.0), Sense::click());
                        let sel = self.studio.settings_tab == i;
                        if sel || b.hovered() {
                            ui.painter().rect_filled(r, 6.0, theme::raised());
                        }
                        ui.painter().text(
                            pos2(r.left() + 10.0, r.center().y),
                            egui::Align2::LEFT_CENTER,
                            *label,
                            theme::strong(14.0),
                            if sel { theme::lilac() } else { theme::muted() },
                        );
                        if b.clicked() {
                            self.studio.settings_tab = i;
                        }
                    }
                });
            });
            let available = ui.available_size();
            ui.allocate_ui_with_layout(available, Layout::top_down(Align::Min), |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("settings", self.studio.settings_tab))
                    .auto_shrink([false; 2])
                    .show(ui, |ui| match self.studio.settings_tab {
                        1 => self.graphics_card(ui),
                        2 => {
                            self.launch_card(ui);
                            self.switches_card(ui);
                        }
                        3 => {
                            theme::card(ui, 16.0, |ui| {
                                ui.heading("Discord Rich Presence");
                                ui.checkbox(&mut self.saved.discord.enabled, "Show gameplay on Discord");
                                note(ui, "Shows Newer Retail Game and elapsed play time while the game is running.");
                                if self.saved.discord.enabled {
                                    note(ui, self.presence.status());
                                    egui::CollapsingHeader::new("Discord application IDs").show(ui, |ui| {
                                        for (label, value) in [
                                            ("Game application ID", &mut self.saved.discord.game_id),
                                            ("PalmVI Launcher ID (optional idle status)", &mut self.saved.discord.launcher_id),
                                        ] {
                                            ui.label(label);
                                            ui.add(egui::TextEdit::singleline(value).char_limit(20).hint_text("Public application ID"));
                                            if !value.trim().is_empty() && crate::discord::app_id(value).is_none() {
                                                ui.colored_label(theme::warn(), "Enter a numeric Discord application ID.");
                                            }
                                        }
                                    });
                                    ui.ctx().request_repaint_after(Duration::from_secs(1));
                                }
                            });
                            self.saved.appearance.controls(ui, &self.studio.artwork);
                            self.pictures_card(ui);
                        }
                        4 => self.about_card(ui, ctx),
                        _ => {
                            self.game_files_card(ui, ctx);
                            self.install_folder_card(ui);
                            theme::card(ui, 16.0, |ui| self.build_tools_status(ui, ctx));
                        }
                    })
            });
        });
    }
}
