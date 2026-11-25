//! Clássicos: the console games found in the player's ROM folders. Each shows
//! its box art (from the libretro thumbnails, by its No-Intro name), its
//! console, the emulator that opens it on this PC, and whether a recomp also
//! plays it natively. Every classic has its own page; a recomp is one more way
//! to play it, linked from there.

use cerberus_shared::tr;
use cerberus_shared::{ClassicPlayer, RecompEntry, RomView};
use leptos::prelude::*;

use crate::art::Poster;
use crate::{Library, Route};

/// The console a ROM is for: its file type, else the folder it sits in
/// ("D:\ROMS\Playstation\..."), for disc images.
pub fn console_of(rom: &RomView) -> &'static str {
    let by_system = match rom.system.as_str() {
        "nes" => "nes",
        "snes" => "snes",
        "n64" => "n64",
        "gb" => "gb",
        "gbc" => "gbc",
        "gba" => "gba",
        "ds" => "ds",
        "3ds" => "3ds",
        "genesis" => "genesis",
        "switch" => "switch",
        "32x" => "32x",
        "sms" => "sms",
        "gg" => "gg",
        _ => "",
    };
    if !by_system.is_empty() {
        return by_system;
    }
    let path = rom.path.to_lowercase().replace('\\', "/");
    let has = |w: &str| path.split('/').any(|part| part.contains(w));
    if has("xbox 360") || has("x360") {
        "x360"
    } else if has("xbox") {
        "xbox"
    } else if has("playstation 3") || has("ps3") {
        "ps3"
    } else if has("playstation 2") || has("ps2") {
        "ps2"
    } else if has("psp") {
        "psp"
    } else if has("playstation") || has("psx") || has("ps1") {
        "ps1"
    } else if has("gamecube") || has("gcn") {
        "gc"
    } else if has("wii u") || has("wiiu") {
        "wiiu"
    } else if has("wii") {
        "wii"
    } else if has("dreamcast") {
        "dc"
    } else if has("saturn") {
        "saturn"
    } else {
        ""
    }
}

/// Consoles RetroArch plays in Cerberus (cores in Configurações › Emulação).
const EMULATED: &[&str] = &["nes", "snes", "n64", "gb", "gbc", "gba", "genesis", "sms", "gg", "ps1", "ps2", "psp", "gc", "wii"];

/// Cerberus has some emulator for the console, installed or not.
fn emulation_exists(console: &str) -> bool {
    EMULATED.contains(&console) || matches!(console, "ps3" | "wiiu")
}

/// The console RetroArch would play this copy of a game on, if it plays it at all.
pub fn emulated_console(path: &str) -> Option<&'static str> {
    let rom = RomView { path: path.to_string(), title: String::new(), system: system_of(path).to_string(), region: String::new(), playtime_secs: 0, last_played: None, discs: Vec::new() };
    let console = console_of(&rom);
    EMULATED.contains(&console).then_some(console)
}

/// Who opens this console's classics here, as Play decides it.
pub fn player_for<'a>(players: &'a [ClassicPlayer], console: &str) -> Option<&'a ClassicPlayer> {
    players.iter().find(|p| p.console == console)
}

/// The emulator that will open it, when nothing is missing.
pub fn ready_player(players: &[ClassicPlayer], console: &str) -> Option<String> {
    player_for(players, console).filter(|p| p.problem.is_none()).and_then(|p| p.name.clone())
}

/// A classic's second line: its console and who opens it, or what's missing.
pub fn classic_sub(rom: &RomView, console: &str, players: &[ClassicPlayer]) -> String {
    let name = console_name(console);
    let player = player_for(players, console);
    match (player.and_then(|p| p.name.clone()), player.and_then(|p| p.problem.clone())) {
        (Some(emulator), None) => match rom.last_played {
            Some(t) => format!("{name} · {emulator} · {}", crate::format::ago(t)),
            None => format!("{name} · {emulator}"),
        },
        (Some(_), Some(_)) => tr!("{} · falta o núcleo", name),
        _ => tr!("{} · sem emulador", name),
    }
}

/// A recomp of this console: a 3DS file isn't a Switch game, even when it was
/// picked as one's copy (the same rule as the backend's `console_fits`).
fn fits(console: &str, origin: &str) -> bool {
    origin.is_empty()
        || matches!(origin, "other" | "arcade")
        || console == origin
        || matches!((console, origin), ("gb", "gbc") | ("gbc", "gb") | ("switch", "switch2") | ("32x", "s32x"))
}

