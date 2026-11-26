use std::fs;

/// The app's commands, as `main.rs` registers them in `generate_handler!`:
/// every one gets an `allow-<command>` permission, and the main window a set
/// with all of them. The overlay and the unlock notice get only their own (see
/// `capabilities/`), so a page shown there can't call the rest.
fn commands() -> Vec<String> {
    let main = fs::read_to_string("src/main.rs").expect("src/main.rs");
    let start = main.find("generate_handler![").expect("generate_handler! in main.rs") + "generate_handler![".len();
    let end = start + main[start..].find(']').expect("end of generate_handler!");
    main[start..end]
        .split(',')
        .map(|c| c.trim().rsplit("::").next().unwrap_or_default().to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=src/main.rs");
    println!("cargo:rerun-if-changed=capabilities");
    let names = commands();
    let allowed: Vec<String> = names.iter().map(|c| format!("\"allow-{}\"", c.replace('_', "-"))).collect();
    let set = format!(
        "# Written by build.rs from generate_handler! in main.rs: don't edit.\n\n[[set]]\nidentifier = \"main-window\"\ndescription = \"Every command of the app, for the main window.\"\npermissions = [\n  {}\n]\n",
        allowed.join(",\n  ")
    );
    fs::create_dir_all("permissions").expect("permissions folder");
    if fs::read_to_string("permissions/main-window.toml").ok().as_deref() != Some(set.as_str()) {
        fs::write("permissions/main-window.toml", set).expect("permissions/main-window.toml");
    }
    let names: &'static [&'static str] = Box::leak(names.into_iter().map(|n| &*Box::leak(n.into_boxed_str())).collect::<Vec<&'static str>>().into_boxed_slice());
    let attributes = tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(names));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
