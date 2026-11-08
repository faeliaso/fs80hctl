use std::{env, fs, path::PathBuf};

fn bundled_root() -> Option<PathBuf> {
    let os = match env::var("CARGO_CFG_TARGET_OS").ok()? {
        value if value == "macos" => "darwin".to_owned(),
        value => value,
    };
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").ok()? {
        value if value == "aarch64" => "arm64".to_owned(),
        value => value,
    };
    let bundles = PathBuf::from(env::var("CARGO_MANIFEST_DIR").ok()?)
        .join("../..")
        .join("vendor/futronic");
    fs::read_dir(bundles)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(&format!("-{os}-{arch}")))
        })
}

fn main() {
    println!("cargo:rerun-if-env-changed=FS80H_DRIVER_DIST_DIR");
    let root = bundled_root()
        .or_else(|| env::var_os("FS80H_DRIVER_DIST_DIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/Volumes/MAIN/Workspace/futronic-fs80h-rs/dist/futronic-fs80h-native-v0.1.0-darwin-arm64"));
    let lib = root.join("lib");
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
    }
}
