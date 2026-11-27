use super::{button_row, note, wrapped};
use crate::save_zip;
use crate::saves::{self, LaunchSave};
use crate::theme::{self, glyph, Look, Size};
use crate::{slug, workspace, Project};
use eframe::egui::{self, vec2, Ui};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    ThisBuild,
    OtherBuilds,
    Favorites,
}

struct SaveOption {
    title: String,
    detail: &'static str,
    choice: LaunchSave,
    favorite_source: Option<PathBuf>,
}

pub(super) enum Action {
    Keep,
    Cancel,
    Launch(LaunchSave),
}

pub(crate) struct SavePicker {
    pub(crate) project: Project,
    pub(super) settings_only:bool,
    tab: Tab,
    selected: usize,
    this_build: Vec<SaveOption>,
    other_builds: Vec<SaveOption>,
    favorites: Vec<SaveOption>,
    checkpoint_root: PathBuf,
    naming: Option<String>,
    focus_name: bool,
    error: Option<String>,
    transfer_status: Option<Result<String, String>>,
    pending_delete: Option<(String, PathBuf)>,
}

impl SaveOption {
    fn copy(title: String, detail: &'static str, path: PathBuf) -> Self {
        Self {
            title,
            detail,
            choice: LaunchSave::Copy(path.clone()),
            favorite_source: Some(path),
        }
    }

    fn favorite(title: String, path: PathBuf) -> Self {
        Self {
            title,
            detail: "Starts from a copy of this checkpoint.",
            choice: LaunchSave::Copy(path),
            favorite_source: None,
        }
    }
}

impl SavePicker {
    pub(crate) fn new(project: Project) -> Self {
        let ws = workspace();
        let data = ws.join("data");
        let project_data = data.join(slug(&project.repo));
        let checkpoint_root = ws.join("checkpoints");
        let current = saves::save_folder(&project_data);
        let selected = usize::from(current.is_none());
        let mut this_build = vec![
            SaveOption {
                title: "Continue".into(),
                detail: "Use this build’s active save folder.",
                choice: LaunchSave::Continue,
                favorite_source: current,
            },
            SaveOption {
                title: "Start fresh".into(),
                detail: "Begin a new game in a separate save profile.",
                choice: LaunchSave::Fresh,
                favorite_source: None,
            },
        ];
        this_build.extend(
            saves::previous_saves(&project_data)
                .into_iter()
                .map(|(name, path)| {
                    SaveOption::copy(name, "Starts from a copy of an earlier save.", path)
                }),
        );
        let mut other_builds = Vec::new();
        for entry in fs::read_dir(&data).into_iter().flatten().flatten() {
            if entry.path() != project_data {
                if let Some(path) = saves::save_folder(&entry.path()) {
                    let name = entry.file_name().to_string_lossy().replacen('_', "/", 1);
                    other_builds.push(SaveOption::copy(
                        name,
                        "Copies this build’s active save.",
                        path,
                    ));
                }
            }
        }
        other_builds.sort_by(|a, b| a.title.cmp(&b.title));
        let mut favorites = Vec::new();
        for entry in fs::read_dir(&checkpoint_root)
            .into_iter()
            .flatten()
            .flatten()
        {
            let filename = entry.file_name();
            if let Some(name) = filename
                .to_str()
                .and_then(|name| name.strip_prefix("Checkpoint - "))
            {
                if entry.path().is_dir() {
                    favorites.push(SaveOption::favorite(name.into(), entry.path()));
                }
            }
        }
        favorites.sort_by(|a, b| a.title.cmp(&b.title));
        Self {
            project,
            settings_only:false,
            tab: Tab::ThisBuild,
            selected,
            this_build,
            other_builds,
            favorites,
            checkpoint_root,
            naming: None,
            focus_name: false,
            error: None,
            transfer_status: None,
            pending_delete: None,
        }
    }

