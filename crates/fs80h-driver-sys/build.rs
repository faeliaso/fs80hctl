use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn compatible_version(path: &Path, target: &str) -> Option<(u64, u64, u64)> {
    let name = path.file_name()?.to_str()?;
    let version = name
        .strip_prefix("futronic-fs80h-native-v")?
        .strip_suffix(&format!("-{target}"))?;
    let mut parts = version.split('.').map(str::parse::<u64>);
    let version = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    if parts.next().is_some() {
        return None;
    }
    let manifest = fs::read_to_string(path.join("manifest.json")).ok()?;
    (manifest.contains("\"abi_version\": 2")
        && manifest.contains(&format!("\"target\": \"{target}\"")))
    .then_some(version)
}

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
    let target = format!("{os}-{arch}");
    fs::read_dir(bundles)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| compatible_version(&path, &target).map(|version| (version, path)))
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

fn main() {
    println!("cargo:rerun-if-env-changed=FS80H_DRIVER_DIST_DIR");
    let root = bundled_root()
        .or_else(|| env::var_os("FS80H_DRIVER_DIST_DIR").map(PathBuf::from))
        .expect("no compatible Futronic ABI v2 bundle; run scripts/vendor-futronic.sh");
    let lib = root.join("lib");
    if !lib.join("libfutronic_fs80h_rs.dylib").is_file() {
        panic!(
            "FS80H driver library missing at {}. Set FS80H_DRIVER_DIST_DIR to the native distribution root for this target.",
            root.display()
        );
    }
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=futronic_fs80h_rs");
    println!("cargo:rerun-if-changed={}", lib.display());
    // Test and executable consumers need the sibling libusb and driver at runtime.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
}
