//! "Personalizar arte": a game's cover, background and logo, picked from
//! SteamGridDB (with the player's own free key) or from a file, and framed here
//! by dragging and zooming inside a frame of the shape it is shown in.

use cerberus_shared::tr;
use cerberus_shared::{ArtGame, ArtOptions, ArtPic, Game};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::art::src;
use crate::icons::{Icon, Svg};
use crate::{Library, api};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Cover,
    Hero,
    Logo,
}

impl Kind {
    fn id(self) -> &'static str {
        match self {
            Kind::Cover => "cover",
            Kind::Hero => "hero",
            Kind::Logo => "logo",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Cover => tr!("Capa"),
            Kind::Hero => tr!("Fundo"),
            Kind::Logo => tr!("Logo"),
        }
    }
    /// The picture's finished size; its shape is the frame's.
    fn size(self) -> (f64, f64) {
        match self {
            Kind::Cover => (600.0, 900.0),
            Kind::Hero => (1920.0, 620.0),
            Kind::Logo => (1000.0, 500.0),
        }
    }
    /// Covers and backgrounds fill their frame; a logo fits in it, see-through around.
    fn fills(self) -> bool {
        self != Kind::Logo
    }
    fn current(self, game: &Game) -> Option<String> {
        match self {
            Kind::Cover => game.cover_path.clone().or(game.cover_url.clone()),
            Kind::Hero => game.hero_path.clone().or(game.hero_url.clone()),
            Kind::Logo => game.logo_path.clone(),
        }
        .map(|v| src(&v))
    }
    fn options(self, all: &ArtOptions) -> Vec<ArtPic> {
        match self {
            Kind::Cover => all.covers.clone(),
            Kind::Hero => all.heroes.clone(),
            Kind::Logo => all.logos.clone(),
        }
    }
}

/// Where the picture sits in a frame of `w` x `h`: left, top, width, height.
fn place(kind: Kind, natural: (f64, f64), zoom: f64, offset: (f64, f64), w: f64, h: f64) -> (f64, f64, f64, f64) {
    let (iw, ih) = natural;
    let base = if kind.fills() { (w / iw).max(h / ih) } else { (w / iw).min(h / ih) };
    let s = base * zoom;
    let (dw, dh) = (iw * s, ih * s);
    (w / 2.0 + offset.0 * w - dw / 2.0, h / 2.0 + offset.1 * h - dh / 2.0, dw, dh)
}

/// The framed picture at its finished size, as a `data:` address.
fn render(kind: Kind, img: &web_sys::HtmlImageElement, natural: (f64, f64), zoom: f64, offset: (f64, f64)) -> Result<String, String> {
    let (w, h) = kind.size();
    let document = web_sys::window().and_then(|w| w.document()).ok_or(tr!("Sem janela."))?;
    let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas").map_err(|_| tr!("Sem canvas."))?.dyn_into().map_err(|_| tr!("Sem canvas."))?;
    canvas.set_width(w as u32);
    canvas.set_height(h as u32);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|c| c.dyn_into().ok())
        .ok_or(tr!("Sem canvas."))?;
    if kind.fills() {
        ctx.set_fill_style_str("#000");
        ctx.fill_rect(0.0, 0.0, w, h);
    }
    let (x, y, dw, dh) = place(kind, natural, zoom, offset, w, h);
    ctx.draw_image_with_html_image_element_and_dw_and_dh(img, x, y, dw, dh).map_err(|_| tr!("Não foi possível desenhar a imagem."))?;
    let (mime, quality) = if kind.fills() { ("image/jpeg", 0.92) } else { ("image/png", 1.0) };
    canvas
        .to_data_url_with_type_and_encoder_options(mime, &wasm_bindgen::JsValue::from_f64(quality))
        .map_err(|_| tr!("Não foi possível gerar a imagem.").to_string())
}

