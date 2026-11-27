//! What Mandyhub shows: the sunset strip with its tabs, the project list, a
//! project's page, the settings page, the status bar and the dialogs. The
//! work they start (install, play, update checks, moving folders) lives in
//! main.rs.

use crate::theme::{self, glyph, Look, Size, Stat};
use crate::{default_workspace, parse_git_repo, project_installed, project_key, rust_install, search_drives, slides, tools, workspace};
use crate::{AddRepo, App, GraphicsSettings, Kind, OutputResolution, Project, ProjectCheckFeedback, View};
use eframe::egui::{self, vec2, Align, Color32, Frame, Label, Layout, Margin, RichText, Sense, Stroke, Ui};
use std::time::Duration;

fn fps_filter(ui: &mut Ui, include_exact_60: &mut bool) {
    let mut exclude = !*include_exact_60;
    if ui.checkbox(&mut exclude, "Exclude exactly 60 FPS")
        .on_hover_text("Applies to all project averages and Library sorting. Uses readings collected since this filter was added.")
        .changed() { *include_exact_60 = !exclude; }
}

fn profile_stat_card(ui: &mut Ui, width: f32, label: &str, value: &str, options: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 48.0), if options { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    painter.add(theme::glass_surface(rect, 11.0, theme::raised()));
    painter.rect_stroke(rect, 11.0, Stroke::new(1.0_f32, if options && response.hovered() { theme::pink() } else { theme::line() }));
    let left = rect.left() + 11.0;
    painter.text(egui::pos2(left, rect.top() + 7.0), egui::Align2::LEFT_TOP, label, theme::strong(9.5), theme::muted());
    painter.text(egui::pos2(left, rect.bottom() - 5.0), egui::Align2::LEFT_BOTTOM, value, theme::strong(17.0), theme::white());
    if options {
        let tip = egui::pos2(rect.right() - 14.0, rect.top() + 16.0);
        painter.add(egui::Shape::line(vec![tip - vec2(3.5, 3.5), tip, tip + vec2(3.5, -3.5)], Stroke::new(1.5_f32, theme::muted())));
        response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("FPS filter and reset")
    } else { response }
}

#[cfg(test)]
pub(crate) use studio::tests::click_library_build;
mod studio;
pub(crate) use studio::{Appearance, Studio, MIDNIGHT};
mod save_picker;
mod launch_presets;

pub(crate) use save_picker::SavePicker;
mod play_preset_picker;
pub(crate) use play_preset_picker::PlayPresetPicker;

