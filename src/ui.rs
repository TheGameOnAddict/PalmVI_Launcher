//! What Mandyhub shows: the sunset strip with its tabs, the project list, a
//! project's page, the settings page, the status bar and the dialogs. The
//! work they start (install, play, update checks, moving folders) lives in
//! main.rs.

use crate::theme::{self, glyph, Look, Size, Stat};
use crate::{default_workspace, parse_git_repo, project_installed, project_key, rust_install, search_drives, slides, tools, workspace};
use crate::{AddRepo, App, GraphicsSettings, Kind, OutputResolution, Project, ProjectCheckFeedback, View};
use eframe::egui::{self, vec2, Align, Color32, Frame, Label, Layout, Margin, RichText, Sense, Stroke, Ui};
use std::time::Duration;

mod save_picker;
mod launch_presets;

#[derive(Clone, Copy, PartialEq)]
enum ProjectColumn { Profile, System }

fn project_columns(ui: &mut Ui, mut render: impl FnMut(&mut Ui, ProjectColumn)) {
    const GAP: f32 = 24.0;
    const SYSTEM_WIDTH: f32 = 280.0;
    let available = ui.available_width();
    let wide = available >= 800.0 + GAP + SYSTEM_WIDTH;
    let profile_width = if wide { available - GAP - SYSTEM_WIDTH } else { available }.min(820.0);
    let item_gap = ui.spacing().item_spacing.x;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        for (column, width) in [(ProjectColumn::Profile, profile_width), (ProjectColumn::System, SYSTEM_WIDTH)] {
            if column == ProjectColumn::System && !wide { break; }
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.x = item_gap;
                render(ui, column);
            });
        }
    });
}

#[cfg(test)]
#[test]
fn system_card_tracks_the_profile_edge_as_the_window_widens() {
    fn measure(width: f32) -> Vec<egui::Rect> {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 720.0))),
            ..Default::default()
        };
        let mut columns = Vec::new();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().frame(Frame::none()).show(ctx, |ui| {
                project_columns(ui, |ui, _| {
                    columns.push(ui.max_rect());
                    ui.label("Content");
                });
            });
        });
        columns
    }
    assert_eq!(measure(1050.0).len(), 1);
    for width in [1104.0, 1200.0, 1800.0] {
        let columns = measure(width);
        assert_eq!(columns.len(), 2);
        let (profile, system) = (columns[0], columns[1]);
        assert_eq!(profile.top(), system.top());
        assert_eq!(system.left() - profile.right(), 24.0);
        assert_eq!(system.width(), 280.0);
        assert!(profile.width() <= 820.0 && system.right() <= width);
    }
}
pub(crate) use save_picker::SavePicker;
mod play_preset_picker;
pub(crate) use play_preset_picker::PlayPresetPicker;

/// The graphics modes: name, Series S profile, render scale, what it does.
const MODES: [(&str, bool, f32, &str); 3] = [
    ("Fast", true, 0.5, "Renders at half resolution and scales up."),
    ("Balanced", true, 0.75, "Renders at three quarters and scales up."),
    ("Best quality", false, 1.0, "Renders at full resolution."),
];
const PROJECT_GRAPHICS_WIDTH: f32 = 310.0;
const GRAPHICS_BAND_HEIGHT: f32 = 40.0;

fn profile_tab_key(repo: &str) -> String {
    format!("project-profile-tab:{repo}")
}

fn project_check_button(ctx: &egui::Context, feedback: Option<ProjectCheckFeedback>, available: bool, now: f64) -> (&'static str, Look) {
    let message = match feedback {
        Some(ProjectCheckFeedback::Current(at)) => Some(("No new update found", at)),
        Some(ProjectCheckFeedback::Failed(at)) => Some(("Update check failed", at)),
        _ => None,
    };
    if let Some((label, at)) = message {
        let age = at.elapsed().as_secs_f32();
        if age < 3.0 {
            let pulse = 0.75 + 0.25 * (age * std::f32::consts::TAU).cos();
            let fade = (3.0 - age).min(1.0);
            ctx.request_repaint_after(Duration::from_millis(40));
            return (label, Look::Signal(theme::BAD, pulse * fade));
        }
    }
    let look = if available {
        ctx.request_repaint_after(Duration::from_millis(40));
        Look::Signal(Color32::from_rgb(255, 219, 95), 0.7 + 0.3 * (now as f32 * 3.0).cos())
    } else {
        Look::Ghost
    };
    ("Check for updates", look)
}

#[cfg(test)]
#[test]
fn project_check_button_fades_back_after_a_manual_result() {
    let ctx = egui::Context::default();
    let (label, look) = project_check_button(&ctx, None, true, 0.0);
    assert_eq!(label, "Check for updates");
    assert!(matches!(look, Look::Signal(color, _) if color != theme::BAD));
    let current = std::time::Instant::now();
    let (label, look) = project_check_button(&ctx, Some(ProjectCheckFeedback::Current(current)), false, 0.0);
    assert_eq!(label, "No new update found");
    assert!(matches!(look, Look::Signal(color, _) if color == theme::BAD));
    let past = current - Duration::from_secs(4);
    let (label, look) = project_check_button(&ctx, Some(ProjectCheckFeedback::Current(past)), false, 4.0);
    assert_eq!(label, "Check for updates");
    assert!(look == Look::Ghost);
    assert_eq!(project_check_button(&ctx, Some(ProjectCheckFeedback::Failed(current)), false, 0.0).0, "Update check failed");
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::regular(13.0)).color(theme::FAINT));
}

fn wrapped(ui: &mut Ui, text: &str, color: Color32) {
    ui.add(Label::new(RichText::new(text).font(theme::regular(14.0)).color(color)).wrap());
}

fn graphics_mode(graphics: GraphicsSettings) -> Option<usize> {
    MODES.iter().position(|(_, series_s, scale, _)| {
        graphics.series_s == *series_s && (!*series_s || (graphics.render_scale - scale).abs() < 0.01)
    })
}

fn graphics_controls(ui: &mut Ui, graphics: &mut GraphicsSettings) {
    ui.horizontal_wrapped(|ui| {
        let names = MODES.map(|mode| mode.0);
        if let Some(i) = theme::segmented(ui, "graphics-mode", &names, graphics_mode(*graphics), None) {
            graphics.series_s = MODES[i].1;
            graphics.render_scale = MODES[i].2;
        }
        let description = graphics_mode(*graphics).map_or_else(
            || format!("Render scale: {:.0}%.", graphics.render_scale * 100.0),
            |i| MODES[i].3.to_owned(),
        );
        note(ui, &description);
    });
    ui.add_space(20.0);
    graphics_options(ui, graphics);
}

fn graphics_options(ui: &mut Ui, graphics: &mut GraphicsSettings) {
    theme::toggle(ui, &mut graphics.fix_8gb, "8 GB graphics card fix", "Smaller textures, and grey in place of missing ones.");
    ui.add_space(14.0);
    theme::toggle(ui, &mut graphics.bright_shadows, "Brighter shadows", "A steady ambient light where ray tracing does not run.");
    ui.add_space(14.0);
    theme::toggle(ui, &mut graphics.fullscreen, "Borderless fullscreen", "The game's window covers its whole monitor, with no frame.");
}

/// A list heading, in line with the text of the rows under it.
fn group(ui: &mut Ui, text: &str, count: Option<usize>) {
    // More air above than below: it heads the rows, not the controls over it.
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        theme::caps(ui, text, 10.5, theme::LILAC);
        if let Some(n) = count {
            theme::caps(ui, &n.to_string(), 10.5, theme::FAINT);
        }
    });
}

/// A settings section: a card with a name, a line on what it is for, and its controls.
fn section(ui: &mut Ui, name: &str, badge: Option<&str>, intro: &str, add: impl FnOnce(&mut Ui)) {
    theme::card(ui, 24.0, |ui| {
        ui.horizontal(|ui| {
            theme::caps(ui, name, 13.0, theme::WHITE);
            if let Some(b) = badge {
                theme::badge(ui, b, theme::WARN);
            }
        });
        if !intro.is_empty() {
            ui.add_space(2.0);
            wrapped(ui, intro, theme::FAINT);
        }
        ui.add_space(14.0);
        add(ui);
    });
}

/// Right-aligned buttons on a row of their own.
fn button_row(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 46.0), Layout::right_to_left(Align::Center), add);
}

fn gpu_text(gpu: &[String]) -> String {
    gpu.join(", ")
}

/// A running job's stage in a word or two, for its button.
fn short_stage(stage: &str) -> &'static str {
    if stage.starts_with("Downloading") {
        "Downloading"
    } else if stage.starts_with("Building") {
        "Building"
    } else if stage == "Playing" {
        "Playing"
    } else if stage.starts_with("Uninstall") {
        "Uninstalling"
    } else if stage.starts_with("Warming") {
        "Warming up"
    } else {
        "Starting"
    }
}

