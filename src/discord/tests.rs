use super::*;
use std::os::windows::io::FromRawHandle;
use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE, System::Pipes::CreateNamedPipeW};

#[test]
fn lifecycle_selects_the_game_only_while_its_process_is_alive() {
    let defaults: Settings = serde_json::from_str("{}").unwrap();
    let game = app_id(&defaults.game_id).unwrap();
    assert!(defaults.enabled && defaults.launcher_id.is_empty());
    assert!(!serde_json::from_str::<Settings>(r#"{"enabled":false}"#).unwrap().enabled);
    let mut state = State::default();
    state.apply(Event::Configure(Ids {
        game: Some(game),
        idle: None,
    }));
    assert_eq!(state.target(), None);
    let activity = Activity::Game {
        pid: 42,
        start: 1234,
    };
    state.apply(Event::Start(activity));
    assert_eq!(state.target(), Some((game, activity)));
    state.apply(Event::Stop(41));
    assert_eq!(state.target(), Some((game, activity)));
    state.apply(Event::Configure(Ids::default()));
    assert_eq!(state.target(), None);
    state.apply(Event::Configure(Ids {
        game: Some(game),
        idle: Some(55),
    }));
    state.apply(Event::Stop(42));
    assert_eq!(state.target(), Some((55, Activity::Launcher)));
}

fn read_packet(pipe: &mut File) -> (u32, Value) {
    let mut header = [0; 8];
    pipe.read_exact(&mut header).unwrap();
    let size = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
    assert!(size < 65536);
    let mut data = vec![0; size];
    pipe.read_exact(&mut data).unwrap();
    (
        u32::from_le_bytes(header[..4].try_into().unwrap()),
        serde_json::from_slice(&data).unwrap(),
    )
}

#[test]
fn local_pipe_handshake_publish_ping_acknowledge_and_clear() {
    let path = format!(r"\\.\pipe\palmvi-presence-test-{}", std::process::id());
    let wide: Vec<_> = path.encode_utf16().chain([0]).collect();
    let handle =
        unsafe { CreateNamedPipeW(wide.as_ptr(), 3, 0, 1, 8192, 8192, 0, std::ptr::null()) };
    assert_ne!(
        handle,
        INVALID_HANDLE_VALUE,
        "{}",
        io::Error::last_os_error()
    );
    let mut server = unsafe { File::from_raw_handle(handle) };
    let client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut session = Connection::handshake(client, 123).unwrap();
    assert_eq!(
        read_packet(&mut server),
        (0, json!({"v":1, "client_id":"123"}))
    );
    let game = Activity::Game {
        pid: 100,
        start: 1234,
    };
    frame(&mut server, 1, &json!({"evt":"READY"})).unwrap();
    session.poll(game).unwrap();
    let (op, command) = read_packet(&mut server);
    assert_eq!(op, 1);
    assert_eq!(
        command["args"]["activity"],
        json!({"type":0, "details":"Newer Retail Game", "status_display_type":2, "timestamps":{"start":1234}})
    );
    assert_eq!(command["args"]["pid"], std::process::id());
    frame(
        &mut server,
        1,
        &json!({"cmd":"SET_ACTIVITY","nonce":"publish","evt":null}),
    )
    .unwrap();
    frame(&mut server, 3, &json!({"nonce":"ping"})).unwrap();
    session.poll(game).unwrap();
    assert!(session.acknowledged);
    assert_eq!(read_packet(&mut server), (4, json!({"nonce":"ping"})));
    drop(session);
    assert_eq!(read_packet(&mut server), (1, activity_command(None)));
}
