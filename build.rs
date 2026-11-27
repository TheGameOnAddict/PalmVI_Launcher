use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=data/palmvi.rc");
    println!("cargo:rerun-if-changed=data/palmvi.ico");
    let target = env::var("TARGET").unwrap();
    if !target.contains("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let (mut compiler, resource) = if target.ends_with("msvc") {
        let mut rc =
            find_msvc_tools::find(&target, "rc.exe").unwrap_or_else(|| Command::new("rc.exe"));
        let res = out.join("palmvi.res");
        rc.arg("/nologo").arg("/fo").arg(&res).arg("palmvi.rc");
        (rc, res)
    } else {
        let mut rc = Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()));
        let obj = out.join("palmvi.o");
        rc.args(["-i", "palmvi.rc", "-O", "coff", "-o"]).arg(&obj);
        (rc, obj)
    };
    let status = compiler
        .current_dir("data")
        .status()
        .expect("Windows resource compiler");
    assert!(status.success(), "Could not compile the PalmVI icon");
    println!("cargo:rustc-link-arg-bins={}", resource.display());
}
