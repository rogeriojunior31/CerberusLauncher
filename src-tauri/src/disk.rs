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
        return Err(format!("Espaço insuficiente em {}. Necessários {} MiB adicionais, disponíveis {} MiB.",
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