#[component]
pub fn CustomizeDialog(game: Game) -> impl IntoView {
    let library = expect_context::<Library>();
    let id = StoredValue::new(game.id.clone());
    let current_game = RwSignal::new(game.clone());
    let kind = RwSignal::new(Kind::Cover);
    let picture = RwSignal::new(None::<String>);
    let natural = RwSignal::new((0.0f64, 0.0f64));
    let zoom = RwSignal::new(1.0f64);
    let offset = RwSignal::new((0.0f64, 0.0f64));
    let busy = RwSignal::new(false);
    let has_key = RwSignal::new(None::<bool>);
    let key_input = RwSignal::new(String::new());
    let term = RwSignal::new(game.name.clone());
    let results = RwSignal::new(Vec::<ArtGame>::new());
    let chosen = RwSignal::new(None::<u64>);
    let options = RwSignal::new(None::<ArtOptions>);
    let looking = RwSignal::new(false);
    let img_ref = NodeRef::<html::Img>::new();
    let frame_ref = NodeRef::<html::Div>::new();
    let drag = StoredValue::new(None::<(f64, f64, (f64, f64))>);
    let close = move || library.customizing.set(None);

    let load_options = move |game_id: u64| {
        chosen.set(Some(game_id));
        options.set(None);
        looking.set(true);
        spawn_local(async move {
            let found = library.report(api::art_options(game_id).await);
            looking.set(false);
            options.set(found);
        });
    };
    let search = move || {
        looking.set(true);
        spawn_local(async move {
            let found = library.report(api::art_search(&term.get_untracked()).await).unwrap_or_default();
            looking.set(false);
            let first = found.first().map(|g| g.id);
            results.set(found);
            match first {
                Some(first) => load_options(first),
                None => options.set(None),
            }
        });
    };
    spawn_local(async move {
        let key = api::get_settings().await.ok().and_then(|s| s.steamgriddb_key).is_some_and(|k| !k.is_empty());
        has_key.set(Some(key));
        if key {
            search();
        }
    });

    // A new picture starts centred, filling its frame.
    let show = move |data: String| {
        natural.set((0.0, 0.0));
        zoom.set(1.0);
        offset.set((0.0, 0.0));
        picture.set(Some(data));
    };
    let pick_pic = move |pic: ArtPic| {
        busy.set(true);
        spawn_local(async move {
            if let Some(data) = library.report(api::art_fetch(&pic.url).await) {
                show(data);
            }
            busy.set(false);
        });
    };
    let pick_file = move |_| {
        spawn_local(async move {
            if let Some(data) = library.report(api::art_pick_file().await).flatten() {
                show(data);
            }
        });
    };
    let switch_kind = move |to: Kind| {
        kind.set(to);
        picture.set(None);
    };

    let save = move |_| {
        let Some(img) = img_ref.get_untracked() else { return };
        let rendered = render(kind.get_untracked(), &img, natural.get_untracked(), zoom.get_untracked(), offset.get_untracked());
        let Some(data) = library.report(rendered) else { return };
        busy.set(true);
        spawn_local(async move {
            let done = library.report(api::art_save(&id.get_value(), kind.get_untracked().id(), Some(data)).await);
            busy.set(false);
            if done.is_some() {
                picture.set(None);
                library.games.refetch();
                library.toast(format!("{} salva.", kind.get_untracked().label()));
            }
        });
    };
    let reset = move |_| {
        busy.set(true);
        spawn_local(async move {
            let done = library.report(api::art_save(&id.get_value(), kind.get_untracked().id(), None).await);
            busy.set(false);
            if done.is_some() {
                picture.set(None);
                library.games.refetch();
                library.toast(tr!("Arte original de volta."));
            }
        });
    };
    // Keep the dialog's preview of the current art in step with the library.
    Effect::new(move |_| {
        if let Some(Ok(all)) = library.games.get() {
            if let Some(g) = all.into_iter().find(|g| g.id == id.get_value()) {
                current_game.set(g);
            }
        }
    });

    let picture_style = move || {
        let (iw, ih) = natural.get();
        if iw <= 0.0 || ih <= 0.0 {
            return "visibility:hidden".to_string();
        }
        let (w, h) = kind.get().size();
        let (x, y, dw, dh) = place(kind.get(), (iw, ih), zoom.get(), offset.get(), w, h);
        format!("left:{}%;top:{}%;width:{}%;height:{}%", x / w * 100.0, y / h * 100.0, dw / w * 100.0, dh / h * 100.0)
    };
    let frame_style = move || {
        let (w, h) = kind.get().size();
        format!("aspect-ratio:{w}/{h}")
    };
    let on_down = move |e: web_sys::PointerEvent| {
        if picture.get_untracked().is_none() {
            return;
        }
        if let Some(target) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let _ = target.set_pointer_capture(e.pointer_id());
        }
        drag.set_value(Some((e.client_x() as f64, e.client_y() as f64, offset.get_untracked())));
    };
    let on_move = move |e: web_sys::PointerEvent| {
        let Some((x0, y0, (ox, oy))) = drag.get_value() else { return };
        let Some(frame) = frame_ref.get_untracked() else { return };
        let rect = frame.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let dx = (e.client_x() as f64 - x0) / rect.width();
        let dy = (e.client_y() as f64 - y0) / rect.height();
        offset.set((ox + dx, oy + dy));
    };
    let on_up = move |_: web_sys::PointerEvent| drag.set_value(None);
    let on_wheel = move |e: web_sys::WheelEvent| {
        if picture.get_untracked().is_none() {
            return;
        }
        e.prevent_default();
        let step = if e.delta_y() < 0.0 { 1.08 } else { 1.0 / 1.08 };
        zoom.update(|z| *z = (*z * step).clamp(0.2, 5.0));
    };

    let frame = move || {
        let kind_now = kind.get();
        match picture.get() {
            Some(data) => view! {
                <img node_ref=img_ref class="cz-pic" src=data style=picture_style draggable="false" alt=""
                    on:load=move |e| {
                        if let Some(img) = e.target().and_then(|t| t.dyn_into::<web_sys::HtmlImageElement>().ok()) {
                            natural.set((img.natural_width() as f64, img.natural_height() as f64));
                        }
                    } />
            }
            .into_any(),
            None => match kind_now.current(&current_game.get()) {
                Some(url) => view! { <img class="cz-now" class:fit=!kind_now.fills() src=url alt="" draggable="false" /> }.into_any(),
                None => view! { <p class="cz-empty">{tr!("Sem imagem. Escolha uma ao lado.")}</p> }.into_any(),
            },
        }
    };

    let tab = move |which: Kind| {
        view! {
            <button type="button" class="tab" class:on=move || kind.get() == which on:click=move |_| switch_kind(which)>{which.label()}</button>
        }
    };

    let sgdb = move || match has_key.get() {
        None => view! { <p class="muted small">{tr!("Carregando...")}</p> }.into_any(),
        Some(false) => {
            let save_key = move |e: leptos::ev::SubmitEvent| {
                e.prevent_default();
                busy.set(true);
                spawn_local(async move {
                    let saved = library.report(api::set_steamgriddb_key(key_input.get_untracked()).await);
                    busy.set(false);
                    if saved.is_some() {
                        has_key.set(Some(true));
                        library.toast(tr!("Chave do SteamGridDB guardada."));
                        search();
                    }
                });
            };
            view! {
                <div class="cz-key">
                    <b>{tr!("Arte da comunidade, do SteamGridDB")}</b>
                    <p class="muted small">{tr!("Milhares de capas, fundos e logos feitos por fãs. Crie uma conta gratuita lá, gere sua chave de API e cole aqui. Ela fica só no seu PC.")}</p>
                    <button type="button" class="link" on:click=move |_| spawn_local(async move { library.report(api::open_steamgriddb_keys().await); })>
                        {tr!("Pegar minha chave no SteamGridDB")}
                    </button>
                    <form class="row" on:submit=save_key>
                        <input class="input" type="password" placeholder=tr!("Chave de API") prop:value=move || key_input.get()
                            on:input=move |e| key_input.set(event_target_value(&e)) />
                        <button type="submit" class="btn sm main" disabled=move || busy.get() || key_input.get().trim().is_empty()>{tr!("Guardar")}</button>
                    </form>
                </div>
            }
            .into_any()
        }
        Some(true) => {
            let games = move || {
                results
                    .get()
                    .into_iter()
                    .map(|g| {
                        let gid = g.id;
                        let label = match g.year {
                            Some(y) => format!("{} ({y})", g.name),
                            None => g.name.clone(),
                        };
                        view! { <button type="button" class="chip-btn" class:on=move || chosen.get() == Some(gid) on:click=move |_| load_options(gid)>{label}</button> }
                    })
                    .collect_view()
            };
            let pics = move || {
                let Some(all) = options.get() else {
                    return if looking.get() {
                        view! { <p class="fr-status"><span class="spin"></span>{tr!("Buscando no SteamGridDB...")}</p> }.into_any()
                    } else {
                        ().into_any()
                    };
                };
                let list = kind.get().options(&all);
                if list.is_empty() {
                    return view! { <p class="muted small">{tr!("O SteamGridDB não tem esse tipo de imagem para esse jogo.")}</p> }.into_any();
                }
                let tiles = list
                    .into_iter()
                    .map(|pic| {
                        let thumb = pic.thumb.clone();
                        view! {
                            <button type="button" class="cz-thumb" disabled=move || busy.get() on:click=move |_| pick_pic(pic.clone())>
                                <img src=thumb alt="" loading="lazy" />
                            </button>
                        }
                    })
                    .collect_view();
                view! { <div class="cz-grid" class:wide=move || kind.get() != Kind::Cover>{tiles}</div> }.into_any()
            };
            view! {
                <form class="row cz-search" on:submit=move |e: leptos::ev::SubmitEvent| { e.prevent_default(); search(); }>
                    <input class="input" placeholder=tr!("Nome do jogo") prop:value=move || term.get() on:input=move |e| term.set(event_target_value(&e)) />
                    <button type="submit" class="btn sm" disabled=move || looking.get()><Svg icon=Icon::Search size=16 />{tr!("Buscar")}</button>
                </form>
                <div class="row cz-games">{games}</div>
                {pics}
            }
            .into_any()
        }
    };

    view! {
        <div class="backdrop-dim">
            <div class="dialog cz" role="dialog" aria-modal="true" aria-label=tr!("Personalizar arte")>
                <div class="cz-head">
                    <h2>{tr!("Personalizar arte · {}", game.name)}</h2>
                    <nav class="tabs">{tab(Kind::Cover)}{tab(Kind::Hero)}{tab(Kind::Logo)}</nav>
                </div>
                <div class="cz-body">
                    <div class="cz-left">
                        <div node_ref=frame_ref class="cz-frame" class:logo=move || kind.get() == Kind::Logo class:editing=move || picture.get().is_some() style=frame_style
                            on:pointerdown=on_down on:pointermove=on_move on:pointerup=on_up on:wheel=on_wheel>
                            {frame}
                        </div>
                        <Show when=move || picture.get().is_some()>
                            <label class="cz-zoom">
                                <span>{tr!("Zoom")}</span>
                                <input type="range" min="0.2" max="5" step="0.01" prop:value=move || zoom.get().to_string()
                                    on:input=move |e| { if let Ok(z) = event_target_value(&e).parse::<f64>() { zoom.set(z) } } />
                            </label>
                            <p class="muted small">{tr!("Arraste a imagem para posicionar. A roda do mouse também dá zoom.")}</p>
                        </Show>
                        <div class="row">
                            <button type="button" class="btn sm" on:click=pick_file disabled=move || busy.get()>
                                <Svg icon=Icon::Folder size=16 />{tr!("Escolher arquivo...")}
                            </button>
                            <button type="button" class="btn sm" on:click=reset disabled=move || busy.get()>{tr!("Restaurar original")}</button>
                        </div>
                    </div>
                    <div class="cz-right">{sgdb}</div>
                </div>
                <div class="dialog-footer">
                    <button type="button" class="btn" on:click=move |_| close()>{tr!("Fechar")}</button>
                    <button type="button" class="btn main" disabled=move || busy.get() || picture.get().is_none() on:click=save>
                        {move || tr!("Salvar {}", kind.get().label().to_lowercase())}
                    </button>
                </div>
            </div>
        </div>
    }
}
