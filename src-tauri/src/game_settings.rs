//! A game's own settings (full screen, resolution, frame rate...), changed from
//! the launcher. The catalog says, per recomp, which settings file the game
//! keeps (`.ini`, `.toml` or `.json`), where, and what each choice writes there:
//! `{"label": "On", "set": {"Window/Fullscreen/Enabled": "true"}}`.
//!
//! Only the lines of the keys being set change: the rest of the file, its
//! comments and its order stay as the game wrote them. A key is `Section/key`
//! in INI and TOML files, and a path through nested objects in JSON. A choice
//! can also be options on the game's command line (`"in": "args"`), kept per game.

use cerberus_shared::tr;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use cerberus_shared::{GameSetting, GameSettings, SettingSlider};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::recomp::saves::resolve;

#[derive(Debug, Clone, Deserialize)]
struct Place {
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigSpec {
    #[serde(default = "main_id")]
    id: String,
    format: String,
    file: Vec<Place>,
}

fn main_id() -> String {
    "main".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChoiceSpec {
    label: String,
    #[serde(default)]
    set: BTreeMap<String, String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingSpec {
    id: String,
    label: String,
    #[serde(default)]
    help: String,
    /// A settings file's id, or "args".
    #[serde(default = "main_id", rename = "in")]
    target: String,
    #[serde(default)]
    choices: Vec<ChoiceSpec>,
    /// The game's own values for keys its file doesn't have yet.
    #[serde(default)]
    defaults: BTreeMap<String, String>,
    /// Only applies while these keys have these values.
    #[serde(default)]
    when: BTreeMap<String, String>,
    #[serde(default)]
    group: String,
    #[serde(default)]
    advanced: bool,
    /// A number between two ends instead of choices.
    #[serde(default)]
    slider: Option<SliderSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SliderSpec {
    /// The key it is written to; the setting's id when empty.
    #[serde(default)]
    key: String,
    min: f64,
    max: f64,
    #[serde(default)]
    step: f64,
    /// printf style: "%d", "%.2f"; plain otherwise.
    #[serde(default)]
    format: String,
}

impl SliderSpec {
    fn step(&self) -> f64 {
        if self.step > 0.0 && self.step.is_finite() { self.step } else { (self.max - self.min) / 100.0 }
    }

    /// A number moved to: inside the ends, on a step.
    fn snap(&self, value: f64) -> f64 {
        let step = self.step();
        let n = ((value.clamp(self.min, self.max) - self.min) / step).round();
        let x = (self.min + n * step).clamp(self.min, self.max);
        (x * 1e6).round() / 1e6
    }

    /// The number written the game's way: "%d" -> "80", "%.2f" -> "0.50".
    fn text(&self, value: f64) -> String {
        let format = self.format.trim();
        if format.contains("%d") || format.contains("%i") {
            return format!("{}", value.round() as i64);
        }
        if let Some(digits) = format.split_once("%.").and_then(|(_, rest)| rest.split('f').next()).and_then(|d| d.parse::<usize>().ok()) {
            return format!("{value:.digits$}");
        }
        let plain = format!("{value}");
        if plain.contains('.') { plain.trim_end_matches('0').trim_end_matches('.').to_string() } else { plain }
    }
}

/// What a catalog entry offers, already checked: files only in the game's or
/// the player's folders, settings with real choices and a file to write them in.
#[derive(Debug, Clone, Default)]
pub struct Offer {
    configs: Vec<ConfigSpec>,
    settings: Vec<SettingSpec>,
}

impl Offer {
    pub fn is_empty(&self) -> bool {
        self.settings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.settings.len()
    }
}

/// A place the catalog names may only be inside the game's folder or the
/// player's own folders, never `..` out of them.
fn safe_place(path: &str) -> bool {
    let path = path.trim().replace('\\', "/");
    let roots = ["{install}/", "{local}/", "{appdata}/", "{config}/", "{documents}/", "{home}/"];
    roots.iter().any(|root| path.starts_with(root)) && !path.split('/').any(|part| part == "..")
}

/// Reads the catalog's `settings` object; anything it can't use is left out.
pub fn offer_of(value: Option<&Value>) -> Offer {
    let Some(value) = value.filter(|v| v.is_object()) else { return Offer::default() };
    let configs: Vec<ConfigSpec> = value
        .get("config")
        .and_then(|c| serde_json::from_value::<Vec<ConfigSpec>>(c.clone()).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|c| matches!(c.format.as_str(), "ini" | "toml" | "json"))
        .filter(|c| !c.file.is_empty() && c.file.iter().all(|p| safe_place(&p.path)))
        .collect();
    // One bad setting mustn't hide the others, so each is read on its own.
    let settings = value
        .get("settings")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(|s| serde_json::from_value::<SettingSpec>(s.clone()).ok()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.choices.len() >= 2 || s.slider.as_ref().is_some_and(|sl| sl.min < sl.max && sl.min.is_finite() && sl.max.is_finite()))
        // A slider is only offered for a settings file; on the command line it would need more rules.
        .filter(|s| if s.target == "args" { s.slider.is_none() && s.choices.iter().any(|c| !c.args.is_empty()) } else { configs.iter().any(|c| c.id == s.target) })
        .collect();
    Offer { configs, settings }
}

/// Two values as the same setting: quotes aside, numbers by value (`1` and
/// `1.0`), words in any letter case.
fn same(a: &str, b: &str) -> bool {
    let bare = |x: &str| x.trim().trim_matches(['"', '\'']).trim().to_string();
    let (a, b) = (bare(a), bare(b));
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() < 1e-6,
        _ => a.eq_ignore_ascii_case(&b),
    }
}

/// A settings file, read once, changed in memory and written back whole.
pub struct ConfigFile {
    path: PathBuf,
    format: String,
    lines: Vec<String>,
    json: Map<String, Value>,
}

/// Where a key's value sits on an INI or TOML line: the text before the value
/// (key, `=`, spaces), the value, and what follows (a comment).
fn value_span(line: &str, format: &str) -> Option<(String, usize, usize)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with(['#', ';', '[']) {
        return None;
    }
    let eq = line.find('=')?;
    let key = line[..eq].trim().to_string();
    let after = &line[eq + 1..];
    let start = eq + 1 + (after.len() - after.trim_start().len());
    let rest = &line[start..];
    let end = if format == "ini" {
        rest.find([';', '#']).unwrap_or(rest.len())
    } else {
        // TOML: a `#` inside a quoted string isn't a comment.
        let mut quoted = None;
        let mut cut = rest.len();
        for (i, c) in rest.char_indices() {
            match (quoted, c) {
                (None, '"' | '\'') => quoted = Some(c),
                (Some(q), c) if c == q => quoted = None,
                (None, '#') => {
                    cut = i;
                    break;
                }
                _ => {}
            }
        }
        cut
    };
    let value_end = start + rest[..end].trim_end().len();
    Some((key, start, value_end))
}

fn header(line: &str) -> Option<&str> {
    let t = line.trim();
    (t.starts_with('[') && !t.starts_with("[[")).then(|| t[1..].split(']').next().unwrap_or("").trim())
}

impl ConfigFile {
    pub fn open(path: PathBuf, format: &str) -> Self {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let json = if format == "json" {
            serde_json::from_str::<Value>(&text).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
        } else {
            Map::new()
        };
        let lines = text.lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
        Self { path, format: format.to_string(), lines, json }
    }

    /// `Graphics/Fullscreen`: `Fullscreen` under `[Graphics]`. A plain key is looked for anywhere.
    fn split<'a>(&self, key: &'a str) -> (Option<&'a str>, &'a str) {
        match key.split_once('/') {
            Some((section, k)) if !section.trim().is_empty() && !k.trim().is_empty() => (Some(section.trim()), k.trim()),
            _ => (None, key.trim()),
        }
    }

    /// The lines a section holds (after its header), or the whole file.
    fn section(&self, name: Option<&str>) -> Option<(usize, usize)> {
        let Some(name) = name else { return Some((0, self.lines.len())) };
        let ini = self.format == "ini";
        let start = self.lines.iter().position(|l| header(l).is_some_and(|h| if ini { h.eq_ignore_ascii_case(name) } else { h == name }))? + 1;
        let end = self.lines[start..].iter().position(|l| l.trim_start().starts_with('[')).map_or(self.lines.len(), |i| start + i);
        Some((start, end))
    }

    fn key_matches(&self, found: &str, wanted: &str) -> bool {
        if self.format == "ini" { found.eq_ignore_ascii_case(wanted) } else { found == wanted }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if self.format == "json" {
            let mut parts = key.split('/');
            let mut value = self.json.get(parts.next()?)?;
            for part in parts {
                value = value.get(part)?;
            }
            return Some(match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        }
        let (section, k) = self.split(key);
        let (a, b) = self.section(section)?;
        self.lines[a..b].iter().find_map(|line| {
            let (found, start, end) = value_span(line, &self.format)?;
            self.key_matches(&found, k).then(|| line[start..end].to_string())
        })
    }

    pub fn set(&mut self, key: &str, value: &str) {
        if self.format == "json" {
            let parsed = serde_json::from_str::<Value>(value).unwrap_or_else(|_| Value::String(value.to_string()));
            let parts: Vec<&str> = key.split('/').collect();
            let Some((last, path)) = parts.split_last() else { return };
            let mut object = &mut self.json;
            for part in path {
                let entry = object.entry(part.to_string()).or_insert_with(|| Value::Object(Map::new()));
                if !entry.is_object() {
                    *entry = Value::Object(Map::new());
                }
                let Value::Object(inner) = entry else { return };
                object = inner;
            }
            object.insert(last.to_string(), parsed);
            return;
        }
        let (section, k) = self.split(key);
        if let Some((a, b)) = self.section(section) {
            for i in a..b {
                if let Some((found, start, end)) = value_span(&self.lines[i], &self.format)
                    && self.key_matches(&found, k)
                {
                    let line = &self.lines[i];
                    self.lines[i] = format!("{}{}{}", &line[..start], value, &line[end..]);
                    return;
                }
            }
            // A new key goes after the section's last line with text; a top-level
            // TOML key before the first table, or it would belong to that table.
            let limit = if section.is_none() && self.format == "toml" {
                self.lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(self.lines.len())
            } else {
                b
            };
            let at = (a..limit).rev().find(|i| !self.lines[*i].trim().is_empty()).map_or(a, |i| i + 1);
            let line = format!("{k}{}{value}", self.equals());
            self.lines.insert(at, line);
            return;
        }
        if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
            self.lines.push(String::new());
        }
        self.lines.push(format!("[{}]", section.unwrap_or_default()));
        self.lines.push(format!("{k}{}{value}", self.equals()));
    }

    /// `key=value` or `key = value`, as the file already writes them.
    fn equals(&self) -> &'static str {
        let tight = self.lines.iter().find_map(|l| {
            let eq = l.find('=')?;
            let key = l[..eq].trim_start();
            (!key.is_empty() && !key.starts_with(['#', ';', '['])).then(|| !l[..eq].ends_with(' ') && !l[eq + 1..].starts_with(' '))
        });
        if tight == Some(true) { "=" } else { " = " }
    }