    pub(super) fn choices(&self)->Vec<(String,String,LaunchSave)>{
        [("This build",&self.this_build),("Favorites",&self.favorites),("Other builds",&self.other_builds)].into_iter().flat_map(|(group,items)|items.iter().map(move |s|(group.to_string(),s.title.clone(),s.choice.clone()))).collect()
    }
    pub(super) fn select(&mut self,choice:&LaunchSave){
        for (tab,items) in [(Tab::ThisBuild,&self.this_build),(Tab::Favorites,&self.favorites),(Tab::OtherBuilds,&self.other_builds)]{
            if let Some(i)=items.iter().position(|s|&s.choice==choice){self.selected=i;self.tab=tab;return;}
        }
    }
    pub(super) fn selection_name(&self)->String{
        let list=match self.tab{Tab::ThisBuild=>&self.this_build,Tab::Favorites=>&self.favorites,Tab::OtherBuilds=>&self.other_builds};
        list.get(self.selected).map(|s|s.title.clone()).unwrap_or_default()
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        opened_at: f64,
        can_play: bool,
        screenshot_name: Option<&str>,
    ) -> (Action, bool) {
        theme::modal(ctx, "save-picker", opened_at, 540.0, |ui| {
            let screenshot = screenshot_name.is_some();
            theme::caps(
                ui,
                screenshot_name.unwrap_or(&self.project.name),
                11.5,
                theme::lilac(),
            );
            theme::title(ui, "Choose a save", 30.0, 540.0);
            if let Some((name, path)) = &self.pending_delete {
                wrapped(
                    ui,
                    &format!(
                        "Delete {name}? The files will move to PalmVI’s deleted-saves folder."
                    ),
                    theme::blush(),
                );
                let mut confirm = false;
                let mut cancel = false;
                button_row(ui, |ui| {
                    confirm =
                        theme::button(ui, Look::DangerSolid, Some(glyph::DELETE), "Delete save")
                            .clicked();
                    cancel = theme::button(ui, Look::Ghost, None, "Cancel").clicked();
                });
                if confirm {
                    match saves::delete_save(&workspace(), path) {
                        Ok(_) => {
                            let tab = self.tab;
                            let settings=self.settings_only;
                            *self = Self::new(self.project.clone());
                            self.settings_only=settings;
                            self.tab = tab;
                            self.transfer_status = Some(Ok("Save moved to deleted-saves.".into()));
                        }
                        Err(error) => {
                            self.pending_delete = None;
                            self.transfer_status =
                                Some(Err(format!("Could not delete save: {error}")));
                        }
                    }
                } else if cancel {
                    self.pending_delete = None;
                }
                return Action::Keep;
            }
            note(ui, "Choose where to start. Fresh games and copied saves keep your earlier progress available.");
            if theme::small(
                ui,
                Look::Secondary,
                Some(glyph::ADD),
                "Import save ZIP",
                !screenshot,
            )
            .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("VIctory save ZIP", &["zip"])
                    .pick_file()
                {
                    self.transfer_status =
                        Some(match save_zip::import(&self.checkpoint_root, &path) {
                            Ok((name, path)) => {
                                let checkpoint = SaveOption::favorite(name.clone(), path);
                                let index = self
                                    .favorites
                                    .partition_point(|option| option.title < checkpoint.title);
                                self.favorites.insert(index, checkpoint);
                                self.tab = Tab::Favorites;
                                self.selected = index;
                                self.naming = None;
                                self.error = None;
                                Ok(format!("Imported “{name}” into Favorites."))
                            }
                            Err(error) => Err(format!("Could not import save: {error}")),
                        });
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::ThisBuild, "This build"),
                    (Tab::OtherBuilds, "Other builds"),
                    (Tab::Favorites, "Favorites"),
                ] {
                    if theme::button(
                        ui,
                        if self.tab == tab {
                            Look::Primary
                        } else {
                            Look::Ghost
                        },
                        None,
                        label,
                    )
                    .clicked()
                        && self.tab != tab
                    {
                        self.tab = tab;
                        self.selected = 0;
                        self.naming = None;
                        self.error = None;
                    }
                }
            });
            ui.add_space(8.0);
            let (options, empty) = match self.tab {
                Tab::ThisBuild => (&self.this_build, ""),
                Tab::OtherBuilds => (
                    &self.other_builds,
                    "Saves from other builds appear here after you save in them.",
                ),
                Tab::Favorites => (
                    &self.favorites,
                    "To create a checkpoint, choose an existing save and select Add to favorites.",
                ),
            };
            let mut selected = self.selected;
            egui::ScrollArea::vertical()
                .id_salt("save-list")
                .max_height((ctx.screen_rect().height()*0.29).clamp(125.0,244.0))
                .show(ui, |ui| {
                    if options.is_empty() {
                        wrapped(ui, empty, theme::faint());
                    }
                    for (index, option) in options.iter().enumerate() {
                        let alias = (screenshot && self.tab != Tab::ThisBuild)
                            .then(|| format!("Example save {:02}", index + 1));
                        if save_row(
                            ui,
                            alias.as_deref().unwrap_or(&option.title),
                            option.detail,
                            selected == index,
                        ) {
                            selected = index;
                        }
                    }
                });
            if selected != self.selected {
                self.selected = selected;
                self.naming = None;
                self.error = None;
            }
            let choice = options.get(selected).map(|option| option.choice.clone());
            let source = options
                .get(selected)
                .and_then(|option| option.favorite_source.clone());