/// The recomp that plays this very file natively, if one is in the catalog.
pub fn recomp_of<'a>(recomps: &'a [RecompEntry], rom: &RomView) -> Option<&'a RecompEntry> {
    let console = console_of(rom);
    recomps.iter().find(|e| e.owned_copy.as_deref() == Some(rom.path.as_str()) && fits(console, &e.origin))
}

/// How a classic plays here, the one answer every screen gives: the emulator
/// that opens it, the recomp that plays it (installed or not), or why nothing
/// does yet.
pub struct Resolution {
    pub emulator: Option<String>,
    pub recomp: Option<RecompEntry>,
    /// The recomp, installed in the library.
    pub recomp_game: Option<cerberus_shared::Game>,
    /// Why no emulator opens it (core missing, no emulator for the console).
    pub problem: String,
}

impl Resolution {
    /// Something here plays it: an emulator, or an installed recomp.
    pub fn playable(&self) -> bool {
        self.emulator.is_some() || self.recomp_game.is_some()
    }
}

pub fn resolve(rom: &RomView, players: &[ClassicPlayer], games: &[cerberus_shared::Game], recomps: &[RecompEntry]) -> Resolution {
    let console = console_of(rom);
    let player = player_for(players, console);
    let emulator = ready_player(players, console);
    let recomp_game = recomp_game(rom, games, recomps).cloned();
    let recomp = recomp_game
        .as_ref()
        .and_then(|g| match &g.source {
            cerberus_shared::Source::Recomp { recomp_id, .. } => recomps.iter().find(|e| e.id == *recomp_id).cloned(),
            _ => None,
        })
        .or_else(|| recomp_of(recomps, rom).cloned());
    let problem = match player {
        Some(p) if p.name.is_some() => p.problem.clone().unwrap_or_default(),
        _ if console.is_empty() => tr!("O arquivo não diz de qual console é.").to_string(),
        _ => tr!("O Cerberus ainda não tem emulador para {}.", console_name(console)),
    };
    Resolution { emulator, recomp, recomp_game, problem }
}

/// A classic as a library tile: its badge says what the page will say.
pub fn tile(rom: &RomView, players: &[ClassicPlayer], recomps: &[RecompEntry], games: &[cerberus_shared::Game]) -> crate::tiles::TileInfo {
    use crate::tiles::Tone;
    let console = console_of(rom);
    let how = resolve(rom, players, games, recomps);
    // The same words everywhere: "nativo" is the recomp, "original" the copy in its emulator.
    let badge = match (&how.emulator, &how.recomp_game, &how.recomp) {
        (Some(_), Some(_), _) => Some((tr!("Nativo e original").to_string(), Tone::Info)),
        (None, Some(_), _) => Some((tr!("Joga nativo").to_string(), Tone::Info)),
        (Some(_), None, Some(_)) => Some((tr!("Tem versão nativa").to_string(), Tone::Info)),
        (Some(_), None, None) => None,
        (None, None, _) => {
            let core = player_for(players, console).is_some_and(|p| p.name.is_some());
            Some((if core { tr!("Falta o núcleo") } else { tr!("Ainda não abre aqui") }.to_string(), Tone::Warn))
        }
    };
    let sub = match (&how.emulator, &how.recomp_game) {
        (None, Some(g)) => format!("{} · {}", console_name(console), g.name),
        _ => classic_sub(rom, console, players),
    };
    crate::tiles::classic_tile(display_name(rom), sub, box_art(rom, console), rom.path.clone(), badge, how.playable())
}

/// The classic is a version of an installed recomp (the recomp was given this
/// very file, or found it): it is that game, played from its page ("Jogar o
/// original"), not a second entry next to it.
pub fn merged(rom: &RomView, games: &[cerberus_shared::Game], recomps: &[RecompEntry]) -> bool {
    recomp_game(rom, games, recomps).is_some()
}

/// The installed recomp that plays this file: the one the catalog matches to it
/// first, else one the player gave it to.
pub fn recomp_game<'a>(rom: &RomView, games: &'a [cerberus_shared::Game], recomps: &[RecompEntry]) -> Option<&'a cerberus_shared::Game> {
    let recomp_id = |g: &cerberus_shared::Game| match &g.source {
        cerberus_shared::Source::Recomp { recomp_id, .. } => Some(recomp_id.clone()),
        _ => None,
    };
    let matched = |g: &&cerberus_shared::Game| {
        recomp_id(g).is_some_and(|id| recomps.iter().any(|e| e.id == id && e.owned_copy.as_deref() == Some(rom.path.as_str())))
    };
    let given = |g: &&cerberus_shared::Game| recomp_id(g).is_some() && g.game_copy.as_deref() == Some(rom.path.as_str());
    // Only a recomp of the file's own console: a 3DS game handed to a Switch recomp isn't that game.
    let console = console_of(rom);
    let same_console = |g: &&cerberus_shared::Game| {
        recomp_id(g).and_then(|id| recomps.iter().find(|e| e.id == id)).is_none_or(|e| fits(console, &e.origin))
    };
    games.iter().filter(same_console).find(matched).or_else(|| games.iter().filter(same_console).find(given))
}

