use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const VISIBLE_LINES: usize = 4_000;
const ARCHIVE_BATCH: usize = 1_000;

#[derive(Default)]
pub struct ActivityLog {
    report: Vec<String>,
    recent: Vec<String>,
    archived: usize,
    archive: Option<LogArchive>,
    archive_failed: bool,
}

impl ActivityLog {
    pub fn is_empty(&self) -> bool {
        self.report.is_empty() && self.recent.is_empty()
    }

    pub fn report(&mut self, line: impl Into<String>) {
        self.report.push(line.into());
    }

    pub fn command(&mut self, label: &str, command: &Command, include_victory_switches: bool) {
        self.report(format!("\n[{label}]"));
        self.report(format!("Executable: {:?}", command.get_program()));
        self.report(format!(
            "Arguments: {:?}",
            command.get_args().collect::<Vec<_>>()
        ));
        if let Some(folder) = command
            .get_current_dir()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
        {
            self.report(format!("Working directory: {folder:?}"));
        }
        if include_victory_switches {
            let mut switches: BTreeMap<_, _> = std::env::vars_os()
                .filter_map(|(key, value)| victory_key(&key).map(|key| (key, value)))
                .collect();
            for (key, value) in command.get_envs() {
                if let Some(key) = victory_key(key) {
                    if let Some(value) = value {
                        switches.insert(key, value.to_owned());
                    } else {
                        switches.remove(&key);
                    }
                }
            }
            self.report("Effective VICTORY_* switches:");
            for (key, value) in switches {
                self.report(format!("  {key}={value:?}"));
            }
        }
    }

    pub fn push(&mut self, line: String) {
        self.recent.push(line);
        if self.recent.len() <= VISIBLE_LINES {
            return;
        }
        if !self.archive_failed && self.archive_batch().is_err() {
            self.archive_failed = true;
            self.archive = None;
        }
        self.recent.drain(..ARCHIVE_BATCH);
        self.archived += ARCHIVE_BATCH;
    }

    fn archive_batch(&mut self) -> io::Result<()> {
        if self.archive.is_none() {
            self.archive = Some(LogArchive::new()?);
        }
        let archive = self.archive.as_mut().expect("archive is open");
        let mut output = BufWriter::new(archive.file.as_mut().expect("archive file is open"));
        for line in &self.recent[..ARCHIVE_BATCH] {
            writeln!(output, "{line}")?;
        }
        output.flush()
    }

    pub fn preview(&self) -> String {
        let mut output = self.header();
        if self.archived > 0 {
            let message = if self.archive_failed {
                "Earlier output is unavailable because the temporary log could not be saved."
            } else {
                "Use Export log to include earlier output."
            };
            output.push_str(&format!(
                "[{} earlier log entries omitted from this view. {message}]\n",
                self.archived
            ));
        }
        output.push_str(&self.recent.join("\n"));
        output
    }

    pub fn export(&self, path: &Path) -> io::Result<()> {
        let mut output = BufWriter::new(File::create(path)?);
        output.write_all(self.header().as_bytes())?;
        if let Some(archive) = &self.archive {
            io::copy(&mut File::open(&archive.path)?, &mut output)?;
        } else if self.archive_failed {
            writeln!(
                output,
                "[{} earlier log entries could not be saved to the temporary log.]",
                self.archived
            )?;
        }
        for line in &self.recent {
            writeln!(output, "{line}")?;
        }
        output.flush()
    }

    fn header(&self) -> String {
        if self.report.is_empty() {
            return String::new();
        }
        format!(
            "== Launch report\n{}\n\n== Activity output\n",
            self.report.join("\n")
        )
    }
}

fn victory_key(key: &std::ffi::OsStr) -> Option<String> {
    let key = key.to_str()?;
    let key = if cfg!(windows) {
        key.to_ascii_uppercase()
    } else {
        key.to_owned()
    };
    key.starts_with("VICTORY_").then_some(key)
}

struct LogArchive {
    path: PathBuf,
    file: Option<File>,
}

impl LogArchive {
    fn new() -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mandyhub-activity-{}-{stamp}.log",
            std::process::id()
        ));
        let file = File::options().write(true).create_new(true).open(&path)?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}

impl Drop for LogArchive {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[test]
fn export_keeps_launch_details_and_output_after_the_visible_buffer_rolls_over() {
    let mut log = ActivityLog::default();
    let mut command = Command::new("victory.exe");
    command.args(["run", "game with spaces.exe", "--console", "series-x"]);
    command.env("VICTORY_PROBE_RAYS", "1");
    command.env("UNRELATED_FIXTURE", "not a VIctory switch");
    log.command("Game launch", &command, true);
    log.report("Shader warm-up result: completed successfully.");
    for line in 0..=VISIBLE_LINES {
        log.push(format!("output-{line}"));
    }
    let preview = log.preview();
    assert!(preview.contains("series-x"));
    assert!(preview.contains("Shader warm-up result: completed successfully."));
    assert!(!preview.contains("output-0\n"));
    let archive = log.archive.as_ref().unwrap().path.clone();
    let output = archive.with_extension("export");
    log.export(&output).unwrap();
    let text = fs::read_to_string(&output).unwrap();
    assert!(text.contains("game with spaces.exe"));
    assert!(text.contains("VICTORY_PROBE_RAYS=\"1\""));
    assert!(!text.contains("UNRELATED_FIXTURE"));
    assert!(text.contains("output-0\n"));
    assert!(text.contains(&format!("output-{VISIBLE_LINES}\n")));
    drop(log);
    assert!(!archive.exists());
    fs::remove_file(output).unwrap();
}