    /// Written to a temporary file first, then put in place, so a crash never
    /// leaves the game a half-written settings file.
    pub fn save(&self) -> Result<(), String> {
        let text = if self.format == "json" {
            serde_json::to_string_pretty(&self.json).map_err(|e| e.to_string())? + "\n"
        } else {
            self.lines.iter().map(|l| format!("{l}\n")).collect()
        };
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let temp = self.path.with_extension("cerberus-tmp");
        fs::write(&temp, text).map_err(|e| format!("{}: {e}", temp.display()))?;
        fs::rename(&temp, &self.path).map_err(|e| tr!("Não foi possível gravar {0}: {1}", self.path.display(), e))
    }
}

impl Offer {
    /// The first of a config's places that exists, else the first that resolves here.
    fn path_of(&self, config: &ConfigSpec, install: &Path) -> Option<PathBuf> {
        let places: Vec<PathBuf> = config.file.iter().filter_map(|p| resolve(&p.path.replace('\\', "/"), install)).collect();
        places.iter().find(|p| p.is_file()).cloned().or_else(|| places.into_iter().next())
    }

    fn files(&self, install: &Path) -> BTreeMap<String, ConfigFile> {
        self.configs
            .iter()
            .filter_map(|c| Some((c.id.clone(), ConfigFile::open(self.path_of(c, install)?, &c.format))))
            .collect()
    }