/// The player's classics that stand on their own: every ROM but those an
/// installed recomp already plays.
pub fn standalone(library: Library) -> Vec<RomView> {
    let games = library.games.get().and_then(Result::ok).unwrap_or_default();
    let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
    library.roms.get().unwrap_or_default().into_iter().filter(|r| !merged(r, &games, &recomps)).collect()
}

/// The classics whose name has the words searched, A to Z.
pub fn matching(roms: Vec<RomView>, needle: &str) -> Vec<RomView> {
    let mut list: Vec<RomView> = roms.into_iter().filter(|r| needle.is_empty() || {
        let hay = format!("{} {}", display_name(r), console_name(console_of(r))).to_lowercase();
        needle.split_whitespace().all(|w| hay.contains(w))
    }).collect();
    list.sort_by_key(|r| display_name(r).to_lowercase());
    list
}

/// A cartridge's console from its file type.
fn system_of(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "z64" | "n64" | "v64" => "n64",
        "nes" | "fds" | "unf" => "nes",
        "sfc" | "smc" => "snes",
        "gb" => "gb",
        "gbc" => "gbc",
        "gba" => "gba",
        "md" | "gen" | "smd" => "genesis",
        "sms" => "sms",
        "gg" => "gg",
        _ => "",
    }
}

/// Starts the original game in RetroArch.
pub fn play_original(library: Library, path: String, console: String) {
    leptos::task::spawn_local(async move {
        if library.report(crate::api::play_classic(path, console).await).is_some() {
            library.toast(tr!("Abrindo o jogo..."));
        }
    });
}

pub fn console_name(console: &str) -> &'static str {
    match console {
        "nes" => "NES",
        "snes" => "Super Nintendo",
        "n64" => "Nintendo 64",
        "gb" => "Game Boy",
        "gbc" => "Game Boy Color",
        "gba" => "Game Boy Advance",
        "ds" => "Nintendo DS",
        "3ds" => "Nintendo 3DS",
        "genesis" => "Mega Drive",
        "32x" => "32X",
        "sms" => "Master System",
        "gg" => "Game Gear",
        "switch" => "Nintendo Switch",
        "ps1" => "PlayStation",
        "ps2" => "PlayStation 2",
        "ps3" => "PlayStation 3",
        "psp" => "PSP",
        "xbox" => "Xbox",
        "x360" => "Xbox 360",
        "gc" => "GameCube",
        "wii" => "Wii",
        "wiiu" => "Wii U",
        "dc" => "Dreamcast",
        "saturn" => "Saturn",
        _ => tr!("Console"),
    }
}

/// The libretro thumbnails repository for a console.
fn thumbnails_repo(console: &str) -> Option<&'static str> {
    Some(match console {
        "nes" => "Nintendo_-_Nintendo_Entertainment_System",
        "snes" => "Nintendo_-_Super_Nintendo_Entertainment_System",
        "n64" => "Nintendo_-_Nintendo_64",
        "gb" => "Nintendo_-_Game_Boy",
        "gbc" => "Nintendo_-_Game_Boy_Color",
        "gba" => "Nintendo_-_Game_Boy_Advance",
        "ds" => "Nintendo_-_Nintendo_DS",
        "3ds" => "Nintendo_-_Nintendo_3DS",
        "genesis" => "Sega_-_Mega_Drive_-_Genesis",
        "32x" => "Sega_-_32X",
        "sms" => "Sega_-_Master_System_-_Mark_III",
        "gg" => "Sega_-_Game_Gear",
        "ps1" => "Sony_-_PlayStation",
        "ps2" => "Sony_-_PlayStation_2",
        "psp" => "Sony_-_PlayStation_Portable",
        "gc" => "Nintendo_-_GameCube",
        "wii" => "Nintendo_-_Wii",
        "xbox" => "Microsoft_-_Xbox",
        "x360" => "Microsoft_-_Xbox_360",
        "dc" => "Sega_-_Dreamcast",
        "saturn" => "Sega_-_Saturn",
        _ => return None,
    })
}

