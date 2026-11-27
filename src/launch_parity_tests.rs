use super::*;
use std::fs;

// Record what the child actually receives, including on a cache hit or failure.
const RECORDER: &str = r#"@echo off
if "%~2"=="--help" (
  echo --used
  exit /b 0
)
>>"%~dp0order.txt" echo %~1
>"%~dp0%~1.txt" echo cwd=%cd%
>>"%~dp0%~1.txt" echo arg1=%~1
>>"%~dp0%~1.txt" echo arg2=%~2
>>"%~dp0%~1.txt" echo arg3=%~3
>>"%~dp0%~1.txt" set VICTORY_
if "%~1"=="warm" echo 0 pipelines known, 0 made before (fixture), 0 bad; 0 to make, 1 processes
if "%~1"=="shaders" (
  if exist "%~dp0fail" (
    echo 0 shaders in the cache, 2 do not translate, 1 s
    exit /b 0
  )
  if not exist "%VICTORY_CACHE_DIR%\shaders" mkdir "%VICTORY_CACHE_DIR%\shaders"
  echo fixture>"%VICTORY_CACHE_DIR%\shaders\fixture"
  echo 3 shaders in the cache, 1 do not translate, 1 s
)
exit /b 0
"#;

#[test]
fn global_warmup_and_play_share_the_launch_context() {
    check_launch(None);
}

#[test]
#[ignore = "set MANDYHUB_TEST_SCARLETT_SCRIPT to a trusted play-scarlett.ps1 to compare its child invocations"]
fn compare_installed_scarlett_script() {
    check_launch(Some(PathBuf::from(std::env::var_os("MANDYHUB_TEST_SCARLETT_SCRIPT").unwrap())));
}

#[test]
#[ignore = "requires MANDYHUB_TEST_VICTORY, MANDYHUB_TEST_GAME and MANDYHUB_TEST_SCARLETT_SCRIPT"]
fn warm_real_scarlett_cache() {
    let var = |key| PathBuf::from(std::env::var_os(key).unwrap());
    let script = var("MANDYHUB_TEST_SCARLETT_SCRIPT");
    let game = var("MANDYHUB_TEST_GAME");
    let exe = var("MANDYHUB_TEST_VICTORY");
    let root = std::env::temp_dir().join(format!("mandyhub-real-warm-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut command = Command::new(exe);
    command.current_dir(script.parent().unwrap().parent().unwrap());
    let (switches, _) = script_preset::parse(&fs::read_to_string(script).unwrap()).unwrap();
    for line in switches.lines() {
        let (key, value) = line.split_once('=').unwrap();
        command.env(key, value);
    }
    command.env("VICTORY_GAME_DIR", &game).env("VICTORY_CACHE_DIR", root.join("cache"))
        .env("VICTORY_DATA_DIR", root.join("save"));
    let launch = Saved { game_dir: game.display().to_string(), warm_before_play: true, warm_pipelines: true,
        ..Default::default() }.launch("fixture", true);
    let job = Arc::new(Mutex::new(Job::default()));
    warm(&job, &egui::Context::default(), &command, &launch).unwrap();
    let first = job.lock().unwrap().log.preview();
    println!("{}", first.lines().find(|s| s.starts_with("Shader totals:")).expect("VIctory shader totals"));
    println!("{}", first.lines().find(|s| s.starts_with("Pipeline inventory:")).expect("VIctory pipeline inventory"));
    warm(&job, &egui::Context::default(), &command, &launch).unwrap();
    assert!(job.lock().unwrap().log.preview().contains("cache current for this build"));
    fs::remove_dir_all(root).unwrap();
}

fn check_launch(direct_script: Option<PathBuf>) {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("mandyhub launch comparison {id}"));
    let game = root.join("game files");
    let cache = root.join("cache");
    let data = root.join("save");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join(EXE_NAME), []).unwrap();
    fs::write(game.join("shaders.rpf"), []).unwrap();
    let recorder = root.join("victory.cmd");
    fs::write(&recorder, RECORDER.replace('\n', "\r\n")).unwrap();

    let mut defaults = "VICTORY_PROBE_RAYS=1".to_owned();
    let snapshot = |name: &str| fs::read_to_string(root.join(name)).unwrap();
    let reference = direct_script.map(|script| {
        let source = fs::read_to_string(script).unwrap();
        defaults = script_preset::parse(&source).unwrap().0;
        let script = root.join("scripts/play-scarlett.ps1");
        fs::create_dir(script.parent().unwrap()).unwrap();
        fs::write(&script, source).unwrap();
        let output = Command::new("powershell.exe").current_dir(&root)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(script)
            .arg("-Game").arg(&game).arg("-Victory").arg(&recorder)
            .arg("-Cache").arg(&cache).arg("-Data").arg(&data).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "run"]);
        fs::remove_file(root.join("order.txt")).unwrap();
        (snapshot("shaders.txt"), snapshot("run.txt"))
    });
    let p = Project {
        name: "fixture".into(), repo: format!("local/fixture-{id}"), branch: None, gpu: vec![],
        fps: None, render: None, verified: false, summary: String::new(), custom: true,
        local_exe: Some(recorder.to_str().unwrap().into()), archive_exe: None,
    };
    let mut saved = Saved {
        game_dir: game.to_str().unwrap().into(), warm_before_play: true, warm_pipelines: true,
        ..Saved::default()
    };
    saved.project_launch.insert(p.repo.clone(), ProjectLaunch {
        switches: Some(format!("{defaults}\nVICTORY_CACHE_DIR={}\nVICTORY_DATA_DIR={}", cache.display(), data.display())),
        script_options: Some(Default::default()), ..Default::default()
    });
    let launch = |saved: &Saved| {
        let job = Arc::new(Mutex::new(Job::default()));
        play(&job, &egui::Context::default(), &p, &saved.launch(&p.repo, true), &saves::LaunchSave::Continue).unwrap();
        let log = job.lock().unwrap().log.preview();
        log
    };
    let first = launch(&saved);
    assert!(first.contains("Shader totals: 3 cached, 1 could not translate"));
    assert!(first.contains("Pipeline inventory: 0 pipelines known"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "warm", "run"]);
    if let Some((shaders, run)) = reference {
        assert_eq!(snapshot("shaders.txt"), shaders);
        assert_eq!(snapshot("run.txt"), run);
    }
    fs::remove_file(root.join("order.txt")).unwrap();
    assert!(launch(&saved).contains("cache current for this build"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["warm", "run"]);
    fs::remove_file(root.join("order.txt")).unwrap();
    saved.project_launch.get_mut(&p.repo).unwrap().switches.as_mut().unwrap().push_str("\nVICTORY_PROBE_RAYS=0");
    fs::write(root.join("fail"), []).unwrap();
    let failed = launch(&saved);
    assert!(failed.contains("Warm-up failed"), "{failed}");
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["shaders", "run"]);
    fs::remove_file(root.join("order.txt")).unwrap();
    saved.warm_before_play = false;
    assert!(launch(&saved).contains("Warm-up before Play: off"));
    assert_eq!(snapshot("order.txt").lines().collect::<Vec<_>>(), ["run"]);
    fs::remove_dir_all(root).unwrap();
}