    /// Each setting with the choice it's on now and whether it applies.
    pub fn view(&self, install: &Path, picks: &BTreeMap<String, usize>) -> GameSettings {
        let files = self.files(install);
        let settings = self
            .settings
            .iter()
            .map(|s| {
                let (current, applies) = match files.get(&s.target) {
                    Some(file) => {
                        let value = |k: &str| file.get(k).or_else(|| s.defaults.get(k).cloned()).unwrap_or_default();
                        // The game may write a value its own way (`1.0` for `1`), so matching is loose;
                        // when nothing matches, the choice holding the game's defaults.
                        let current = s
                            .choices
                            .iter()
                            .position(|c| !c.set.is_empty() && c.set.iter().all(|(k, v)| same(&value(k), v)))
                            .or_else(|| s.choices.iter().position(|c| !c.set.is_empty() && c.set.iter().all(|(k, v)| s.defaults.get(k).is_some_and(|d| same(d, v)))))
                            .unwrap_or(0);
                        let applies = s.when.iter().all(|(k, v)| same(&self.value_anywhere(&files, k), v));
                        (current, applies)
                    }
                    None => (picks.get(&s.id).copied().filter(|i| *i < s.choices.len()).unwrap_or(0), true),
                };
                let slider = s.slider.as_ref().map(|sl| {
                    let key = if sl.key.is_empty() { s.id.as_str() } else { sl.key.as_str() };
                    let now = files.get(&s.target).and_then(|f| f.get(key)).or_else(|| s.defaults.get(key).cloned());
                    let value = now.and_then(|v| v.trim().trim_matches(['"', '\'']).parse::<f64>().ok()).map_or(sl.min, |v| sl.snap(v));
                    SettingSlider { min: sl.min, max: sl.max, step: sl.step(), value, text: sl.text(value) }
                });
                GameSetting {
                    slider,
                    id: s.id.clone(),
                    label: s.label.clone(),
                    help: s.help.clone(),
                    group: s.group.clone(),
                    advanced: s.advanced,
                    choices: s.choices.iter().map(|c| c.label.clone()).collect(),
                    current,
                    applies,
                }
            })
            .collect();
        let files = files.values().map(|f| f.path.display().to_string()).collect();
        GameSettings { settings, files }
    }