/// The game's name from its file: "Monster Hunter Generations (USA).3ds" ->
/// "Monster Hunter Generations (USA)", the name libretro's art goes by.
fn file_name(rom: &RomView) -> String {
    let base = rom.path.rsplit(['\\', '/']).next().unwrap_or(&rom.path);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem).to_string()
}

/// The name shown: without the region and revision tags.
pub fn display_name(rom: &RomView) -> String {
    let name = file_name(rom);
    let cut = name.find(" (").unwrap_or(name.len());
    name[..cut].trim().to_string()
}

pub fn box_art(rom: &RomView, console: &str) -> Vec<String> {
    let Some(repo) = thumbnails_repo(console) else { return Vec::new() };
    // libretro replaces these characters in file names with "_".
    let name: String = file_name(rom).chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect();
    vec![format!("https://raw.githubusercontent.com/libretro-thumbnails/{}/master/Named_Boxarts/{}.png", repo, encoded)]
}

/// The classics, as a grid. `embedded`: under the Library's other games, with
/// nothing at all when no classic matches.
#[component]
pub fn ClassicsGrid(needle: String, #[prop(optional)] embedded: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let grid = move || {
        let Some(all) = library.roms.get() else {
            return (!embedded).then(|| view! { <p class="muted" role="status">{tr!("Procurando seus clássicos…")}</p> }).into_any();
        };
        let players = library.players.get().unwrap_or_default();
        let recomps: Vec<RecompEntry> = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = matching(all, &needle);
        if list.is_empty() {
            if embedded {
                return ().into_any();
            }
            return view! {
                <div class="empty">
                    <h2>{tr!("Nenhum clássico encontrado")}</h2>
                    <p class="muted">{tr!("Adicione as pastas onde você guarda ROMs e imagens de disco em Configurações › Biblioteca e ROMs.")}</p>
                    <button type="button" class="btn" on:click=move |_| library.go(Route::Settings)>{tr!("Abrir Configurações")}</button>
                </div>
            }
            .into_any();
        }
        let count = list.len();
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let tiles = list.iter().map(|rom| view! { <crate::tiles::Tile info=tile(rom, &players, &recomps, &games) /> }).collect_view();
        let heading = match (embedded, count) {
            (true, 1) => tr!("1 clássico nas suas pastas").to_string(),
            (true, n) => tr!("{} clássicos nas suas pastas", n),
            (false, 1) => tr!("1 clássico").to_string(),
            (false, n) => tr!("{} clássicos", n),
        };
        view! {
            <section class="catalog-results">
                <h2>{heading}</h2>
                <div class="grid">{tiles}</div>
            </section>
        }
        .into_any()
    };
    view! { {grid} }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(path: &str, system: &str) -> RomView {
        RomView { path: path.into(), title: String::new(), system: system.into(), region: String::new(), playtime_secs: 0, last_played: None, discs: Vec::new() }
    }

    #[test]
    fn consoles_come_from_the_file_or_its_folder() {
        assert_eq!(console_of(&rom("D:/ROMS/x/Mario.z64", "n64")), "n64");
        assert_eq!(console_of(&rom("D:\\ROMS\\Xbox 360\\FIFA Street.iso", "")), "x360");
        assert_eq!(console_of(&rom("D:\\ROMS\\Playstation\\Crash\\Crash Bandicoot (USA).cue", "")), "ps1");
        assert_eq!(display_name(&rom("D:/x/Monster Hunter Generations (USA).3ds", "3ds")), "Monster Hunter Generations");
    }
}