impl App {
    fn screenshot_alias(&self, repo: &str) -> Option<(String, String)> {
        let projects = self.screenshot_projects.as_ref()?;
        let index = projects.iter().position(|project| project == repo).map_or(projects.len() + 1, |i| i + 1);
        Some((format!("Example Project {index:02}"), format!("example/project-{index:02}")))
    }

    /// The whole window, every frame.
    pub(crate) fn draw(&mut self, ctx: &egui::Context) {
        self.tools = tools::current();
        // A drive search that finished.
        let found = {
            let mut f = self.finding.lock().unwrap();
            if matches!(&*f, Some(Some(_))) { f.take().flatten() } else { None }
        };
        match found {
            Some(Ok(dir)) => {
                self.saved.game_dir = dir;
                self.files.clear();
                self.find_error = None;
            }
            Some(Err(e)) => self.find_error = Some(e),
            None => {}
        }
        self.check_files();

        self.top_bar(ctx);
        self.status_bar(ctx);
        match self.view {
            View::Projects => {
                let width = (ctx.screen_rect().width() * 0.28).clamp(300.0, 380.0);
                egui::SidePanel::left("list")
                    .resizable(false)
                    .exact_width(width)
                    .frame(Frame::none().fill(theme::DEEP).inner_margin(Margin { left: 16.0, right: 16.0, top: 18.0, bottom: 0.0 }))
                    .show(ctx, |ui| self.list(ui, ctx));
                let selected = self.selected_project();
                egui::CentralPanel::default().frame(Frame::none().fill(theme::BG)).show(ctx, |ui| self.page(ui, ctx, selected));
            }
            View::Settings => {
                egui::CentralPanel::default().frame(Frame::none().fill(theme::BG)).show(ctx, |ui| self.settings_page(ui, ctx));
            }
        }
        self.auto_check(ctx);
        self.dialogs(ctx);
    }

