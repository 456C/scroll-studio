use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/scroll-studio.ico");
    println!("cargo:rerun-if-env-changed=RC");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon = manifest.join("assets/scroll-studio.ico");
    assert!(
        icon.is_file(),
        "Missing approved icon: run scripts/prepare-icon.ps1"
    );
    let rc_file = output.join("scroll-studio-icon.rc");
    // Forward slashes avoid RC string escapes; the resource compiler receives
    // the full quoted path, including spaces, without a shell command.
    let icon_path = icon
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\\\"");
    fs::write(&rc_file, format!("1 ICON \"{icon_path}\"\n"))
        .expect("Write Windows icon resource script");

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let (compiler, resource, arguments) = if target_env == "gnu" {
        let architecture = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("x86") => "i686",
            Ok("x86_64") => "x86_64",
            Ok("aarch64") => "aarch64",
            other => panic!("Unsupported Windows GNU icon resource architecture: {other:?}"),
        };
        let compiler = env::var_os("RC")
            .unwrap_or_else(|| format!("{architecture}-w64-mingw32-windres").into());
        let resource = output.join("scroll-studio-icon.o");
        let arguments = vec![
            "-i".into(),
            rc_file.into_os_string(),
            "-O".into(),
            "coff".into(),
            "-o".into(),
            resource.clone().into_os_string(),
        ];
        (compiler, resource, arguments)
    } else if target_env == "msvc" {
        let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
        let resource = output.join("scroll-studio-icon.res");
        let arguments = vec![
            "/nologo".into(),
            "/fo".into(),
            resource.clone().into_os_string(),
            rc_file.into_os_string(),
        ];
        (compiler, resource, arguments)
    } else {
        panic!("Unsupported Windows resource toolchain: {target_env}");
    };
    let result = Command::new(&compiler).args(arguments).output().unwrap_or_else(|error| {
        panic!("Unable to run Windows resource compiler {compiler:?}: {error}. Use scripts/build.ps1 with the portable LLVM-MinGW toolchain, or set RC to your target's windres/rc.exe.")
    });
    assert!(
        result.status.success(),
        "Windows icon resource compilation failed ({compiler:?}):\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!(
        "cargo:rustc-link-arg-bin=scroll-studio={}",
        resource.display()
    );
}