/// A classic's page, built like every other game's: the head with the cover,
/// the way to play and the favorite; tabs for the overview and its file; the
/// player's own status, rating and notes. Its backdrop is the console itself,
/// not a box cover stretched past its resolution.
#[component]
pub fn ClassicPage(path: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let wanted = StoredValue::new(path);
    let manage = RwSignal::new(false);
    // Status, rating, notes and favorite live apart from the ROM list.
    let note_id = format!("classic:{}", wanted.get_value());
    let note = LocalResource::new({
        let id = note_id.clone();
        move || {
            let id = id.clone();
            async move { crate::api::player_note(&id).await }
        }
    });
    // Play time may have changed since the list was read.
    library.roms.refetch();
    let content = move || {
        let list = library.roms.get()?;
        let Some(rom) = list.into_iter().find(|r| r.path == wanted.get_value()) else {
            return Some(view! { <div class="page"><p class="muted">{tr!("Esse jogo não está mais nas suas pastas de ROMs.")}</p></div> }.into_any());
        };
        let console = console_of(&rom);
        let title = display_name(&rom);
        let players = library.players.get().unwrap_or_default();
        let games = library.games.get().and_then(Result::ok).unwrap_or_default();
        let recomps = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let how = resolve(&rom, &players, &games, &recomps);
        // Merged into an installed recomp: one game, one page; this copy is its Original tab.
        if let Some(g) = how.recomp_game.clone() {
            crate::game::open_original(library, g.id);
            return Some(view! { <div class="page"><p class="muted">{tr!("Abrindo…")}</p></div> }.into_any());
        }
        let emulator = how.emulator.clone();
        let art = box_art(&rom, console);
        let player_note = note.get().unwrap_or_default();
        let id = format!("classic:{}", rom.path);

        // ---------- the head ----------
        let (path, console_id) = (rom.path.clone(), console.to_string());
        let action = match (&emulator, &how.recomp_game) {
            (Some(name), _) => {
                let name = name.clone();
                view! {
                    <button type="button" class="btn big main play" on:click=move |_| play_original(library, path.clone(), console_id.clone())>
                        <crate::icons::Svg icon=crate::icons::Icon::Play size=20 />{tr!("Jogar no {}", name)}
                    </button>
                }.into_any()
            }
            // No emulator here, but its recomp is installed: that is how it plays.
            (None, Some(g)) => {
                let g = g.clone();
                view! {
                    <button type="button" class="btn big main play" on:click=move |_| crate::play_game(library, g.clone())>
                        <crate::icons::Svg icon=crate::icons::Icon::Play size=20 />{tr!("Jogar pelo recomp")}
                    </button>
                }.into_any()
            }
            (None, None) => ().into_any(),
        };
        let to_recomp = how.recomp.clone().map(|e| {
            let route = how.recomp_game.as_ref().map(|g| Route::Game(g.id.clone())).unwrap_or_else(|| Route::Game(format!("recomp:{}", e.id)));
            view! { <button type="button" class="btn big glass" on:click=move |_| library.go(route.clone())>{tr!("Ver o recomp")}</button> }
        });
        let heart = {
            let (id, on) = (id.clone(), player_note.favorite);
            let label = if on { tr!("Tirar dos favoritos") } else { tr!("Adicionar aos favoritos") };
            view! {
                <button type="button" class="btn glass icon-btn fav-btn" class:on=on title=label aria-label=label aria-pressed=on.to_string()
                    on:click=move |_| {
                        let id = id.clone();
                        leptos::task::spawn_local(async move {
                            if library.report(crate::api::set_favorite(id, !on).await).is_some() {
                                note.refetch();
                                library.toast(if on { tr!("Tirado dos favoritos.") } else { tr!("Adicionado aos favoritos.") });
                            }
                        });
                    }>
                    <crate::icons::Svg icon=if on { crate::icons::Icon::HeartFill } else { crate::icons::Icon::Heart } size=20 />
                </button>
            }
        };
        let who = match (&emulator, &how.recomp_game) {
            (Some(name), _) => format!("{} · {}", console_name(console), name),
            (None, Some(g)) => format!("{} · {}", console_name(console), g.name),
            (None, None) => tr!("{} · sem emulador", console_name(console)),
        };
        let played = match (rom.playtime_secs, rom.last_played) {
            (0, None) => tr!("Você ainda não jogou").to_string(),
            (t, Some(last)) => tr!("Jogou por {} · última vez {}", crate::format::playtime(t), crate::format::ago(last)),
            (t, None) => tr!("Jogou por {}", crate::format::playtime(t)),
        };

        // ---------- nothing plays it yet: why, and where to go ----------
        let blocked = (!how.playable()).then(|| {
            let core = player_for(&players, console).is_some_and(|p| p.name.is_some());
            let catalog_recomp = how.recomp.clone();
            // Installed recomps of the same series ("Monster Hunter" for Generations): one of them may be what the player wants.
            let series = title.split([':', '-']).next().unwrap_or(&title).split_whitespace().take(2).collect::<Vec<_>>().join(" ").to_lowercase();
            // By the recomp's name or by its game's ("MHGU Recomp" is Monster Hunter Generations Ultimate).
            let game_of = |g: &cerberus_shared::Game| match &g.source {
                cerberus_shared::Source::Recomp { recomp_id, .. } => recomps.iter().find(|e| e.id == *recomp_id).map(|e| e.game.clone()).unwrap_or_default(),
                _ => String::new(),
            };
            let related: Vec<(cerberus_shared::Game, String)> = games.iter()
                .filter(|g| matches!(g.source, cerberus_shared::Source::Recomp { .. }) && !series.is_empty()
                    && (g.name.to_lowercase().contains(&series) || game_of(g).to_lowercase().contains(&series)))
                .map(|g| (g.clone(), { let n = game_of(g); if n.is_empty() { g.name.clone() } else { n } }))
                .collect();
            let related_view = (!related.is_empty()).then(|| {
                let names = related.iter().map(|(_, n)| n.clone()).collect::<Vec<_>>().join(", ");
                let related: Vec<cerberus_shared::Game> = related.into_iter().map(|(g, _)| g).collect();
                let buttons = related.into_iter().map(|g| {
                    let id = g.id.clone();
                    view! { <button type="button" class="btn sm main" on:click=move |_| library.go(Route::Game(id.clone()))>{tr!("Abrir {}", g.name)}</button> }
                }).collect_view();
                view! {
                    <p class="muted small">{tr!("Você tem {} instalado: é outro jogo da mesma série, já nativo no PC.", names)}</p>
                    <div class="row">{buttons}</div>
                }
            });
            // The series name finds its other games too ("Monster Hunter" for Generations).
            let search_for = title.split([':', '-']).next().unwrap_or(&title).split_whitespace().take(2).collect::<Vec<_>>().join(" ");
            let console_id = console.to_string();
            view! {
                <section class="card blocked" role="status">
                    <h3>{tr!("Ainda não dá para jogar aqui")}</h3>
                    <p>{how.problem.clone()}</p>
                    {related_view}
                    <div class="row">
                        {catalog_recomp.map(|e| {
                            let rid = e.id.clone();
                            view! { <button type="button" class="btn sm main" on:click=move |_| library.go(Route::Game(format!("recomp:{rid}")))>{tr!("Ver o recomp {}", e.name)}</button> }
                        })}
                        {(core || emulation_exists(console)).then(|| view! {
                            <button type="button" class="btn sm" on:click=move |_| crate::settings_page::open_console(library, console_id.clone())>{tr!("Escolher o emulador")}</button>
                        })}
                        <button type="button" class="btn sm" on:click=move |_| crate::discover::open_search(library, search_for.clone())>{tr!("Procurar recomps desse jogo")}</button>
                    </div>
                </section>
            }
        });

        // ---------- the overview ----------
        let history_id = format!("classic:{}", rom.path);
        let (ach_path, ach_console) = (rom.path.clone(), console.to_string());
        let achievements = LocalResource::new(move || {
            let (p, c) = (ach_path.clone(), ach_console.clone());
            async move {
                match crate::api::classic_achievements(&p, &c).await {
                    Ok(Some(d)) => Ok(d),
                    Ok(None) => Err(String::new()),
                    Err(e) => Err(e),
                }
            }
        });
        let on_all = Callback::new(move |_| {
            if let Some(Ok(d)) = achievements.get_untracked() {
                library.go(Route::AchievementGame("retro".into(), d.game.key.clone()));
            }
        });
        let facts = [
            (tr!("Console"), console_name(console).to_string()),
            (tr!("Região"), if rom.region.is_empty() { "—".to_string() } else { rom.region.clone() }),
        ]
        .into_iter()
        .chain((rom.discs.len() > 1).then(|| (tr!("Discos"), rom.discs.len().to_string())))
        .chain(emulator.clone().map(|e| (tr!("Abre no"), e)))
        .map(|(label, value)| view! { <div><dt>{label}</dt><dd>{value}</dd></div> })
        .collect_view();
        // The play chart once there's something to chart.
        let has_played = rom.playtime_secs > 0 || rom.last_played.is_some();

        // ---------- its file ----------
        let folder = rom.path.clone();
        let open_folder = move |_| {
            let folder = folder.clone();
            leptos::task::spawn_local(async move {
                library.report(crate::api::open_rom_folder(folder).await);
            });
        };
        let file_note = if rom.discs.len() > 1 {
            match emulator.as_deref() {
                Some("RetroArch") if matches!(console, "ps1" | "gc") => tr!("O RetroArch abre o disco 1. Quando o jogo pedir outro, troque no menu do RetroArch (F1) › Controle de disco. O Cerberus nunca altera esses arquivos.").to_string(),
                Some(name) => tr!("O {} abre o disco 1; quando o jogo pedir outro, troque pelo menu dele. O Cerberus nunca altera esses arquivos.", name),
                None => tr!("O Cerberus nunca altera esses arquivos.").to_string(),
            }
        } else {
            match emulator.as_deref() {
                Some(name) => tr!("O {} abre esse arquivo como está; o Cerberus nunca o altera.", name),
                None => tr!("O Cerberus nunca altera esse arquivo.").to_string(),
            }
        };
        let files = if rom.discs.len() > 1 {
            view! { <ol class="disc-list">{rom.discs.iter().map(|d| view! { <li class="path">{d.clone()}</li> }).collect_view()}</ol> }.into_any()
        } else {
            view! { <span class="path">{rom.path.clone()}</span> }.into_any()
        };
        let console_pick = console.to_string();
        let file_card = view! {
            <section class="card classic-file">
                <div class="card-h">
                    <h3>{if rom.discs.len() > 1 { tr!("Arquivos do jogo") } else { tr!("Arquivo do jogo") }}</h3>
                    <button type="button" class="btn sm" on:click=open_folder>{tr!("Abrir pasta")}</button>
                </div>
                {files}
                <p class="muted small">{file_note}</p>
            </section>
            <section class="card">
                <div class="card-h">
                    <h3>{tr!("Quem abre")}</h3>
                    {(emulator.is_some() || emulation_exists(console)).then(|| view! {
                        <button type="button" class="btn sm" on:click=move |_| crate::settings_page::open_console(library, console_pick.clone())>{tr!("Escolher o emulador")}</button>
                    })}
                </div>
                <p class="muted small">{match &emulator {
                    Some(name) => tr!("Os jogos de {} abrem no {}. Dá para trocar por outro emulador nas Configurações › Emulação.", console_name(console), name),
                    None => how.problem.clone(),
                }}</p>
            </section>
        };

        let tab = move |on: bool, label: &'static str, icon: crate::icons::Icon| {
            view! {
                <button type="button" class="tab with-icon" role="tab" class:on=move || manage.get() == on aria-selected=move || (manage.get() == on).to_string()
                    tabindex=move || if manage.get() == on { "0" } else { "-1" } on:click=move |_| manage.set(on)>
                    <crate::icons::Svg icon size=16 />{label}
                </button>
            }
        };
        let overview = view! {
            <div class="ov">
                <div class="ov-main">
                    {blocked}
                    {has_played.then(|| view! { <crate::game_overview::PlayCard id=history_id.clone() total=rom.playtime_secs last=rom.last_played /> })}
                    <crate::game_notes::NoteCardFor id=id.clone() status=player_note.status rating=player_note.rating notes=player_note.notes.clone() />
                    <crate::game::History id=history_id.clone() />
                </div>
                <aside class="ov-rail">
                    <crate::game_overview::AchievementsCard data=achievements on_all />
                    <section class="card rail-card"><h2>{tr!("Detalhes")}</h2><dl class="facts">{facts}</dl></section>
                </aside>
            </div>
        };
        let console_bg = crate::settings_page::console_art(console);
        Some(
            view! {
                <div class="gp-hero cl-hero">
                    {console_bg.map(|src| view! { <img class="cl-console" src=src alt="" /> })}
                    <div class="gp-shade"></div>
                </div>
                <div class="page gp-page">
                    <section class="gp-head">
                        <div class="gp-cover"><Poster sources=art title=title.clone() /></div>
                        <div class="gp-info">
                            <h1>{title.clone()}</h1>
                            <span class="gp-source">{who}</span>
                            <span class="gp-played"><crate::icons::Svg icon=crate::icons::Icon::Clock size=15 />{played}</span>
                        </div>
                        <div class="gp-act">{heart}{to_recomp}{action}</div>
                    </section>
                    <nav class="tabs" role="tablist" aria-label=tr!("Seções do jogo")>
                        {tab(false, tr!("Visão geral"), crate::icons::Icon::Info)}
                        {tab(true, tr!("Gerenciar"), crate::icons::Icon::Edit)}
                    </nav>
                    <div class="tab-body" role="tabpanel">
                        <h2 class="vh">{move || if manage.get() { tr!("Gerenciar") } else { tr!("Visão geral") }}</h2>
                        <div class:hidden=move || manage.get()>{overview}</div>
                        <div class="cl-manage" class:hidden=move || !manage.get()>{file_card}</div>
                    </div>
                </div>
            }
            .into_any(),
        )
    };
    view! {
        <div class="gp">
            <button type="button" class="btn glass back" on:click=move |_| library.back()>
                <crate::icons::Svg icon=crate::icons::Icon::Back size=18 />{tr!("Voltar")}
            </button>
            {content}
        </div>
    }
}


