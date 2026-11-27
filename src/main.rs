//! Mandyhub: a community launcher for VIctory-based projects that run an
//! Xbox Series game on PC.
//!
//! - The list of projects comes from PortWatch (portwatch-vi.xyz), live.
//! - Any other GitHub repo can be added by link.
//! - The game folder is checked for the Xbox files; without them Play stays off.
//! - Play: the selected repo is downloaded (git), built (cargo) and started.
//!
//! No game files are included or downloaded: the player supplies their own.
#![windows_subsystem = "windows"]

mod fullscreen;
mod slides;
mod theme;
mod tools;
mod ui;
mod saves;
mod save_zip;
mod build_activity;
mod activity_log;
mod updater;
mod script_preset;
#[cfg(all(test, windows))]
mod launch_parity_tests;

use eframe::egui;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

const PORTWATCH: &str = "https://portwatch-vi.xyz/data/tracker.json";
const EXE_NAME: &str = "game_scarlett_master.exe";
/// What a game folder must hold: the executable, its config and the main archives.
const REQUIRED: &[&str] = &[
    EXE_NAME,
    "MicrosoftGame.Config",
    "common.rpf",
    "data.rpf",
    "levels.rpf",
    "script.rpf",
    "shaders.rpf",
    "textures.rpf",
];

/// A project the player can pick.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Project {
    name: String,
    /// `owner/name` on GitHub.
    repo: String,
    branch: Option<String>,
    gpu: Vec<String>,
    fps: Option<f64>,
    render: Option<f64>,
    verified: bool,
    summary: String,
    custom: bool,
    /// A build already on this PC (not downloaded or built by the launcher).
    #[serde(default)]
    local_exe: Option<String>,
    #[serde(default)]
    archive_exe: Option<PathBuf>,
}

mod build_zip;
mod rust_project;

impl Project {
    fn is_git(&self) -> bool {
        self.local_exe.is_none() && self.archive_exe.is_none()
    }

    fn archive_program(&self) -> Option<PathBuf> {
        self.archive_exe.as_ref().map(|exe| repo_dir(&self.repo).join(exe))
    }

    fn runs_standalone(&self) -> bool {
        self.archive_exe.as_deref().is_some_and(|exe| !build_zip::is_victory(exe))
            || rust_install(&self.repo).is_some()
    }

    fn web_url(&self) -> String {
        if self.repo.contains("://") { self.repo.clone() } else { format!("https://github.com/{}", self.repo) }
    }

    fn clone_url(&self) -> String {
        if self.repo.contains("://") { self.repo.clone() } else { format!("https://github.com/{}.git", self.repo) }
    }
}

/// What is kept between runs.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    game_dir: String,
    /// "8 GB graphics card fix": smaller textures and a grey stand-in for missing ones.
    fix_8gb: bool,
    /// "Brighter shadows": a constant ambient light where ray tracing does not run.
    bright_shadows: bool,
    /// Whether the defaults above were set once.
    defaults_set: bool,
    /// Play in borderless fullscreen (off: a normal window). Off by default.
    fullscreen: bool,
    /// Where projects are downloaded and built (empty: the default folder).
    install_dir: String,
    /// Updates put off with "Later": (repo, the new version's commit).
    later: Vec<(String, String)>,
    selected: Option<String>,
    custom: Vec<Project>,
    series_s: bool,
    render_scale: f32,
    extra_env: String,
    gpu_filter: usize,
    /// Translate the shaders into the cache (`victory translate --cache`) before each Play.
    warm_before_play: bool,
    /// With the warm-up, also build the GPU pipelines (`--pipelines`; can take hours).
    warm_pipelines: bool,
    /// Extra arguments for `victory run`; the game's own go after `--`.
    launch_args: String,
    project_launch: std::collections::BTreeMap<String, ProjectLaunch>,
    launch_presets: Vec<LaunchPreset>,
}

#[derive(Clone, Serialize, Deserialize)]
struct LaunchPreset {
    name: String,
    args: String,
    switches: Option<String>,
    #[serde(default)]
    script_options: Option<script_preset::Options>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    projects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repo_script: Option<String>,
}

