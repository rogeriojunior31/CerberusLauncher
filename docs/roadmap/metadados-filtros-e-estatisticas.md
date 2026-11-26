# Metadados, filtros salvos e estatísticas: plano de implementação

Plano dos itens 6 e 7 do Mapa rápido do `ROADMAP.md` (6.3 "Organizar a coleção" e 7.3 "Biblioteca rica"). Escrito em
25/11/2026 depois de ler o código do Cerberus (branch `ajustes-rapidos`) e as referências do Playnite (MIT) e do Hydra
(MIT). O que não foi confirmado no código ou numa fonte primária está marcado **(não verificado)**.

Sumário: [Objetivo](#1-objetivo) · [Como está hoje](#2-como-está-hoje-conferido-no-código) ·
[Modelo de dados](#3-modelo-de-dados) · [Fontes e licenças](#4-fontes-e-licenças) · [Desenho da UI](#5-desenho-da-ui) ·
[Desempenho](#6-desempenho) · [Plano em etapas](#7-plano-em-etapas) · [Riscos](#8-riscos) ·
[O que não fazer](#9-o-que-não-fazer)

---

## 1. Objetivo

1. **Cada jogo guarda seus dados**: gênero, desenvolvedora, publicadora, data e ano de lançamento, série, classificação
   indicativa, descrição, links, nota da crítica, plataforma e recursos (um jogador, cooperativo, controle...). Os dados
   vêm da loja ou de outra fonte, e o jogador pode editá-los. Uma edição do jogador nunca é sobrescrita sem que ele
   peça.
2. **Filtros salvos, agrupar e editar vários jogos de uma vez** na Biblioteca. Os filtros salvos aparecem também no Big
   Picture e, se o jogador quiser, no menu lateral.
3. **Estatísticas e "seu ano"** calculadas só com dados deste PC (sessões, tempo das contas, conquistas). Nada é
   enviado a servidor nenhum.

Critério de sucesso geral: com a biblioteca real do usuário (cerca de 950 jogos), a Biblioteca continua respondendo na
hora a uma busca ou a um filtro, e os dados de 90% ou mais dos jogos de loja ficam preenchidos sozinhos em segundo plano
sem nenhum bloqueio da Steam (HTTP 429) que dure.

---

## 2. Como está hoje (conferido no código)

| Peça | Onde | O que faz |
| --- | --- | --- |
| `Game` | `shared/src/lib.rs:9` | Montado a cada `list_games`. Não tem nenhum campo de metadados (gênero, desenvolvedora...). |
| `list_games` | `src-tauri/src/main.rs:1586` | Junta `library.json` (recomps e jogos adicionados), Steam, Epic e Xbox instalados, Game Pass, contas Epic e Xbox e os não instalados da Steam. Depois aplica arte, tempo de jogo (`store_time`, Steam, contas), `favorites`, `collections`, `game_notes` e as atualizações pausadas. |
| Jogos de loja | `steam.rs`, `epic.rs`, `xbox.rs`, `epic_account.rs`, `xbox_account.rs`, `gamepass.rs` | Não ficam em disco na biblioteca: são lidos das lojas toda vez. Os ids são `steam:<app>`, `epic:<app_name>`, `xbox:<família>` (ou `xbox:<produto>` para Game Pass sem família), `local:<exe>`, `recomp:<id>`. |
| Dados do jogador por id | `favorites.json`, `collections.json` (coleções e ocultos), `game-notes.json` (situação, nota, anotações), `store-playtime.json` | Um JSON por assunto, gravado por inteiro com troca atômica (`.tmp` + `rename`) e aplicado no `Game` em `list_games`. **É o padrão a seguir para tudo o que o jogador edita.** |
| Backup | `data_backup.rs:20` | Lista fixa de arquivos (`library.json`, `favorites.json`, `collections.json`, `game-notes.json`, `classics.json`, `art.json`, `sessions.jsonl`, `trail.jsonl`, `store-playtime.json`). Arquivo novo do jogador precisa entrar nessa lista. |
| Página da Steam | `steam_details.rs` | `appdetails?appids=<app>&l=brazilian\|english&cc=br`, sem chave. Cache por idioma em `cache/store/<app>-<idioma>.json`, válido por 3 dias, revalidado em segundo plano. Já extrai `developers`, `publishers`, `release_date.date` (texto), `genres` (só o nome, **o id é descartado**), `categories` (como `features`), `metacritic.score`, `website`. |
| Microsoft Store | `ms_store.rs` | `displaycatalog.mp.microsoft.com/v7.0`, sem login, cache de 7 dias em `cache/ms-store`. Extrai desenvolvedora, publicadora, `OriginalReleaseDate` (convertida para `dd/mm/aaaa`), `Categories` (gêneros) e site. Não tem nota da crítica. |
| Epic e Xbox | `main.rs:1213` (`store_page`) | Procura o jogo na Steam pelo nome (`steam_store::app_for`). Se não acha, usa a Microsoft Store. |
| IGDB | `recomp/catalog.rs:509` (`original_of`) | **O app não chama o IGDB.** Os fatos do "jogo original" (`OriginalGame`: lançamento, desenvolvedora, publicadora, série, gêneros, modos, plataformas, nota 0–100, resumo) vêm prontos no campo `about` do catálogo de recomps. Não há chave do IGDB/Twitch no repositório (`tools/` não menciona IGDB). |
| Wikipedia | `wikipedia.rs` | Primeiro parágrafo do artigo no idioma do jogador, cache de 30 dias (inclusive "não achou"). |
| Sessões | `play_log.rs` | `sessions.jsonl`, uma linha por partida (`game_id`, `ended`, `seconds`). Sessões com menos de 30 s não entram. É preenchido pelo `sessions.rs` (jogos que o Cerberus abre) e pelo `store_time.rs` (jogos de loja vistos rodando enquanto o Cerberus está aberto). Partidas jogadas com o Cerberus fechado ou em outro PC não aparecem. |
| Tempo das contas | `achievements/store.rs`, tabela `played(source, account, key, seconds, last)` | Só o **total** e a **última vez** por jogo (Epic e Xbox). Steam: `localconfig.vdf` e cache do `GetOwnedGames` (`achievements/mod.rs:91`). **Não há histórico por dia**, então não dá para saber quanto do total foi jogado neste ano. |
| Conquistas | `achievements.db`, tabelas `games`, `lists`, `unlocks(unlocked_at)` | Cada desbloqueio tem data (índice `unlocks_when`). `games.data` guarda o JSON do `AchievementGame` com `award` e `award_at` (Platinado, Zerado...). |
| Biblioteca (UI) | `ui/src/library_page.rs` | Abas (Todos, Favoritos, Coleções, Nativos, PC, Clássicos, Ocultos), busca, chips de plataforma (Steam/Epic/Xbox/Adicionados), instalados e não instalados, chips de situação e **5 ordens** (A a Z, Recentes, Mais jogados, Sua nota, Conquistas). O estado (`LibraryView`) só existe na memória. **Todos os cartões são desenhados de uma vez**, sem virtualização. |
| Big Picture | `ui/src/tv/pages.rs:343` | Chips fixos (Todos, Favoritos, Nativos, PC, Clássicos) e 3 ordens. X abre a folha de opções do jogo. |

Duas divergências entre o `ROADMAP.md` e o código, para acertar no próprio roadmap:

- O item 6.3 "Mais ordens: adicionado em, nota, situação, tamanho, vezes jogado" está marcado como feito, mas
  `library_page.rs:109` só tem Name, Recent, Playtime, Rating e Achievements. Não existe data de "adicionado em" para
  jogos de loja, nem tamanho instalado.
- O 6.3 fala em "nota pessoal (0 a 100)", mas `Game.rating` e `game_notes::Note` usam **0 a 5 estrelas** (`min(5)` em
  `game_notes.rs:39`). Este plano mantém as estrelas.

---

## 3. Modelo de dados

### 3.1 Princípio: o que o jogador escreve fica separado do que é baixado

| O quê | Onde | Por quê |
| --- | --- | --- |
| **Edições do jogador** (o valor que ele escolheu para cada campo) | `game-meta.json`, mapa por id, igual a `game-notes.json` | São pequenas, insubstituíveis e precisam entrar no backup (`data_backup.rs`) do mesmo jeito que os outros arquivos do jogador. |
| **Dados baixados** (por fonte, com data) | `library.db` (SQLite, `rusqlite`, que o `achievements.db` já usa) | São milhares de linhas (cerca de 950 jogos × 1 a 3 fontes) gravadas aos poucos por uma tarefa em segundo plano. Um JSON único seria regravado inteiro a cada jogo baixado. Tudo isso pode ser baixado de novo, então fica fora do backup. |
| **Filtros salvos** | `filters.json` | São do jogador e vão para o backup. |
| **Histórico do tempo das contas** (fotos diárias do total) e **primeira vez visto** | `library.db` | Servem às estatísticas. Não podem ser baixados de novo (o passado se perde), então o backup deve levar o `library.db` com `VACUUM INTO` (ver Riscos). |

Por que não guardar no `achievements.db`: o `Store::forget` apaga tudo de uma conta quando o jogador sai dela
(`store.rs:274`). Os metadados e o histórico pertencem ao jogo, não à conta. Misturar os dois pede regras de exceção.

### 3.2 Campos e tipos (crate `shared`)

```rust
/// Um campo de metadados que pode ser baixado, editado e filtrado.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum MetaField {
    Genres, Developers, Publishers, Released, Series, AgeRating,
    Description, Links, CriticScore, Platforms, Features,
}

/// De onde veio um valor. "Player" é a edição do jogador e sempre vence.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetaSource { Player, Steam, MsStore, Catalog, Wikidata, Igdb, Derived }

/// Data de lançamento com a precisão que a fonte deu.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReleaseDate { pub year: u16, pub month: Option<u8>, pub day: Option<u8> }

/// O que a Biblioteca precisa para filtrar, agrupar e ordenar. Vai no `Game`
/// (sem a descrição, que é longa e só a página do jogo usa).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameFacts {
    /// Chaves canônicas ("action", "rpg"...) quando conhecidas, ou o texto da fonte.
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub developers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub publishers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub released: Option<ReleaseDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub series: Vec<String>,
    /// "ClassInd 14", "PEGI 16", "ESRB T".
    #[serde(default, skip_serializing_if = "Option::is_none")] pub age_rating: Option<String>,
    /// 0 a 100.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub critic_score: Option<u8>,
    /// "PC (Windows)", "Nintendo 64"...
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub platforms: Vec<String>,
    /// Chaves canônicas: "single-player", "co-op", "controller-full", "achievements", "cloud"...
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub features: Vec<String>,
    /// Campos que o jogador editou, para a UI mostrar o selo "Você".
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub edited: Vec<MetaField>,
    /// Quando o Cerberus viu este jogo pela primeira vez (Unix s). Serve para "adicionado em".
    #[serde(default, skip_serializing_if = "Option::is_none")] pub first_seen: Option<u64>,
}

/// A ficha completa de um jogo para a página › Editar dados.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameMetaView {
    pub facts: GameFacts,
    pub description: String,
    pub links: Vec<GameLink>,
    /// Para cada campo: o valor de cada fonte que o tem, para comparar e escolher.
    pub by_source: Vec<(MetaField, MetaSource, serde_json::Value, u64 /* baixado em */)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameLink { pub name: String, pub url: String }
```

No `Game`, um campo novo igual aos que já vêm de arquivos à parte:

```rust
/// Gênero, desenvolvedora, ano... (de library.db e game-meta.json, não daqui).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub facts: Option<GameFacts>,
```

A **edição** (em `game-meta.json`, no backend):

```rust
/// Só os campos que o jogador mexeu. `Some(vazio)` quer dizer "deixe vazio de propósito".
#[derive(Default, Serialize, Deserialize, PartialEq)]
pub struct MetaEdit {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<MetaField, serde_json::Value>,
    /// Unix s da última edição, para mostrar e para resolver backup x atual.
    #[serde(default)] pub at: u64,
}
// game-meta.json: BTreeMap<String /* game id */, MetaEdit>
```

### 3.3 Tabelas do `library.db`

```sql
PRAGMA journal_mode=WAL;
-- Um registro por "chave da fonte", não por jogo: "steam:620", "msstore:9NBLGGH4R315",
-- "catalog:zelda64recomp". Dois jogos da biblioteca que apontam para o mesmo app da Steam
-- (o Epic e o Steam do mesmo jogo) dividem o mesmo registro.
CREATE TABLE IF NOT EXISTS fetched(
  source_key TEXT NOT NULL, lang TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  status INTEGER NOT NULL,          -- 0 ok, 1 a fonte não tem esse jogo, 2 erro (tentar depois)
  data TEXT NOT NULL,               -- JSON: GameFacts + description + links, como a fonte deu
  PRIMARY KEY(source_key, lang));
-- Qual chave de cada fonte vale para cada jogo da biblioteca, e como foi achada.
CREATE TABLE IF NOT EXISTS links(
  game_id TEXT NOT NULL, source TEXT NOT NULL, source_key TEXT NOT NULL,
  how TEXT NOT NULL,                -- "id" (exato), "name" (pelo nome), "player" (escolhido)
  PRIMARY KEY(game_id, source));
-- Primeira vez que cada id apareceu em list_games.
CREATE TABLE IF NOT EXISTS seen(game_id TEXT PRIMARY KEY, first_seen INTEGER NOT NULL);
-- Fotos do total de tempo que as lojas contam, só quando muda (para as estatísticas).
CREATE TABLE IF NOT EXISTS playtime_marks(
  game_id TEXT NOT NULL, at INTEGER NOT NULL, total_secs INTEGER NOT NULL, origin TEXT NOT NULL,
  PRIMARY KEY(game_id, at));
-- Espera depois de recusa, por fonte (o mesmo esquema do achievements store).
CREATE TABLE IF NOT EXISTS stamps(name TEXT PRIMARY KEY, value INTEGER NOT NULL);
```

O `game_id` é normalizado antes de gravar: `xbox:` sempre em minúsculas (o `account_times` já compara a família em
minúsculas em `achievements/mod.rs:478`; o `gamepass.rs:220` usa a família como veio).

### 3.4 Qual valor vale (resolução por campo)

Para cada jogo e cada campo, nesta ordem:

1. A edição do jogador em `game-meta.json`, se existir. Um valor vazio também vale: o jogador apagou de propósito.
2. A primeira fonte da **ordem do campo** que tem valor não vazio. Ordem padrão:
   - Jogos Steam: Steam → Wikidata → catálogo.
   - Epic e Xbox: Steam (quando o nome bate com um app) → Microsoft Store → Wikidata.
   - Recomps: catálogo (o `about` do IGDB) → Wikidata. A plataforma é a do jogo original.
   - Jogos adicionados à mão: Steam (pelo `steam_store::identify`, que o "Adicionar jogo" já usa) → nada.
   - Clássicos (ROMs): só o derivado (console, nome). Os outros campos esperam o reconhecimento por CRC (6.5).
3. **Derivado**, que não é baixado: plataforma "PC (Windows)" para jogos de loja e adicionados, console para
   clássicos; recursos "Conquistas" quando o hub conhece o jogo; links montados (Steam, SteamDB, PCGamingWiki, que já
   existe em `main.rs:1113`, e o artigo da Wikipedia).

A ordem por campo é configurável (Configurações › Biblioteca › Dados dos jogos), como o `MetadataFieldSettings.Sources`
do Playnite (`Playnite/Metadata/MetadataDownloaderSettings.cs`). Na primeira versão basta a ordem padrão acima, e a
escolha por campo vem numa etapa própria (E12).

**Atualizar nunca apaga uma edição.** Uma atualização só troca o registro da fonte em `fetched`. A resolução continua
dando preferência à edição. "Voltar ao da loja" num campo remove a edição daquele campo.

### 3.5 Gêneros, recursos e idioma

- Os gêneros da Steam vêm traduzidos (`l=brazilian`), e os da Microsoft Store e do IGDB vêm em inglês. Para filtrar e
  agrupar sem duplicar ("Ação" e "Action"), grava-se uma **chave canônica**. A Steam manda um id por gênero
  (`genres[].id`, hoje descartado em `steam_details.rs:75`): 1 Ação, 2 Estratégia, 3 RPG, 4 Casual, 9 Corrida,
  18 Esportes, 23 Indie, 25 Aventura, 28 Simulação, 29 MMO, 37 Gratuito para jogar **(ids não verificados um a um)**.
  Os nomes em inglês do IGDB e da MS Store passam por uma tabela de sinônimos para a mesma chave. A UI traduz a chave
  com `tr!`. Um gênero desconhecido fica como texto e aparece como veio.
- Gêneros de aplicativo da Steam (Utilitários, Design e Ilustração...) não contam como gênero de jogo.
- Recursos: os ids de `categories` da Steam (já se usa 28 = controle completo e 18 = parcial em
  `steam_details.rs:86`) viram chaves canônicas.
- Desenvolvedora, publicadora e série não se traduzem. Ao comparar, ignoram-se maiúsculas, "™®©" e sufixos como
  "Inc.", "LLC" e "Ltd." para agrupar "PSYONIX LLC" com "Psyonix".
- Descrição: no idioma do jogador quando a fonte tem. Em `fetched` a chave inclui `lang`. Trocar o idioma do app não
  invalida gênero nem ano (são canônicos), só a descrição.
- Ano: a Steam manda a data como texto no idioma pedido ("17 fev. 2016", **formato não verificado** para `brazilian`).
  Para não depender do idioma, o ano é **o último número de 4 dígitos entre 1970 e o ano atual + 5**. Mês e dia só
  quando o formato for reconhecido. "Em breve" e "A definir" ficam sem data. A MS Store já dá ISO.

### 3.6 Filtro salvo (`filters.json`)

O modelo segue o `FilterPresetSettings` do Playnite (`PlayniteSDK/Models/FilterPreset.cs`): **campos diferentes se
combinam com E, valores do mesmo campo com OU**, e uma chave "todos os marcados" troca listas como gêneros para E
(`UseAndFilteringStyle` em `GameDatabase_Filters.cs:336`).

```rust
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    #[serde(default)] pub search: String,
    /// Vazio = todas. Steam, Epic, Xbox, GamePass, Added, Recomp, Classic.
    #[serde(default)] pub sources: Vec<SourceKind>,
    /// None = instalados e não instalados.
    #[serde(default)] pub installed: Option<bool>,
    #[serde(default)] pub favorite: bool,
    /// Ocultos ficam fora, a não ser que o filtro seja dos ocultos.
    #[serde(default)] pub hidden: bool,
    #[serde(default)] pub statuses: Vec<StatusPick>,      // inclui "Sem situação"
    #[serde(default)] pub collections: Vec<String>,
    #[serde(default)] pub genres: Vec<String>,
    #[serde(default)] pub developers: Vec<String>,
    #[serde(default)] pub publishers: Vec<String>,
    #[serde(default)] pub series: Vec<String>,
    #[serde(default)] pub platforms: Vec<String>,
    #[serde(default)] pub features: Vec<String>,
    #[serde(default)] pub years: Option<(u16, u16)>,
    #[serde(default)] pub playtime: Vec<PlaytimeBand>,    // Nunca, <1 h, 1–10 h, 10–100 h, >100 h
    #[serde(default)] pub last_played: Vec<RecencyBand>,  // Hoje, Ontem, Esta semana, Este mês, Este ano, Antes, Nunca
    #[serde(default)] pub min_rating: u8,                 // estrelas do jogador
    #[serde(default)] pub min_critic: Option<u8>,
    /// Listas (gêneros, recursos, coleções): todos os marcados em vez de qualquer um.
    #[serde(default)] pub match_all: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedFilter {
    pub id: String,               // "builtin:all", "builtin:recent"... ou aleatório curto
    pub name: String,
    pub rule: FilterRule,
    pub sort: SortKey,            // Name, Recent, Playtime, Rating, Achievements, Added, Released, Critic, Status
    pub descending: bool,
    pub group: GroupKey,          // None, Source, Status, Genre, Developer, Publisher, Series, Year, Decade,
                                  // Platform, Collection, Playtime, LastPlayed, Installed
    #[serde(default)] pub in_big_picture: bool,
    #[serde(default)] pub in_sidebar: bool,
}
// filters.json: { "filters": [SavedFilter...], "hidden_builtins": ["builtin:most"] }
```

Detalhes que importam:

- `matches(rule, game, now) -> bool`, `group_of(key, game, now) -> Vec<GroupLabel>` e `sort` ficam no crate
  **`shared`**, para a Biblioteca, o Big Picture e as estatísticas usarem o mesmo código e para os testes rodarem com
  `cargo test -p cerberus-shared` (o nome exato do crate é **não verificado**), sem WebView.
- Um jogo com dois gêneros aparece nos dois grupos, como no Playnite. O total de jogos acima dos grupos conta cada jogo
  uma vez.
- Coleções são guardadas pelo nome. `collections::rename` e `collections::delete` passam a atualizar os filtros que as
  citam. Se o filtro ficasse com um nome que não existe mais, mostraria vazio sem explicar.
- Filtros prontos, que não se apagam (dá para escondê-los): **Todos**, **Jogados recentemente** (últimos 30 dias, ordem
  Recentes), **Favoritos** e **Mais jogados** (tempo > 0, ordem Mais jogados). Por padrão aparecem no Big Picture.
- As abas de hoje (Nativos, PC, Clássicos, Ocultos, Coleções) continuam como estão. Um filtro salvo vale "por cima" da
  aba Todos. Não se transforma a aba em filtro nesta fase, para não mexer na navegação que já funciona.
- Os Clássicos (`RomView`, que não é `Game`) só passam por filtros que eles conseguem cumprir, como hoje
  (`library_page.rs:499`): busca, Recentes e Mais jogados. Um filtro de gênero ou de loja deixa os clássicos de fora e
  diz isso numa linha.

### 3.7 Situação com data

As estatísticas precisam saber **quando** um jogo foi zerado. `game_notes::Note` ganha:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub status_at: Option<u64>, // Unix s em que a situação mudou pela última vez
```

O campo é opcional, então os arquivos antigos continuam lendo. Os jogos zerados antes da mudança ficam sem data e não
entram no "seu ano". Quando houver `award_at` no hub (Zerado ou Platinado), ele serve de data.

### 3.8 Migração

Não há dados a converter, só arquivos e campos novos:

| Mudança | Como migra |
| --- | --- |
| `Game.facts` | Campo opcional com `#[serde(default)]`. Uma UI velha com backend novo (ou o contrário) continua funcionando. |
| `library.db` | `CREATE TABLE IF NOT EXISTS`, criado na primeira abertura. Uma coluna `user_version` (`PRAGMA user_version`) para as próximas mudanças. |
| `seen` | Na primeira execução, todos os ids presentes recebem `first_seen = agora` e uma marca "antes do registro". A UI mostra "adicionado antes de 25/11/2026" e não inventa data. Os recomps podem usar a data de instalação, quando o `trail.jsonl` tiver um passo de instalação **(não verificado se o passo tem nome fixo)**. |
| `game-meta.json`, `filters.json` | Ausentes = vazios. Entram na lista `FILES` de `data_backup.rs`. |
| `Note.status_at` | Opcional. |
| Cache da Steam | Já existe em `cache/store/`. A etapa E2 lê os fatos de lá na primeira vez, sem baixar de novo, e só baixa o que falta. |

---

## 4. Fontes e licenças

| Fonte | Chave? | Limites | Licença e termos | Campos úteis | Uso proposto |
| --- | --- | --- | --- | --- | --- |
| **Steam `appdetails`** (`store.steampowered.com/api/appdetails`) | Não | Não documentado. A comunidade fala em cerca de 200 pedidos a cada 5 min por IP **(não verificado)**. Um app por pedido (vários `appids` só funcionam com `filters=price_overview`, **não verificado**). | Endpoint não documentado da loja e fora da Steam Web API oficial. Mostrar e guardar para uso do próprio jogador é o que o Cerberus já faz. Não redistribuir em massa. | gêneros (id e nome), desenvolvedora, publicadora, data, categorias, Metacritic (nota e link), `required_age`, `ratings` com a classificação brasileira (`dejus`, **não verificado**), site, descrição | Fonte principal para jogos Steam e para Epic/Xbox que a Steam vende. Fila em segundo plano com no máximo 1 pedido a cada 1,5 s. |
| **Microsoft Store** (`displaycatalog.mp.microsoft.com/v7.0`) | Não | Não documentado | Catálogo público da loja, sem termos próprios para terceiros **(não verificado)** | desenvolvedora, publicadora, data ISO, categorias, site, classificações (`ContentRatings`, **não verificado onde fica no JSON**) | Xbox e Game Pass sem app na Steam. |
| **Catálogo de recomps** (campo `about`) | Não | O catálogo baixado | O que os autores do catálogo publicam. Os fatos vêm do IGDB (`docs/guide/recomps.md:22`). | `OriginalGame` inteiro | Recomps (o jogo original). |
| **IGDB** (`api.igdb.com/v4`) | **Sim**: Twitch Client ID e Client Secret, token por *client credentials* (`POST id.twitch.tv/oauth2/token`) que expira | 4 pedidos/s e 8 abertos ao mesmo tempo (HTTP 429 acima disso) | Grátis para uso **não comercial** pelo Twitch Developer Services Agreement. Uso comercial pede parceria. Fonte: [api-docs.igdb.com](https://api-docs.igdb.com/). A documentação não diz se um segredo pode ir embutido num app distribuído. **Não pode**: o segredo seria público. | gêneros, empresas (`involved_companies`), `first_release_date`, `franchises`/`collections` (série), `age_ratings`, `summary`, `aggregated_rating`, `websites` (nomes dos campos vistos na documentação, **lista não conferida campo a campo**) | **Não chamar do app com chave do projeto.** Duas saídas: (a) o catálogo gerado no CI com o segredo nos *secrets* do GitHub Actions, que já é o caminho dos recomps; (b) opção avançada em que o jogador cola o próprio Client ID e Secret, como a chave do SteamGridDB. Fica para a etapa E13. |
| **Wikidata** (`wikidata.org`, SPARQL ou `wbgetentities`) | Não | Pede User-Agent descritivo e tem limite de tempo por consulta **(não verificado o número)** | **CC0**: pode guardar e mostrar sem atribuição obrigatória | Por id da Steam (P1733), série (P179), gênero (P136), desenvolvedora (P178), publicadora (P123), data (P577), PEGI (P908), ClassInd (P3216), id do IGDB (P5794) **(ids das propriedades e cobertura não verificados)** | Completar **série** e **classificação**, que a Steam não tem. Bom candidato a substituir o IGDB sem chave. Cobertura de jogos indie provavelmente baixa **(não verificado)**. |
| **Wikipedia** | Não | Já usada | Texto **CC BY-SA**: mostrar com atribuição e link, como já é feito em `game.rs:1824` | Descrição | Continua só para a descrição, sempre com a linha "da Wikipedia". Não vira "descrição editável" sem guardar a fonte. |
| PCGamingWiki | Não | — | Conteúdo CC BY-NC-SA **(não verificado)** | — | Só o link, que já existe. |
| Metacritic, HowLongToBeat, OpenCritic | — | — | Raspagem ou API não oficial | — | Fora deste plano. A nota da crítica vem só do campo `metacritic` da Steam e do `aggregated_rating` do IGDB pelo catálogo. |

**Cache e quando atualizar**

- Os fatos baixados ficam em `library.db.fetched`. Validade: **60 dias** para jogos lançados há mais de um ano,
  **7 dias** para os lançados há menos de um ano ou "em breve" (data e nota ainda mudam). "Não tem esse jogo"
  (`status = 1`) vale 30 dias. Erro (`status = 2`) espera de 2 min a 6 h, com a mesma progressão de
  `Store::refused`.
- Quando a página do jogo abre e o `steam_details` baixa a página, os fatos são gravados também. Isso não custa
  nenhum pedido a mais.
- A fila de segundo plano só roda com o app aberto, **pausa enquanto há jogo rodando** (há `throttle::set_playing` em
  `recomp/throttle.rs:29`; é preciso ler esse estado, e a função de leitura é **não verificada**) e sem internet.
  Prioridade: instalados, depois jogados alguma vez, depois o resto da conta. Jogos do catálogo do Game Pass que o
  jogador nunca instalou ficam por último, porque podem ser centenas que nem são dele.
- Botão "Baixar dados de novo" no jogo (e para vários jogos): ignora a validade e abre a comparação (ver 5.2).

---

## 5. Desenho da UI

### 5.1 Biblioteca (desktop)

Barra de cima, da esquerda para a direita: **[Filtro salvo ▾] [Busca] [Ordem ▾] [Agrupar ▾] [Filtros (n)]
[Selecionar] [grade|lista]**.

- **Filtro salvo ▾**: lista os prontos e os do jogador, com o número de jogos de cada um. Embaixo: "Salvar filtro
  atual…", "Gerenciar filtros…". Quando o estado na tela difere do filtro escolhido, o nome ganha um "•" e aparecem
  "Salvar" e "Salvar como novo".
- **Filtros (n)**: o painel que já existe (`lib-filters`) ganha seções em sanfona: Plataforma, Situação (já
  existem), Gênero, Desenvolvedora, Publicadora, Série, Ano (dois campos, de e até), Recursos, Tempo de jogo, Última vez,
  Nota (estrelas), Nota da crítica. Cada seção mostra os **8 valores com mais jogos**, com o número ao lado, e um
  "Mais…" que abre uma caixa de busca dentro da seção. Seções sem nenhum valor na biblioteca não aparecem. O
  "Todos os marcados" (E) aparece só nas seções de lista. "Limpar filtros" limpa tudo menos a busca.
- **Agrupar ▾**: Nenhum, Loja, Situação, Gênero, Desenvolvedora, Publicadora, Série, Ano, Década, Coleção, Tempo de
  jogo, Última vez, Instalação. Cada grupo tem um cabeçalho fixo no rolar, com nome, número de jogos e seta para
  recolher. "Sem gênero" e equivalentes ficam por último. O agrupamento escolhido vai junto com o filtro salvo.
- **Ordens novas**: Adicionado em (por `first_seen`), Lançamento, Nota da crítica, Situação. Faltam hoje, apesar do
  roadmap. "Vezes jogado" sai de `sessions.jsonl`, que só conta as sessões vistas pelo Cerberus. Fica para depois e a
  dica explica o limite.

**Seleção múltipla** (Playnite: `GameEditViewModel` com `IsMultiGameEdit` e `ShowCheckBoxes`):

| Ação | Mouse | Teclado |
| --- | --- | --- |
| Entrar no modo de seleção | botão **Selecionar** ou Ctrl+clique numa capa | Ctrl+Espaço na capa com foco |
| Marcar ou desmarcar uma | clique na caixa da capa ou Ctrl+clique | Espaço |
| Intervalo | Shift+clique (na ordem da tela, atravessando grupos) | Shift+setas |
| Todos os da tela | — | Ctrl+A (só os visíveis no filtro, não os de grupos recolhidos) |
| Sair e limpar | **Cancelar** na barra ou Esc | Esc |

No modo de seleção, Enter e o clique simples **não abrem** o jogo, para não sair da página e perder a seleção. A capa
usa `aria-pressed`, e uma região `aria-live` anuncia "3 jogos selecionados". A navegação por setas que já existe
(`navigate_grid`) continua.

**Barra de ações** fixa embaixo enquanto há seleção: "**3 selecionados**" · Situação ▾ · Coleção ▾ (adicionar ou
tirar, com Nova coleção) · Favoritar/Desfavoritar · Ocultar/Mostrar · Nota ▾ · **Editar dados…** · Baixar dados de
novo · Cancelar. O menu do botão direito (`game_menu.rs`) aberto numa capa selecionada vale para a seleção toda ("3
jogos"). Depois de uma ação em massa, um aviso com **Desfazer** por 8 s. A UI já tem os valores anteriores e manda o
lote de volta.

**Editar dados de vários** (como o Playnite): o mesmo formulário da página do jogo, com uma caixa "Alterar" ao lado de
cada campo. Só os campos marcados mudam. Nas listas (gêneros, recursos, coleções) há três modos: **Adicionar**,
**Tirar** e **Substituir**. O padrão é Adicionar, porque substituir em massa destrói dados.

### 5.2 Página do jogo › Gerenciar › Editar dados

- Uma lista de campos. Cada um mostra o valor que vale, um selo da origem (**Steam**, **Microsoft Store**,
  **Catálogo/IGDB**, **Wikidata**, **Você**) e o botão de editar. Listas aceitam fichas com sugestões dos valores que já
  existem na biblioteca, para não nascer "Acao" ao lado de "Ação".
- Num campo com selo **Você**: "Voltar ao da loja".
- **Baixar dados de novo** abre uma comparação lado a lado (atual × novo) só dos campos que mudaram, com uma caixa por
  campo. Ficam marcados por padrão os campos sem edição do jogador. Os editados ficam desmarcados e com o aviso "você
  mudou este campo". É o "comparar antes de sobrescrever" do 7.3.
- A descrição editada é texto puro (sem HTML), com até 4.000 letras, o mesmo limite das anotações.
- Os fatos também aparecem no painel lateral da página (desenvolvedora, publicadora, lançamento, gêneros, Metacritic),
  que hoje sai direto do `StoreDetails`. Passam a sair do `GameMetaView`, então uma edição aparece na página.

### 5.3 Big Picture

- A fileira de chips da Biblioteca (`tv/pages.rs:352`) passa a mostrar os **filtros salvos marcados "no Big
  Picture"**, depois o chip de ordem. Os chips de tipo de hoje (Nativos, PC, Clássicos) viram filtros prontos com o
  mesmo nome, para não sumir nada que o jogador já usa.
- O agrupamento escolhido vira **fileiras com título** (estilo prateleira) na etapa E10b. Na primeira versão o Big
  Picture ignora o agrupamento e só usa filtro e ordem.
- A folha de opções do jogo (X) ganha **Situação**, **Nota** e **Coleções**, como escolhas de lista no `ChoiceSheet`
  (`tv/sheets.rs:41`). É o item do 6.6.
- **Sem seleção múltipla no Big Picture** nesta fase: com controle é lenta e propensa a erro. Ver "O que não fazer".
- "Seu ano" em tela cheia, com A/→ para avançar e B para sair (ver 5.4).

### 5.4 Estatísticas (nova página)

Fica no menu lateral, depois de Conquistas. Uma linha de controles no topo, acima de todos os gráficos: **Período**
(Últimos 7 dias · 30 dias · Este ano · <ano> · Todo o tempo) e **Loja/plataforma** (chips). A escolha vale para a
página inteira. Os jogos ocultos ficam de fora por padrão ("Incluir ocultos" desligado), porque um jogo oculto muitas
vezes é o que o jogador não quer ver.

**Números do topo** (cartões de número, sem gráfico):

| Cartão | Fórmula | Fonte |
| --- | --- | --- |
| Tempo jogado | soma do tempo atribuído ao período (ver "Atribuição" abaixo) | `sessions.jsonl` + `playtime_marks` |
| Jogos jogados | jogos distintos com tempo > 0 no período | idem |
| Sessões | número de sessões ≥ 30 s no período; média e mediana por sessão | `sessions.jsonl` (só o que o Cerberus viu) |
| Conquistas | desbloqueios com `unlocked_at` no período, por fonte | `achievements.db.unlocks` |
| Zerados e completos | `Note.status_at` ou `award_at` no período | `game-notes.json`, `achievements.db.games` |
| Novos na biblioteca | `first_seen` no período (desde o início do registro) | `library.db.seen` |

**Gráficos**, cada um com uma pergunta, um tipo e uma regra de cor:

1. **Quanto você jogou ao longo do tempo**: barras verticais por dia (7 e 30 dias), semana (ano) ou mês (todo o
   tempo). Uma série, um tom só. Dica ao passar o mouse com o valor exato. O tempo das contas, que só tem data
   aproximada, entra como segmento empilhado mais claro, "outras plataformas (aprox.)", com legenda. Nunca dois eixos.
2. **Mais jogados no período**: barras horizontais, as 10 maiores em ordem decrescente, com a capa pequena e as horas
   escritas na ponta. Embaixo, "e mais N jogos". Clicar abre o jogo.
3. **Por loja ou plataforma**: barras horizontais (não pizza), cor categórica fixa por loja (Steam é sempre a mesma
   cor, mesmo quando um filtro tira outra loja), rótulo direto e legenda. Mais de 6 viram "Outras".
4. **Situação da coleção**: barras horizontais com o número de jogos por situação, inclusive "Sem situação" e "Nunca
   jogados" (o acúmulo de jogos parados). As cores das situações vêm do app (`game_notes::class`) e são validadas como
   paleta categórica.
5. **Calendário do ano**: um quadrado por dia (estilo GitHub), escala sequencial de um tom do claro ao escuro, cinco
   faixas de minutos. Dica por dia. Só com sessões locais, porque a hora das contas não é confiável.
6. **Quando você joga**: mapa de calor dia da semana × hora, a partir do início das sessões (`ended − seconds`). Uma
   sessão que atravessa a meia-noite se divide entre os dois dias.
7. **Conquistas por mês**: barras verticais. Ao lado, a lista "as mais raras do período" (com `rarity` do `lists`),
   reaproveitando os componentes do hub.

Sem período (a coleção agora): total de jogos, instalados, ocultos, favoritos, tempo total e médio por jogo jogado, os
50 mais jogados de sempre (Playnite, `StatisticsViewModel.TopPlayed`) e o número por loja. **Espaço ocupado** depende do
item "tamanho instalado" (6.2) e fica de fora até ele existir.

Regras de visualização em todos os gráficos: a paleta passa pelo validador do skill de dataviz em claro e escuro antes
de entrar no CSS; marcas finas; grade e eixos discretos; texto nas cores de texto, nunca na cor da série; com duas
séries ou mais, sempre legenda; cada gráfico tem um "Ver como tabela"; o modo escuro tem tons escolhidos para ele, não
uma inversão. Os gráficos são SVG simples gerados no Leptos. Não entra biblioteca de gráficos de JS.

**Atribuição do tempo ao período** (a parte delicada):

- Uma sessão local conta inteira no dia (ou nos dias) em que aconteceu.
- O tempo das contas (Steam, Epic e Xbox) só existe como total. Uma vez por dia (e a cada `list_games`, no máximo uma
  vez por hora), grava-se em `playtime_marks` o total de cada jogo **quando ele mudou**. A diferença entre duas marcas
  seguidas é o tempo jogado naquele intervalo, inclusive em outro PC.
- Para não contar duas vezes: no intervalo entre duas marcas de um jogo, vale **o maior** entre "diferença das marcas"
  e "soma das sessões locais no intervalo". A sobra (diferença − locais) vai para o dia da marca mais nova, como
  "aprox.".
- A primeira marca de cada jogo é só o ponto de partida e não conta como tempo jogado. A página diz: "o tempo jogado
  fora do Cerberus conta desde <data da primeira marca>".

**"Seu ano"**

- Aparece **de 1º de dezembro a 31 de janeiro**, para o ano que termina, como cartão no Início e no topo de
  Estatísticas. Depois disso fica em Estatísticas › Seu ano, com qualquer ano que tenha dados.
- Só aparece se o ano tiver **pelo menos 5 h jogadas ou 10 sessões**. Com menos que isso a retrospectiva fica vazia e
  sem graça.
- Telas, uma por vez (setas e controle): tempo total e comparação com o ano anterior, se houver; número de jogos; **o
  jogo do ano** (mais tempo); os 5 mais jogados; o mês mais ativo; a maior sessão; a maior sequência de dias seguidos
  jogando; o jogo novo que mais prendeu (primeira sessão no ano); conquistas do ano e a mais rara; zerados, completos e
  platinados; por loja. Cada número diz de onde vem ("sessões vistas pelo Cerberus" ou "contas, aproximado").
- "Salvar como imagem" (PNG desenhado num canvas, salvo onde o jogador escolher) é opcional, na etapa E15. Não existe
  link nem envio.

**Privacidade**: tudo é calculado no backend, a partir de arquivos deste PC. Nenhum número sai do computador.
Comparações com outros jogadores ("top 5%", como o Hydra faz em `user-stats-box.tsx`) precisam de servidor e ficam de
fora. O Wrapped do Hydra é um `iframe` de `hydrawrapped.com` (`wrapped-tab.tsx:93`), ou seja, servidor. Aproveita-se só
a ideia. A página "Dados e privacidade" (`docs/help/data-and-privacy.md`) ganha os arquivos novos.

---

## 6. Desempenho

Meta com 1.000 jogos (a biblioteca do usuário tem cerca de 950; os testes usam 1.500):

| Operação | Meta | Como |
| --- | --- | --- |
| `list_games` (custo a mais) | ≤ 30 ms | Uma consulta `SELECT` em `fetched` + `links` + `seen`, montada num `HashMap` e aplicada como `game_notes::apply`. A descrição **não** vai no `Game`. |
| Tamanho da resposta do `list_games` | ≤ +400 KB | Cerca de 300 bytes de `GameFacts` por jogo, sem descrição. Medir com `serde_json::to_vec(&games).len()`. |
| Digitar na busca ou ligar um chip | ≤ 16 ms no WASM | Hoje `found()` monta `format!` e chama `game_names` para cada jogo a cada tecla, e `counts` e `statuses` refazem a varredura. Proposta: um `Memo` com o índice da biblioteca (nome em minúsculas e sem acento, fatos normalizados), refeito só quando `library.games` muda. Contagens de todas as seções numa única passada. |
| Desenhar a grade | sem travar ao trocar de filtro | Hoje são cerca de 950 cartões no DOM. Primeiro passo, barato: `content-visibility: auto` com `contain-intrinsic-size` em cada grupo e em cada cartão; grupos recolhidos não são desenhados. Virtualizar só se a medição mostrar necessidade **(não medido)**. |
| Marcar um cartão na seleção | ≤ 16 ms | A seleção num `RwSignal<HashSet<String>>`. Cada cartão lê `contains` num `class:` próprio. São O(n) leituras baratas por clique, aceitável com 1.000 jogos. |
| Fila de metadados | não pesar no jogo nem no app | 1 pedido a cada 1,5 s na Steam, pausada durante partidas. Uma passada inteira com 950 jogos leva cerca de 25 min **(estimativa)**. Grava só os fatos e não o JSON inteiro da página: o cache de páginas completas (capturas, vídeos) de 950 jogos teria dezenas de MB **(não medido)**. |
| Avisar a UI | sem refazer a biblioteca 950 vezes | Emitir `catalog-changed` a cada 25 jogos preenchidos ou 30 s, nunca por jogo. Hoje um `catalog-changed` dispara `list_games` inteiro. |
| Estatísticas | ≤ 150 ms para "Todo o tempo" | `sessions.jsonl` lido numa passada só (10 sessões/dia por 10 anos dá cerca de 36 mil linhas). Agregados por dia guardados na memória até o arquivo mudar (tamanho e data). Conquistas por mês com `GROUP BY` no índice `unlocks_when`. Roda em `spawn_blocking`. |

Uma ferramenta de medida só para desenvolvimento: um teste que gera 1.500 `Game` sintéticos com fatos e mede
`matches` + `group_of` + `sort` no host. Na UI, um parâmetro de desenvolvimento que duplica a biblioteca para medir o
desenho. Os números entram no PR.

---

## 7. Plano em etapas

Cada etapa é pequena, entra num PR próprio e só fecha com teste real no app (regra do `ROADMAP.md`), além dos testes
automáticos.

| # | Etapa | Critério de aceite | Testes |
| --- | --- | --- | --- |
| **E1** | **Tipos e regras puras no `shared`**: `GameFacts`, `FilterRule`, `SavedFilter`, `SortKey`, `GroupKey`, faixas de tempo e de última vez; `matches`, `group_of`, `sort_games`. `Game.facts` opcional. Nenhuma mudança visível. | Compila nos dois lados. JSON antigo do `Game` continua lendo. | Unitários: E entre campos e OU dentro do campo; `match_all`; faixas na virada do dia, da semana e do ano (fuso local); jogo com dois gêneros em dois grupos; "Sem gênero" por último; ordem estável com empate. Medida com 1.500 jogos sintéticos (meta < 5 ms no host). |
| **E2** | **`library.db` e fatos da Steam**: tabelas, `links`, `seen`. Ao abrir a página de um jogo Steam (ou Epic/Xbox achado na Steam), os fatos vão para `fetched`. `list_games` aplica `facts`. Lê o cache que já existe em `cache/store/`. | Abrir a página de 3 jogos Steam, reiniciar e ver gênero e ano na lista (por ora, numa dica do cartão). `list_games` + ≤ 30 ms. | Ano a partir de "17 fev. 2016", "Feb 17, 2016", "Q1 2025", "Em breve"; ids de gênero para chave canônica; dois jogos com o mesmo `source_key`; `xbox:` em minúsculas; banco em memória como `Store::memory`. |
| **E3** | **Edições do jogador**: `game-meta.json`, resolução por campo, comando `game_meta(id)` → `GameMetaView`, `set_game_meta(id, campo, valor)`, "Voltar ao da loja". Página do jogo › Gerenciar › Editar dados. Entra no backup. | Editar o gênero de um jogo, baixar de novo e o gênero editado continua. Restaurar backup e a edição volta. | Edição vence fonte; vazio de propósito vence fonte; apagar edição volta ao valor da fonte; `data_backup` leva o arquivo. |
| **E4** | **Fila em segundo plano**: Steam, depois Epic/Xbox (Steam pelo nome, senão MS Store), recomps pelo catálogo, adicionados pelo `identify`. Prioridade, espera após recusa, pausa ao jogar, aviso em lotes. Linha em Configurações › Biblioteca: "Dados dos jogos: 812 de 950 · Pausar". | Na biblioteca real (cerca de 950): passada completa sem 429 em sequência, app fluido, nenhuma partida medida errado durante a fila. | Fila com relógio falso: ordem de prioridade, espera crescente após 429, retomar depois de reiniciar, "não tem" não pergunta de novo antes de 30 dias. |
| **E5** | **Filtros por metadados na Biblioteca** (sem salvar ainda): seções novas no painel Filtros, com contagens e "Mais…"; ordens Adicionado em, Lançamento, Nota da crítica e Situação. Índice em `Memo`. | Filtrar "RPG" e "2015 a 2020" com 950 jogos sem atraso perceptível. Clássicos somem com aviso quando o filtro não vale para eles. | Os de E1 cobrem a lógica. Medida de tempo da busca no WASM (console). |
| **E6** | **Filtros salvos**: `filters.json`, os 4 prontos, salvar, salvar como novo, renomear, apagar, esconder pronto, fixar no menu lateral. Renomear ou apagar coleção atualiza os filtros. | Criar "RPGs não zerados", reiniciar, aparece igual. Fixado no menu lateral, abre a Biblioteca com ele. | Ler e gravar; filtro com coleção renomeada; id de pronto não apaga; arquivo corrompido vira vazio sem perder o resto (como `collections::load`). |
| **E7** | **Agrupar** na Biblioteca (grade e lista): cabeçalhos fixos, recolher, contagem; o agrupamento vai com o filtro salvo. `content-visibility` nos grupos. | Agrupar 950 jogos por gênero e por última vez, recolher e abrir sem travar. | `group_of` (E1). Teste manual de leitor de tela nos cabeçalhos (`role="heading"`, `aria-expanded`). |
| **E8** | **Seleção múltipla e ações em massa**: modo de seleção, teclado e mouse da tabela 5.1, barra de ações, menu do botão direito na seleção, Desfazer. Comandos em lote no backend (`set_status_many`, `set_collection_many`, `set_hidden_many`, `set_favorite_many`, `set_rating_many`) com uma gravação de arquivo por lote. | Marcar 40 jogos com Shift+clique, pôr na coleção "Para jogar", desfazer, refazer. Uma gravação de `collections.json` por ação. | Unitários dos lotes (um id inexistente não derruba o lote; o lote é atômico no arquivo). |
| **E9** | **Editar dados de vários**: formulário com "Alterar" por campo e Adicionar/Tirar/Substituir nas listas. `edit_meta_many`. | Adicionar o gênero "Soulslike" a 6 jogos sem mexer nos outros gêneros deles. | Os três modos de lista; campo desmarcado não muda. |
| **E10** | **Big Picture**: chips com os filtros salvos marcados; Situação, Nota e Coleções na folha X. (E10b: fileiras por grupo.) | Pelo controle: escolher "Jogados recentemente", marcar um jogo como Zerado, ver a mudança na Biblioteca do desktop. | Teste manual com controle Xbox. A lógica é a do `shared`. |
| **E11** | **Base das estatísticas**: `Note.status_at`; `playtime_marks` diário; comando `stats(período, filtro)` em `spawn_blocking` que devolve um `StatsView` com os agregados; divisão de sessões na meia-noite; regra do maior entre marcas e sessões locais. | Com o `sessions.jsonl` real, os totais do período batem com a soma manual de uma amostra de 5 jogos. | Fixtures: sessão que atravessa a meia-noite e a virada do ano; marca de conta + sessão local no mesmo intervalo sem contar dobrado; primeira marca não conta; ocultos fora por padrão. |
| **E12** | **Página Estatísticas** (desktop): controles, cartões, gráficos 1 a 7, "Ver como tabela", claro e escuro com a paleta validada. Ordem de fontes por campo em Configurações (pendente da 3.4). | Leitura correta nos dois temas, conferida em captura de tela. Nenhum gráfico com dois eixos. Dicas no mouse e no teclado. | Validador de paleta (dataviz) nos dois modos, rodado e colado no PR. Testes de formatação (h/min, meses em pt-BR). |
| **E13** | **IGDB/Wikidata opcionais** para série e classificação: Wikidata sem chave primeiro; IGDB só com as chaves do próprio jogador. | Série preenchida para uma amostra conhecida (Dark Souls, Halo, Zelda). | Leitura das respostas com fixtures gravadas. |
| **E14** | **"Seu ano"** no desktop (cartão no Início de dezembro a janeiro) e no Big Picture em tela cheia. | Com os dados de 2026, as telas aparecem e cada número bate com a página de Estatísticas. | Regra de quando mostrar (datas e mínimo de 5 h ou 10 sessões); maior sequência de dias. |
| **E15** | (opcional) Salvar "Seu ano" como imagem; "Vezes jogado". | — | — |

**Primeira etapa recomendada: E1.** Não toca em dados do jogador, não depende de rede, destrava as outras etapas e já
dá a medida de desempenho com 1.500 jogos sintéticos. Se só uma coisa visível couber agora, E1 + E2 juntas mostram o
gênero e o ano de verdade.

---

## 8. Riscos

| Risco | Efeito | Como reduzir |
| --- | --- | --- |
| A Steam limita o `appdetails` (429) | Fila parada ou IP bloqueado por um tempo, e a página do jogo também falha | 1 pedido/1,5 s, espera crescente gravada em `stamps`, a página do jogo (pedido do jogador) passa à frente da fila, nunca mais de um pedido ao mesmo tempo. |
| Endpoints não documentados mudam (Steam, MS Store) | Campos vazios | Leitura tolerante, como o catálogo (`catalog.rs:493`). Os últimos fatos bons ficam (não sobrescrever com vazio). Testes com JSON gravado. |
| Jogo errado pelo nome (Epic/Xbox → Steam, adicionados) | Gênero e ano de outro jogo | `links.how = "name"` aparece na UI como "achado pelo nome". Botão "Não é este jogo" grava `how = "player"` com outro app, ou com nenhum. Comparação de nomes como em `ms_store::plain`. |
| Id do jogo muda (Game Pass que passou a ser comprado, caixa da família Xbox) | Edição, coleção e nota "somem" | Normalizar o id antes de gravar. Estudar um "apelido" de ids no `library.db` quando dois ids forem o mesmo jogo, junto com o item "o mesmo jogo na Steam e na Epic aparece como um só" (6.2). O problema já existe hoje para favoritos e notas. |
| Backup de SQLite aberto em WAL | Cópia inconsistente do `library.db` | Usar `VACUUM INTO` ou a API de backup do SQLite, nunca copiar o arquivo. Ou deixar o `library.db` fora do backup e aceitar que o histórico de marcas se perde. A decisão fica na E11. |
| "Seu ano" com números errados (tempo de conta sem data) | Perda de confiança | Separar sempre "visto pelo Cerberus" de "aproximado, das contas", dizer desde quando há registro, nunca atribuir ao ano um total antigo. |
| Gêneros duplicados e sujos ("Ação", "Action", "Acao") | Filtros e grupos poluídos | Chave canônica, sinônimos e sugestões na edição. Ferramenta "Juntar valores" (renomear "Acao" para "Ação" em todos) numa etapa futura. |
| Edição em massa destrutiva | Perder gêneros de dezenas de jogos | Padrão "Adicionar", Desfazer, contagem visível ("vai mudar 40 jogos"). |
| Coleção renomeada e filtros por nome | Filtro vazio sem explicação | Atualizar os filtros em `collections::rename` e `delete`, com teste. |
| `list_games` ficar lento | Biblioteca demora a abrir | Meta de 30 ms medida na E2. Se passar disso, um comando separado `library_facts()` carregado depois. |
| Termos do IGDB (não comercial, segredo) | Problema legal ou chave vazada | Nunca embutir o segredo. Usar o catálogo do CI ou a chave do jogador. Ler o Twitch Developer Services Agreement antes da E13 **(não lido)**. |
| Texto da Wikipedia (CC BY-SA) guardado como descrição | Falta de atribuição | Guardar a fonte com o texto e mostrar sempre "da Wikipedia" com o link. |

---

## 9. O que não fazer

- **Não gravar metadados dentro do `library.json`** nem tentar pôr jogos de loja nele: os jogos de loja são lidos a
  cada vez, e o padrão do projeto é "dados por id à parte" (`favorites`, `collections`, `game_notes`).
- **Não sobrescrever uma edição do jogador** em nenhuma atualização automática. Só a comparação, com escolha
  explícita, troca um campo editado.
- **Não embutir chave ou segredo do IGDB/Twitch** no app nem no repositório.
- **Não raspar** Metacritic, HowLongToBeat ou PCGamingWiki para preencher campos.
- **Não criar um editor de expressões E/OU aninhadas** (parênteses, NÃO por campo). O modelo do Playnite (E entre campos,
  OU dentro do campo, chave "todos") cobre o uso real e é o que o jogador entende.
- **Não pôr seleção múltipla no Big Picture** nesta fase.
- **Não incluir desinstalar ou remover da biblioteca nas ações em massa.** São irreversíveis, e cada loja desinstala de
  um jeito.
- **Não enviar estatísticas para fora**, nem "anônimas". Sem percentis contra outros jogadores e sem link de "seu ano"
  (precisaria de servidor, 6.9).
- **Não inventar datas**: jogos que já estavam na biblioteca não recebem "adicionado em" falso. Totais de conta sem
  histórico não entram em período nenhum.
- **Não usar pizza nem dois eixos**, e não repintar as lojas quando um filtro muda o número de séries.
- **Não trazer biblioteca de gráficos de JS** para a WebView. SVG gerado no Leptos basta para barras, calendário e mapa
  de calor.
- **Não emitir `catalog-changed` por jogo** durante a fila de metadados.
- **Não copiar código do RecompLauncher (GPL).** Playnite e Hydra (MIT) podem ser adaptados com crédito. Aqui foram
  usados só como referência de modelo.
