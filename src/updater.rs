use eframe::egui;
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

pub const REPOSITORY: &str = "TheGameOnAddict/mandyhub";
pub const RELEASES: &str = "https://github.com/TheGameOnAddict/mandyhub/releases";
const ASSET: &str = "Mandyhub.exe";
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
pub struct Release {
    pub tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading,
    Downloaded(PathBuf),
    Failed(String),
}

#[derive(Default)]
pub struct Updater {
    pub state: State,
    result: Option<Receiver<Result<State, String>>>,
}

impl Updater {
    pub fn poll(&mut self) {
        let Some(result) = &self.result else { return };
        self.state = match result.try_recv() {
            Ok(Ok(state)) => state,
            Ok(Err(error)) => State::Failed(error),
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                State::Failed("Update task stopped unexpectedly.".into())
            }
        };
        self.result = None;
    }

    pub fn check(&mut self, ctx: &egui::Context) {
        self.start(ctx, State::Checking, check_release);
    }

    pub fn download(&mut self, ctx: &egui::Context) {
        let State::Available(release) = std::mem::take(&mut self.state) else {
            return;
        };
        self.start(ctx, State::Downloading, move || {
            download_release(&release).map(State::Downloaded)
        });
    }

    fn start(
        &mut self,
        ctx: &egui::Context,
        state: State,
        task: impl FnOnce() -> Result<State, String> + Send + 'static,
    ) {
        self.state = state;
        let (send, receive) = mpsc::channel();
        self.result = Some(receive);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = send.send(task());
            ctx.request_repaint();
        });
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("Mandyhub/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn version(tag: &str) -> Option<[u32; 3]> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let mut version = [0; 3];
    for number in &mut version {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(version)
}

fn check_release() -> Result<State, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
    let response = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call();
    let release: Release = match response {
        Ok(response) => response.into_json().map_err(|error| error.to_string())?,
        Err(ureq::Error::Status(404, _)) => return Ok(State::Current),
        Err(error) => return Err(error.to_string()),
    };
    let available =
        version(&release.tag_name).ok_or("The release tag is not a supported version.")?;
    let current = version(env!("CARGO_PKG_VERSION")).ok_or("The installed version is invalid.")?;
    if available > current {
        Ok(State::Available(release))
    } else {
        Ok(State::Current)
    }
}

fn download_release(release: &Release) -> Result<PathBuf, String> {
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == ASSET)
        .ok_or("This release has no Mandyhub.exe download.")?;
    let digest = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .ok_or("GitHub did not provide a SHA-256 checksum for the executable.")?;
    if asset.size == 0 || asset.size > MAX_BYTES {
        return Err("The executable size is outside the supported limit.".into());
    }
    let url = format!("{RELEASES}/download/{}/{ASSET}", release.tag_name);
    let response = agent()
        .get(&url)
        .call()
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(asset.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if !matches_asset(&bytes, asset.size, digest) {
        return Err("The download did not match GitHub’s size and SHA-256 checksum.".into());
    }
    let folder = crate::default_workspace()
        .join("updates")
        .join(&release.tag_name);
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let temporary = folder.join(format!("{ASSET}.{}.tmp", std::process::id()));
    let executable = folder.join(ASSET);
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&temporary, &executable) {
        let _ = fs::remove_file(temporary);
        return Err(error.to_string());
    }
    Ok(executable)
}

fn matches_asset(bytes: &[u8], size: u64, sha256: &str) -> bool {
    let actual = ring::digest::digest(&ring::digest::SHA256, bytes);
    let actual: String = actual
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    bytes.len() as u64 == size && actual.eq_ignore_ascii_case(sha256)
}

#[cfg(test)]
#[test]
fn compares_stable_versions_numerically() {
    assert!(version("v0.10.0") > version("0.2.0"));
    assert_eq!(version("v0.2.0"), version("0.2.0"));
    assert!(version("v0.3.0-beta").is_none());
    assert!(version("v0.3.0/other").is_none());
}

#[cfg(test)]
#[test]
fn rejects_changed_or_truncated_downloads() {
    let known = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert!(matches_asset(b"abc", 3, known));
    assert!(!matches_asset(b"abd", 3, known));
    assert!(!matches_asset(b"abc", 4, known));
}