impl LaunchPreset {
    fn apply(&self, launch: &mut ProjectLaunch) {
        launch.args = Some(self.args.clone());
        if let Some(switches) = &self.switches {
            launch.switches = Some(switches.clone());
            launch.script_options = self.script_options;
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ProjectLaunch {
    args: Option<String>,
    switches: Option<String>,
    script_options: Option<script_preset::Options>,
    selected_preset: Option<String>,
    preset_choice_confirmed: bool,
    reviewed_script_revision: Option<String>,
    /// Records a custom script accepted by an earlier build.
    confirmed_repo_script: Option<String>,
    graphics: Option<GraphicsSettings>,
    notes: String,
}

impl ProjectLaunch {
    fn scripts_need_review(&self, installed: Option<&str>) -> bool {
        matches!((self.reviewed_script_revision.as_deref(), installed), (Some(old), Some(new)) if old != new)
    }

    fn has_confirmed_preset(&self) -> bool {
        self.preset_choice_confirmed || self.selected_preset.is_some() || self.confirmed_repo_script.is_some()
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
struct GraphicsSettings {
    series_s: bool,
    render_scale: f32,
    fix_8gb: bool,
    bright_shadows: bool,
    fullscreen: bool,
    #[serde(default)]
    output: OutputResolution,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
enum OutputResolution {
    #[default]
    Default,
    P720,
    P936,
    P1080,
    P1440,
    P2160,
}

impl OutputResolution {
    const OPTIONS: [Self; 6] = [Self::Default, Self::P720, Self::P936, Self::P1080, Self::P1440, Self::P2160];

    fn label(self) -> &'static str {
        match self {
            Self::Default => "Game default",
            Self::P720 => "720p",
            Self::P936 => "936p",
            Self::P1080 => "1080p",
            Self::P1440 => "1440p",
            Self::P2160 => "2160p (4K)",
        }
    }

    fn dimensions(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::P720 => Some("1280x720"),
            Self::P936 => Some("1664x936"),
            Self::P1080 => Some("1920x1080"),
            Self::P1440 => Some("2560x1440"),
            Self::P2160 => Some("3840x2160"),
        }
    }
}

impl Saved {
    fn launch(&self, repo: &str, scarlett_shader_pass: bool) -> Launch {
        let settings = self.project_launch.get(&project_key(repo));
        let (args, extra) = self.launch_overrides(repo);
        let graphics = self.graphics_for(repo);
        Launch {
            game_dir: std::path::absolute(&self.game_dir).unwrap_or_else(|_| PathBuf::from(&self.game_dir)),
            series_s: graphics.series_s,
            scale: graphics.render_scale,
            output: graphics.output,
            extra,
            use_script_defaults: settings.is_some_and(|s| s.switches.is_some() && s.script_options.is_some()),
            scarlett_shader_pass,
            fullscreen: graphics.fullscreen,
            warm_first: self.warm_before_play,
            pipelines: self.warm_pipelines,
            args,
            program_args: settings.and_then(|s| s.args.as_deref()).map(split_args).unwrap_or_default(),
        }
    }

    /// Follow the installed script when a profile uses a repository preset.
    /// The stored preset is the baseline against which the player's edits were made.
    fn refresh_repo_preset(&mut self, project: &Project) -> Result<Option<(String, bool)>, String> {
        let key = project_key(&project.repo);
        let Some(launch) = self.project_launch.get_mut(&key) else { return Ok(None) };
        let Some(preset) = self.launch_presets.iter_mut().find(|p|
            launch.selected_preset.as_ref() == Some(&p.name) && p.projects.contains(&key) && p.repo_script.is_some()
        ) else { return Ok(None) };
        let (Some(name), Some(profile), Some(previous)) =
            (&preset.repo_script, &mut launch.switches, &mut preset.switches) else { return Ok(None) };
        if !name.eq_ignore_ascii_case("play") && !name.to_ascii_lowercase().starts_with("play-")
            || name.contains(['/', '\\', ':']) {
            return Err("Invalid repository script name".into());
        }
        let path = project_script_root(project).join("scripts").join(format!("{name}.ps1"));
        let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (current, options) = script_preset::parse(&source)?;
        if *previous != current {
            *profile = script_preset::refresh_defaults(previous, &current, profile);
            *previous = current;
        }
        launch.script_options = Some(options);
        preset.script_options = Some(options);
        let shader_pass = source.lines().any(|line| line.trim().eq_ignore_ascii_case("& $Victory shaders $rpf"));
        Ok(Some((format!("{name}.ps1"), shader_pass)))
    }

    fn default_graphics(&self) -> GraphicsSettings {
        GraphicsSettings {
            series_s: self.series_s,
            render_scale: self.render_scale,
            fix_8gb: self.fix_8gb,
            bright_shadows: self.bright_shadows,
            fullscreen: self.fullscreen,
            output: OutputResolution::Default,
        }
    }

    fn graphics_for(&self, repo: &str) -> GraphicsSettings {
        self.project_launch.get(&project_key(repo))
            .and_then(|project| project.graphics)
            .unwrap_or_else(|| self.default_graphics())
    }

    fn launch_overrides(&self, repo: &str) -> (Vec<String>, String) {
        let graphics = self.graphics_for(repo);
        let project = self.project_launch.get(&project_key(repo));
        let args = project.and_then(|p| p.args.as_deref()).unwrap_or(&self.launch_args);
        let switches = project.and_then(|p| p.switches.as_deref());
        let mut extra = switches.map_or_else(|| self.extra_env.clone(), |_| String::new());
        if graphics.fix_8gb {
            extra.push_str("\nVICTORY_TEXTURE_DROP_MIPS=2\nVICTORY_NULL_COLOR=grey");
        }
        if graphics.bright_shadows {
            extra.push_str("\nVICTORY_AMBIENT=0.6");
        }
        if let Some(switches) = switches {
            extra.push('\n');
            extra.push_str(switches);
        }
        (split_args(args), extra)
    }
}

#[cfg(test)]
#[test]
fn project_launch_overrides_are_independent_and_can_be_empty() {
    let mut saved = Saved {
        launch_args: "--strict".into(),
        extra_env: "VICTORY_GLOBAL=1".into(),
        ..Saved::default()
    };
    saved.project_launch.insert("owner/test".into(), ProjectLaunch {
        args: Some(String::new()),
        switches: None,
        ..ProjectLaunch::default()
    });
    assert_eq!(saved.launch_overrides("Owner/Test"), (Vec::new(), "VICTORY_GLOBAL=1".into()));
    saved.project_launch.get_mut("owner/test").unwrap().switches = Some("VICTORY_LOCAL=1".into());
    assert_eq!(saved.launch_overrides("owner/test"), (Vec::new(), "\nVICTORY_LOCAL=1".into()));
    assert_eq!(saved.launch_overrides("owner/other"), (vec!["--strict".into()], "VICTORY_GLOBAL=1".into()));
    let mut graphics = saved.default_graphics();
    graphics.series_s = true;
    graphics.render_scale = 0.5;
    graphics.fix_8gb = true;
    graphics.fullscreen = true;
    saved.project_launch.get_mut("owner/test").unwrap().graphics = Some(graphics);
    assert!(saved.graphics_for("Owner/Test").fullscreen);
    assert!(!saved.graphics_for("owner/other").fullscreen);
    assert!(saved.launch_overrides("owner/test").1.contains("VICTORY_TEXTURE_DROP_MIPS=2"));
    assert!(!saved.launch_overrides("owner/other").1.contains("VICTORY_TEXTURE_DROP_MIPS"));
}

/// Shared state of the background job (download, build, play).
#[derive(Default)]
struct Job {
    running: bool,
    /// The project the job works on.
    repo: Option<String>,
    stage: String,
    warm_status: Option<String>,
    log: activity_log::ActivityLog,
    imported_project: Option<Project>,
    removed_archive: Option<String>,
    updated_project: Option<Project>,
    import_error: Option<String>,
    failed: bool,
    /// The process running now, while the player may stop it (a warm-up).
    pid: Option<u32>,
    stoppable: bool,
    /// The player pressed Stop.
    stopped: bool,
}

/// How a project is started, read from the settings when its job starts.
/// The warm-up and Play share it, so the warm-up fills the cache Play reads.
#[derive(Clone)]
struct Launch {
    game_dir: PathBuf,
    series_s: bool,
    scale: f32,
    output: OutputResolution,
    /// `VICTORY_*` lines: the player's switches and the graphics fixes.
    extra: String,
    use_script_defaults: bool,
    scarlett_shader_pass: bool,
    fullscreen: bool,
    warm_first: bool,
    pipelines: bool,
    args: Vec<String>,
    program_args: Vec<String>,
}

/// The game files `victory translate --cache` reads shaders from (those that exist are passed).
const WARM_FILES: &[&str] = &["shaders.rpf", EXE_NAME, "XboxBvhBuilder.dll", "amd_fidelityfx_upscaler_xbox.dll"];

/// Launch arguments as the player typed them: split at spaces, "double quotes" keeping one together.
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for ch in s.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split_args;

    #[test]
    fn launch_arguments_split_at_spaces_and_keep_quotes_together() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args("  --strict   "), ["--strict"]);
        assert_eq!(split_args("--strict -- -windowed"), ["--strict", "--", "-windowed"]);
        assert_eq!(split_args(r#"-- -name "two words" """#), ["--", "-name", "two words", ""]);
    }
}

struct App {
    saved: Saved,
    screenshot_projects: Option<Vec<String>>,
    updater: updater::Updater,
    projects: Arc<Mutex<Option<Result<Vec<Project>, String>>>>,
    job: Arc<Mutex<Job>>,
    files: Vec<(&'static str, bool)>,
    files_for: String,
    tools: Option<tools::Tools>,
    /// The project an uninstall is being confirmed for.
    confirm_uninstall: Option<String>,
    save_picker: Option<ui::SavePicker>,
    play_preset_picker: Option<ui::PlayPresetPicker>,
    /// A new install folder waiting for "move the projects there?" (from, to).
    pending_move: Option<(PathBuf, PathBuf)>,
    /// The folder the files were checked in was empty.
    empty: bool,
    /// The drive search: `None` idle, `Some(None)` searching, `Some(Some(r))` done.
    finding: Arc<Mutex<Option<Option<Result<String, String>>>>>,
    /// What is known about each project's install, by repo.
    repos: Arc<Mutex<std::collections::HashMap<String, RepoState>>>,
    show_log: bool,
    log_expanded: bool,
    log_export_status: Option<(String, bool)>,
    /// The player's own pictures, across the top of a project's page.
    slides: slides::Slides,
    /// When every installed project was last checked for updates (seconds).
    last_auto_check: f64,
    /// Which page shows: the projects or the settings.
    view: View,
    /// What the project list is narrowed to.
    search: String,
    /// The "Add a repo by link" dialog, while it is open.
    add_repo: Option<AddRepo>,
    /// Why the last drive search found no game.
    find_error: Option<String>,
    /// The project whose page shows, and when it came in (for its entrance).
    shown: Option<String>,
    shown_at: f64,
    /// When the repo link was last copied: its button shows a tick for a moment.
    copied_at: f64,
    /// When the open dialog opened (for its entrance), and whether one was open last frame.
    modal_at: f64,
    modal_was_open: bool,
    /// Whether the window's title bar was made dark.
    title_bar_done: bool,
    /// Whether the window's remembered size was checked at start.
    size_checked: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Projects,
    Settings,
}

/// The "Add a repo by link" dialog.
struct AddRepo {
    link: String,
    error: Option<String>,
    /// Put the cursor in the field on the first frame.
    focus: bool,
}

/// How often installed projects are checked for updates: every 15 minutes.
const AUTO_CHECK_SECS: f64 = 15.0 * 60.0;

/// A project's install on this PC and the newest version online.
#[derive(Clone, Default)]
struct RepoState {
    checking: Option<Arc<()>>,
    feedback: Option<ProjectCheckFeedback>,
    /// Built and ready to play.
    installed: bool,
    /// The commit the installed build was made from.
    built: Option<String>,
    /// The newest commit online (`None` while unknown or unreachable).
    remote: Option<String>,
    activity: Option<build_activity::BuildActivity>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ProjectCheckFeedback {
    Available,
    Current(std::time::Instant),
    Failed(std::time::Instant),
}

impl RepoState {
    fn finish_check(&mut self, ticket: &Arc<()>, mut next: Self, manual: bool) {
        if !self.checking.as_ref().is_some_and(|active| Arc::ptr_eq(active, ticket)) {
            return;
        }
        next.feedback = if manual {
            let now = std::time::Instant::now();
            match (&next.remote, &next.built) {
                (None, _) => Some(ProjectCheckFeedback::Failed(now)),
                (Some(remote), Some(built)) if remote == built => Some(ProjectCheckFeedback::Current(now)),
                (Some(_), _) if next.installed => Some(ProjectCheckFeedback::Available),
                _ => None,
            }
        } else if next.remote.is_some() && next.remote != self.remote {
            None
        } else {
            self.feedback
        };
        if next.remote.is_none() {
            next.remote = self.remote.take();
        }
        *self = next;
    }
}

#[cfg(test)]
#[test]
fn project_checks_distinguish_manual_results_and_ignore_replaced_requests() {
    let result = |remote: Option<&str>| RepoState {
        installed: true,
        built: Some("built".into()),
        remote: remote.map(str::to_owned),
        ..RepoState::default()
    };
    let mut state = result(Some("built"));
    let ticket = Arc::new(());
    state.checking = Some(ticket.clone());
    state.finish_check(&Arc::new(()), result(Some("obsolete")), true);
    assert_eq!(state.remote.as_deref(), Some("built"));
    assert!(state.checking.is_some());
    state.finish_check(&ticket, result(Some("new")), false);
    assert!(state.checking.is_none());
    assert!(state.feedback.is_none());
    let manual = |state: &mut RepoState, remote| {
        let ticket = Arc::new(());
        state.checking = Some(ticket.clone());
        state.finish_check(&ticket, result(remote), true);
    };
    manual(&mut state, Some("new"));
    assert_eq!(state.feedback, Some(ProjectCheckFeedback::Available));
    manual(&mut state, None);
    assert!(matches!(state.feedback, Some(ProjectCheckFeedback::Failed(_))));
    assert_eq!(state.remote.as_deref(), Some("new"));
    manual(&mut state, Some("built"));
    assert!(matches!(state.feedback, Some(ProjectCheckFeedback::Current(_))));
}

/// What the background job does.
#[derive(Clone, PartialEq)]
enum Kind {
    Install,
    Play(saves::LaunchSave),
    Uninstall,
    /// Shaders into the cache ahead of time, without playing.
    Warm,
}

fn project_key(repo: &str) -> String {
    if repo.contains("://") { repo.to_owned() } else { repo.to_ascii_lowercase() }
}

fn slug(repo: &str) -> String {
    if repo.contains("://") {
        let hash = ring::digest::digest(&ring::digest::SHA256, repo.as_bytes());
        let hex: String = hash.as_ref()[..16].iter().map(|byte| format!("{byte:02x}")).collect();
        return format!("git-{hex}");
    }
    repo.replace('/', "_")
}

#[derive(Serialize, Deserialize)]
struct RustInstall {
    executable: Option<PathBuf>,
}

fn rust_install_file(repo: &str) -> PathBuf {
    target_dir(repo).join("mandyhub-rust.json")
}

fn rust_install(repo: &str) -> Option<RustInstall> {
    serde_json::from_slice(&std::fs::read(rust_install_file(repo)).ok()?).ok()
}

fn project_installed(repo: &str) -> bool {
    match rust_install(repo) {
        Some(install) => install.executable.is_none_or(|exe| target_dir(repo).join(exe).is_file()),
        None => built_exe(repo).is_file(),
    }
}
fn repo_dir(repo: &str) -> PathBuf {
    workspace().join("repos").join(slug(repo))
}
fn project_script_root(p: &Project) -> PathBuf {
    let dir = repo_dir(&p.repo);
    if dir.join("scripts").is_dir() { return dir; }
    if let Some(exe) = p.archive_program().or_else(|| p.local_exe.as_ref().map(PathBuf::from)) {
        if let Some(parent) = exe.parent() { return parent.to_owned(); }
    }
    cargo_workspace(&dir).unwrap_or(dir)
}
fn target_dir(repo: &str) -> PathBuf {
    workspace().join("target").join(slug(repo))
}
fn built_exe(repo: &str) -> PathBuf {
    let dir = repo_dir(repo);
    let workspace = cargo_workspace(&dir).unwrap_or_else(|_| dir.clone());
    let target = target_dir(repo);
    let executable = target.join(detect_build_profile(&workspace)).join("victory.exe");
    let previous = target.join("release").join("victory.exe");
    if workspace != dir && !executable.is_file() && previous.is_file() {
        return previous;
    }
    executable
}

/// Most projects build at the root. Some wrap a Cargo workspace one level down.
fn cargo_workspace(repo: &Path) -> Result<PathBuf, String> {
    if repo.join("Cargo.toml").is_file() {
        return Ok(repo.to_owned());
    }
    let entries = std::fs::read_dir(repo).map_err(|error| format!("Could not inspect {}: {error}", repo.display()))?;
    let mut nested = Vec::new();
    let mut victory = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not inspect {}: {error}", repo.display()))?;
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) && path.join("Cargo.toml").is_file() {
            if path.join("apps/victory/Cargo.toml").is_file() {
                victory.push(path.clone());
            }
            nested.push(path);
        }
    }
    let candidates = if victory.is_empty() { &nested } else { &victory };
    match candidates.as_slice() {
        [workspace] => Ok(workspace.clone()),
        [] => Err(format!("No Cargo.toml found at {} or one folder down.", repo.display())),
        _ => Err(format!("Multiple nested Cargo projects found in {}. Put a workspace manifest at the repository root.", repo.display())),
    }
}

#[cfg(test)]
#[test]
fn finds_a_unique_nested_victory_workspace_and_prefers_the_root() {
    let root = std::env::temp_dir().join(format!("mandyhub-workspace-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let nested = root.join("cool_fox");
    std::fs::create_dir_all(nested.join("apps/victory")).unwrap();
    std::fs::write(nested.join("Cargo.toml"), "[profile.diag]").unwrap();
    std::fs::write(nested.join("apps/victory/Cargo.toml"), "").unwrap();
    assert_eq!(cargo_workspace(&root).unwrap(), nested);
    assert_eq!(detect_build_profile(&nested), "diag");
    let other = root.join("other");
    std::fs::create_dir_all(other.join("apps/victory")).unwrap();
    std::fs::write(other.join("Cargo.toml"), "").unwrap();
    std::fs::write(other.join("apps/victory/Cargo.toml"), "").unwrap();
    assert!(cargo_workspace(&root).is_err());
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    assert_eq!(cargo_workspace(&root).unwrap(), root);
    std::fs::remove_dir_all(root).unwrap();
}

/// Checks a cloned repo's Cargo.toml for a custom build profile (e.g. `diag`).
fn detect_build_profile(repo_dir: &Path) -> &'static str {
    let cargo_toml = repo_dir.join("Cargo.toml");
    if let Ok(text) = std::fs::read_to_string(&cargo_toml) {
        if text.contains("[profile.diag]") {
            return "diag";
        }
    }
    "release"
}

/// Reads VICTORY_* environment variables from a project's scripts/play.ps1.
fn detect_launch_env(repo_dir: &Path) -> Vec<(String, String)> {
    let script = repo_dir.join("scripts").join("play.ps1");
    let Ok(text) = std::fs::read_to_string(&script) else { return Vec::new() };
    script_preset::parse(&text).map(|(switches, _)| switches.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned())).collect()).unwrap_or_default()
}
/// The commit an installed build was made from, written after a build.
fn built_file(repo: &str) -> PathBuf {
    target_dir(repo).join("built-from.txt")
}

fn installed_revision(repo: &str) -> Option<String> {
    std::fs::read_to_string(built_file(repo)).ok().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// The newest commit of `p`'s branch online (`git ls-remote`, no download).
fn remote_head(p: &Project) -> Option<String> {
    let url = p.clone_url();
    let mut c = tools::git();
    c.args(["ls-remote", "--"]).arg(&url);
    match &p.branch {
        Some(b) => c.arg(format!("refs/heads/{b}")),
        None => c.arg("HEAD"),
    };
    hide_window(&mut c);
    let out = c.output().ok()?;
    if !out.status.success() { return None; }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().next()
        .filter(|hash| matches!(hash.len(), 40 | 64) && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_string)
}

fn repo_state(p: &Project) -> RepoState {
    if let Some(exe) = p.archive_program() {
        return RepoState {
            installed: exe.is_file(),
            activity: build_activity::BuildActivity::read(&target_dir(&p.repo)),
            ..RepoState::default()
        };
    }
    if let Some(exe) = &p.local_exe {
        return RepoState { installed: Path::new(exe).is_file(), built: Some("local".into()), ..RepoState::default() };
    }
    RepoState {
        installed: project_installed(&p.repo),
        built: std::fs::read_to_string(built_file(&p.repo)).ok().map(|s| s.trim().to_string()),
        remote: remote_head(p),
        activity: build_activity::BuildActivity::read(&target_dir(&p.repo)),
        ..RepoState::default()
    }
}

/// A folder from what the player typed or picked: quotes and spaces off, and
/// the folder of a file if a file was given (the game's exe, say).
fn normalize_dir(s: &str) -> PathBuf {
    let t = s.trim().trim_matches('"').trim();
    let p = PathBuf::from(t);
    if p.is_file() {
        p.parent().map(Path::to_path_buf).unwrap_or(p)
    } else {
        p
    }
}

/// The folder holding the game's exe: `dir` itself or a folder under it, at
/// most `depth` levels down.
fn find_game_in(dir: &Path, depth: u32) -> Option<PathBuf> {
    if dir.join(EXE_NAME).is_file() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        // Not worth walking into.
        let skip = ["windows", "program files", "program files (x86)", "programdata", "appdata"];
        if name.starts_with('$') || name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(found) = find_game_in(&e.path(), depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Every drive's folders, three levels down, for the game's exe.
fn search_drives() -> Result<String, String> {
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if !root.is_dir() {
            continue;
        }
        if let Some(found) = find_game_in(&root, 3) {
            return Ok(found.display().to_string());
        }
    }
    Err("The game was not found on any drive. Choose its folder with Browse.".into())
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mandyhub")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 620.0])
            .with_icon(theme::app_icon()),
        ..Default::default()
    };
    migrate_old_folders();
    eframe::run_native("Mandyhub", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// Where the launcher keeps repos it downloads: `%LOCALAPPDATA%\Mandyhub`.
fn workspace() -> PathBuf {
    INSTALL_DIR.lock().unwrap().clone().unwrap_or_else(default_workspace)
}

/// `%LOCALAPPDATA%\Mandyhub`: where projects go unless the player chose
/// another folder (Settings: Install folder).
fn default_workspace() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Mandyhub")
}

/// The install folder the player chose, if any.
static INSTALL_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn set_install_dir(dir: &str) {
    let d = dir.trim();
    *INSTALL_DIR.lock().unwrap() = (!d.is_empty()).then(|| std::path::absolute(d).unwrap_or_else(|_| PathBuf::from(d)));
}

/// The parts of an install folder that belong to projects.
const PROJECT_PARTS: &[&str] = &["repos", "target", "cache", "data", "checkpoints", "deleted-saves"];

/// Whether `dir` holds any installed project.
fn has_projects(dir: &Path) -> bool {
    std::fs::read_dir(dir.join("repos")).map(|mut d| d.next().is_some()).unwrap_or(false)
}

/// Moves a folder: a rename on the same drive, else a copy and a delete.
fn move_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !to.exists() && std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to, log)?;
    std::fs::remove_dir_all(from)
}

fn copy_dir(from: &Path, to: &Path, log: &dyn Fn(String)) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target, log)?;
        } else {
            std::fs::copy(e.path(), &target)?;
        }
    }
    log(format!("copied {}", from.display()));
    Ok(())
}