            let export_source = match &choice {
                Some(LaunchSave::Copy(path)) => Some(path.as_path()),
                _ => source.as_deref(),
            };
            if let Some(path) = export_source.filter(|_| !screenshot) {
                ui.horizontal_wrapped(|ui| {
                    if theme::small(
                        ui,
                        Look::Ghost,
                        Some(glyph::OPEN),
                        "Export selected save",
                        true,
                    )
                    .clicked()
                    {
                        if let Some(destination) = rfd::FileDialog::new()
                            .add_filter("ZIP archive", &["zip"])
                            .set_file_name("PalmVI Save.zip")
                            .save_file()
                        {
                            let destination = if destination.extension().is_none() {
                                destination.with_extension("zip")
                            } else {
                                destination
                            };
                            self.transfer_status = Some(
                                save_zip::export(path, &destination)
                                    .map(|()| format!("Exported to {}.", destination.display()))
                                    .map_err(|error| format!("Could not export save: {error}")),
                            );
                        }
                    }
                    if theme::small(
                        ui,
                        Look::Danger,
                        Some(glyph::DELETE),
                        "Delete selected save",
                        true,
                    )
                    .clicked()
                    {
                        if let Some(option) = options.get(selected) {
                            let name = if option.choice == LaunchSave::Continue {
                                format!("the active save for {}", self.project.name)
                            } else {
                                format!("“{}”", option.title)
                            };
                            self.pending_delete = Some((name, path.to_owned()));
                        }
                    }
                });
            }
            if let Some(status) = &self.transfer_status {
                match status {
                    Ok(message) => wrapped(ui, message, theme::good()),
                    Err(error) => wrapped(ui, error, theme::bad()),
                }
            }
            if let (Some(name), Some(source)) = (&mut self.naming, &source) {
                ui.add_space(10.0);
                note(ui, "Give this checkpoint a name you’ll remember.");
                let width = ui.available_width();
                let field = theme::field(ui, name, "e.g. Intro complete", width, None);
                if self.focus_name {
                    field.request_focus();
                    self.focus_name = false;
                }
                if field.changed() {
                    self.error = None;
                }
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some(error) = &self.error {
                    wrapped(ui, error, theme::bad());
                }
                let mut action = Action::Keep;
                button_row(ui, |ui| {
                    let valid = !name.trim().is_empty();
                    let save = theme::button_ex(
                        ui,
                        Look::Primary,
                        Size::Normal,
                        Some(glyph::CHECK),
                        "Save checkpoint",
                        valid,
                        false,
                    )
                    .clicked();
                    if valid && (enter || save) {
                        match saves::checkpoint(&self.checkpoint_root, name, source) {
                            Ok(path) => {
                                let checkpoint = SaveOption::favorite(name.trim().into(), path);
                                let index = self
                                    .favorites
                                    .partition_point(|option| option.title < checkpoint.title);
                                self.favorites.insert(index, checkpoint);
                                self.tab = Tab::Favorites;
                                self.selected = index;
                                self.error = None;
                                action = Action::Cancel;
                            }
                            Err(error) => {
                                self.error = Some(format!("Could not save checkpoint: {error}"))
                            }
                        }
                    }
                    if theme::button(ui, Look::Ghost, None, "Back").clicked() {
                        action = Action::Cancel;
                    }
                });
                if matches!(action, Action::Cancel) {
                    self.naming = None;
                    self.error = None;
                }
                return Action::Keep;
            }
            ui.add_space(12.0);
            let mut action = Action::Keep;
            button_row(ui, |ui| {
                let label = match &choice {
                    Some(LaunchSave::Continue) => "Continue game",
                    Some(LaunchSave::Fresh) => "Start new game",
                    Some(LaunchSave::Copy(_)) => "Play from copy",
                    None => "Play",
                };
                if theme::button_ex(
                    ui,
                    Look::Primary,
                    Size::Normal,
                    Some(if self.settings_only{glyph::CHECK}else{glyph::PLAY}),
                    if self.settings_only{"Use this save"}else{label},
                    (self.settings_only || can_play) && choice.is_some(),
                    false,
                )
                .clicked()
                {
                    if let Some(choice) = choice {
                        action = Action::Launch(choice);
                    }
                }
                if theme::button(ui, Look::Ghost, None, "Cancel").clicked() {
                    action = Action::Cancel;
                }
                if source.is_some()
                    && !screenshot
                    && theme::button(ui, Look::Secondary, Some(glyph::ADD), "Add to favorites")
                        .clicked()
                {
                    self.naming = Some(String::new());
                    self.focus_name = true;
                }
            });
            action
        })
    }
}

fn save_row(ui:&mut Ui,name:&str,detail:&str,selected:bool)->bool{
    let (r,b)=ui.allocate_exact_size(vec2(ui.available_width(),52.0),egui::Sense::click());
    let accent=theme::lilac();
    let fill=if selected{theme::mix(theme::surface(),accent,0.10)}else{theme::raised()};
    ui.painter().rect(r,8.0,fill,egui::Stroke::new(1.0_f32,if selected||b.hovered(){accent}else{theme::line()}));
    let dot=r.left_center()+vec2(17.0,0.0);
    ui.painter().circle_stroke(dot,6.0,egui::Stroke::new(1.4_f32,if selected{accent}else{theme::muted()}));
    if selected{ui.painter().circle_filled(dot,3.0,accent);}
    let width=(r.width()-46.0).max(40.0);
    let title=theme::one_line(ui,name,theme::strong(15.0),theme::white(),width);
    let sub=theme::one_line(ui,detail,theme::regular(12.0),theme::muted(),width);
    ui.painter().galley(r.min+vec2(35.0,7.0),title,theme::white());
    ui.painter().galley(r.min+vec2(35.0,29.0),sub,theme::muted());
    b.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