/// The player's own copy as a way to play its recomp: the "Original" tab of
/// the recomp's page. Its emulator and Play, its time, its achievements and its
/// file, all in one place with the native version.
#[component]
pub fn OriginalTab(rom: RomView) -> impl IntoView {
    let library = expect_context::<Library>();
    let console = console_of(&rom);
    let players = library.players.get_untracked().unwrap_or_default();
    let emulator = ready_player(&players, console);
    let problem = player_for(&players, console).and_then(|p| p.problem.clone())
        .unwrap_or_else(|| tr!("O Cerberus ainda não tem emulador para {}.", console_name(console)));
    let (path, console_id) = (rom.path.clone(), console.to_string());
    let console_pick = console.to_string();
    let play = match emulator.clone() {
        Some(name) => view! {
            <button type="button" class="btn main" on:click=move |_| play_original(library, path.clone(), console_id.clone())>
                <crate::icons::Svg icon=crate::icons::Icon::Play size=18 />{tr!("Jogar o original no {}", name)}
            </button>
        }.into_any(),
        None => view! { <p class="muted small">{problem.clone()}</p> }.into_any(),
    };
    let id = format!("classic:{}", rom.path);
    let (ach_path, ach_console) = (rom.path.clone(), console.to_string());
    let achievements = LocalResource::new(move || {
        let (p, c) = (ach_path.clone(), ach_console.clone());
        async move {
            match crate::api::classic_achievements(&p, &c).await {
                Ok(Some(d)) => Ok(d),
                Ok(None) => Err(String::new()),
                Err(e) => Err(e),
            }
        }
    });
    let on_all = Callback::new(move |_| {
        if let Some(Ok(d)) = achievements.get_untracked() {
            library.go(Route::AchievementGame("retro".into(), d.game.key.clone()));
        }
    });
    let has_played = rom.playtime_secs > 0 || rom.last_played.is_some();
    view! {
        <div class="ov">
            <div class="ov-main">
                <section class="card original-way">
                    <div class="card-h">
                        <div>
                            <h3>{tr!("A sua cópia de {}", console_name(console))}</h3>
                            <p class="muted small">{match &emulator {
                                Some(name) => tr!("{} abre o jogo original, como ele era. O recomp é a versão nativa para PC.", name),
                                None => tr!("O jogo original, como ele era. Por enquanto só pelo recomp.").to_string(),
                            }}</p>
                        </div>
                        <div class="row">
                            {play}
                            {emulator.is_some().then(|| view! {
                                <button type="button" class="link" on:click=move |_| crate::settings_page::open_console(library, console_pick.clone())>{tr!("Trocar o emulador")}</button>
                            })}
                        </div>
                    </div>
                </section>
                {has_played.then(|| view! { <crate::game_overview::PlayCard id=id.clone() total=rom.playtime_secs last=rom.last_played /> })}
                {file_card(library, &rom, emulator.clone())}
            </div>
            <aside class="ov-rail">
                <crate::game_overview::AchievementsCard data=achievements on_all />
            </aside>
        </div>
    }
}