/// The launcher's folders had another name before: they are moved once, so
/// installed projects, settings and pictures stay.
fn migrate_old_folders() {
    for var in ["LOCALAPPDATA", "APPDATA"] {
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else { continue };
        let (old, new) = (base.join("VI Launcher"), base.join("Mandyhub"));
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
}

/// Words kept out of the source as text: the game's name in the ways other
/// sites write it, to replace or skip (`clean_text`, `names_the_game`), and
/// PortWatch's field name for scores. Each byte is XORed with 0x5A.
const NAMES: &[&[u8]] = &[&[29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 12, 19], &[29, 40, 59, 52, 62, 122, 14, 50, 63, 60, 46, 122, 27, 47, 46, 53, 122, 108], &[29, 14, 27, 122, 12, 19], &[29, 14, 27, 122, 108], &[29, 14, 27, 108], &[29, 14, 27, 12, 19], &[61, 46, 59, 108], &[61, 46, 59, 122, 108]];
const KEYS: &[&[u8]] = &[&[61, 46, 59, 108], &[61, 46, 59, 44, 51], &[61, 40, 59, 52, 62, 46, 50, 63, 60, 46, 59, 47, 46, 53]];
const FIELD: &[u8] = &[61, 46, 59];

fn hidden(words: &[&[u8]]) -> Vec<String> {
    words.iter().map(|w| w.iter().map(|b| (b ^ 0x5A) as char).collect()).collect()
}

/// Text from other sites without the game's name: "the game" in its place.
fn clean_text(s: &str) -> String {
    let mut out = s.to_string();
    for pat in hidden(NAMES) {
        let pat = pat.as_str();
        for (from, to) in [(format!("{pat}'s"), "the game's".to_string()), (pat.to_string(), "the game".to_string())] {
            out = out.replace(&from, &to);
        }
    }
    out.replace(" PC port of the game", " PC port").replace("the game PC", "the game on PC")
}

/// Whether a repo's own name carries the game's name (not shown).
fn names_the_game(s: &str) -> bool {
    let l = s.to_lowercase().replace(['-', '_', ' ', '.'], "");
    hidden(KEYS).iter().any(|k| l.contains(k.as_str()))
}

/// `owner/name` from a GitHub link or `owner/name` itself.
fn parse_git_repo(link: &str) -> Option<String> {
    let link = link.trim();
    let normalized;
    let link = if !link.contains("://") && link.split('/').next()?.contains('.') && !link.starts_with("github.com/") {
        normalized = format!("https://{link}");
        &normalized
    } else {
        link
    };
    if !link.contains("://") {
        return parse_repo(link);
    }
    let mut url = url::Url::parse(link).ok()?;
    if !matches!(url.scheme(), "https" | "http" | "ssh" | "git")
        || url.host_str().is_none()
        || (url.scheme() != "ssh" && !url.username().is_empty())
        || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
    {
        return None;
    }
    let path = url.path().trim_end_matches('/').to_owned();
    if path.is_empty() {
        return None;
    }
    if url.host_str() == Some("github.com") && matches!(url.scheme(), "https" | "http") && url.port().is_none() {
        return parse_repo(url.as_str());
    }
    url.set_path(&path);
    Some(url.into())
}

#[cfg(test)]
#[test]
fn custom_git_links_preserve_non_github_paths_and_old_github_ids() {
    assert_eq!(parse_git_repo("https://github.com/Owner/Project.git").as_deref(), Some("Owner/Project"));
    assert_eq!(parse_git_repo("Owner/Project").as_deref(), Some("Owner/Project"));
    assert_eq!(
        parse_git_repo("gitlab.example.org/group/SubGroup/Project.git").as_deref(),
        Some("https://gitlab.example.org/group/SubGroup/Project.git")
    );
    assert_eq!(
        parse_git_repo("ssh://git@192.0.2.1:2222/group/project.git").as_deref(),
        Some("ssh://git@192.0.2.1:2222/group/project.git")
    );
    for invalid in ["file:///C:/", "ext::cmd/path", "https://user:password@git.example/repo", "https://git.example/"] {
        assert!(parse_git_repo(invalid).is_none(), "{invalid}");
    }
}

fn parse_repo(link: &str) -> Option<String> {
    let s = link.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s
        .strip_prefix("https://github.com/")
        .or_else(|| s.strip_prefix("http://github.com/"))
        .or_else(|| s.strip_prefix("github.com/"))
        .unwrap_or(s);
    let mut parts = s.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    let ok = |p: &str| p.chars().any(|c| c.is_ascii_alphanumeric()) && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}

/// The public projects on PortWatch, best first.
fn fetch_portwatch() -> Result<Vec<Project>, String> {
    let body: serde_json::Value = ureq::get(PORTWATCH)
        .set("User-Agent", "mandyhub")
        .call()
        .map_err(|e| format!("PortWatch could not be reached: {e}"))?
        .into_json()
        .map_err(|e| format!("PortWatch sent something unreadable: {e}"))?;
    let efforts = body["efforts"].as_array().ok_or("PortWatch's list has no projects")?;
    // PortWatch keeps each project's scores under a field named after the game.
    let field = hidden(&[FIELD]).remove(0);
    let mut out: Vec<Project> = efforts
        .iter()
        .filter_map(|e| {
            let repo = e["repo"].as_str()?.to_string();
            parse_repo(&repo)?;
            if names_the_game(&repo) || names_the_game(e["name"].as_str().unwrap_or("")) {
                return None;
            }
            if e["stats"]["missing"].as_bool() == Some(true) || e["stats"]["archived"].as_bool() == Some(true) {
                return None;
            }
            Some(Project {
                name: clean_text(e["name"].as_str().unwrap_or(&repo)),
                branch: e["branch"].as_str().map(str::to_string),
                gpu: e["gpu"].as_array().map(|g| g.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                fps: e[field.as_str()]["fps"].as_f64(),
                render: e[field.as_str()]["render"].as_f64(),
                verified: e["verification"].as_str() == Some("portwatch"),
                summary: clean_text(e["summary"].as_str().unwrap_or("")),
                custom: false,
                local_exe: None,
                archive_exe: None,
                repo,
            })
        })
        .collect();
    let score = |p: &Project| p.render.unwrap_or(0.0) * 10.0 + p.fps.unwrap_or(0.0) + if p.verified { 5.0 } else { 0.0 };
    out.sort_by(|a, b| score(b).total_cmp(&score(a)));
    Ok(out)
}

#[cfg(windows)]
pub(crate) fn hide_window(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    c.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
pub(crate) fn hide_window(_: &mut Command) {}

impl App {
    fn import_build_zip(&mut self, ctx: &egui::Context, input: PathBuf) {
        let name = input.file_stem().and_then(|name| name.to_str()).unwrap_or("Imported build").to_owned();
        let job = self.job.clone();
        let ctx = ctx.clone();
        {
            let mut state = job.lock().unwrap();
            if state.running {
                return;
            }
            *state = Job { running: true, stage: "Extracting build ZIP".into(), ..Job::default() };
        }
        self.log_export_status = None;
        std::thread::spawn(move || {
            let result = (|| -> Result<Project, String> {
                use ring::rand::{SecureRandom, SystemRandom};
                let mut id = [0; 16];
                SystemRandom::new().fill(&mut id).map_err(|_| "Could not create a build ID.")?;
                let hex: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
                let repo = format!("archive://{hex}");
                std::fs::create_dir_all(workspace().join("repos")).map_err(|error| error.to_string())?;
                let folder = repo_dir(&repo);
                let executable = build_zip::extract(&input, &folder)?;
                let target = target_dir(&repo);
                if let Err(error) = std::fs::create_dir_all(&target)
                    .and_then(|_| std::fs::write(built_file(&repo), "zip"))
                    .and_then(|_| build_activity::BuildActivity::record_build(&target, false))
                {
                    job.lock().unwrap().log.push(format!("Could not record the import date: {error}"));
                }
                Ok(Project {
                    name, repo, archive_exe: Some(executable), custom: true,
                    branch: None, gpu: Vec::new(), fps: None, render: None, verified: false,
                    summary: String::new(), local_exe: None,
                })
            })();
            let mut state = job.lock().unwrap();
            state.running = false;
            match result {
                Ok(project) => {
                    state.stage = "Build imported".into();
                    state.imported_project = Some(project);
                }
                Err(error) => {
                    state.stage = "Build import failed".into();
                    state.failed = true;
                    state.log.push(format!("ERROR: {error}"));
                    state.import_error = Some(error);
                }
            }
            ctx.request_repaint();
        });
    }

    fn new(cc: &eframe::CreationContext<'_>) -> App {
        theme::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let mut saved: Saved = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        if saved.render_scale <= 0.0 {
            saved.render_scale = 0.5;
        }
        let mut app = App {
            saved,
            screenshot_projects: None,
            updater: updater::Updater::default(),
            projects: Arc::new(Mutex::new(None)),
            job: Arc::new(Mutex::new(Job::default())),
            files: Vec::new(),
            files_for: String::new(),
            tools: None,
            confirm_uninstall: None,
            save_picker: None,
            play_preset_picker: None,
            pending_move: None,
            empty: false,
            finding: Arc::new(Mutex::new(None)),
            repos: Arc::new(Mutex::new(std::collections::HashMap::new())),
            show_log: false,
            log_expanded: false,
            log_export_status: None,
            slides: slides::Slides::new(),
            last_auto_check: -1.0,
            view: View::Projects,
            search: String::new(),
            add_repo: None,
            find_error: None,
            shown: None,
            shown_at: 0.0,
            copied_at: -10.0,
            modal_at: 0.0,
            modal_was_open: false,
            title_bar_done: false,
            size_checked: false,
        };
        if !app.saved.defaults_set {
            app.saved.defaults_set = true;
            app.saved.fix_8gb = true;
            app.saved.bright_shadows = true;
            app.saved.series_s = true;
            app.saved.render_scale = 0.5;
        }
        set_install_dir(&app.saved.install_dir);
        app.updater.check(&cc.egui_ctx);
        app.refresh_projects(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let job = app.job.clone();
        std::thread::spawn(move || {
            let _ = tools::detect();
            job.lock().unwrap().stage = String::new();
            ctx.request_repaint();
        });
        app
    }

    fn refresh_projects(&mut self, ctx: &egui::Context) {
        *self.projects.lock().unwrap() = None;
        let (slot, ctx) = (self.projects.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = fetch_portwatch();
            *slot.lock().unwrap() = Some(r);
            ctx.request_repaint();
        });
    }

    fn check_files(&mut self) {
        if self.files_for == self.saved.game_dir && !self.files.is_empty() {
            return;
        }
        let mut dir = normalize_dir(&self.saved.game_dir);
        // The exe in a folder under the one given: that folder is the game's.
        if !self.saved.game_dir.trim().is_empty() && !dir.join(EXE_NAME).is_file() {
            if let Some(found) = find_game_in(&dir, 2) {
                dir = found;
            }
        }
        let text = dir.display().to_string();
        if !self.saved.game_dir.trim().is_empty() && text != self.saved.game_dir {
            self.saved.game_dir = text;
        }
        self.empty = !self.saved.game_dir.is_empty()
            && std::fs::read_dir(&dir).map(|mut d| d.next().is_none()).unwrap_or(true);
        self.files = REQUIRED.iter().map(|f| (*f, !self.saved.game_dir.is_empty() && dir.join(f).is_file())).collect();
        self.files_for = self.saved.game_dir.clone();
    }

    fn files_ok(&self) -> bool {
        !self.files.is_empty() && self.files.iter().all(|f| f.1)
    }

    fn all_projects(&self) -> Vec<Project> {
        let mut v: Vec<Project> = self.saved.custom.clone();
        if let Some(Ok(list)) = &*self.projects.lock().unwrap() {
            v.extend(list.iter().cloned());
        }
        v
    }

    fn selected_project(&self) -> Option<Project> {
        let sel = self.saved.selected.as_ref()?;
        self.all_projects().into_iter().find(|p| &p.repo == sel)
    }

    /// Looks again at `p`'s install and the newest version online.
    fn check_repo(&self, ctx: &egui::Context, p: &Project, manual: bool) {
        let ticket = Arc::new(());
        {
            let mut m = self.repos.lock().unwrap();
            let e = m.entry(p.repo.clone()).or_default();
            if e.checking.is_some() {
                return;
            }
            e.checking = Some(ticket.clone());
            if manual { e.feedback = None; }
        }
        let (repos, ctx, p) = (self.repos.clone(), ctx.clone(), p.clone());
        std::thread::spawn(move || {
            let st = repo_state(&p);
            if let Some(state) = repos.lock().unwrap().get_mut(&p.repo) {
                state.finish_check(&ticket, st, manual);
            }
            ctx.request_repaint();
        });
    }

    fn state_of(&self, repo: &str) -> Option<RepoState> {
        self.repos.lock().unwrap().get(repo).cloned()
    }

    /// A newer version online than the one installed, and not put off.
    fn update_for(&self, p: &Project) -> Option<String> {
        if !p.is_git() {
            return None;
        }
        // Not while this project is being installed or updated.
        {
            let j = self.job.lock().unwrap();
            if j.running && j.repo.as_deref() == Some(p.repo.as_str()) {
                return None;
            }
        }
        let remote = self.state_of(&p.repo)?.remote?;
        // What is installed now, from disk: a check that started before the
        // last update finished must not bring the old version back.
        if !project_installed(&p.repo) {
            return None;
        }
        let built = std::fs::read_to_string(built_file(&p.repo)).ok().map(|s| s.trim().to_string());
        let newer = built.as_deref() != Some(remote.as_str());
        let later = self.saved.later.iter().any(|(r, c)| r == &p.repo && c == &remote);
        (newer && !later).then_some(remote)
    }

    fn start(&mut self, ctx: &egui::Context, p: Project, kind: Kind) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Starting".into(), repo: Some(p.repo.clone()), ..Default::default() };
        }
        // An install over an installed project is an update.
        let updating = kind == Kind::Install && project_installed(&p.repo);
        // Anchor legacy choices to the currently installed commit before a build can replace it.
        if updating && !p.runs_standalone() {
            if let Some(revision) = installed_revision(&p.repo) {
                self.saved.project_launch.entry(project_key(&p.repo)).or_default()
                    .reviewed_script_revision.get_or_insert(revision);
            }
        }
        if kind == Kind::Install || kind == Kind::Warm {
            self.show_log = true;
        }
        self.log_export_status = None;
        let script = if matches!(kind, Kind::Play(_) | Kind::Warm) {
            self.saved.refresh_repo_preset(&p)
        } else { Ok(None) };
        let settings = self.saved.project_launch.get(&project_key(&p.repo));
        let launch = self.saved.launch(&p.repo, matches!(&script, Ok(Some((_, true)))));
        if matches!(kind, Kind::Play(_) | Kind::Warm) {
            let mut job = job.lock().unwrap();
            job.log.report(concat!("Mandyhub version: ", env!("CARGO_PKG_VERSION")));
            job.log.report(format!("Project: {} ({})", p.name, p.repo));
            match script {
                Ok(Some((name, _))) => job.log.report(format!("Loaded current repository preset: {name} (profile edits retained).")),
                Err(error) => job.log.report(format!("Using saved launch switches. Could not refresh repository preset: {error}")),
                Ok(None) => {}
            }
            if let Ok(revision) = std::fs::read_to_string(built_file(&p.repo)) {
                job.log.report(format!("Installed revision: {}", revision.trim()));
            }
            if let Some(preset) = settings.and_then(|settings| settings.selected_preset.as_deref()) {
                job.log.report(format!("Selected launch preset: {preset}"));
            }
        }
        let (ctx, repos) = (ctx.clone(), self.repos.clone());
        std::thread::spawn(move || {
            let r = match &kind {
                Kind::Install => install(&job, &ctx, &p, updating),
                Kind::Play(choice) => play(&job, &ctx, &p, &launch, choice),
                Kind::Uninstall => uninstall(&job, &ctx, &p),
                Kind::Warm => project_exe(&p).and_then(|exe| {
                    let mut command = Command::new(exe);
                    victory_env(&mut command, &p, &launch);
                    warm(&job, &ctx, &command, &launch)
                }),
            };
            {
                let mut j = job.lock().unwrap();
                j.running = false;
                j.pid = None;
                j.stoppable = false;
                if matches!(kind, Kind::Play(_) | Kind::Warm) {
                    let result = match &r {
                        _ if j.stopped => "Stopped by user".to_owned(),
                        Ok(()) => "Completed successfully".to_owned(),
                        Err(error) => format!("Failed: {error}"),
                    };
                    j.log.report(format!("\nJob result: {result}"));
                }
                match r {
                    _ if j.stopped => j.stage = "Stopped".into(),
                    Ok(()) => {
                        if kind == Kind::Install && updating {
                            j.updated_project = Some(p.clone());
                        }
                        if kind == Kind::Uninstall && p.archive_exe.is_some() {
                            j.removed_archive = Some(p.repo.clone());
                        }
                        j.stage = match kind {
                            Kind::Install if updating => "Updated: ready to play".into(),
                            Kind::Install => "Installed: ready to play".into(),
                            Kind::Play(_) => if p.runs_standalone() { "Program closed".into() } else { "Game closed".into() },
                            Kind::Uninstall => "Uninstalled".into(),
                            Kind::Warm => "Shaders warmed up: ready to play".into(),
                        }
                    }
                    Err(e) => {
                        j.failed = true;
                        j.stage = match kind {
                            Kind::Install if updating => "Update failed: the old version is still installed (see the log)".into(),
                            Kind::Install => "Install failed (see the log)".into(),
                            Kind::Warm => "Warm-up failed (see the log)".into(),
                            _ => "Failed".into(),
                        };
                        j.log.push(format!("ERROR: {e}"));
                    }
                }
            }
            if matches!(kind, Kind::Install | Kind::Uninstall) {
                let st = repo_state(&p);
                repos.lock().unwrap().insert(p.repo.clone(), st);
            } else if matches!(kind, Kind::Play(_)) && p.local_exe.is_none() {
                if let Some(state) = repos.lock().unwrap().get_mut(&p.repo) {
                    state.activity = build_activity::BuildActivity::read(&target_dir(&p.repo));
                }
            }
            ctx.request_repaint();
        });
    }
}


fn stage(job: &Arc<Mutex<Job>>, ctx: &egui::Context, s: &str) {
    let mut j = job.lock().unwrap();
    j.stage = s.to_string();
    j.log.push(format!("== {s}"));
    ctx.request_repaint();
}

/// Downloads `p` (or its newest version) and builds it, once; it stays
/// installed until the player updates it.
fn install(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project, updating: bool) -> Result<(), String> {
    let ws = workspace();
    std::fs::create_dir_all(ws.join("repos")).map_err(|e| format!("cannot make {}: {e}", ws.display()))?;
    let dir = repo_dir(&p.repo);
    let url = p.clone_url();
    if dir.join(".git").is_dir() {
        stage(job, ctx, "Downloading the new version");
        let refspec = p.branch.clone().unwrap_or_else(|| "HEAD".into());
        run(job, ctx, tools::git().args(["fetch", "--depth", "1", "origin", &refspec]).current_dir(&dir))?;
        run(job, ctx, tools::git().args(["reset", "--hard", "FETCH_HEAD"]).current_dir(&dir))?;
    } else {
        stage(job, ctx, "Downloading the project");
        let mut c = tools::git();
        c.args(["clone", "--depth", "1"]);
        if let Some(b) = &p.branch {
            c.args(["--branch", b]);
        }
        c.arg("--").arg(&url).arg(&dir);
        run(job, ctx, &mut c)?;
    }
    stage(job, ctx, "Building (the first time takes several minutes)");
    let target = target_dir(&p.repo);
    build_project(job, ctx, p, &dir, &target)?;
    let mut c = tools::git();
    c.args(["rev-parse", "HEAD"]).current_dir(&dir);
    hide_window(&mut c);
    if let Ok(out) = c.output() {
        if out.status.success() {
            let record = std::fs::write(built_file(&p.repo), String::from_utf8_lossy(&out.stdout).trim())
                .and_then(|_| build_activity::BuildActivity::record_build(&target, updating));
            if let Err(error) = record {
                job.lock().unwrap().log.push(format!("Could not record the update: {error}"));
            }
        }
    }
    Ok(())
}

fn build_project(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project, dir: &Path, target: &Path) -> Result<(), String> {
    let workspace = cargo_workspace(dir)?;
    let metadata = rust_project::metadata(&workspace)?;
    let victory = !p.custom || rust_project::is_victory(&workspace, &metadata);
    let profile = if victory { detect_build_profile(&workspace) } else { "release" };
    let has_lockfile = workspace.join("Cargo.lock").is_file();
    let mut cargo = tools::cargo();
    cargo.args(["build", "--profile", profile, "--target-dir"]);
    cargo.arg(target);
    if has_lockfile {
        cargo.arg("--locked");
    }
    cargo.current_dir(&workspace);
    if victory {
        cargo.args(["-p", "victory"]);
        // Scarlett's diag profile and Cargo's diagnostics feature are separate switches.
        if profile == "diag"
            && metadata["packages"].as_array().is_some_and(|packages| packages.iter().any(|pkg|
                pkg["name"] == "victory" && pkg["features"].get("diagnostics").is_some()))
            && std::fs::read_to_string(workspace.join("scripts/play-scarlett.ps1")).is_ok_and(|script|
                script.contains("cargo build --profile diag -p victory --features diagnostics"))
        {
            cargo.args(["--features", "diagnostics"]);
            job.lock().unwrap().log.push("Building with the Scarlett launcher's diagnostics feature.".into());
        }
        run(job, ctx, &mut cargo)?;
        let executable = target.join(profile).join("victory.exe");
        if !executable.is_file() {
            return Err(format!("the build made no {}", executable.display()));
        }
        let marker = rust_install_file(&p.repo);
        if marker.exists() {
            std::fs::remove_file(marker).map_err(|error| error.to_string())?;
        }
    } else {
        cargo.args(["--workspace", "--message-format=json-render-diagnostics"]);
        let artifacts = Arc::new(Mutex::new(Vec::new()));
        let captured = artifacts.clone();
        run_with_output(job, ctx, &mut cargo, |_| {}, move |line| {
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else { return Some(line) };
            if message["reason"] == "compiler-artifact" && message["executable"].is_string() {
                captured.lock().unwrap().push(message);
                return None;
            }
            message["message"]["rendered"].as_str().map(str::to_owned)
        })?;
        let artifacts = artifacts.lock().unwrap();
        let executable = rust_project::select_executable(&metadata, &artifacts)
            .map(|exe| Path::new(exe).strip_prefix(target).map(Path::to_owned))
            .transpose().map_err(|_| "Cargo reported an executable outside the install folder")?;
        let marker = rust_install_file(&p.repo);
        let temporary = marker.with_extension("tmp");
        let record = serde_json::to_vec(&RustInstall { executable }).map_err(|error| error.to_string())?;
        std::fs::write(&temporary, record).and_then(|_| std::fs::rename(&temporary, &marker))
            .map_err(|error| format!("Could not record the installed Rust project: {error}"))?;
    }
    Ok(())
}

/// Deletes `p`'s download and build (and its shader cache); its saves stay.
fn uninstall(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project) -> Result<(), String> {
    if p.local_exe.is_some() {
        return Err("this build was not installed by Mandyhub".into());
    }
    stage(job, ctx, "Uninstalling");
    let ws = workspace();
    for dir in [repo_dir(&p.repo), target_dir(&p.repo), ws.join("cache").join(slug(&p.repo))] {
        if dir.exists() {
            job.lock().unwrap().log.push(format!("removing {}", dir.display()));
            ctx.request_repaint();
            std::fs::remove_dir_all(&dir).map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

/// The victory.exe `p` runs with: its build, or a build already on this PC.
fn project_exe(p: &Project) -> Result<PathBuf, String> {
    let exe = p.archive_program().or_else(|| p.local_exe.as_ref().map(PathBuf::from))
        .unwrap_or_else(|| built_exe(&p.repo));
    if exe.is_file() {
        Ok(exe)
    } else {
        Err("this project is not installed yet: press Install".into())
    }
}

/// The environment victory.exe runs with, the same for the warm-up and for
/// Play: the project's own env vars (from its scripts/play.ps1) first, then
/// the mode, its data and cache folders, and the player's switches.
fn victory_env(c: &mut Command, p: &Project, l: &Launch) {
    let root = project_script_root(p);
    if let Some(program) = p.archive_program() {
        if let Some(directory) = program.parent() {
            c.current_dir(directory);
            if !l.use_script_defaults {
                for (key, value) in build_zip::launch_defaults(directory) {
                    c.env(key, value);
                }
            }
        }
    } else if p.is_git() && root.is_dir() {
        c.current_dir(&root);
    } else if let Some(parent) = p.local_exe.as_deref().and_then(|exe| Path::new(exe).parent()).filter(|p| p.is_dir()) {
        c.current_dir(parent);
    }
    if l.scarlett_shader_pass {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().to_ascii_uppercase().starts_with("VICTORY_") {
                c.env_remove(key);
            }
        }
    }
    c.env("VICTORY_GAME_DIR", &l.game_dir);
    if !l.use_script_defaults {
        for (k, v) in detect_launch_env(&root) {
            c.env(&k, &v);
        }
    }
    if l.series_s {
        c.env("VICTORY_OWN_UPSCALER", "1");
        c.env("VICTORY_RENDER_SCALE", format!("{:.2}", l.scale));
    }
    if let Some(size) = l.output.dimensions() {
        c.env("VICTORY_OUTPUT", size);
    }
    let ws = workspace();
    let data = ws.join("data").join(slug(&p.repo));
    c.env("VICTORY_DATA_DIR", &data);
    c.env("VICTORY_CACHE_DIR", ws.join("cache").join(slug(&p.repo)));
    for line in l.extra.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if k.starts_with("VICTORY_") {
                c.env(k, v.trim());
            }
        }
    }
}

fn child_command(base: &Command) -> Command {
    let mut command = Command::new(base.get_program());
    if let Some(dir) = base.get_current_dir() { command.current_dir(dir); }
    for (key, value) in base.get_envs() {
        if let Some(value) = value { command.env(key, value); } else { command.env_remove(key); }
    }
    command
}

fn victory_help(base: &Command, subcommand: &str) -> Option<String> {
    let mut command = child_command(base);
    command.args([subcommand, "--help"]);
    hide_window(&mut command);
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Some forks open their pipeline bookkeeping files even before the first game run.
fn prepare_pipeline_cache(command: &Command) -> Result<(), String> {
    let Some(cache) = launch_folder(command, "VICTORY_CACHE_DIR") else {
        return Ok(());
    };
    let folder = cache.join("pipelines");
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create the pipeline cache folder {}: {error}", folder.display()))
}

fn launch_folder(command: &Command, key: &str) -> Option<PathBuf> {
    let value = command.get_envs().find(|(name, _)| *name == key)?.1?;
    if value.is_empty() { return None; }
    Some(command.get_current_dir().unwrap_or(Path::new(".")).join(value))
}

fn prepare_launch_folders(command: &Command) -> Result<(), String> {
    for key in ["VICTORY_DATA_DIR", "VICTORY_CACHE_DIR"] {
        if let Some(folder) = launch_folder(command, key) {
            std::fs::create_dir_all(&folder).map_err(|e| format!("Could not create {key} at {}: {e}", folder.display()))?;
        }
    }
    Ok(())
}

/// Reuse Scarlett's pretranslation only when the executable, archive and effective
/// translation settings still match and the shader cache still contains files.
fn shader_stamp(base: &Command, archive: &Path) -> Option<(PathBuf, String)> {
    let cache = launch_folder(base, "VICTORY_CACHE_DIR")?;
    let mut inputs = Vec::new();
    for path in [Path::new(base.get_program()), archive] {
        let info = path.metadata().ok()?;
        inputs.push(format!("{path:?} {:?} {}", info.modified().ok()?, info.len()));
    }
    for (key, value) in base.get_envs() {
        if key != "VICTORY_DATA_DIR" {
            inputs.push(format!("{key:?}={value:?}"));
        }
    }
    Some((cache.join("mandyhub-shaders-ahead.txt"), inputs.join("\n")))
}

#[cfg(test)]
#[test]
fn pipeline_warm_up_creates_the_configured_cache_on_first_use() {
    let root = std::env::temp_dir().join(format!("mandyhub-pipeline-cache-{}", std::process::id()));
    let mut command = Command::new("victory.exe");
    command.current_dir(&root).env("VICTORY_CACHE_DIR", "custom cache");
    prepare_pipeline_cache(&command).unwrap();
    prepare_pipeline_cache(&command).unwrap();
    assert!(root.join("custom cache").join("pipelines").is_dir());
    std::fs::remove_dir(root.join("custom cache").join("pipelines")).unwrap();
    std::fs::remove_dir(root.join("custom cache")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

/// Fills the same shader cache as Play, using the commands the installed fork exposes.
fn warm(job: &Arc<Mutex<Job>>, ctx: &egui::Context, base: &Command, l: &Launch) -> Result<(), String> {
    stage(job, ctx, "Warming up shaders (checking build support)");
    prepare_launch_folders(base)?;
    let files: Vec<PathBuf> = WARM_FILES.iter().map(|f| l.game_dir.join(f)).filter(|f| f.is_file()).collect();
    if files.is_empty() {
        return Err("the game files were not found: choose their folder in Settings".into());
    }
    let translate = if l.scarlett_shader_pass { None } else { victory_help(base, "translate") };
    let supports_flag = |help: &str, flag: &str| help.split_whitespace().any(|word| word == flag);
    let translate_cache = translate.as_deref().filter(|help| supports_flag(help, "--cache"));
    let mut commands = Vec::new();
    let mut shaders = child_command(base);
    let mut stamp = None;
    if l.scarlett_shader_pass {
        let rpf = l.game_dir.join("shaders.rpf");
        if !rpf.is_file() { return Err(format!("Shader archive missing: {}", rpf.display())); }
        stamp = shader_stamp(base, &rpf);
        shaders.arg("shaders").arg(rpf);
    } else if let Some(help) = translate_cache {
        shaders.args(["translate", "--cache"]);
        if l.pipelines && supports_flag(help, "--pipelines") {
            shaders.arg("--pipelines");
        }
        shaders.args(&files);
    } else if victory_help(base, "shaders").is_some() {
        shaders.arg("shaders").args(&files);
    } else {
        return Err("This build offers neither translate --cache nor shaders for shader warm-up.".into());
    }
    let combined_pipelines = shaders.get_args().any(|arg| arg == "--pipelines");
    let cached = stamp.as_ref().and_then(|(path, expected)| {
        let old = std::fs::read_to_string(path).ok()?;
        let (inputs, summary) = old.rsplit_once("\n\n")?;
        (inputs == expected && path.with_file_name("shaders").read_dir().is_ok_and(|mut entries| entries.next().is_some_and(|entry| entry.is_ok())))
            .then(|| summary.to_owned())
    });
    let mut status = if let Some(summary) = cached {
        job.lock().unwrap().log.report(format!("Shader warm-up: cache current for this build and settings. Previous pass: {summary}"));
        format!("Shader cache current · {summary}")
    } else {
        commands.push(shaders);
        "Shader warm-up completed".to_owned()
    };
    if l.pipelines && !combined_pipelines {
        if let Some(help) = victory_help(base, "warm") {
            let mut pipelines = child_command(base);
            pipelines.arg("warm");
            if supports_flag(&help, "--used") { pipelines.arg("--used"); }
            commands.push(pipelines);
        } else {
            status.push_str(" · pipeline warm-up unavailable");
            job.lock().unwrap().log.report("Pipeline warm-up skipped: the build has no supported pipeline command.");
        }
    }
    if !l.pipelines {
        job.lock().unwrap().log.report("Pipeline warm-up skipped: disabled in Settings.");
    }
    let warming_pipelines = combined_pipelines || commands.iter().any(|cmd| cmd.get_args().next().is_some_and(|arg| arg == "warm"));
    let mut pipeline_status = "pipeline warm-up completed".to_owned();
    for mut command in commands {
        let separate_pipelines = command.get_args().next().is_some_and(|arg| arg == "warm");
        let label = if separate_pipelines {
            "Pipeline warm-up"
        } else if command.get_args().any(|arg| arg == "--pipelines") {
            "Shader and pipeline warm-up"
        } else {
            "Shader warm-up"
        };
        stage(job, ctx, match label {
            "Pipeline warm-up" => "Warming up pipelines",
            "Shader and pipeline warm-up" => "Warming up shaders and pipelines",
            _ => "Warming up shaders",
        });
        job.lock().unwrap().log.command(label, &command, true);
        prepare_pipeline_cache(&command)?;
        let shader_totals = Arc::new(Mutex::new(None));
        let captured = shader_totals.clone();
        let pipeline_summary = Arc::new(Mutex::new(None));
        let pipeline_output = pipeline_summary.clone();
        let result = run_with_output(job, ctx, &mut command, |pid| {
            let mut j = job.lock().unwrap();
            j.pid = Some(pid);
            j.stoppable = true;
        }, move |line| {
            if separate_pipelines && line.contains(" pipelines known, ") && line.contains(" to make, ") {
                *pipeline_output.lock().unwrap() = Some(line.clone());
            }
            if let Some((count, rest)) = line.split_once(" shaders in the cache, ") {
                if let Some((failed, _)) = rest.split_once(" do not translate,") {
                    if let (Ok(count), Ok(failed)) = (count.trim().parse::<usize>(), failed.parse::<usize>()) {
                        *captured.lock().unwrap() = Some((count, failed));
                    }
                }
            }
            Some(line)
        });
        let stopped = {
            let mut j = job.lock().unwrap();
            j.stoppable = false;
            j.pid = None;
            j.stopped
        };
        if stopped {
            job.lock().unwrap().log.report(format!("{label} result: stopped by user."));
            return Err("stopped".into());
        }
        job.lock().unwrap().log.report(match &result {
            Ok(()) => format!("{label} result: completed successfully."),
            Err(error) => format!("{label} result: failed ({error})."),
        });
        result?;
        if let Some(summary) = pipeline_summary.lock().unwrap().as_deref() {
            job.lock().unwrap().log.report(format!("Pipeline inventory: {summary}"));
            if summary.starts_with("0 pipelines known,") {
                pipeline_status = "no recorded pipelines to warm".into();
            } else if summary.contains("; 0 to make,") {
                pipeline_status = "no pending pipelines".into();
            }
        }
        if let Some((count, failed)) = *shader_totals.lock().unwrap() {
            job.lock().unwrap().log.report(format!("Shader totals: {count} cached, {failed} could not translate."));
            if count == 0 { return Err("The shader command completed with zero shaders cached".into()); }
            if failed > 0 {
                status = format!("{count} shaders cached · {failed} could not translate");
            }
        }
        if !separate_pipelines {
            if let Some((path, value)) = &stamp {
                if let Err(e) = std::fs::write(path, format!("{value}\n\n{status}")) {
                    job.lock().unwrap().log.report(format!("Could not remember shader warm-up: {e}"));
                }
            }
        }
    }
    if warming_pipelines { status.push_str(&format!(" · {pipeline_status}")); }
    job.lock().unwrap().warm_status = Some(status);
    Ok(())
}

/// Starts the installed build of `p`, warming its shaders first when the player asked for that.
fn game_command(exe: &Path, p: &Project, l: &Launch) -> Command {
    let mut c = Command::new(exe);
    c.arg("run").arg(l.game_dir.join(EXE_NAME)).arg("--execute");
    let explicit_console = l.args.iter().take_while(|arg| arg.as_str() != "--")
        .any(|arg| arg == "--console" || arg.starts_with("--console="));
    if !explicit_console && (l.series_s || l.output != OutputResolution::Default) {
        c.args(["--console", if l.series_s { "series-s" } else { "series-x" }]);
    }
    c.args(&l.args);
    victory_env(&mut c, p, l);
    c
}

fn play(job: &Arc<Mutex<Job>>, ctx: &egui::Context, p: &Project, l: &Launch, choice: &saves::LaunchSave) -> Result<(), String> {
    if p.runs_standalone() {
        let (exe, dir) = if let Some(exe) = p.archive_program() {
            let dir = exe.parent().ok_or("The archive executable has no parent folder.")?.to_owned();
            (exe, dir)
        } else {
            let exe = rust_install(&p.repo).and_then(|install| install.executable)
                .ok_or("This Rust workspace has no runnable binary.")?;
            (target_dir(&p.repo).join(exe), cargo_workspace(&repo_dir(&p.repo))?)
        };
        let mut command = Command::new(exe);
        command.current_dir(dir).args(&l.program_args);
        let mut j = job.lock().unwrap();
        j.log.report("VIctory shader warm-up skipped: this profile launches a standalone program.");
        j.log.command("Program launch", &command, false);
        drop(j);
        stage(job, ctx, "Playing");
        return run_with(job, ctx, &mut command, |_| {
            if let Err(error) = build_activity::BuildActivity::record_run(&target_dir(&p.repo)) {
                job.lock().unwrap().log.push(format!("Could not record the launch: {error}"));
            }
        });
    }
    let exe = project_exe(p)?;
    job.lock().unwrap().log.report(format!(
        "Warm-up before Play: {}. Pipeline warm-up requested: {}.",
        if l.warm_first { "on" } else { "off" },
        if l.warm_first && l.pipelines { "yes" } else { "no" },
    ));
    let mut c = game_command(&exe, p, l);
    let project_data = workspace().join("data").join(slug(&p.repo));
    let data = saves::prepare(&project_data, choice).map_err(|error| format!("Could not prepare saves: {error}"))?;
    if data != project_data {
        c.env("VICTORY_DATA_DIR", data);
    }
    prepare_launch_folders(&c)?;
    if l.warm_first {
        if let Err(e) = warm(job, ctx, &c, l) {
            let mut job = job.lock().unwrap();
            if job.stopped { return Err("stopped".into()); }
            job.warm_status = Some("Warm-up failed · see log".into());
            job.log.report(format!("Warm-up failed: {e}. Continued to game launch."));
            job.log.push(format!("Warm-up failed: {e}. Starting the game anyway."));
        }
    } else {
        job.lock().unwrap().warm_status = Some("Warm-up off in Settings".into());
    }
    stage(job, ctx, "Playing");
    {
        let mut job = job.lock().unwrap();
        job.log.command("Game launch", &c, true);
        let args: Vec<_> = c.get_args().take_while(|arg| *arg != "--").collect();
        let console = args.iter().enumerate().find_map(|(index, arg)| {
            let arg = arg.to_str()?;
            arg.strip_prefix("--console=").or_else(|| {
                (arg == "--console").then(|| args.get(index + 1).and_then(|mode| mode.to_str()).unwrap_or("(missing value)"))
            })
        });
        job.log.report(format!("Configured graphics profile: {}", if l.series_s { "Series S" } else { "Series X" }));
        job.log.report(format!("Console passed to VIctory: {}", console.unwrap_or("build default (no --console argument)")));
        job.log.report(format!("Configured render scale: {:.0}%", l.scale * 100.0));
        job.log.report(format!("Configured output resolution: {}", l.output.label()));
        job.log.report(format!("Borderless fullscreen: {}", if l.fullscreen { "on" } else { "off" }));
        let save_choice = match choice {
            saves::LaunchSave::Continue => "Continue",
            saves::LaunchSave::Fresh => "Start fresh",
            saves::LaunchSave::Copy(_) => "Copy existing save or checkpoint",
        };
        job.log.report(format!("Save choice: {save_choice}"));
        if let saves::LaunchSave::Copy(source) = choice {
            job.log.report(format!("Save source: {source:?}"));
        }
    }
    let fullscreen = l.fullscreen;
    run_with(job, ctx, &mut c, |pid| {
        if p.local_exe.is_none() {
            if let Err(error) = build_activity::BuildActivity::record_run(&target_dir(&p.repo)) {
                job.lock().unwrap().log.push(format!("Could not record the launch: {error}"));
            }
        }
        if fullscreen {
            fullscreen::keep_borderless(pid);
        }
    })
}

/// Runs `c`, its output lines into the job's log.
fn run(job: &Arc<Mutex<Job>>, ctx: &egui::Context, c: &mut Command) -> Result<(), String> {
    run_with(job, ctx, c, |_| {})
}

/// [`run`], telling `spawned` the process id once it started.
fn run_with(job: &Arc<Mutex<Job>>, ctx: &egui::Context, c: &mut Command, spawned: impl FnOnce(u32)) -> Result<(), String> {
    run_with_output(job, ctx, c, spawned, Some)
}

fn run_with_output(
    job: &Arc<Mutex<Job>>,
    ctx: &egui::Context,
    c: &mut Command,
    spawned: impl FnOnce(u32),
    filter: impl Fn(String) -> Option<String> + Send + Sync + 'static,
) -> Result<(), String> {
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    hide_window(c);
    let mut child = c.spawn().map_err(|e| format!("could not start {:?}: {e}", c.get_program()))?;
    spawned(child.id());
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let filter = Arc::new(filter);
    let pipe = |r: Box<dyn std::io::Read + Send>, filter_stdout: bool| {
        let (job, ctx) = (job.clone(), ctx.clone());
        let filter = filter.clone();
        std::thread::spawn(move || {
            for bytes in BufReader::new(r).split(b'\n').map_while(Result::ok) {
                let line = String::from_utf8_lossy(bytes.strip_suffix(b"\r").unwrap_or(&bytes)).into_owned();
                let Some(line) = (if filter_stdout { filter(line) } else { Some(line) }) else { continue };
                let mut j = job.lock().unwrap();
                j.log.push(line);
                drop(j);
                ctx.request_repaint();
            }
        })
    };
    let a = out.map(|o| pipe(Box::new(o), true));
    let b = err.map(|e| pipe(Box::new(e), false));
    let status = child.wait().map_err(|e| e.to_string())?;
    for t in [a, b].into_iter().flatten() {
        let _ = t.join();
    }
    status.success().then_some(()).ok_or_else(|| format!("{:?} ended with {status}", c.get_program()))
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.saved);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.updater.poll();
        let (imported, removed, updated) = {
            let mut job = self.job.lock().unwrap();
            (job.imported_project.take(), job.removed_archive.take(), job.updated_project.take())
        };
        if let Some(project) = updated {
            if self.view == View::Projects && self.saved.selected.as_ref() == Some(&project.repo)
                && self.screenshot_projects.is_none() && self.add_repo.is_none() && self.confirm_uninstall.is_none()
                && self.pending_move.is_none() && self.play_preset_picker.is_none() && self.save_picker.is_none()
            {
                let launch = self.saved.project_launch.get(&project_key(&project.repo));
                self.play_preset_picker = ui::PlayPresetPicker::after_project_update(&project, &self.saved.launch_presets, launch);
            }
        }
        if let Some(project) = imported {
            self.saved.selected = Some(project.repo.clone());
            self.saved.custom.insert(0, project);
            self.view = View::Projects;
        }
        if let Some(repo) = removed {
            self.saved.custom.retain(|project| project.repo != repo);
            if self.saved.selected.as_deref() == Some(&repo) {
                self.saved.selected = None;
            }
        }
        if !self.title_bar_done {
            self.title_bar_done = fullscreen::dark_title_bar(std::process::id(), theme::BG, theme::WHITE);
        }
        // A size remembered from a minimized window can come back tiny: then the normal size.
        if !self.size_checked {
            let (rect, minimized) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().minimized));
            if let Some(r) = rect {
                if minimized != Some(true) && (r.width() < 960.0 || r.height() < 620.0) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
                }
                self.size_checked = true;
            }
        }
        self.draw(ctx);
        if self.updater.restart_requested && !self.job.lock().unwrap().running {
            if let updater::State::Downloaded(update) = &self.updater.state {
                self.updater.restart_requested = false;
                if let Some(storage) = frame.storage_mut() {
                    eframe::set_value(storage, eframe::APP_KEY, &self.saved);
                    storage.flush();
                }
                match updater::restart_with(update) {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => self.updater.restart_error = Some(error),
                }
            }
        } else if matches!(self.updater.state, updater::State::Downloaded(_)) {
            self.updater.restart_requested = false;
        }
    }
}

