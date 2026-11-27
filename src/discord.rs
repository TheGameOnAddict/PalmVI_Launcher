//! Discord desktop Rich Presence. A single worker owns the pipe and switches identities.
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::io::AsRawHandle,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub game_id: String,
    pub launcher_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            game_id: "1445160676522856458".into(),
            launcher_id: String::new(),
        }
    }
}

pub fn app_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if !(17..=20).contains(&text.len()) || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|v| *v != 0)
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Ids {
    game: Option<u64>,
    idle: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Activity {
    Launcher,
    Game { pid: u32, start: u64 },
}

enum Event {
    Configure(Ids),
    Start(Activity),
    Stop(u32),
}

#[derive(Clone)]
pub struct Presence {
    tx: mpsc::Sender<Event>,
    ids: Ids,
    status: Arc<Mutex<&'static str>>,
}

impl Presence {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new("Idle"));
        let output = status.clone();
        std::thread::spawn(move || worker(rx, output));
        Self {
            tx,
            status,
            ids: Ids::default(),
        }
    }
    pub fn configure(&mut self, settings: &Settings) {
        let ids = if settings.enabled {
            Ids {
                game: app_id(&settings.game_id),
                idle: app_id(&settings.launcher_id),
            }
        } else {
            Ids::default()
        };
        if self.ids != ids {
            self.ids = ids;
            let _ = self.tx.send(Event::Configure(ids));
        }
    }
    pub fn status(&self) -> &'static str {
        *self.status.lock().unwrap()
    }
    pub fn game_started(&self, pid: u32) -> GameSession {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = self.tx.send(Event::Start(Activity::Game { pid, start }));
        GameSession {
            tx: self.tx.clone(),
            pid,
        }
    }
}

pub struct GameSession {
    tx: mpsc::Sender<Event>,
    pid: u32,
}
impl Drop for GameSession {
    fn drop(&mut self) {
        let _ = self.tx.send(Event::Stop(self.pid));
    }
}

#[derive(Default)]
struct State {
    ids: Ids,
    game: Option<Activity>,
}
impl State {
    fn apply(&mut self, event: Event) {
        match event {
            Event::Configure(ids) => self.ids = ids,
            Event::Start(game) => self.game = Some(game),
            Event::Stop(pid) if matches!(self.game, Some(Activity::Game { pid: active, .. }) if active == pid) => {
                self.game = None
            }
            Event::Stop(_) => {}
        }
    }
    fn target(&self) -> Option<(u64, Activity)> {
        match self.game {
            Some(game) => Some((self.ids.game?, game)),
            None => Some((self.ids.idle?, Activity::Launcher)),
        }
    }
}

fn activity_command(activity: Option<Activity>) -> Value {
    let value = match activity {
        Some(Activity::Game { start, .. }) => json!({
            "type": 0, "status_display_type": 0,
            "timestamps": { "start": start }
        }),
        Some(Activity::Launcher) => {
            json!({"type": 0, "details": "PalmVI Launcher", "status_display_type": 2})
        }
        None => Value::Null,
    };
    json!({"cmd":"SET_ACTIVITY", "args":{"pid":std::process::id(),"activity":value},
           "nonce": if activity.is_some() { "publish" } else { "clear" } })
}

fn frame(writer: &mut impl Write, opcode: u32, payload: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(payload)?;
    let mut packet = Vec::with_capacity(8 + bytes.len());
    packet.extend(opcode.to_le_bytes());
    packet.extend((bytes.len() as u32).to_le_bytes());
    packet.extend(bytes);
    writer.write_all(&packet)
}

struct Connection {
    pipe: File,
    input: Vec<u8>,
    connected_at: Instant,
    sent: bool,
    acknowledged: bool,
}

impl Connection {
    fn connect(id: u64) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        for n in 0..10 {
            // Only read/write pipe data; identify-level SQOS on a local named pipe.
            let Ok(pipe) = OpenOptions::new()
                .access_mode(3)
                .security_qos_flags(0x10000)
                .open(format!(r"\\?\pipe\discord-ipc-{n}"))
            else {
                continue;
            };
            return Self::handshake(pipe, id);
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Discord desktop unavailable",
        ))
    }
    fn handshake(mut pipe: File, id: u64) -> io::Result<Self> {
        frame(&mut pipe, 0, &json!({"v":1, "client_id":id.to_string()}))?;
        Ok(Self {
            pipe,
            input: Vec::new(),
            connected_at: Instant::now(),
            sent: false,
            acknowledged: false,
        })
    }
    fn poll(&mut self, target: Activity) -> io::Result<()> {
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;
        let mut available = 0;
        // The worker owns the file. Peek before reading so an idle Discord client cannot block a stop event.
        let ok = unsafe {
            PeekNamedPipe(
                self.pipe.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buf = [0; 8192];
        let count = (available as usize).min(buf.len());
        if count > 0 {
            let n = self.pipe.read(&mut buf[..count])?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            self.input.extend_from_slice(&buf[..n]);
        }
        while self.input.len() >= 8 {
            let op = u32::from_le_bytes(self.input[..4].try_into().unwrap());
            let len = u32::from_le_bytes(self.input[4..8].try_into().unwrap()) as usize;
            if len > 65536 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if self.input.len() < 8 + len {
                break;
            }
            let payload: Value = serde_json::from_slice(&self.input[8..8 + len])?;
            self.input.drain(..8 + len);
            match op {
                1 if payload["evt"] == "READY" && !self.sent => {
                    frame(&mut self.pipe, 1, &activity_command(Some(target)))?;
                    self.sent = true;
                }
                1 if payload["evt"] == "ERROR" => {
                    return Err(io::ErrorKind::PermissionDenied.into())
                }
                1 if self.sent
                    && payload["cmd"] == "SET_ACTIVITY"
                    && payload["nonce"] == "publish" =>
                {
                    self.acknowledged = true
                }
                2 => return Err(io::ErrorKind::ConnectionAborted.into()),
                3 => frame(&mut self.pipe, 4, &payload)?,
                _ => {}
            }
        }
        if !self.acknowledged && self.connected_at.elapsed() > Duration::from_secs(8) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.sent {
            let _ = frame(&mut self.pipe, 1, &activity_command(None));
        }
    }
}

fn worker(rx: mpsc::Receiver<Event>, status: Arc<Mutex<&'static str>>) {
    let mut state = State::default();
    let mut previous = None;
    let mut connection: Option<Connection> = None;
    let mut retry = Instant::now();
    loop {
        let event = if state.target().is_none() {
            match rx.recv() {
                Ok(e) => Some(e),
                Err(_) => return,
            }
        } else {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(e) => Some(e),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };
        if let Some(e) = event {
            state.apply(e);
        }
        let target = state.target();
        if target != previous {
            connection = None;
            previous = target;
            retry = Instant::now();
            *status.lock().unwrap() = "Idle";
        }
        let Some((id, activity)) = target else {
            continue;
        };
        if connection.is_none() && Instant::now() >= retry {
            connection = Connection::connect(id).ok();
            *status.lock().unwrap() = if connection.is_some() {
                "Connecting to Discord…"
            } else {
                "Waiting for Discord desktop"
            };
            retry = Instant::now() + Duration::from_secs(10);
        }
        if let Some(c) = &mut connection {
            if c.poll(activity).is_err() {
                connection = None;
                retry = Instant::now() + Duration::from_secs(10);
                *status.lock().unwrap() = "Discord connection ended or was rejected. Retrying…";
            } else if c.acknowledged {
                *status.lock().unwrap() = "Discord acknowledged the activity";
            }
        }
    }
}