/// A classic's file (or discs), with the promise that it stays as it is.
fn file_card(library: Library, rom: &RomView, emulator: Option<String>) -> AnyView {
    let console = console_of(rom);
    let folder = rom.path.clone();
    let open_folder = move |_| {
        let folder = folder.clone();
        leptos::task::spawn_local(async move {
            library.report(crate::api::open_rom_folder(folder).await);
        });
    };
    let note = if rom.discs.len() > 1 {
        match emulator.as_deref() {
            Some("RetroArch") if matches!(console, "ps1" | "gc") => tr!("O RetroArch abre o disco 1. Quando o jogo pedir outro, troque no menu do RetroArch (F1) › Controle de disco. O Cerberus nunca altera esses arquivos.").to_string(),
            Some(name) => tr!("O {} abre o disco 1; quando o jogo pedir outro, troque pelo menu dele. O Cerberus nunca altera esses arquivos.", name),
            None => tr!("O Cerberus nunca altera esses arquivos.").to_string(),
        }
    } else {
        match emulator.as_deref() {
            Some(name) => tr!("O {} abre esse arquivo como está; o Cerberus nunca o altera.", name),
            None => tr!("O Cerberus nunca altera esse arquivo.").to_string(),
        }
    };
    let files = if rom.discs.len() > 1 {
        view! { <ol class="disc-list">{rom.discs.iter().map(|d| view! { <li class="path">{d.clone()}</li> }).collect_view()}</ol> }.into_any()
    } else {
        view! { <span class="path">{rom.path.clone()}</span> }.into_any()
    };
    view! {
        <section class="card classic-file">
            <div class="card-h">
                <h3>{if rom.discs.len() > 1 { tr!("Arquivos do jogo") } else { tr!("Arquivo do jogo") }}</h3>
                <button type="button" class="btn sm" on:click=open_folder>{tr!("Abrir pasta")}</button>
            </div>
            {files}
            <p class="muted small">{note}</p>
        </section>
    }
    .into_any()
}