    /// A key's value for a `when` condition: from whichever file has it, or a setting's default.
    fn value_anywhere(&self, files: &BTreeMap<String, ConfigFile>, key: &str) -> String {
        files
            .values()
            .find_map(|f| f.get(key))
            .or_else(|| self.settings.iter().find_map(|s| s.defaults.get(key).cloned()))
            .unwrap_or_default()
    }

    /// Puts a setting on one of its choices. File settings are written to the
    /// game's file; command-line ones go into `picks`, kept with the game.
    pub fn choose(&self, install: &Path, picks: &mut BTreeMap<String, usize>, setting: &str, choice: usize) -> Result<(), String> {
        let spec = self.settings.iter().find(|s| s.id == setting).ok_or(tr!("Essa opção não existe mais no catálogo."))?;
        let picked = spec.choices.get(choice).ok_or(tr!("Essa escolha não existe mais."))?;
        if spec.target == "args" {
            picks.insert(spec.id.clone(), choice);
            return Ok(());
        }
        let config = self.configs.iter().find(|c| c.id == spec.target).ok_or(tr!("O arquivo de configuração dessa opção não está no catálogo."))?;
        let path = self.path_of(config, install).ok_or(tr!("A pasta de configuração desse jogo não existe neste sistema."))?;
        let mut file = ConfigFile::open(path, &config.format);
        for (key, value) in &picked.set {
            file.set(key, value);
        }
        file.save()
    }