impl App {
    /// Installs what is missing with winget, then looks again.
    fn start_tools(&mut self, ctx: &egui::Context) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Installing tools (Windows will ask for permission)".into(), ..Default::default() };
        }
        self.show_log = true;
        self.log_export_status = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let have = tools::detect();
            let result = if !tools::has_winget() {
                Err("winget (the Windows package manager) is not on this PC. Install them by hand: git-scm.com, rustup.rs and the Visual Studio Build Tools (Desktop development with C++).".to_string())
            } else {
                let mut r = Ok(());
                for (what, args) in tools::install_steps(&have) {
                    stage(&job, &ctx, &format!("Installing {what} (this can take a while)"));
                    let mut c = Command::new("winget");
                    c.args(&args);
                    if let Err(e) = run(&job, &ctx, &mut c) {
                        r = Err(format!("{what}: {e}"));
                        break;
                    }
                }
                r
            };
            let now = tools::detect();
            let mut j = job.lock().unwrap();
            j.running = false;
            match result {
                Ok(()) if now.ready() => j.stage = "Tools installed: ready to install projects".into(),
                Ok(()) => {
                    j.failed = true;
                    j.stage = format!("Still missing: {}. A restart of Windows can help.", now.missing().join(", "));
                }
                Err(e) => {
                    j.failed = true;
                    j.stage = "Tools not installed (see the log)".into();
                    j.log.push(format!("ERROR: {e}"));
                }
            }
            ctx.request_repaint();
        });
    }

    /// A new install folder: asks first if projects are installed in the old one.
    fn choose_install_dir(&mut self, new: PathBuf) {
        let old = workspace();
        if new == old {
            return;
        }
        if has_projects(&old) {
            self.pending_move = Some((old, new));
        } else {
            self.apply_install_dir(&new);
        }
    }

    fn apply_install_dir(&mut self, new: &Path) {
        self.saved.install_dir = if new == default_workspace() { String::new() } else { new.display().to_string() };
        set_install_dir(&self.saved.install_dir);
        // What is installed where is looked at again.
        self.repos.lock().unwrap().clear();
        self.last_auto_check = -1.0;
    }

    /// Moves every installed project from `old` to `new`, then uses `new`.
    fn start_move(&mut self, ctx: &egui::Context, old: PathBuf, new: PathBuf) {
        let job = self.job.clone();
        {
            let mut j = job.lock().unwrap();
            if j.running {
                return;
            }
            *j = Job { running: true, stage: "Moving installed projects".into(), ..Default::default() };
        }
        let (ctx, repos) = (ctx.clone(), self.repos.clone());
        std::thread::spawn(move || {
            let log = |line: String| {
                job.lock().unwrap().log.push(line);
                ctx.request_repaint();
            };
            let mut result = Ok(());
            for part in PROJECT_PARTS {
                log(format!("moving {}", old.join(part).display()));
                if let Err(e) = move_dir(&old.join(part), &new.join(part), &log) {
                    result = Err(format!("could not move {}: {e}", old.join(part).display()));
                    break;
                }
            }
            let mut j = job.lock().unwrap();
            j.running = false;
            match result {
                Ok(()) => j.stage = "Moved: projects are in the new folder".into(),
                Err(e) => {
                    j.failed = true;
                    j.stage = "Failed".into();
                    j.log.push(format!("ERROR: {e}"));
                }
            }
            repos.lock().unwrap().clear();
            ctx.request_repaint();
        });
    }

    /// At start and every 15 minutes: every installed project is checked for
    /// a newer version online (no download; `git ls-remote`).
    fn auto_check(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let listed = self.projects.lock().unwrap().is_some();
        let due = if self.last_auto_check < 0.0 { listed || now > 10.0 } else { now - self.last_auto_check > AUTO_CHECK_SECS };
        if !due {
            ctx.request_repaint_after(std::time::Duration::from_secs(30));
            return;
        }
        self.last_auto_check = now;
        for p in self.all_projects() {
            if p.is_git() && project_installed(&p.repo) {
                self.check_repo(ctx, &p, false);
            }
        }
    }
}