    /// The sunset strip: the name, the two pages, and adding a repo.
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(56.0)
            .show_separator_line(false)
            .frame(Frame::none().inner_margin(Margin::symmetric(24.0, 0.0)))
            .show(ctx, |ui| {
                theme::strip(ui.painter(), ui.max_rect() + Margin::symmetric(24.0, 0.0));
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("MANDYHUB").font(theme::deco(16.0)).color(theme::PLUM).extra_letter_spacing(3.8));
                    ui.add_space(22.0);
                    if let Some(i) = theme::tabs(ui, "views", &["Projects", "Settings"], self.view as usize) {
                        self.view = if i == 0 { View::Projects } else { View::Settings };
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.screenshot_projects.is_none() && theme::button(ui, Look::Night, Some(glyph::ADD), "Import Repo/Zip").clicked() {
                            self.open_add_repo();
                        }
                        if matches!(self.updater.state, crate::updater::State::Available(_))
                            && theme::button(ui, Look::Night, Some(glyph::DOWNLOAD), "Launcher update").clicked()
                        {
                            self.open_launcher_updates(ctx);
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let running = self.any_job_running();
        let installs = self.project_jobs.0.values().filter(|job| {
            let j = job.lock().unwrap();
            j.running && j.installing
        }).count();
        egui::TopBottomPanel::bottom("status")
            .exact_height(42.0)
            .frame(Frame::none().fill(theme::DEEP).inner_margin(Margin::symmetric(20.0, 0.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let small = |ui: &mut Ui, text: &str, color: Color32| {
                        ui.label(RichText::new(text).font(theme::regular(13.0)).color(color));
                    };
                    if self.files_ok() {
                        theme::dot(ui, theme::GOOD);
                        small(ui, "Game files found", theme::MUTED);
                        let dir = if self.screenshot_projects.is_some() { "Path hidden".into() } else { self.saved.game_dir.clone() };
                        let path_width = (ui.available_width() - 640.0).clamp(60.0, 300.0);
                        ui.allocate_ui(vec2(path_width, 20.0), |ui| {
                            ui.add(Label::new(RichText::new(dir).font(theme::regular(12.5)).color(theme::FAINT)).truncate());
                        });
                    } else {
                        theme::dot(ui, theme::BAD);
                        small(ui, "Game files not found", theme::WHITE);
                        if theme::small(ui, Look::Ghost, Some(glyph::FOLDER), "Choose folder", true).clicked() {
                            self.pick_game_folder();
                        }
                    }
                    theme::divider(ui);
                    match self.tools.clone() {
                        _ if installs > 0 => {
                            ui.add(egui::Spinner::new().size(12.0).color(theme::PINK));
                            let label = if installs == 1 { "1 install/update in progress".into() } else { format!("{installs} installs/updates in progress") };
                            small(ui, &label, theme::MUTED);
                        }
                        None => {
                            ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                            small(ui, "Checking build tools", theme::FAINT);
                        }
                        Some(t) if t.ready() => {
                            theme::dot(ui, theme::GOOD);
                            small(ui, "Build tools ready", theme::MUTED);
                        }
                        Some(t) => {
                            theme::dot(ui, theme::WARN);
                            small(ui, &format!("Missing {}", t.missing().join(", ")), theme::WHITE);
                            if theme::small(ui, Look::Ghost, Some(glyph::DOWNLOAD), "Install tools", !running).clicked() {
                                self.start_tools(ctx);
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut screenshot = self.screenshot_projects.is_some();
                        if ui.checkbox(&mut screenshot, "Screenshot mode").on_hover_text("Replace repo names and hide private text for screenshots").changed() {
                            self.screenshot_projects = screenshot.then(|| self.all_projects().into_iter().map(|project| project.repo).collect());
                        }
                        theme::divider(ui);
                        let label = format!("Default graphics: {}", self.mode_name());
                        if theme::small(ui, Look::Ghost, Some(glyph::SETTINGS), &label, true).clicked() {
                            self.view = View::Settings;
                        }
                        theme::divider(ui);
                        let ready = !matches!(self.updater.state, crate::updater::State::Checking | crate::updater::State::Downloading);
                        if theme::small(ui, Look::Ghost, Some(glyph::SYNC), "Check for update", ready).clicked() {
                            self.updater.check(ctx);
                            self.open_launcher_updates(ctx);
                        }
                    });
                });
            });
    }

    fn list(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        // The controls stop where the rows stop: the 10 px past that is the scroll bar's.
        ui.horizontal(|ui| {
            let w = ui.available_width() - 10.0 - theme::CONTROL_H - ui.spacing().item_spacing.x;
            if self.screenshot_projects.is_some() {
                theme::field(ui, &mut String::new(), "Search hidden", w, Some(glyph::SEARCH));
            } else {
                theme::field(ui, &mut self.search, "Search projects", w, Some(glyph::SEARCH));
            }
            if theme::icon_button(ui, Look::Secondary, Size::Normal, glyph::REFRESH, "Reload the list from PortWatch").clicked() {
                self.refresh_projects(ctx);
            }
        });
        let w = ui.available_width() - 10.0;
        if let Some(i) = theme::segmented(ui, "gpu", &["All", "NVIDIA", "AMD"], Some(self.saved.gpu_filter.min(2)), Some(w)) {
            self.saved.gpu_filter = i;
        }
        ui.horizontal_wrapped(|ui| {
            theme::dot(ui, theme::GOOD);
            note(ui, "New install to try");
            ui.add_space(6.0);
            theme::dot(ui, theme::PINK);
            note(ui, "Update to try");
        });

        let query = if self.screenshot_projects.is_some() { String::new() } else { self.search.trim().to_lowercase() };
        let filter = self.saved.gpu_filter;
        let keep = |p: &Project| {
            let text = query.is_empty() || p.name.to_lowercase().contains(&query) || p.repo.to_lowercase().contains(&query);
            let card = |name: &str| p.custom || p.gpu.iter().any(|g| g.eq_ignore_ascii_case(name));
            text && match filter {
                1 => card("nvidia"),
                2 => card("amd"),
                _ => true,
            }
        };
        let custom: Vec<Project> = self.saved.custom.iter().filter(|p| keep(p)).cloned().collect();
        let listed = self.projects.lock().unwrap().clone();
        let (mut pick, mut retry) = (None, false);
        let out = egui::ScrollArea::vertical().id_salt("list").auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if !custom.is_empty() {
                group(ui, "Your repos", Some(custom.len()));
                for p in &custom {
                    if self.row(ui, p).clicked() {
                        pick = Some(p.repo.clone());
                    }
                }
                ui.add_space(14.0);
            }
            match &listed {
                None => {
                    group(ui, "From PortWatch", None);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                        note(ui, "Loading the list");
                    });
                }
                Some(Err(e)) => {
                    group(ui, "From PortWatch", None);
                    theme::callout(ui, theme::BAD, glyph::WARNING, |ui| {
                        wrapped(ui, if self.screenshot_projects.is_some() { "Project list unavailable." } else { e }, theme::MUTED);
                        if theme::small(ui, Look::Secondary, Some(glyph::REFRESH), "Try again", true).clicked() {
                            retry = true;
                        }
                    });
                }
                Some(Ok(list)) => {
                    let shown: Vec<&Project> = list.iter().filter(|p| keep(p)).collect();
                    group(ui, "From PortWatch", Some(shown.len()));
                    if shown.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            note(ui, "No projects match.");
                        });
                    }
                    for p in shown {
                        if self.row(ui, p).clicked() {
                            pick = Some(p.repo.clone());
                        }
                    }
                }
            }
            ui.add_space(16.0);
        });
        // While more rows are below, the list fades out at the bottom instead of stopping hard.
        let r = out.inner_rect;
        if out.content_size.y - out.state.offset.y - r.height() > 1.0 {
            let fade = egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - 40.0), r.right_bottom());
            theme::fade_bottom(ui.painter(), fade, theme::DEEP);
        }
        if let Some(r) = pick {
            self.saved.selected = Some(r);
        }
        if retry {
            self.refresh_projects(ctx);
        }
    }

    /// One project in the list: its name, frame rate, repo and where it stands.
    fn row(&self, ui: &mut Ui, p: &Project) -> egui::Response {
        let selected = self.saved.selected.as_deref() == Some(p.repo.as_str());
        // A gutter on the right keeps the scroll bar off the rows.
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 10.0, 60.0), Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let ctx = ui.ctx().clone();
        let hovered = resp.hovered();
        let t = ctx.animate_bool_with_time(resp.id.with("hover"), hovered, if hovered { 0.15 } else { 0.12 });
        let sel = ctx.animate_bool_with_time(resp.id.with("sel"), selected, 0.15);
        let painter = ui.painter().clone();
        // One quiet card for the picked row; hover is a lighter step of the same.
        let fill = theme::mix(theme::mix(Color32::TRANSPARENT, theme::SURFACE, t), theme::RAISED, sel);
        painter.rect(rect, theme::R, fill, Stroke::new(1.0, theme::LINE_STRONG.gamma_multiply(sel)));
        let (left, right) = (rect.left() + 14.0, rect.right() - 16.0);

        // Right: the frame rate as a figure, set like the page's stats; a dash when none was measured.
        let fps = match p.fps {
            Some(f) if f >= 0.5 => Some((format!("{f:.0}"), theme::mix(theme::WHITE, theme::PINK, sel))),
            Some(_) => Some(("—".to_string(), theme::FAINT)),
            None => None,
        };
        let fps_w = fps.as_ref().map_or(0.0, |(text, color)| {
            let num = painter.layout_no_wrap(text.clone(), theme::display(24.0), *color);
            let unit = painter.layout_no_wrap("FPS".into(), theme::deco(9.0), theme::FAINT);
            let w = num.size().x.max(unit.size().x);
            let cx = right - w / 2.0;
            painter.galley(egui::pos2(cx - num.size().x / 2.0, rect.top() + 9.0), num, *color);
            painter.galley(egui::pos2(cx - unit.size().x / 2.0, rect.top() + 39.0), unit, theme::FAINT);
            w + 14.0
        });

        // Left: the name, with a tick when PortWatch verified it.
        let tick_w = if p.verified && theme::has_icons() { 18.0 } else { 0.0 };
        let state = self.state_of(&p.repo);
        let pending = state.as_ref().filter(|state| state.installed)
            .and_then(|state| state.activity.as_ref())
            .and_then(|activity| activity.pending_label());
        let dot_w = if pending.is_some() { 14.0 } else { 0.0 };
        let alias = self.screenshot_alias(&p.repo);
        let (display_name, display_repo) = alias.as_ref().map_or((p.name.as_str(), p.repo.as_str()), |(name, repo)| (name.as_str(), repo.as_str()));
        let name = theme::one_line(ui, display_name, theme::strong(14.5), theme::WHITE, right - left - fps_w - tick_w - dot_w);
        let name_size = name.size();
        let y1 = rect.top() + 10.0;
        painter.galley(egui::pos2(left, y1), name, theme::WHITE);
        if tick_w > 0.0 {
            let at = egui::pos2(left + name_size.x + 6.0, y1 + name_size.y / 2.0 + 1.0);
            let tick = painter.text(at, egui::Align2::LEFT_CENTER, glyph::CHECK, theme::glyphs(11.0), theme::GOOD);
            ui.interact(tick.expand(4.0), resp.id.with("tick"), Sense::hover()).on_hover_text("Verified by PortWatch");
        }
        if pending.is_some() {
            let at = egui::pos2(left + name_size.x + tick_w + 8.0, y1 + name_size.y / 2.0 + 1.0);
            let color = if state.as_ref().and_then(|state| state.activity.as_ref()).is_some_and(|activity| activity.new_install) {
                theme::GOOD
            } else {
                theme::PINK
            };
            painter.circle_filled(at, 3.0, color);
        }

        // Under it, one line: where it stands when that matters, else its repo.
        let y2 = rect.top() + 34.0;
        match self.row_status(p) {
            Some((status, color)) => {
                let g = painter.layout_job(egui::text::LayoutJob::single_section(
                    status.to_uppercase(),
                    egui::TextFormat { font_id: theme::deco(10.0), color, extra_letter_spacing: 1.4, ..Default::default() },
                ));
                painter.galley(egui::pos2(left, y2 + 2.0), g, color);
            }
            None => {
                let repo = theme::one_line(ui, display_repo, theme::regular(12.0), theme::FAINT, right - left - fps_w);
                painter.galley(egui::pos2(left, y2), repo, theme::FAINT);
            }
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if let Some(label) = pending { resp.on_hover_text(label) } else { resp }
    }

    /// Where a project stands, when that is more than "listed".
    fn row_status(&self, p: &Project) -> Option<(String, Color32)> {
        if let Some(job) = self.project_jobs.get(&p.repo) {
            let j = job.lock().unwrap();
            if j.running {
                return Some((short_stage(&j.stage).to_string(), theme::PINK));
            }
        }
        if self.update_for(p).is_some() {
            return Some(("Update ready".into(), theme::WARN));
        }
        if self.state_of(&p.repo).is_some_and(|state| state.installed) {
            return Some(("Installed".into(), theme::GOOD));
        }
        None
    }

    fn empty_page(&self, ui: &mut Ui) {
        theme::ground(ui.painter(), ui.max_rect());
        let h = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.add_space((h * 0.3).max(40.0));
            let (mark, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            theme::mark(ui.painter(), mark.center(), 22.0);
            ui.add_space(14.0);
            let w = ui.available_width();
            theme::title(ui, "Pick a project", 40.0, w);
            note(ui, "Choose one from the list to see how it runs, then install it.");
        });
    }

    /// The selected project's page.
    fn page(&mut self, ui: &mut Ui, ctx: &egui::Context, selected: Option<Project>) {
        let Some(p) = selected else {
            self.shown = None;
            return self.empty_page(ui);
        };
        let now = ctx.input(|i| i.time);
        let just_selected = self.shown.as_deref() != Some(p.repo.as_str());
        let found_on_request = self.repos.lock().unwrap().get_mut(&p.repo).is_some_and(|state| {
            if state.feedback != Some(ProjectCheckFeedback::Available) { return false; }
            state.feedback = None;
            true
        });
        if found_on_request {
            // An explicit check makes a previously deferred update actionable again.
            self.saved.later.retain(|(repo, _)| repo != &p.repo);
        }
        if (just_selected || found_on_request) && self.update_for(&p).is_some() {
            ctx.data_mut(|data| data.insert_temp(egui::Id::new(profile_tab_key(&p.repo)), 0_usize));
        }
        if just_selected {
            self.log_export_status = None;
            self.shown = Some(p.repo.clone());
            self.shown_at = now;
        }
        // The page comes in once when picked: it fades up 10 px into place.
        let enter = theme::ease_out(((now - self.shown_at) / 0.26) as f32);
        if enter < 1.0 {
            ctx.request_repaint();
        }
        let playing = self.project_jobs.0.values().any(|job| {
            let j = job.lock().unwrap();
            j.running && j.stage == "Playing"
        });
        theme::ground(ui.painter(), ui.max_rect());
        egui::ScrollArea::vertical().id_salt("page").auto_shrink([false; 2]).show(ui, |ui| {
            let pictures = self.screenshot_projects.is_none() && self.slides.ready(ctx);
            if pictures {
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 200.0), Sense::hover());
                self.slides.paint_in(ui.painter(), rect, now as f32, theme::BG);
                // Still while a game runs: the graphics card is the game's.
                if !playing {
                    ctx.request_repaint_after(Duration::from_millis(40));
                }
            }
            let top = if pictures { 0.0 } else { 40.0 };
            Frame::none().inner_margin(Margin { left: 44.0, right: 44.0, top, bottom: 44.0 }).show(ui, |ui| {
                ui.set_opacity(enter);
                ui.add_space(10.0 * (1.0 - enter));
                project_columns(ui, |ui, column| {
                    if column == ProjectColumn::System {
                        self.hardware.show(ui);
                    } else {
                        self.page_header(ui, ctx, &p);
                        self.profile_tabs(ui, ctx, &p);
                        ui.add_space(36.0);
                        if self.screenshot_projects.is_none() { self.activity(ui, &p.repo); }
                    }
                });
            });
        });
    }

    /// Where it comes from, its name, the main action and the repo link buttons.
    fn page_header(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let (eyebrow, eyebrow_color) = if p.archive_exe.is_some() {
            ("Imported ZIP", theme::LILAC)
        } else if p.custom {
            ("Added by link", theme::WARN)
        } else if p.verified {
            ("Verified by PortWatch", theme::LILAC)
        } else {
            ("From PortWatch", theme::LILAC)
        };
        let folder_error = ui.make_persistent_id(("open-profile-folder", &p.repo));
        ui.horizontal(|ui| {
            theme::caps(ui, eyebrow, 11.5, eyebrow_color);
            if self.screenshot_projects.is_none() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let folder = p.local_exe.as_deref()
                        .and_then(|exe| std::path::Path::new(exe).parent())
                        .map_or_else(|| crate::repo_dir(&p.repo), std::path::Path::to_path_buf);
                    let available = folder.is_dir();
                    let tooltip = if available {
                        folder.display().to_string()
                    } else {
                        "Install the project to open its folder.".into()
                    };
                    if theme::small(ui, Look::Secondary, Some(glyph::FOLDER), "Open profile folder", available)
                        .on_hover_text(tooltip)
                        .clicked()
                    {
                        let error = std::process::Command::new("explorer").arg(&folder).spawn().err();
                        ctx.data_mut(|data| data.insert_temp(folder_error, error.map(|error| error.to_string())));
                    }
                });
            }
        });
        if self.screenshot_projects.is_none() {
            if let Some(error) = ctx.data(|data| data.get_temp::<Option<String>>(folder_error)).flatten() {
                wrapped(ui, &format!("Could not open the profile folder: {error}"), theme::BAD);
            }
        }
        let width = ui.available_width();
        let alias = self.screenshot_alias(&p.repo);
        let private = alias.is_some();
        let (display_name, display_repo) = alias.as_ref().map_or((p.name.as_str(), p.repo.as_str()), |(name, repo)| (name.as_str(), repo.as_str()));
        theme::title(ui, display_name, 72.0, width);
        if p.archive_exe.is_none() {
            let display_repo = if display_repo.contains("://") { display_repo.to_owned() } else { format!("github.com/{display_repo}") };
            ui.add(Label::new(RichText::new(display_repo).font(theme::regular(13.5)).color(theme::MUTED)).truncate());
        }
        ui.add_space(20.0);

        let st = match self.state_of(&p.repo) {
            Some(st) => st,
            None => {
                self.check_repo(ctx, p, false);
                self.state_of(&p.repo).unwrap_or_default()
            }
        };
        // While the first look runs, what is on disk already says installed or not.
        let installed = st.installed || (st.checking.is_some() && (
            p.archive_program().is_some_and(|exe| exe.is_file())
            || (p.is_git() && project_installed(&p.repo))
        ));
        let blocker = self.blocking_job(&p.repo, !installed);
        let running = blocker.is_some();
        let (job_repo, stage) = blocker.map(|job| {
            let j = job.lock().unwrap();
            (j.repo.clone(), j.stage.clone())
        }).unwrap_or_default();
        let this_job = self.project_jobs.get(&p.repo).is_some_and(|j| j.lock().unwrap().running);
        let install_blocked = self.blocking_job(&p.repo, true).is_some();
        let rust = installed.then(|| rust_install(&p.repo)).flatten();
        let victory = !p.runs_standalone();
        let runnable = !victory && (p.archive_exe.is_some() || rust.is_some_and(|install| install.executable.is_some()));
        let tools = self.tools.clone();
        let tools_ok = tools.as_ref().is_some_and(|t| t.ready());
        let url = p.web_url();
        let browser_link = url.starts_with("https://") || url.starts_with("http://");
        let now = ctx.input(|i| i.time);

        const GRAPHICS_GAP: f32 = 26.0;
        const ACTION_BUTTON_HEIGHT: f32 = 46.0;
        let graphics_below = ui.available_width() < 800.0;
        let actions = |app: &mut Self, ui: &mut Ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (icon, label) = if this_job {
                (None, short_stage(&stage))
            } else if installed {
                if victory || runnable { (Some(glyph::PLAY), "Play") } else { (Some(glyph::CHECK), "Built") }
            } else {
                (Some(glyph::DOWNLOAD), "Install project")
            };
            let enabled = !running && if installed { runnable || (victory && app.files_ok()) } else { p.is_git() && tools_ok };
            if theme::button_ex(ui, Look::Primary, Size::Large, icon, label, enabled, this_job).clicked() {
                if installed {
                    app.play_preset_picker = (!private).then(|| {
                        let launch = app.saved.project_launch.get(&project_key(&p.repo));
                        PlayPresetPicker::for_project(p, &app.saved.launch_presets, launch)
                    }).flatten();
                    if app.play_preset_picker.is_none() {
                        app.continue_play(ctx, p.clone());
                    }
                } else {
                    app.start(ctx, p.clone(), Kind::Install);
                }
            }
            if browser_link && theme::button_ex(ui, Look::Secondary, Size::Large, Some(glyph::OPEN), "Open repository", !private, false)
                .on_hover_text(if private { "Repo link hidden" } else { &url })
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab(&url));
            }
            // The copy button shows a tick for a moment after it copied.
            let copied = now - app.copied_at < 1.6;
            if copied {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            let (icon, tip) = if copied { (glyph::CHECK, "Link copied") } else { (glyph::COPY, "Copy link") };
            if p.is_git() && ui.add_enabled_ui(!private, |ui| theme::icon_button(ui, Look::Secondary, Size::Large, icon, tip)).inner.clicked() {
                ctx.copy_text(url.clone());
                app.copied_at = now;
            }
        };
        if !victory {
            ui.horizontal_wrapped(|ui| actions(self, ui));
        } else if graphics_below {
            ui.horizontal_wrapped(|ui| actions(self, ui));
            ui.add_space(14.0);
            ui.push_id(&p.repo, |ui| self.project_graphics_menu(ui, &p.repo));
        } else {
            let width = ui.available_width();
            ui.allocate_ui_with_layout(vec2(width, ACTION_BUTTON_HEIGHT), Layout::left_to_right(Align::Min), |ui| {
                ui.vertical(|ui| {
                    let offset = GRAPHICS_BAND_HEIGHT + ui.spacing().item_spacing.y
                        + (theme::CONTROL_H - ACTION_BUTTON_HEIGHT) / 2.0;
                    ui.add_space(offset);
                    ui.horizontal(|ui| actions(self, ui));
                });
                ui.add_space(GRAPHICS_GAP);
                ui.push_id(&p.repo, |ui| self.project_graphics_menu(ui, &p.repo));
            });
        }

        ui.add_space(8.0);
        if installed {
            if let Some(activity) = &st.activity {
                ui.horizontal_wrapped(|ui| {
                    let updated = if activity.new_install {
                        activity.updated_ago().replacen("Updated", "Installed", 1)
                    } else {
                        activity.updated_ago()
                    };
                    note(ui, &updated);
                    if activity.pending_label().is_some() {
                        note(ui, "· Not run yet");
                    }
                    #[cfg(windows)]
                    if let Some(last_run) = activity.last_run.and_then(crate::build_activity::local_time) {
                        note(ui, &format!("· Last run {last_run}"));
                    }
                });
            }
        }
        let other = job_repo.as_ref().and_then(|r| self.all_projects().into_iter().find(|q| &q.repo == r)).map(|q| q.name);
        ui.horizontal_wrapped(|ui| {
            if this_job {
                note(ui, if private { short_stage(&stage) } else { &stage });
            } else if running && stage == "Playing" {
                note(ui, "Close the running game first.");
            } else if running {
                note(ui, &format!("Waiting for {} to finish.", if private { "the current job" } else { other.as_deref().unwrap_or("the current job") }));
            } else if installed && !victory {
                note(ui, if runnable { "Starts the installed Rust program." } else { "This Rust workspace has no runnable binary." });
            } else if installed && self.files_ok() && self.saved.warm_before_play {
                note(ui, "Warms up the shaders first, then starts your installed build with your own game files.");
            } else if installed && self.files_ok() {
                note(ui, "Starts your installed build with your own game files.");
            } else if installed {
                note(ui, "Choose the folder with your game files to play.");
                if theme::small(ui, Look::Ghost, Some(glyph::FOLDER), "Choose folder", true).clicked() {
                    self.pick_game_folder();
                }
            } else if tools.is_none() {
                note(ui, "Checking for Git, Rust and the C++ build tools.");
            } else if tools_ok {
                note(ui, "Downloads and builds it on this PC once. The first build takes several minutes.");
            } else {
                let missing = tools.as_ref().map(|t| t.missing().join(", ")).unwrap_or_default();
                note(ui, &format!("Installing needs {missing}."));
                if theme::small(ui, Look::Ghost, Some(glyph::DOWNLOAD), "Install tools", true).clicked() {
                    self.start_tools(ctx);
                }
            }
        });

        if installed {
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                if st.checking.is_some() {
                    ui.add(egui::Spinner::new().size(12.0).color(theme::FAINT));
                    note(ui, "Checking for updates");
                } else {
                    theme::dot(ui, theme::GOOD);
                    let v = st.built.as_deref().map(|c| &c[..c.len().min(7)]).unwrap_or("unknown");
                let label = if private || p.archive_exe.is_some() { "Installed".into() } else { format!("Installed, version {v}") };
                    ui.label(RichText::new(label).font(theme::regular(13.0)).color(theme::MUTED));
                }
                if p.local_exe.is_none() {
                    ui.add_space(6.0);
                    if p.is_git() {
                        let (label, look) = project_check_button(ctx, st.feedback, self.update_for(p).is_some(), now);
                        if theme::small(ui, look, Some(glyph::SYNC), label, st.checking.is_none() && !this_job).clicked() {
                            self.check_repo(ctx, p, true);
                        }
                    }
                    if victory && theme::small(ui, Look::Ghost, Some(glyph::WARM), "Warm up shaders", !running && self.files_ok())
                        .on_hover_text("Translate the game's shaders into this project's cache now, so it does not stop to translate them while you play.").clicked() {
                        self.start(ctx, p.clone(), Kind::Warm);
                    }
                    if p.is_git() && theme::small(ui, Look::Ghost, Some(glyph::REPAIR), "Reinstall", !install_blocked).clicked() {
                        self.start(ctx, p.clone(), Kind::Install);
                    }
                    if theme::small(ui, Look::Danger, Some(glyph::DELETE), "Uninstall", !running).clicked() {
                        self.confirm_uninstall = Some(p.repo.clone());
                    }
                }
            });
        }
    }

    /// Update, the figures from PortWatch, and what the project is.
    fn page_body(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let running = self.blocking_job(&p.repo, true).is_some();
        if let Some(remote) = self.update_for(p) {
            ui.add_space(18.0);
            theme::callout(ui, theme::WARN, glyph::SYNC, |ui| {
                let short = &remote[..remote.len().min(7)];
                let label = if self.screenshot_projects.is_some() { "Update available.".into() } else { format!("A new version is out: {short}.") };
                wrapped(ui, &label, theme::WHITE);
                ui.horizontal(|ui| {
                    if theme::small(ui, Look::Secondary, Some(glyph::DOWNLOAD), "Update now", !running).clicked() {
                        self.start(ctx, p.clone(), Kind::Install);
                    }
                    if theme::small(ui, Look::Ghost, None, "Later", true).clicked() {
                        self.saved.later.retain(|(r, _)| r != &p.repo);
                        self.saved.later.push((p.repo.clone(), remote.clone()));
                    }
                });
            });
        }

        if p.archive_exe.is_some() {
            return;
        }
        if p.custom {
            ui.add_space(18.0);
            let mut remove = false;
            theme::callout(ui, theme::WARN, glyph::WARNING, |ui| {
                wrapped(
                    ui,
                    "You added this repo by link. Mandyhub downloads, builds and runs its code on your PC, so keep only repos from people you trust.",
                    theme::BLUSH,
                );
                if theme::small(ui, Look::Danger, Some(glyph::DELETE), "Remove from list", true).clicked() {
                    remove = true;
                }
            });
            if remove {
                self.saved.custom.retain(|q| q.repo != p.repo);
                self.saved.selected = None;
                return;
            }
        }

        let tracker = self.projects.lock().unwrap();
        let p = if p.custom {
            let report = tracker.as_ref().and_then(|result| result.as_ref().ok())
                .and_then(|list| list.iter().find(|q| q.repo.eq_ignore_ascii_case(&p.repo)
                    && (p.branch.is_none() || q.branch == p.branch)));
            let Some(report) = report else { return; };
            report
        } else { p };
        ui.add_space(30.0);
        let gpu = gpu_text(&p.gpu);
        theme::stats(
            ui,
            &[
                Stat {
                    label: "Frame rate",
                    value: p.fps.map_or("Not reported".into(), |f| format!("{f:.0} fps")),
                    color: theme::WHITE,
                    tip: Some("The average in game on the author's PC."),
                },
                Stat {
                    label: "Rendering",
                    value: p.render.map_or("Not reported".into(), |r| format!("{:.0}%", r * 100.0)),
                    color: theme::WHITE,
                    tip: None,
                },
                Stat {
                    label: "Made for",
                    value: if gpu.is_empty() { "Not stated".into() } else { gpu },
                    color: theme::WHITE,
                    tip: None,
                },
                Stat {
                    label: "PortWatch",
                    value: if p.verified { "Verified" } else { "Not verified" }.into(),
                    color: if p.verified { theme::GOOD } else { theme::MUTED },
                    tip: None,
                },
            ],
        );
        if !p.gpu.is_empty() && p.gpu.iter().all(|g| !g.eq_ignore_ascii_case("nvidia")) {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                theme::icon(ui, glyph::WARNING, 13.0, theme::WARN);
                ui.label(RichText::new("Made for AMD cards: it may not start on NVIDIA.").font(theme::regular(13.0)).color(theme::MUTED));
            });
        }
        if !p.summary.trim().is_empty() {
            ui.add_space(32.0);
            theme::caps(ui, "About", 11.5, theme::LILAC);
            ui.add_space(2.0);
            // Lines of a readable length, whatever the window's width.
            ui.scope(|ui| {
                ui.set_max_width(680.0);
                let summary = if self.screenshot_projects.is_some() { "Project description hidden." } else { &p.summary };
                ui.add(Label::new(RichText::new(summary).font(theme::regular(15.0)).color(theme::BLUSH)).wrap());
            });
        }
    }

    fn profile_tabs(&mut self, ui: &mut Ui, ctx: &egui::Context, project: &Project) {
        ui.add_space(20.0);
        let tab_key = profile_tab_key(&project.repo);
        let id = egui::Id::new(&tab_key);
        let mut tab = ctx.data(|data| data.get_temp::<usize>(id)).unwrap_or_default();
        if let Some(selected) = theme::segmented(ui, &tab_key, &["Overview", "Profile notes", "Launch settings"], Some(tab), None) {
            tab = selected;
            ctx.data_mut(|data| data.insert_temp(id, selected));
        }
        match tab {
            1 | 2 if self.screenshot_projects.is_some() => note(ui, "Hidden in Screenshot mode."),
            1 => {
                ui.add_space(12.0);
                self.project_notes(ui, project);
            }
            2 => {
                ui.add_space(12.0);
                self.project_launch_card(ui, project);
            }
            _ => self.page_body(ui, ctx, project),
        }
        let key = project_key(&project.repo);
        if self.saved.project_launch.get(&key).is_some_and(|launch| {
            launch.args.is_none() && launch.switches.is_none() && launch.graphics.is_none()
                && launch.notes.is_empty() && !launch.has_confirmed_preset() && launch.reviewed_script_revision.is_none()
        }) {
            self.saved.project_launch.remove(&key);
        }
    }

    fn project_graphics_menu(&mut self, ui: &mut Ui, repo: &str) {
        let default_graphics = self.saved.default_graphics();
        let key = project_key(repo);
        let launch = self.saved.project_launch.entry(key).or_default();
        ui.vertical_centered(|ui| {
            ui.set_width(PROJECT_GRAPHICS_WIDTH);
            // Equal space above and below the presets keeps their center aligned
            // with the neighboring Play button.
            let band_layout = Layout::top_down(Align::Center).with_main_align(Align::Center);
            ui.allocate_ui_with_layout(vec2(PROJECT_GRAPHICS_WIDTH, GRAPHICS_BAND_HEIGHT), band_layout, |ui| {
                ui.set_min_height(GRAPHICS_BAND_HEIGHT);
                ui.spacing_mut().item_spacing.y = 2.0;
                theme::caps(ui, "Project graphics", 10.5, theme::LILAC);
                note(ui, if launch.graphics.is_some() { "Overrides global" } else { "Using global" });
            });
            let mut graphics = launch.graphics.unwrap_or(default_graphics);
            let names = MODES.map(|mode| mode.0);
            if let Some(index) = theme::segmented(ui, "project-graphics-preset", &names, graphics_mode(graphics), None) {
                graphics.series_s = MODES[index].1;
                graphics.render_scale = MODES[index].2;
                launch.graphics = Some(graphics);
            }
            ui.allocate_ui_with_layout(vec2(PROJECT_GRAPHICS_WIDTH, GRAPHICS_BAND_HEIGHT), band_layout, |ui| {
                ui.set_min_height(GRAPHICS_BAND_HEIGHT);
                let label = if graphics.output == OutputResolution::Default {
                    "Other graphics settings".into()
                } else {
                    format!("Other graphics settings · {}", graphics.output.label())
                };
                ui.menu_button(label, |ui| {
                    ui.set_width(340.0);
                    let before = graphics;
                    egui::ComboBox::from_label("Console profile")
                        .selected_text(if graphics.series_s { "Series S" } else { "Series X" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut graphics.series_s, false, "Series X");
                            ui.selectable_value(&mut graphics.series_s, true, "Series S");
                        });
                    egui::ComboBox::from_label("Output resolution")
                        .selected_text(graphics.output.label())
                        .show_ui(ui, |ui| {
                            for output in OutputResolution::OPTIONS {
                                ui.selectable_value(&mut graphics.output, output, output.label());
                            }
                        });
                    note(ui, "Output resolution requires support in the selected build.");
                    ui.add_space(8.0);
                    graphics_options(ui, &mut graphics);
                    if graphics != before {
                        launch.graphics = Some(graphics);
                    }
                    if launch.graphics.is_some() && ui.button("Use global settings").clicked() {
                        launch.graphics = None;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn project_launch_card(&mut self, ui: &mut Ui, project: &Project) {
        let revision = self.state_of(&project.repo).and_then(|s| s.built);
        let can_rescan = self.blocking_job(&project.repo, true).is_none() && crate::project_script_root(project).is_dir();
        let global_args = &self.saved.launch_args;
        let global_switches = &self.saved.extra_env;
        let key = project_key(&project.repo);
        let launch = self.saved.project_launch.entry(key.clone()).or_default();
        let review = ui.push_id(key, |ui| {
            launch_presets::show(ui, &mut self.saved.launch_presets, launch, global_args, global_switches, project,
                revision.as_deref(), can_rescan)
        }).inner;
        if review.is_some() { self.play_preset_picker = review; }
    }

    fn project_notes(&mut self, ui: &mut Ui, project: &Project) {
        let launch = self.saved.project_launch.entry(project_key(&project.repo)).or_default();
        section(ui, "Profile notes", None, "Saved locally for this project.", |ui| {
            ui.add(egui::TextEdit::multiline(&mut launch.notes)
                .hint_text("Record performance, glitches, or settings to try…")
                .desired_rows(7)
                .desired_width((ui.available_width() - 28.0).max(0.0))
                .margin(Margin::symmetric(14.0, 10.0)));
        });
    }

    /// What the launcher is doing, and its log.
    fn activity(&mut self, ui: &mut Ui, repo: &str) {
        let job = if self.job.lock().unwrap().running { self.job.clone() }
            else { self.project_jobs.get(repo).unwrap_or(&self.job).clone() };
        let (running, failed, stage, has_log, stoppable, warm_status) = {
            let j = job.lock().unwrap();
            (j.running, j.failed, j.stage.clone(), !j.log.is_empty(), j.running && j.stoppable, j.warm_status.clone())
        };
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
            theme::caps(ui, "Activity", 11.5, theme::LILAC);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if has_log {
                    let label = if self.show_log { "Hide log" } else { "Show log" };
                    if theme::small(ui, Look::Ghost, None, label, true).clicked() {
                        self.show_log = !self.show_log;
                    }
                }
                if stoppable && theme::small(ui, Look::Danger, Some(glyph::STOP), "Stop", true).clicked() {
                    Self::stop_job(&job);
                }
            });
        });
        ui.horizontal(|ui| {
            if running && stage != "Playing" {
                ui.add(egui::Spinner::new().size(12.0).color(theme::PINK));
            } else {
                theme::dot(ui, if failed { theme::BAD } else if stage.is_empty() { theme::LINE_STRONG } else { theme::GOOD });
            }
            let (text, color) = if stage.is_empty() {
                ("Nothing running".to_string(), theme::FAINT)
            } else if failed {
                (stage, theme::BAD)
            } else if running {
                (stage, theme::WHITE)
            } else {
                (stage, theme::MUTED)
            };
            ui.label(RichText::new(text).font(theme::regular(13.5)).color(color));
            if let Some(status) = warm_status {
                note(ui, &status);
            }
        });
        if has_log && self.show_log {
            ui.add_space(6.0);
            let log = job.lock().unwrap().log.preview();
            ui.horizontal_wrapped(|ui| {
                let label = if self.log_expanded { "Collapse" } else { "Expand" };
                if theme::small(ui, Look::Ghost, None, label, true).clicked() {
                    self.log_expanded = !self.log_expanded;
                }
                if theme::small(ui, Look::Ghost, Some(glyph::COPY), "Copy log", true).clicked() {
                    ui.ctx().copy_text(log.clone());
                    self.log_export_status = Some(("Visible log copied.".into(), false));
                }
                if theme::small(ui, Look::Ghost, None, "Export log…", true).clicked() {
                    if let Some(path) = rfd::FileDialog::new().set_title("Export Activity log")
                        .set_file_name("mandyhub.log").save_file()
                    {
                        self.log_export_status = Some(match job.lock().unwrap().log.export(&path) {
                            Ok(()) => ("Log exported.".into(), false),
                            Err(error) => (format!("Could not export the log: {error}"), true),
                        });
                    }
                }
            });
            if let Some((message, error)) = &self.log_export_status {
                wrapped(ui, message, if *error { theme::BAD } else { theme::MUTED });
            }
            let height = if self.log_expanded { (ui.ctx().screen_rect().height() - 190.0).max(320.0) } else { 240.0 };
            Frame::none()
                .fill(theme::DEEP)
                .stroke(Stroke::new(1.0, theme::LINE))
                .rounding(theme::R)
                .inner_margin(Margin::same(14.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("log").max_height(height).stick_to_bottom(true).auto_shrink([false, true]).show(
                        ui,
                        |ui| {
                            let mut text = log.as_str();
                            ui.add(egui::TextEdit::multiline(&mut text)
                                .id_salt("activity-log-text")
                                .font(theme::mono(12.0))
                                .desired_width(f32::INFINITY)
                                .desired_rows(1)
                                .frame(false));
                        },
                    );
                });
        }
    }

    fn settings_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        theme::ground(ui.painter(), ui.max_rect());
        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false; 2]).show(ui, |ui| {
            let avail = ui.available_width();
            let col = (avail - 64.0).min(720.0);
            let side = ((avail - col) / 2.0).max(32.0);
            Frame::none().inner_margin(Margin { left: side, right: side, top: 40.0, bottom: 48.0 }).show(ui, |ui| {
                ui.set_width(col);
                theme::caps(ui, "Mandyhub", 11.5, theme::LILAC);
                theme::title(ui, "Settings", 64.0, col);
                ui.add_space(18.0);
                if self.screenshot_projects.is_some() {
                    note(ui, "Settings are hidden in Screenshot mode.");
                    return;
                }
                self.game_files_card(ui, ctx);
                ui.add_space(16.0);
                self.graphics_card(ui);
                ui.add_space(16.0);
                self.launch_card(ui);
                ui.add_space(16.0);
                self.install_folder_card(ui);
                ui.add_space(16.0);
                self.pictures_card(ui);
                ui.add_space(16.0);
                self.switches_card(ui);
                ui.add_space(16.0);
                self.about_card(ui, ctx);
            });
        });
    }

    fn game_files_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let searching = matches!(&*self.finding.lock().unwrap(), Some(None));
        section(
            ui,
            "Game files",
            None,
            "The folder with game_scarlett_master.exe from your own copy of the game. Mandyhub never downloads game files.",
            |ui| {
                ui.allocate_ui_with_layout(vec2(ui.available_width(), theme::CONTROL_H), Layout::right_to_left(Align::Center), |ui| {
                    let label = if searching { "Searching drives" } else { "Find game files" };
                    if theme::button_ex(ui, Look::Ghost, Size::Normal, Some(glyph::SEARCH), label, !searching, searching).clicked() {
                        self.find_error = None;
                        *self.finding.lock().unwrap() = Some(None);
                        let (slot, ctx) = (self.finding.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let r = search_drives();
                            *slot.lock().unwrap() = Some(Some(r));
                            ctx.request_repaint();
                        });
                    }
                    if theme::button(ui, Look::Secondary, Some(glyph::FOLDER), "Choose folder").clicked() {
                        self.pick_game_folder();
                    }
                    let w = ui.available_width();
                    if theme::field(ui, &mut self.saved.game_dir, "Paste the folder's path", w, None).changed() {
                        self.files.clear();
                    }
                });
                ui.add_space(10.0);
                self.files_status(ui);
                if let Some(e) = &self.find_error {
                    ui.add_space(4.0);
                    wrapped(ui, e, theme::BAD);
                }
            },
        );
    }

    fn files_status(&self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.saved.game_dir.trim().is_empty() {
                note(ui, "No folder chosen yet.");
            } else if self.files_ok() {
                theme::icon(ui, glyph::CHECK, 13.0, theme::GOOD);
                ui.label(RichText::new("All game files are here.").font(theme::regular(13.0)).color(theme::GOOD));
            } else if self.empty {
                theme::icon(ui, glyph::WARNING, 13.0, theme::BAD);
                ui.label(RichText::new("This folder is empty.").font(theme::regular(13.0)).color(theme::BAD));
            } else {
                theme::icon(ui, glyph::WARNING, 13.0, theme::BAD);
                ui.label(RichText::new("Missing").font(theme::regular(13.0)).color(theme::BAD));
                for (f, ok) in &self.files {
                    if !ok {
                        ui.label(RichText::new(*f).font(theme::mono(12.0)).color(theme::BLUSH));
                    }
                }
            }
        });
    }

    fn graphics_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Graphics",
            Some("Beta"),
            "These options may not work with every project or on older PCs. If a game will not start or looks wrong, choose Best quality and turn the fixes off.",
            |ui| {
                let mut graphics = self.saved.default_graphics();
                graphics_controls(ui, &mut graphics);
                self.saved.series_s = graphics.series_s;
                self.saved.render_scale = graphics.render_scale;
                self.saved.fix_8gb = graphics.fix_8gb;
                self.saved.bright_shadows = graphics.bright_shadows;
                self.saved.fullscreen = graphics.fullscreen;
            },
        );
    }

    fn launch_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Shaders and launch",
            None,
            "VIctory can translate the game's shaders into its cache before you play, so the game does not stop to translate them on the way. Each project keeps its own cache.",
            |ui| {
                theme::toggle(
                    ui,
                    &mut self.saved.warm_before_play,
                    "Warm up shaders before playing",
                    "Checks the shader cache before Play. Scarlett reuses a completed pass for the same build and settings. Activity shows the result, including any shaders that could not translate.",
                );
                ui.add_space(14.0);
                theme::toggle(
                    ui,
                    &mut self.saved.warm_pipelines,
                    "Also build the GPU pipelines",
                    "Slow: about 20 minutes on 16 threads, hours after a translator update. They belong to this graphics card and driver.",
                );
                ui.add_space(20.0);
                ui.label(RichText::new("Default launch arguments").font(theme::strong(14.0)).color(theme::WHITE));
                note(ui, "Added to victory run, for example --strict. Arguments for the game itself go after --.");
                ui.add_space(6.0);
                let w = ui.available_width();
                theme::field(ui, &mut self.saved.launch_args, "--strict -- -title-argument", w, None);
            },
        );
    }

    fn install_folder_card(&mut self, ui: &mut Ui) {
        let running = self.any_job_running();
        section(
            ui,
            "Install folder",
            None,
            "Projects are downloaded and built here. Each one takes several GB, so a big drive is best.",
            |ui| {
                ui.add(Label::new(RichText::new(workspace().display().to_string()).font(theme::mono(12.5)).color(theme::BLUSH)).truncate());
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if theme::button_ex(ui, Look::Secondary, Size::Normal, Some(glyph::FOLDER), "Change folder", !running, false).clicked() {
                        if let Some(d) = rfd::FileDialog::new().set_title("Where projects are downloaded and built").pick_folder() {
                            self.choose_install_dir(d.join("Mandyhub"));
                        }
                    }
                    if !self.saved.install_dir.is_empty()
                        && theme::button_ex(ui, Look::Ghost, Size::Normal, None, "Use the default folder", !running, false).clicked()
                    {
                        self.choose_install_dir(default_workspace());
                    }
                });
            },
        );
    }

    fn pictures_card(&mut self, ui: &mut Ui) {
        section(
            ui,
            "Background pictures",
            None,
            "Put .jpg or .png pictures in this folder and they show across the top of each project's page. Mandyhub reads them when it starts.",
            |ui| {
                let dir = slides::folder();
                ui.add(Label::new(RichText::new(dir.display().to_string()).font(theme::mono(12.5)).color(theme::BLUSH)).truncate());
                ui.add_space(10.0);
                if theme::button(ui, Look::Secondary, Some(glyph::FOLDER), "Open the folder").clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                }
            },
        );
    }

    fn switches_card(&mut self, ui: &mut Ui) {
        section(ui, "Default launch switches", None, "Used by projects that inherit these switches. Enter one VICTORY_NAME=value per line.", |ui| {
            let w = ui.available_width();
            ui.add(
                egui::TextEdit::multiline(&mut self.saved.extra_env)
                    .font(theme::mono(12.5))
                    .text_color(theme::WHITE)
                    .hint_text(RichText::new("VICTORY_NAME=value").color(theme::FAINT))
                    .desired_rows(3)
                    .desired_width(w - 28.0)
                    .margin(Margin::symmetric(14.0, 10.0)),
            );
        });
    }

    fn about_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.credits_card(ui);
        ui.add_space(14.0);
        use crate::updater::{State, RELEASES};
        let busy = self.any_job_running();
        let card_start = ui.next_widget_position();
        section(ui, "Launcher updates", None, concat!("Mandyhub ", env!("CARGO_PKG_VERSION")), |ui| {
            match &self.updater.state {
                State::Idle => {}
                State::Checking => note(ui, "Checking GitHub for updates…"),
                State::Current => note(ui, "You’re on the latest release."),
                State::Available(release) => note(ui, &format!("{} is available.", release.tag_name)),
                State::Downloading => note(ui, "Downloading and verifying the update…"),
                State::Downloaded(update) => {
                    if let Some(error) = &self.updater.restart_error {
                        wrapped(ui, error, theme::BAD);
                    } else {
                        wrapped(ui, "Verified update ready.", theme::BLUSH);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if theme::button_ex(ui, Look::Primary, Size::Normal, Some(glyph::SYNC), "Restart to apply", !busy, false).clicked() {
                            self.updater.restart_requested = true;
                        }
                        if theme::button(ui, Look::Secondary, Some(glyph::FOLDER), "Open download folder").clicked() {
                            if let Some(folder) = update.path.parent() {
                                let _ = std::process::Command::new("explorer").arg(folder).spawn();
                            }
                        }
                    });
                }
                State::Failed(error) => wrapped(ui, error, theme::BAD),
            }
            ui.horizontal_wrapped(|ui| {
                match self.updater.state {
                    State::Available(_) => {
                        if theme::button_ex(ui, Look::Primary, Size::Normal, Some(glyph::DOWNLOAD), "Update and restart", !busy, false).clicked() {
                            self.updater.download(ctx);
                        }
                    }
                    _ => {}
                }
                let ready = !matches!(self.updater.state, State::Checking | State::Downloading);
                if theme::button_ex(ui, Look::Secondary, Size::Normal, Some(glyph::SYNC), "Check for update", ready, false).clicked() {
                    self.updater.check(ctx);
                }
                if theme::button(ui, Look::Ghost, Some(glyph::OPEN), "Release notes").clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(RELEASES));
                }
            });
        });
        let scroll = ctx.data_mut(|data| data.remove_temp::<bool>(egui::Id::new("scroll-to-launcher-updates")));
        if scroll.unwrap_or(false) {
            ui.scroll_to_rect(egui::Rect::from_two_pos(card_start, ui.next_widget_position()), Some(Align::BOTTOM));
        }
    }

    fn open_launcher_updates(&mut self, ctx: &egui::Context) {
        self.view = View::Settings;
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("scroll-to-launcher-updates"), true));
    }

    fn credits_card(&self, ui: &mut Ui) {
        section(ui, "About Mandyhub", None, "", |ui| {
            wrapped(
                ui,
                "Mandyhub by Zettrox. The project list comes from PortWatch. The projects are their authors' work, built on VIctory by Marvisak and its contributors.",
                theme::BLUSH,
            );
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("ellion369", "https://github.com/ellion369");
                note(ui, "Launcher redesign");
            });
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("TheGameOnAddict", "https://github.com/TheGameOnAddict");
                note(ui, "Save picker, checkpoints, project settings, notes, build tracking, and compatibility improvements");
            });
            wrapped(ui, "No game files are included or downloaded: you need your own copy of the game.", theme::FAINT);
            wrapped(ui, "Display type: Josefin Sans and Big Shoulders Display, under the SIL Open Font License.", theme::FAINT);
            ui.add_space(10.0);
            if theme::button(ui, Look::Secondary, Some(glyph::OPEN), "Open portwatch-vi.xyz").clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab("https://portwatch-vi.xyz"));
            }
        });
    }

    fn continue_play(&mut self, ctx: &egui::Context, project: Project) {
        if project.runs_standalone() {
            self.start(ctx, project, Kind::Play(crate::saves::LaunchSave::Continue));
        } else {
            self.save_picker = Some(SavePicker::new(project));
        }
    }

    /// The open dialog, if any: add a repo, uninstall, or move the install folder.
    fn dialogs(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let (running, import_error) = {
            let job = self.job.lock().unwrap();
            (job.running, job.import_error.clone())
        };
        let open = self.add_repo.is_some() || self.confirm_uninstall.is_some() || self.pending_move.is_some()
            || self.save_picker.is_some() || self.play_preset_picker.is_some() || import_error.is_some();
        if open && !self.modal_was_open {
            self.modal_at = now;
        }
        self.modal_was_open = open;
        let t0 = self.modal_at;

        if let Some(mut picker) = self.save_picker.take() {
            let alias = self.screenshot_alias(&picker.project.repo).map(|(name, _)| name);
            let ready = self.blocking_job(&picker.project.repo, false).is_none() && self.files_ok();
            let (action, dismissed) = picker.show(ctx, t0, ready, alias.as_deref());
            match action {
                save_picker::Action::Launch(choice) if !dismissed => {
                    self.start(ctx, picker.project, Kind::Play(choice));
                }
                save_picker::Action::Keep if !dismissed => self.save_picker = Some(picker),
                _ => {}
            }
        }

        if let Some(mut picker) = self.play_preset_picker.take() {
            let (accepted, dismissed) = picker.show(ctx, t0);
            if !dismissed {
                if accepted && self.blocking_job(&picker.project.repo, picker.settings_only).is_none() {
                    let settings_only = picker.settings_only;
                    let revision = picker.revision.clone();
                    let (project, preset) = picker.into_choice();
                    let key = project_key(&project.repo);
                    let launch = self.saved.project_launch.entry(key.clone()).or_default();
                    if let Some(preset) = preset {
                        if let Err(error) = launch_presets::apply_to_profile(&mut self.saved.launch_presets, launch, &key, preset) {
                            self.project_jobs.get(&project.repo).unwrap_or(&self.job).lock().unwrap().log.push(format!("Could not apply launch preset: {error}"));
                            self.show_log = true;
                            return;
                        }
                    } else {
                        // Keep the exact switches already in the profile, even if the old script changed or disappeared.
                        if self.saved.launch_presets.iter().any(|p| p.repo_script.is_some() && Some(&p.name) == launch.selected_preset.as_ref()) {
                            launch.selected_preset = None;
                        }
                        launch.confirmed_repo_script = None;
                    }
                    launch.preset_choice_confirmed = true;
                    launch.reviewed_script_revision = revision;
                    if !settings_only { self.continue_play(ctx, project); }
                    ctx.request_repaint();
                } else {
                    self.play_preset_picker = Some(picker);
                }
            }
        }

        if let Some(mut add) = self.add_repo.take() {
            let mut keep = true;
            let (_, dismissed) = theme::modal(ctx, "add-repo", t0, 460.0, |ui| {
                theme::caps(ui, "Custom repo", 11.5, theme::LILAC);
                theme::title(ui, "Import Repo/Zip", 40.0, 460.0);
                note(ui, "Add a Rust project by Git URL or import a prebuilt ZIP.");
                ui.add_space(14.0);
                let w = ui.available_width();
                let field = theme::field(ui, &mut add.link, "https://git.example.com/owner/project.git", w, Some(glyph::LINK));
                if add.focus {
                    field.request_focus();
                    add.focus = false;
                }
                if field.changed() {
                    add.error = None;
                }
                let mut submit = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(e) = &add.error {
                    ui.add_space(2.0);
                    wrapped(ui, e, theme::BAD);
                }
                ui.add_space(14.0);
                theme::callout(ui, theme::WARN, glyph::WARNING, |ui| {
                    wrapped(
                        ui,
                        "Mandyhub downloads, builds and runs this repo's code on your PC. Add only repos from people you trust: fake repos with malware exist.",
                        theme::BLUSH,
                    );
                });
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::Primary, Some(glyph::ADD), "Import repo link").clicked() {
                        submit = true;
                    }
                    if theme::small(ui, Look::Secondary, Some(glyph::FOLDER), "Import build ZIP…", !running).clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Build ZIP", &["zip"]).pick_file() {
                            self.import_build_zip(ctx, path);
                            keep = false;
                        }
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        keep = false;
                    }
                });
                if submit {
                    match parse_git_repo(&add.link) {
                        Some(repo) => {
                            self.add_custom(repo);
                            keep = false;
                        }
                        None => add.error = Some("Enter an HTTPS, HTTP, SSH, or Git clone URL without embedded credentials.".into()),
                    }
                }
            });
            if keep && !dismissed {
                self.add_repo = Some(add);
            }
        }

        if let Some(error) = import_error {
            let (close, dismissed) = theme::modal(ctx, "zip-import-error", t0, 440.0, |ui| {
                theme::title(ui, "Build import failed", 34.0, 440.0);
                wrapped(ui, &error, theme::BAD);
                theme::button(ui, Look::Primary, None, "Close").clicked()
            });
            if close || dismissed {
                self.job.lock().unwrap().import_error = None;
            }
        }

        if let Some(repo) = self.confirm_uninstall.clone() {
            let project = self.all_projects().into_iter().find(|p| p.repo == repo);
            let name = self.screenshot_alias(&repo).map(|(name, _)| name)
                .unwrap_or_else(|| project.as_ref().map_or_else(|| repo.clone(), |p| p.name.clone()));
            let mut close = false;
            let (_, dismissed) = theme::modal(ctx, "uninstall", t0, 440.0, |ui| {
                theme::caps(ui, "Uninstall", 11.5, theme::BAD);
                theme::title(ui, &name, 40.0, 440.0);
                wrapped(ui, "This deletes its download, its build and its shader cache. Your saves stay, and you can install it again later.", theme::BLUSH);
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::DangerSolid, Some(glyph::DELETE), "Uninstall").clicked() {
                        if let Some(p) = project.clone() {
                            self.start(ctx, p, Kind::Uninstall);
                        }
                        close = true;
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close || dismissed {
                self.confirm_uninstall = None;
            }
        }

        if let Some((old, new)) = self.pending_move.clone() {
            let ready = !self.any_job_running();
            let mut close = false;
            let (_, dismissed) = theme::modal(ctx, "move", t0, 520.0, |ui| {
                theme::caps(ui, "Install folder", 11.5, theme::LILAC);
                theme::title(ui, "Move installed projects?", 40.0, 520.0);
                wrapped(ui, "Your installed projects and their saves are in the old folder. Moving them to another drive can take a while.", theme::BLUSH);
                ui.add_space(10.0);
                ui.add(Label::new(RichText::new(format!("From  {}", old.display())).font(theme::mono(12.0)).color(theme::FAINT)).truncate());
                ui.add(Label::new(RichText::new(format!("To    {}", new.display())).font(theme::mono(12.0)).color(theme::WHITE)).truncate());
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::small(ui, Look::Primary, None, "Move them", ready).clicked() {
                        self.apply_install_dir(&new);
                        self.start_move(ctx, old.clone(), new.clone());
                        close = true;
                    }
                    if theme::small(ui, Look::Secondary, None, "Don't move them", ready).clicked() {
                        self.apply_install_dir(&new);
                        close = true;
                    }
                    if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            if close || dismissed {
                self.pending_move = None;
            }
        }
    }

    /// Ends the running warm-up (its process and the workers it started).
    fn stop_job(job: &crate::JobHandle) {
        let pid = {
            let mut j = job.lock().unwrap();
            j.stopped = true;
            j.pid
        };
        if let Some(pid) = pid {
            let mut c = std::process::Command::new("taskkill");
            c.args(["/PID", &pid.to_string(), "/T", "/F"]);
            crate::hide_window(&mut c);
            let _ = c.spawn();
        }
    }

    fn open_add_repo(&mut self) {
        self.add_repo = Some(AddRepo { link: String::new(), error: None, focus: true });
    }

    /// Puts a repo added by link at the top of the list and opens its page.
    fn add_custom(&mut self, repo: String) {
        let name = repo.split('/').next_back().unwrap_or(&repo).trim_end_matches(".git").to_string();
        self.saved.custom.retain(|p| p.repo != repo);
        self.saved.custom.insert(
            0,
            Project {
                name,
                repo: repo.clone(),
                branch: None,
                gpu: Vec::new(),
                fps: None,
                render: None,
                verified: false,
                summary: String::new(),
                custom: true,
                local_exe: None,
                archive_exe: None,
            },
        );
        self.saved.selected = Some(repo);
        self.view = View::Projects;
    }

    fn pick_game_folder(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_title("The folder with your game files").pick_folder() {
            self.saved.game_dir = d.display().to_string();
            self.files.clear();
        }
    }

    fn mode_name(&self) -> String {
        match graphics_mode(self.saved.default_graphics()) {
            Some(i) => MODES[i].0.to_string(),
            None => format!("{:.0}%", self.saved.render_scale * 100.0),
        }
    }
}