/// The graphics modes: name, Series S profile, render scale, what it does.
const MODES: [(&str, bool, f32, &str); 3] = [
    ("Fast", true, 0.5, "Renders at half resolution and scales up."),
    ("Balanced", true, 0.75, "Renders at three quarters and scales up."),
    ("Best quality", false, 1.0, "Renders at full resolution."),
];

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
            return (label, Look::Signal(theme::bad(), pulse * fade));
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
    assert!(matches!(look, Look::Signal(color, _) if color != theme::bad()));
    let current = std::time::Instant::now();
    let (label, look) = project_check_button(&ctx, Some(ProjectCheckFeedback::Current(current)), false, 0.0);
    assert_eq!(label, "No new update found");
    assert!(matches!(look, Look::Signal(color, _) if color == theme::bad()));
    let past = current - Duration::from_secs(4);
    let (label, look) = project_check_button(&ctx, Some(ProjectCheckFeedback::Current(past)), false, 4.0);
    assert_eq!(label, "Check for updates");
    assert!(look == Look::Ghost);
    assert_eq!(project_check_button(&ctx, Some(ProjectCheckFeedback::Failed(current)), false, 0.0).0, "Update check failed");
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::regular(13.0)).color(theme::faint()));
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
    ui.horizontal_wrapped(|ui|{
        ui.label("Console profile");ui.selectable_value(&mut graphics.series_s,true,"Series S");ui.selectable_value(&mut graphics.series_s,false,"Series X");
    });
    ui.horizontal_wrapped(|ui|{
        ui.label("Output resolution");
        for resolution in OutputResolution::OPTIONS{ui.selectable_value(&mut graphics.output,resolution,resolution.label());}
    });
    note(ui,"Output resolution requires support in the selected build.");
    ui.add_space(12.0);
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
fn section(ui: &mut Ui, name: &str, badge: Option<&str>, intro: &str, add: impl FnOnce(&mut Ui)) {
    theme::card(ui, 24.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name).font(theme::strong(20.0)).color(theme::white()));
            if let Some(b) = badge {
                theme::badge(ui, b, theme::warn());
            }
        });
        if !intro.is_empty() {
            ui.add_space(2.0);
            wrapped(ui, intro, theme::faint());
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

        self.draw_workspace(ctx);
        self.auto_check(ctx);
        self.dialogs(ctx);
    }


    /// Where a project stands, when that is more than "listed".
    fn row_status(&self, p: &Project) -> Option<(String, Color32)> {
        if let Some(job) = self.project_jobs.get(&p.repo) {
            let j = job.lock().unwrap();
            if j.running {
                return Some((short_stage(&j.stage).to_string(), theme::pink()));
            }
        }
        if self.update_for(p).is_some() {
            return Some(("Update ready".into(), theme::warn()));
        }
        if self.state_of(&p.repo).is_some_and(|state| state.installed) {
            return Some(("Installed".into(), theme::good()));
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
        egui::ScrollArea::vertical().id_salt(("profile", &p.repo)).auto_shrink([false; 2]).show(ui, |ui| {
            ui.set_opacity(enter);
            let background = ui.painter().add(egui::Shape::Noop);
            let header = Frame::none().inner_margin(20.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.page_header(ui, ctx, &p);
                self.update_offer(ui, ctx, &p);
            });
            // The tabs share the header's surface, so omit the normal gap between widgets.
            ui.add_space(-ui.spacing().item_spacing.y);
            let nav = Frame::none().inner_margin(Margin::symmetric(9.0, 4.0))
                .show(ui, |ui| self.profile_tabs(ui, ctx, &p));
            ui.painter().set(background, theme::profile_glass(header.response.rect, nav.response.rect));
            ui.add_space(10.0);
            self.profile_tab_content(ui, ctx, &p, nav.inner);
        });
    }

    /// Where it comes from, its name, the main action and the repo link buttons.
    fn page_header(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let private = self.screenshot_projects.is_some();
        let name=self.display_name(p);
        let source=if p.archive_exe.is_some(){"ZIP build"}else if p.local_exe.is_some(){"Local program"}else if p.verified{"Verified by PortWatch"}else{"Git repository"};

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
        let stage=blocker.map(|job|job.lock().unwrap().stage.clone()).unwrap_or_default();
        let this_job = self.project_jobs.get(&p.repo).is_some_and(|j| j.lock().unwrap().running);
        let rust = installed.then(|| rust_install(&p.repo)).flatten();
        let victory = !p.runs_standalone();
        let runnable = !victory && (p.archive_exe.is_some() || rust.is_some_and(|install| install.executable.is_some()));
        let tools = self.tools.clone();
        let tools_ok = tools.as_ref().is_some_and(|t| t.ready());
        let url = p.web_url();
        let browser_link = url.starts_with("https://") || url.starts_with("http://");
        let now = ctx.input(|i| i.time);

        let width = ui.available_width();
        let heading = if width < 695.0 { 30.0 } else { 46.0 };
        let controls_width = 314.0_f32.min(width);
        let title_width = (width - controls_width - ui.spacing().item_spacing.x).max(0.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(title_width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(title_width);
                theme::caps(ui,source,10.5,theme::muted());
                theme::title(ui,&name,heading,ui.available_width());
                ui.horizontal_wrapped(|ui|{
                    let (label,color)=self.project_status(p);
                    theme::dot(ui,color);note(ui,if installed&&label=="Available"{"Installed"}else{&label});
                    if let Some(a)=&st.activity{
                        let update=if a.new_install{a.updated_ago().replacen("Updated","Installed",1)}else{a.updated_ago()};
                        ui.label(RichText::new(update).small().color(theme::muted()));
                        if let Some(when)=a.last_run.and_then(crate::build_activity::local_time){ui.label(RichText::new(format!("Last run {when}")).small().color(theme::muted()));}
                    }
                });
            });
            ui.allocate_ui_with_layout(vec2(controls_width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(controls_width);
                ui.allocate_ui_with_layout(vec2(controls_width, 0.0), Layout::right_to_left(Align::Center).with_main_wrap(true), |ui| {
                    let label=if this_job{short_stage(&stage)}else if installed{if victory||runnable{"Play project"}else{"Built"}}else{"Install project"};
                    let enabled=!running && if installed{runnable||(victory&&self.files_ok())}else{p.is_git()&&tools_ok};
                    if theme::button_ex(ui,Look::Primary,Size::Normal,Some(if installed{glyph::PLAY}else{glyph::DOWNLOAD}),label,enabled,this_job).clicked(){
                        if installed{self.request_play(ctx,p);}else{self.start(ctx,p.clone(),Kind::Install);}
                    }
                    if p.is_git()&&installed{
                        let (label,look)=project_check_button(ctx,st.feedback,self.update_for(p).is_some(),now);
                        let look = if look == Look::Ghost { Look::Secondary } else { look };
                        if theme::button_ex(ui,look,Size::Normal,Some(glyph::SYNC),label,st.checking.is_none()&&!this_job,false).clicked(){self.check_repo(ctx,p,true);}
                    }
                });
                if !private {
                    ui.allocate_ui_with_layout(vec2(controls_width, 28.0), Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let folder = p.local_exe.as_deref().and_then(|e| std::path::Path::new(e).parent())
                            .map_or_else(|| crate::repo_dir(&p.repo), std::path::Path::to_path_buf);
                        let folder_button = theme::button_ex(ui, Look::Ghost, Size::Small, Some(glyph::FOLDER),
                            if theme::has_icons() { "" } else { "Folder" }, folder.is_dir(), false);
                        if folder_button.on_hover_text(format!("Open project folder\n{}", folder.display())).clicked() {
                            if let Err(e) = std::process::Command::new("explorer").arg(&folder).spawn() {
                                self.log_export_status = Some((e.to_string(), true));
                            }
                        }
                        if p.is_git() {
                            if browser_link && theme::icon_button(ui, Look::Ghost, Size::Small, glyph::OPEN, "Open repository").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(&url));
                            }
                            let copy_id = ui.make_persistent_id(("copied-repo", &p.repo));
                            let copied = ctx.data(|d| d.get_temp::<f64>(copy_id)).is_some_and(|at| now-at < 1.6);
                            let icon = if copied { glyph::CHECK } else { glyph::COPY };
                            if theme::icon_button(ui, Look::Ghost, Size::Small, icon, if copied { "Copied" } else { "Copy repo link" }).clicked() {
                                ctx.copy_text(url.clone());
                                ctx.data_mut(|d| d.insert_temp(copy_id, now));
                                ctx.request_repaint_after(Duration::from_secs(2));
                            }
                            if studio::widgets::repo_link(ui, &url, browser_link).clicked() && browser_link {
                                ctx.open_url(egui::OpenUrl::new_tab(&url));
                            }
                        }
                    });
                }
                self.profile_stat_cards(ui, p);
            });
        });
        if installed&&victory&&!self.files_ok(){if ui.button("Choose game files to play").clicked(){self.pick_game_folder();}}
        else if !installed&&!tools_ok{self.build_tools_status(ui,ctx);}
        else if running&&!this_job{note(ui,"Waiting for the current job to finish.");}
        else if installed&&!victory&&!runnable{note(ui,"This Rust workspace has no runnable binary.");}
    }

    fn profile_stat_cards(&mut self, ui: &mut Ui, p: &Project) {
        let key = project_key(&p.repo);
        let stats = self.saved.play_stats.profile(&key, self.saved.include_exact_60_fps);
        let avg = stats.as_ref().and_then(|s| s.average_fps);
        let fps = avg.map_or_else(|| "—".into(), |v| format!("{v:.1} FPS"));
        let playtime = stats.as_ref().map_or("0m", |s| s.playtime.as_str());
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let card_width = (ui.available_width() - gap) / 2.0;
            let card = profile_stat_card(ui, card_width, "AVERAGE FPS", &fps, true);
            let popup = ui.make_persistent_id(("fps-options", &key));
            if card.clicked() { ui.memory_mut(|m| m.toggle_popup(popup)); }
            egui::popup_below_widget(ui, popup, &card, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                fps_filter(ui, &mut self.saved.include_exact_60_fps);
                ui.separator();
                if ui.button("Reset FPS average").clicked() {
                    self.saved.play_stats.reset_fps(&key);
                    ui.memory_mut(|m| m.close_popup());
                }
            });
            profile_stat_card(ui, card_width, "PLAYTIME", playtime, false);
        });
        if let Some(stats) = stats.as_ref().filter(|s| s.playing) {
            let live = stats.latest_fps.map_or_else(|| "Waiting for FPS".into(), |f| format!("Live {f:.1} FPS"));
            note(ui, &live);
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
    }

    fn request_play(&mut self,ctx:&egui::Context,p:&Project){
        self.play_preset_picker=self.screenshot_projects.is_none().then(||{
            let launch=self.saved.project_launch.get(&project_key(&p.repo));
            PlayPresetPicker::for_project(p,&self.saved.launch_presets,launch)
        }).flatten();
        if self.play_preset_picker.is_none(){self.continue_play(ctx,p.clone());}
    }

    fn build_tools_status(&mut self,ui:&mut Ui,ctx:&egui::Context){
        ui.horizontal_wrapped(|ui|match self.tools.clone(){
            None=>{ui.spinner();note(ui,"Checking build tools");}
            Some(t) if t.ready()=>{theme::dot(ui,theme::good());note(ui,"Build tools ready");}
            Some(t)=>{note(ui,&format!("Missing {}",t.missing().join(", ")));if theme::small(ui,Look::Secondary,Some(glyph::DOWNLOAD),"Install tools",!self.any_job_running()).clicked(){self.start_tools(ctx);}}
        });
    }

    /// Update, the figures from PortWatch, and what the project is.
    fn update_offer(&mut self, ui: &mut Ui, ctx: &egui::Context, p: &Project) {
        let running = self.blocking_job(&p.repo, true).is_some();
        if let Some(remote) = self.update_for(p) {
            ui.add_space(8.0);
            theme::callout(ui, theme::warn(), glyph::SYNC, |ui| {
                let short = &remote[..remote.len().min(7)];
                let label = if self.screenshot_projects.is_some() { "Update available.".into() } else { format!("A new version is out: {short}.") };
                wrapped(ui, &label, theme::white());
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

    }

    fn installation(&mut self,ui:&mut Ui,ctx:&egui::Context,p:&Project){
        let installed=self.state_of(&p.repo).is_some_and(|s|s.installed);
        let running=self.blocking_job(&p.repo,false).is_some();
        let install_blocked=self.blocking_job(&p.repo,true).is_some();
        if installed && p.local_exe.is_none(){ui.horizontal_wrapped(|ui|{
            if !p.runs_standalone() && theme::small(ui,Look::Ghost,Some(glyph::WARM),"Warm up shaders",!running&&self.files_ok()).clicked(){self.start(ctx,p.clone(),Kind::Warm);}
            if p.is_git() && theme::small(ui,Look::Secondary,Some(glyph::REPAIR),"Reinstall",!install_blocked).clicked(){self.start(ctx,p.clone(),Kind::Install);}
            if theme::small(ui,Look::Danger,Some(glyph::DELETE),"Uninstall",!running).clicked(){self.confirm_uninstall=Some(p.repo.clone());}
        });}
        if p.archive_exe.is_some(){return;}
        if p.custom {
            ui.add_space(18.0);
            let mut remove = false;
            theme::callout(ui, theme::warn(), glyph::WARNING, |ui| {
                wrapped(
                    ui,
                    "You added this repo by link. PalmVI downloads, builds and runs its code on your PC, so keep only repos from people you trust.",
                    theme::blush(),
                );
                if theme::small(ui, Look::Danger, Some(glyph::DELETE), "Remove from list", true).clicked() {
                    remove = true;
                }
            });
            if remove {
                self.saved.custom.retain(|q| q.repo != p.repo);
                self.saved.pinned.retain(|r|r!=&p.repo);
                self.view=View::Library;
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
                    color: theme::white(),
                    tip: Some("The average in game on the author's PC."),
                },
                Stat {
                    label: "Rendering",
                    value: p.render.map_or("Not reported".into(), |r| format!("{:.0}%", r * 100.0)),
                    color: theme::white(),
                    tip: None,
                },
                Stat {
                    label: "Made for",
                    value: if gpu.is_empty() { "Not stated".into() } else { gpu },
                    color: theme::white(),
                    tip: None,
                },
                Stat {
                    label: "PortWatch",
                    value: if p.verified { "Verified" } else { "Not verified" }.into(),
                    color: if p.verified { theme::good() } else { theme::muted() },
                    tip: None,
                },
            ],
        );
        if !p.gpu.is_empty() && p.gpu.iter().all(|g| !g.eq_ignore_ascii_case("nvidia")) {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                theme::icon(ui, glyph::WARNING, 13.0, theme::warn());
                ui.label(RichText::new("Made for AMD cards: it may not start on NVIDIA.").font(theme::regular(13.0)).color(theme::muted()));
            });
        }
        if !p.summary.trim().is_empty() {
            ui.add_space(32.0);
            theme::caps(ui, "About", 11.5, theme::lilac());
            ui.add_space(2.0);
            // Lines of a readable length, whatever the window's width.
            ui.scope(|ui| {
                ui.set_max_width(680.0);
                let summary = if self.screenshot_projects.is_some() { "Project description hidden." } else { &p.summary };
                ui.add(Label::new(RichText::new(summary).font(theme::regular(15.0)).color(theme::blush())).wrap());
            });
        }
    }

    fn profile_tabs(&mut self, ui: &mut Ui, ctx: &egui::Context, project: &Project) -> usize {
        let id=egui::Id::new(profile_tab_key(&project.repo));
        let mut tab=ctx.data(|d|d.get_temp::<usize>(id)).unwrap_or(0);
        let tabs=["Overview","Graphics","Launch settings","Live logs","Notes","Installation"].iter().enumerate()
            .filter(|(i,_)|*i!=1||!project.runs_standalone()).map(|(i,s)|(i,s.to_string())).collect::<Vec<_>>();
        if let Some(next)=studio::widgets::tab_row(ui,&tabs,tab){tab=next;self.switch_tab(ctx,&project.repo,tab);}
        tab
    }

    fn profile_tab_content(&mut self, ui: &mut Ui, ctx: &egui::Context, project: &Project, tab: usize) {
        match tab {
            1|2|3|4 if self.screenshot_projects.is_some()=>note(ui,"Hidden in sharing mode."),
            1=>theme::card(ui,18.0,|ui|self.project_graphics_menu(ui,&project.repo)),
            2=>self.project_launch_card(ui,project),
            3=>theme::card(ui,18.0,|ui|self.activity(ui,&project.repo)),
            4=>self.project_notes(ui,project),
            5=>theme::card(ui,18.0,|ui|self.installation(ui,ctx,project)),
            _=>{
                self.quick_cards(ui,project);
                if self.screenshot_projects.is_none(){ui.add_space(8.0);theme::card(ui,16.0,|ui|self.activity(ui,&project.repo));}
            }
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
        ui.heading("Graphics preset");
        note(ui,if launch.graphics.is_some(){"Overrides global settings"}else{"Using global settings"});
        let mut graphics=launch.graphics.unwrap_or(default_graphics);
        let before=graphics;
        graphics_controls(ui,&mut graphics);
        if before!=graphics{launch.graphics=Some(graphics);}
        if launch.graphics.is_some() && ui.button("Use global settings").clicked(){launch.graphics=None;}
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
            let (r,_)=ui.allocate_exact_size(vec2(18.0,18.0),Sense::hover());studio::widgets::pulse(ui.painter(),r.center(),theme::lilac());
            ui.label(RichText::new("Live logs").font(theme::strong(18.0)).color(theme::white()));
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
                ui.add(egui::Spinner::new().size(12.0).color(theme::pink()));
            } else {
                theme::dot(ui, if failed { theme::bad() } else if stage.is_empty() { theme::line_strong() } else { theme::good() });
            }
            let (text, color) = if stage.is_empty() {
                ("Nothing running".to_string(), theme::faint())
            } else if failed {
                (stage, theme::bad())
            } else if running {
                (stage, theme::white())
            } else {
                (stage, theme::muted())
            };
            ui.label(RichText::new(text).font(theme::regular(13.5)).color(color));
            if let Some(status) = warm_status {
                note(ui, &status);
            }
        });
        if !has_log || !self.show_log {
            theme::glass_frame(ui, 7.0, 14.0, theme::bg(), |ui|{
                ui.set_min_size(vec2(ui.available_width(),if self.log_expanded{(ui.ctx().screen_rect().height()-370.0).max(280.0)}else{118.0}));
                ui.label(RichText::new(if has_log{"Log hidden"}else{"Output will appear here when the project runs."}).font(theme::mono(12.0)).color(theme::muted()));
            });
        }
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
                wrapped(ui, message, if *error { theme::bad() } else { theme::muted() });
            }
            let height = if self.log_expanded { (ui.ctx().screen_rect().height() - 190.0).max(320.0) } else { 240.0 };
            theme::glass_frame(ui, theme::R, 14.0, theme::deep(), |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("log").max_height(height).stick_to_bottom(true).auto_shrink([false, true]).show(
                        ui,
                        |ui| {
                            ui.set_min_height(height.min(ui.ctx().screen_rect().height()*0.55));
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
        if self.view!=View::Activity && ui.button("Previous session logs").clicked(){self.view=View::Activity;}
    }

    fn settings_page(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        self.settings_workspace(ui,ctx);
    }

    fn game_files_card(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let searching = matches!(&*self.finding.lock().unwrap(), Some(None));
        section(
            ui,
            "Game files",
            None,
            "The folder with game_scarlett_master.exe from your own copy of the game. PalmVI never downloads game files.",
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
                    wrapped(ui, e, theme::bad());
                }
            },
        );
    }

    fn files_status(&self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if self.saved.game_dir.trim().is_empty() {
                note(ui, "No folder chosen yet.");
            } else if self.files_ok() {
                theme::icon(ui, glyph::CHECK, 13.0, theme::good());
                ui.label(RichText::new("All game files are here.").font(theme::regular(13.0)).color(theme::good()));
            } else if self.empty {
                theme::icon(ui, glyph::WARNING, 13.0, theme::bad());
                ui.label(RichText::new("This folder is empty.").font(theme::regular(13.0)).color(theme::bad()));
            } else {
                theme::icon(ui, glyph::WARNING, 13.0, theme::bad());
                ui.label(RichText::new("Missing").font(theme::regular(13.0)).color(theme::bad()));
                for (f, ok) in &self.files {
                    if !ok {
                        ui.label(RichText::new(*f).font(theme::mono(12.0)).color(theme::blush()));
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
                self.saved.output=graphics.output;
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
                ui.label(RichText::new("Default launch arguments").font(theme::strong(14.0)).color(theme::white()));
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
                ui.add(Label::new(RichText::new(workspace().display().to_string()).font(theme::mono(12.5)).color(theme::blush())).truncate());
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
            "Select My slideshow to cycle through JPG and PNG files from this folder. Restart after adding pictures.",
            |ui| {
                let dir = slides::folder();
                ui.add(Label::new(RichText::new(dir.display().to_string()).font(theme::mono(12.5)).color(theme::blush())).truncate());
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
                    .text_color(theme::white())
                    .hint_text(RichText::new("VICTORY_NAME=value").color(theme::faint()))
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
        section(ui, "Launcher updates", None, concat!("PalmVI ", env!("CARGO_PKG_VERSION")), |ui| {
            match &self.updater.state {
                State::Idle => {}
                State::Checking => note(ui, "Checking GitHub for updates…"),
                State::Current => note(ui, "You’re on the latest release."),
                State::Available(release) => note(ui, &format!("{} is available.", release.tag_name)),
                State::Downloading => note(ui, "Downloading and verifying the update…"),
                State::Downloaded(update) => {
                    if let Some(error) = &self.updater.restart_error {
                        wrapped(ui, error, theme::bad());
                    } else {
                        wrapped(ui, "Verified update ready.", theme::blush());
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
                State::Failed(error) => wrapped(ui, error, theme::bad()),
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
        self.studio.settings_tab = 4;
        self.view = View::Settings;
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("scroll-to-launcher-updates"), true));
    }

    fn credits_card(&self, ui: &mut Ui) {
        section(ui, "PalmVI credits", None, "", |ui| {
            wrapped(
                ui,
                "PalmVI grew out of Mandyhub by Zettrox. The project list comes from PortWatch. The projects are their authors' work, built on VIctory by Marvisak and its contributors.",
                theme::blush(),
            );
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("ellion369", "https://github.com/ellion369");
                note(ui, "Launcher redesign");
            });
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("TheGameOnAddict", "https://github.com/TheGameOnAddict");
                note(ui, "Save picker, checkpoints, project settings, notes, build tracking, and compatibility improvements");
            });
            wrapped(ui, "No game files are included or downloaded: you need your own copy of the game.", theme::faint());
            wrapped(ui, "Display type: Josefin Sans and Big Shoulders Display, under the SIL Open Font License.", theme::faint());
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
            self.prepare_save_picker(project,false);
        }
    }

    fn prepare_save_picker(&mut self,project:Project,settings_only:bool){
        let mut picker=SavePicker::new(project);
        if let Some((_,choice))=self.saved.save_choices.get(&project_key(&picker.project.repo)){picker.select(choice);}
        picker.settings_only=settings_only;self.save_picker=Some(picker);
    }
    fn open_save_settings(&mut self,project:Project){self.prepare_save_picker(project,true);}

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
                    if self.screenshot_projects.is_none(){self.saved.save_choices.insert(project_key(&picker.project.repo),(picker.selection_name(),choice.clone()));}
                    if !picker.settings_only{self.start(ctx, picker.project, Kind::Play(choice));}
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
                theme::caps(ui, "Project library", 11.5, theme::lilac());
                theme::title(ui, "Import project", 36.0, 460.0);
                note(ui, "Add a Rust project by Git URL or import a prebuilt ZIP.");
                ui.add_space(14.0);
                let w = ui.available_width();
                if let Some(tab)=studio::widgets::tabs(ui,&[(0,"Git repository".into()),(1,"Build ZIP".into())],usize::from(add.zip)){add.zip=tab==1;add.focus=!add.zip;add.error=None;}
                if add.zip{
                    note(ui,"Choose a ZIP containing the compiled program and its supporting files.");
                    button_row(ui,|ui|{
                        if theme::button_ex(ui,Look::Primary,Size::Normal,Some(glyph::FOLDER),"Choose build ZIP…",!running,false).clicked(){
                            if let Some(path)=rfd::FileDialog::new().add_filter("Build ZIP",&["zip"]).pick_file(){self.import_build_zip(ctx,path);keep=false;}
                        }
                        if ui.button("Cancel").clicked(){keep=false;}
                    });
                    return;
                }
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
                    wrapped(ui, e, theme::bad());
                }
                ui.add_space(14.0);
                theme::callout(ui, theme::warn(), glyph::WARNING, |ui| {
                    wrapped(
                        ui,
                        "PalmVI downloads, builds and runs this repo's code on your PC. Add only repos from people you trust: fake repos with malware exist.",
                        theme::blush(),
                    );
                });
                ui.add_space(20.0);
                button_row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    if theme::button(ui, Look::Primary, Some(glyph::ADD), "Import repo link").clicked() {
                        submit = true;
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
                wrapped(ui, &error, theme::bad());
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
                theme::caps(ui, "Uninstall", 11.5, theme::bad());
                theme::title(ui, &name, 40.0, 440.0);
                wrapped(ui, "This deletes its download, its build and its shader cache. Your saves stay, and you can install it again later.", theme::blush());
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
                theme::caps(ui, "Install folder", 11.5, theme::lilac());
                theme::title(ui, "Move installed projects?", 40.0, 520.0);
                wrapped(ui, "Your installed projects and their saves are in the old folder. Moving them to another drive can take a while.", theme::blush());
                ui.add_space(10.0);
                ui.add(Label::new(RichText::new(format!("From  {}", old.display())).font(theme::mono(12.0)).color(theme::faint())).truncate());
                ui.add(Label::new(RichText::new(format!("To    {}", new.display())).font(theme::mono(12.0)).color(theme::white())).truncate());
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
        self.add_repo = Some(AddRepo { zip:false, link: String::new(), error: None, focus: true });
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

}