    /// Moves a slider: the number, on a step and written the game's way, goes
    /// into its file, quoted when the file had it quoted.
    pub fn set_number(&self, install: &Path, setting: &str, value: f64) -> Result<(), String> {
        let spec = self.settings.iter().find(|s| s.id == setting).ok_or(tr!("Essa opção não existe mais no catálogo."))?;
        let slider = spec.slider.as_ref().ok_or(tr!("Essa opção não é um controle deslizante."))?;
        if !value.is_finite() {
            return Err(tr!("Esse valor não é um número.").into());
        }
        let config = self.configs.iter().find(|c| c.id == spec.target).ok_or(tr!("O arquivo de configuração dessa opção não está no catálogo."))?;
        let path = self.path_of(config, install).ok_or(tr!("A pasta de configuração desse jogo não existe neste sistema."))?;
        let key = if slider.key.is_empty() { spec.id.as_str() } else { slider.key.as_str() };
        let mut file = ConfigFile::open(path, &config.format);
        let text = slider.text(slider.snap(value));
        let now = file.get(key).unwrap_or_default();
        let quoted = now.len() >= 2 && (now.starts_with('"') && now.ends_with('"'));
        file.set(key, &if quoted { format!("\"{text}\"") } else { text });
        file.save()
    }

    /// Every setting back on the game's own defaults.
    pub fn reset(&self, install: &Path, picks: &mut BTreeMap<String, usize>) -> Result<(), String> {
        for s in &self.settings {
            if s.target == "args" {
                picks.remove(&s.id);
                continue;
            }
            let default = s.choices.iter().position(|c| !c.set.is_empty() && c.set.iter().all(|(k, v)| s.defaults.get(k).is_some_and(|d| same(d, v))));
            if let Some(index) = default {
                self.choose(install, picks, &s.id, index)?;
            }
        }
        Ok(())
    }

    /// The command-line options the player's picks add when the game starts.
    pub fn args(&self, picks: &BTreeMap<String, usize>) -> Vec<String> {
        self.settings
            .iter()
            .filter(|s| s.target == "args")
            .flat_map(|s| {
                let index = picks.get(&s.id).copied().unwrap_or(0);
                s.choices.get(index).map(|c| c.args.clone()).unwrap_or_default()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn offer(format: &str, file: &str, settings: Value) -> Offer {
        offer_of(Some(&json!({
            "config": [{"id": "main", "format": format, "file": [{"path": format!("{{install}}/{file}")}]}],
            "settings": settings,
        })))
    }

    fn fullscreen(key: &str, on: &str, off: &str) -> Value {
        json!([{"id": "fullscreen", "label": "Full screen", "choices": [
            {"label": "Off", "set": {key: off}}, {"label": "On", "set": {key: on}}
        ], "defaults": {key: off}}])
    }

    #[test]
    fn ini_changes_only_the_line_it_sets() {
        let dir = tempfile::tempdir().unwrap();
        let original = "; Settings\r\n[Graphics]\r\nfullscreen = 0 ; keep me\r\nvsync=1\r\n\r\n[Audio]\r\nvolume=80\r\n";
        fs::write(dir.path().join("game.ini"), original).unwrap();
        let offer = offer("ini", "game.ini", fullscreen("Graphics/fullscreen", "1", "0"));
        let mut picks = BTreeMap::new();
        assert_eq!(offer.view(dir.path(), &picks).settings[0].current, 0);

        offer.choose(dir.path(), &mut picks, "fullscreen", 1).unwrap();
        let text = fs::read_to_string(dir.path().join("game.ini")).unwrap();
        assert_eq!(text, "; Settings\n[Graphics]\nfullscreen = 1 ; keep me\nvsync=1\n\n[Audio]\nvolume=80\n");
        assert_eq!(offer.view(dir.path(), &picks).settings[0].current, 1);
    }

    #[test]
    fn a_missing_key_or_section_is_added_in_the_files_style() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.ini"), "[Graphics]\nvsync=1\n").unwrap();
        let mut picks = BTreeMap::new();
        offer("ini", "game.ini", fullscreen("Graphics/fullscreen", "1", "0")).choose(dir.path(), &mut picks, "fullscreen", 1).unwrap();
        offer("ini", "game.ini", fullscreen("Video/scale", "2", "1")).choose(dir.path(), &mut picks, "fullscreen", 1).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("game.ini")).unwrap(), "[Graphics]\nvsync=1\nfullscreen=1\n\n[Video]\nscale=2\n");
    }

