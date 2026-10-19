use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=assets/ferrite.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");

    // Embed the exe icon (resource id 1; main.rs loads it for the window too).
    let ico = manifest.join("assets/ferrite.ico").display().to_string().replace('\\', "/");
    let rc = out.join("app.rc");
    fs::write(&rc, format!("1 ICON \"{ico}\"\n")).unwrap();
    if gnu {
        let obj = out.join("app_res.o");
        let ok = Command::new("windres").arg(&rc).args(["-O", "coff", "-o"]).arg(&obj).status();
        assert!(ok.is_ok_and(|s| s.success()), "windres failed; install MinGW binutils");
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        let res = out.join("app.res");
        let ok = Command::new("rc").args(["/nologo", "/fo"]).arg(&res).arg(&rc).status();
        assert!(ok.is_ok_and(|s| s.success()), "rc.exe failed; run from a VS developer prompt");
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }

    // ponytail: windows-gnu can't static-link WebView2Loader, so ship the DLL beside the exe.
    // Found via Cargo.lock + registry source so it works on a clean build regardless of build order.
    if !gnu { return; }
    let lock = fs::read_to_string(manifest.join("Cargo.lock")).unwrap().replace("\r\n", "\n");
    let ver = lock.split("name = \"webview2-com-sys\"\nversion = \"").nth(1).and_then(|s| s.split('"').next()).unwrap();
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() { "x86_64" => "x64", "aarch64" => "arm64", _ => "x86" };
    let cargo_home = env::var("CARGO_HOME").map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env::var("USERPROFILE").unwrap()).join(".cargo"));
    let dll = fs::read_dir(cargo_home.join("registry/src")).unwrap().flatten()
        .map(|d| d.path().join(format!("webview2-com-sys-{ver}/{arch}/WebView2Loader.dll")))
        .find(|p| p.exists())
        .expect("WebView2Loader.dll not found in cargo registry");
    let profile_dir = out.ancestors().nth(3).unwrap(); // target/<profile>/build/<pkg>/out
    let dest = profile_dir.join("WebView2Loader.dll");
    // Skip when already identical: a running ferrite.exe locks the DLL.
    if fs::read(&dest).ok() != fs::read(&dll).ok() {
        fs::copy(dll, dest).unwrap();
    }
}
