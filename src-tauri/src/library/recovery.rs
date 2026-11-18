//! Recover complete records from an incompatible or truncated library array.
use super::{Game, save};
use serde::Deserializer;
use serde::de::{SeqAccess, Visitor};
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

pub(super) fn read(file: &Path) -> Result<(Vec<Game>, Option<String>), String> {
    let bytes = match fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok((Vec::new(), None)),
        Err(e) => return Err(format!("Não foi possível ler {}: {e}", file.display())),
    };
    if let Ok(games) = serde_json::from_slice(&bytes) {
        return Ok((games, None));
    }

    let mut games = Vec::new();
    let mut parser = serde_json::Deserializer::from_slice(&bytes);
    let parsed = parser.deserialize_seq(CompleteGames(&mut games)).and_then(|()| parser.end());
    // Only EOF has an unambiguous prefix. Leave other malformed documents in place for repair.
    if games.is_empty() || parsed.is_err_and(|e| !e.is_eof()) {
        return Err(format!("O arquivo {} precisa de reparo. O original está preservado. Restaure uma cópia e tente novamente.", file.display()));
    }
    let backup = preserve_original(file, &bytes).map_err(|e| format!("Não foi possível guardar o original de {}: {e}", file.display()))?;
    save(file, &games).map_err(|e| format!("Falha ao gravar a biblioteca recuperada. Original em {}: {e}", backup.display()))?;
    let notice = format!("Biblioteca recuperada. Jogos preservados: {}. Cópia do arquivo original: {}", games.len(), backup.display());
    Ok((games, Some(notice)))
}

// Collect complete JSON values one at a time, even if the final record is cut short.
struct CompleteGames<'a>(&'a mut Vec<Game>);

impl<'de> Visitor<'de> for CompleteGames<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a library array")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(value) = seq.next_element::<serde_json::Value>()? {
            if let Ok(game) = serde_json::from_value(value) {
                self.0.push(game);
            }
        }
        Ok(())
    }
}

fn preserve_original(file: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    for number in 1u64.. {
        let path = file.with_file_name(format!("library.original-{number}.json"));
        match fs::File::create_new(&path) {
            Ok(mut backup) => {
                backup.write_all(bytes)?;
                backup.sync_all()?;
                return Ok(path);
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if fs::read(&path)? == bytes {
                    return Ok(path);
                }
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