    #[test]
    fn toml_keeps_top_level_keys_above_tables_and_quoted_hashes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("cfg.toml"), "name = \"a # b\" # note\n\n[video]\nfps = 30\n").unwrap();
        let mut picks = BTreeMap::new();
        offer("toml", "cfg.toml", fullscreen("fullscreen", "true", "false")).choose(dir.path(), &mut picks, "fullscreen", 1).unwrap();
        offer("toml", "cfg.toml", fullscreen("video/fps", "60", "30")).choose(dir.path(), &mut picks, "fullscreen", 1).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("cfg.toml")).unwrap(), "name = \"a # b\" # note\nfullscreen = true\n\n[video]\nfps = 60\n");
    }

    #[test]
    fn json_sets_nested_values_with_their_types() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("cfg.json"), r#"{"CVars": {"gMSAAValue": 1, "other": "x"}, "Window": {"Fullscreen": {"Enabled": false}}}"#).unwrap();
        let settings = json!([
            {"id": "fs", "label": "Full screen", "choices": [{"label": "Off", "set": {"Window/Fullscreen/Enabled": "false"}}, {"label": "On", "set": {"Window/Fullscreen/Enabled": "true"}}]},
            {"id": "res", "label": "Resolution", "choices": [{"label": "1x", "set": {"CVars/gInternalResolution": "1.0"}}, {"label": "2x", "set": {"CVars/gInternalResolution": "2.0"}}], "defaults": {"CVars/gInternalResolution": "1.0"}}
        ]);
        let offer = offer("json", "cfg.json", settings);
        let mut picks = BTreeMap::new();
        let before = offer.view(dir.path(), &picks);
        assert_eq!((before.settings[0].current, before.settings[1].current), (0, 0), "a missing key reads as the game's default");
        offer.choose(dir.path(), &mut picks, "fs", 1).unwrap();
        offer.choose(dir.path(), &mut picks, "res", 1).unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("cfg.json")).unwrap()).unwrap();
        assert_eq!(saved["Window"]["Fullscreen"]["Enabled"], json!(true));
        assert_eq!(saved["CVars"]["gInternalResolution"], json!(2.0));
        assert_eq!(saved["CVars"]["other"], json!("x"));
        // Written by the game as `2` instead of `2.0`: still the 2x choice.
        fs::write(dir.path().join("cfg.json"), r#"{"CVars": {"gInternalResolution": 2}}"#).unwrap();
        assert_eq!(offer.view(dir.path(), &picks).settings[1].current, 1);
    }

    #[test]
    fn when_greys_out_and_reset_returns_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.ini"), "[V]\nmode=window\nsize=2\n").unwrap();
        let settings = json!([
            {"id": "mode", "label": "Mode", "choices": [{"label": "Window", "set": {"V/mode": "window"}}, {"label": "Full", "set": {"V/mode": "full"}}], "defaults": {"V/mode": "window"}},
            {"id": "size", "label": "Window size", "when": {"V/mode": "window"}, "choices": [{"label": "1x", "set": {"V/size": "1"}}, {"label": "2x", "set": {"V/size": "2"}}], "defaults": {"V/size": "1"}}
        ]);
        let offer = offer("ini", "game.ini", settings);
        let mut picks = BTreeMap::new();
        assert!(offer.view(dir.path(), &picks).settings[1].applies);
        offer.choose(dir.path(), &mut picks, "mode", 1).unwrap();
        assert!(!offer.view(dir.path(), &picks).settings[1].applies);
        offer.reset(dir.path(), &mut picks).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("game.ini")).unwrap(), "[V]\nmode=window\nsize=1\n");
    }

    #[test]
    fn sliders_snap_to_their_steps_and_keep_the_games_format() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("game.ini"), "[audio]\nvolume=80\nmaster=\"0.50\"\n").unwrap();
        let offer = offer("ini", "game.ini", json!([
            {"id": "volume", "label": "Volume", "slider": {"key": "audio/volume", "min": 0, "max": 100, "step": 5, "format": "%d"}},
            {"id": "master", "label": "Master", "slider": {"key": "audio/master", "min": 0, "max": 1, "step": 0.05, "format": "%.2f"}}
        ]));
        let now = offer.view(dir.path(), &BTreeMap::new());
        assert_eq!(now.settings[0].slider.as_ref().unwrap().value, 80.0);
        assert_eq!(now.settings[1].slider.as_ref().unwrap().text, "0.50");
        offer.set_number(dir.path(), "volume", 62.0).unwrap();
        offer.set_number(dir.path(), "master", 0.333).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("game.ini")).unwrap(), "[audio]\nvolume=60\nmaster=\"0.35\"\n");
    }

    #[test]
    fn command_line_choices_are_kept_per_game() {
        let offer = offer_of(Some(&json!({"config": [], "settings": [
            {"id": "renderer", "label": "Renderer", "in": "args", "choices": [{"label": "Auto", "args": []}, {"label": "Vulkan", "args": ["--vulkan"]}]}
        ]})));
        let mut picks = BTreeMap::new();
        assert!(offer.args(&picks).is_empty());
        offer.choose(Path::new("."), &mut picks, "renderer", 1).unwrap();
        assert_eq!(offer.args(&picks), vec!["--vulkan"]);
    }

    #[test]
    fn files_outside_the_players_folders_are_refused() {
        let offer = offer_of(Some(&json!({
            "config": [{"format": "ini", "file": [{"path": "C:/Windows/win.ini"}]}, {"id": "b", "format": "ini", "file": [{"path": "{install}/../x.ini"}]}],
            "settings": [{"id": "x", "label": "X", "choices": [{"label": "a", "set": {"k": "1"}}, {"label": "b", "set": {"k": "2"}}]}]
        })));
        assert!(offer.is_empty());
    }
}

#[cfg(test)]
#[test]
#[ignore = "reads the downloaded catalog on this PC"]
fn real_catalog_offers() {
    let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher/cache/pc-catalog.json");
    let catalog: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let apps = catalog["apps"].as_array().unwrap();
    let (mut with, mut usable, mut total, mut kept) = (0, 0, 0, 0);
    for app in apps {
        let Some(raw) = app["boot"].get("settings") else { continue };
        with += 1;
        total += raw["settings"].as_array().map_or(0, Vec::len);
        let offer = offer_of(Some(raw));
        kept += offer.len();
        if !offer.is_empty() { usable += 1 } else { println!("sem opções usáveis: {}", app["id"]); }
    }
    println!("jogos com opções: {with}, usáveis: {usable}; opções: {total}, aceitas: {kept}");
}
