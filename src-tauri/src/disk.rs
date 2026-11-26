use cerberus_shared::tr;
use std::{io, path::Path};

/// Settings can point to a directory that will be created on the first install.
pub fn available(path: &Path) -> Option<u64> {
    for dir in path.ancestors() {
        match fs2::available_space(dir) {
            Ok(bytes) => return Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => break,
        }
    }
    None
}

pub fn require(dir: &Path, additional: u64) -> Result<(), String> {
    if let Some(free) = available(dir) && additional > free {
        return Err(tr!("Espaço insuficiente em {}. Necessários {} MiB adicionais, disponíveis {} MiB.",
            dir.display(), additional.div_ceil(1 << 20), free >> 20));
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn checks_the_volume_of_a_future_directory() {
    let dir = tempfile::tempdir().unwrap();
    let future = dir.path().join("new/game");
    assert!(available(&future).is_some());
    assert!(require(&future, 0).is_ok());
    assert!(require(&future, u64::MAX).is_err());
}


/// A name Windows takes as an ordinary file: not a device (`CON`, `NUL`,
/// `COM1`... with or without an extension), no `:` (an NTFS stream), nothing
/// Windows forbids, and no dot or space at the end (Windows drops them, so two
/// names would land on one file).
pub fn plain_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with(['.', ' ']) {
        return false;
    }
    if name.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)) {
        return false;
    }
    let stem = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0');
    !device
}

/// A path inside an archive or a disc that stays inside where it's unpacked:
/// only plain names, separated by `/` or `\`.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty() && path.split(['/', '\\']).filter(|p| !p.is_empty()).all(plain_name) && !path.starts_with(['/', '\\'])
}

#[cfg(test)]
mod name_tests {
    use super::{plain_name, safe_path};

    #[test]
    fn reserved_and_stream_names_are_refused() {
        for bad in ["CON", "nul", "COM1", "lpt9.txt", "con.ini", "a:b", "file.", "file ", "..", "x\u{1}y"] {
            assert!(!plain_name(bad), "{bad}");
        }
        for good in ["SLUS_206.51", "COM10", "console.cfg", "Data", "nulled.bin", "a b.txt"] {
            assert!(plain_name(good), "{good}");
        }
        assert!(safe_path("Game/Data/x.bin") && safe_path("Game\\x.bin"));
        assert!(!safe_path("../x") && !safe_path("C:\\x") && !safe_path("/etc/x") && !safe_path("a/../../b") && !safe_path("a/NUL/b"));
    }
}
