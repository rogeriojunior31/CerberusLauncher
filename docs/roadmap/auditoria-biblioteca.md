# Auditoria da Biblioteca

Revisão só de leitura (25/11/2026) do código em `CerberusLauncher-ajustes`. Não rodei build, testes nem o app; nada
foi medido em tempo real. As contagens de custo saem da leitura do código. O que não confirmei está marcado
**não verificado**.

Para não repetir o que já está escrito:

- Duplicatas por nome entre lojas e a chave `names::key`: [lojas L13](lojas-e-contas.md).
- Big Picture e Início coerentes com a Biblioteca: [lojas L15](lojas-e-contas.md).
- `settings.json` gravado ao mesmo tempo por várias tarefas: [lojas L1](lojas-e-contas.md), ROADMAP 8.0.
- Metas de desempenho da busca e da grade (índice em `Memo`, `content-visibility`) e `catalog-changed` em lote:
  [metadados §6](metadados-filtros-e-estatisticas.md).
- Filtros salvos, edição em lote e mais ordens: [metadados](metadados-filtros-e-estatisticas.md), ROADMAP 6.3 e 7.3.

---

## 1. Mapa

### 1.1 O que o jogador consegue fazer

| Ação | Tela | Comando (backend) | O que é gravado |
| --- | --- | --- | --- |
| Ver todos os jogos em abas (Todos, Favoritos, Coleções, Nativos, PC, Clássicos, Ocultos), buscar, filtrar por loja, instalação e situação, ordenar (5 ordens), grade ou lista | `ui/src/library_page.rs` | `list_games`, `list_collections`, `achievements_hub(false)`, `library_recovery_notice` | nada (o estado `LibraryView` fica só na memória) |
| Adicionar um `.exe` à mão | botão "Adicionar jogo" → `Library::add_game` (`ui/src/main.rs:186`) | `add_local_game` (`main.rs:2029`): `steam_store::identify` para nome e capa | `library.json` (id `local:<caminho do exe>`) |
| Renomear, achar capa na Steam, escolher capa | editor do jogo | `rename_game`, `find_cover`, `pick_cover` (`main.rs:2051-2102`) | `library.json`; a capa vai para `covers/` |
| Remover | página do jogo | `remove_game` (`main.rs:2105`) | `library.json`; recomp: apaga os arquivos (depois do backup dos saves) |
| Favoritar | menu do jogo, página | `set_favorite` → `favorites::set` | `favorites.json` |
| Ocultar e mostrar | menu do jogo | `set_hidden` → `collections::set_hidden` | `collections.json` (`hidden`) |
| Coleções: criar, renomear, apagar, pôr e tirar jogo | barra de Coleções, menu do jogo | `create_collection`, `rename_collection`, `delete_collection`, `set_in_collection` | `collections.json` |
| Situação, nota (0 a 5) e anotações | `NoteCardFor` (`ui/src/game_notes.rs`) | `set_game_note` | `game-notes.json` |
| Arte própria (capa, fundo, logo) do SteamGridDB ou de arquivo | `ui/src/customize.rs` | `art_search`, `art_options`, `art_fetch`, `art_pick_file`, `art_save` | `art.json` + `covers/<id>-<tipo>-<hora>.<ext>` |
| Atalho na área de trabalho | menu do jogo | `create_shortcut` (`main.rs:2940`) → `shortcut::create` (COM `IShellLinkW`) | `<Área de trabalho>/<nome>.lnk` com `--play "<id>"` |
| Adicionar à Steam, um ou todos; tirar | página do jogo, Configurações | `add_to_steam`, `add_all_to_steam`, `remove_from_steam`, `in_steam` (`main.rs:3049-3128`) | `userdata/<conta>/config/shortcuts.vdf` (+ `.cerberus.bak`) e `config/grid/<appid>p.<ext>`, `_hero` |
| Jogar algo aleatório | Biblioteca | nenhum (sorteio na UI, `RandomPick`) | nada |
| Início: destaque giratório, Jogue de novo, Favoritos, Mais jogados, prateleiras do catálogo | `ui/src/home.rs` | `list_games`, `list_recomps`, `steam_details` | nada |
| Barra lateral "Prontos para jogar" e menu da bandeja | `ui/src/shell.rs:164` | `set_tray_games` | memória (`background::set_recent`) |

### 1.2 Como a lista é montada (`list_games`, `src-tauri/src/main.rs:1584-1658`)

Nada da biblioteca de lojas fica salvo como biblioteca: **tudo é refeito a cada chamada**. Na ordem:

1. `LocalLibrary::games_with_parts` (`library.rs:72`): clona o `library.json` que está na memória. Em cada recomp:
   `recomp::parts_of` → `entry_for` → `manifest::read` (até 3 `stat`, cache por data de alteração) e uma busca linear
   no catálogo (cerca de 1.900 entradas).
2. `recomp::apply_art` e `recomp::next_step` em cada recomp: outra busca no catálogo, `manifest::read` de novo,
   `still_fits` (`stat` da cópia) e `missing_for` (um `stat` por arquivo exigido).
3. `owned_games` (`main.rs:110`): clona o índice de ROMs e roda `roms::match_catalog` contra o catálogo inteiro.
4. Em `spawn_blocking`: `steam::installed_games` (`libraryfolders.vdf`, `read_dir` de cada biblioteca, leitura de cada
   `appmanifest_*.acf` e `local_art` duas vezes por jogo), `epic::installed_games` (lê o `catcache.bin` inteiro,
   os manifestos, `installations.jsonl` e `LauncherInstalled.dat`) e `xbox::installed_games`.
5. `game_pass`, `epic_library`, `xbox_library`: cada um com `settings::load` e a leitura do seu cache JSON. Se o
   cache está velho, uma tarefa de fundo que, ao terminar, emite `catalog-changed`.
6. `xbox_art`: lê o JSON da arte do Xbox; procura na Steam, em segundo plano, a arte do que falta.
7. `owned_not_installed` (`main.rs:1997`): `settings::load`, lê e interpreta o `owned-<id>.json` (a conta inteira) e,
   **para cada jogo da conta que não está instalado**, chama `steam::not_installed`. Ela faz `local_art` duas vezes
   (capa e fundo): `is_file` + `read_dir` da pasta do app + `is_file` em cada subpasta + `canonicalize`.
8. `drop_owned_elsewhere`, ordenação por nome.
9. `ArtCache::use_copies_for_games`: SHA-256 e `is_file` da capa e do fundo de cada jogo sem caminho local.
10. `CustomArt::apply`: `is_file` de cada arte própria.
11. `store_time::apply`: lê `store-playtime.json`.
12. `settings::load` (`steam_id`), depois `steam_playtimes` em `spawn_blocking`: `loginusers.vdf`, o
    **`localconfig.vdf` inteiro** e o `owned-<id>.json` **pela segunda vez**.
13. `achievements::account_times`: `settings::load` e `Store::open` (SQLite), com 2 consultas.
14. Uma tarefa de fundo `sync_accounts`: `settings::load` e outro `Store::open`.
15. `favorites::load`, `collections::apply` (lê `collections.json`), `game_notes::apply` (lê `game-notes.json`),
    `settings::load` (`paused_updates`).
16. `ArtCache::fetch_later(missing)`: baixa em segundo plano, um por vez, toda arte ainda sem cópia.

**Custo por chamada, numa biblioteca de cerca de 950 jogos (estimado pela leitura, não medido):**

| Item | Quantidade |
| --- | --- |
| `settings.json` lido e interpretado | **8 vezes** (6 no corpo, mais `account_times` e `sync_accounts`) |
| JSON de dados lidos | `favorites`, `collections`, `game-notes`, `store-playtime`, caches do Game Pass, da Epic, do Xbox e da arte do Xbox, `owned-<id>.json` (2 vezes) |
| Arquivos grandes da Steam e da Epic | `localconfig.vdf` (costuma ter vários MB, **não medido**), `catcache.bin` (**não medido**), todos os `.acf` |
| Chamadas de sistema de arquivos para arte | com cerca de 900 jogos da conta não instalados: cerca de 2 × (2 a 6) por jogo em `local_art`, mais 2 `is_file` por jogo em `use_copies_for_games`. Entre **4 e 10 mil** por chamada |
| SQLite | 2 aberturas (uma em segundo plano) |
| Tarefas de fundo que podem emitir `catalog-changed` | até 6: Game Pass, Epic, Xbox, arte do Xbox, jogos da Steam e contas |
| Resposta | cerca de 950 `Game` serializados (cada um com caminhos, URLs, `parts` e `inputs`), desserializados no WASM |

**Quantas vezes a lista é pedida:**

- Na abertura (`LocalResource::new(api::list_games)`, `ui/src/main.rs:257`).
- A cada `catalog-changed` (`ui/src/main.rs:316`), que também refaz `list_recomps`, `roms` e `players`. O evento sai
  de 20 lugares no backend. Os principais:
  - as tarefas de fundo da própria `list_games` (uma por fonte, quando o cache passa de uma hora);
  - o `list_recomps` quando o catálogo muda;
  - o `store_time::watch` a cada 60 s quando o conjunto de jogos instalados muda (`store_time.rs:138`);
  - login e saída de contas.
  Na primeira abertura do dia, com Epic, Xbox, Game Pass e Steam Web API ligados, uma `list_games` pode gerar **5 ou 6
  `list_games` completas em sequência**. Não há coalescência: chamadas simultâneas rodam inteiras em paralelo.
- A cada `playtime-updated`, que sai no fim de cada partida (`sessions.rs:490`, `store_time.rs:193`).
- A cada ação do jogador: favoritar, ocultar, pôr em coleção, criar, renomear ou apagar coleção, arte, renomear,
  escolher cópia, localizar. Cada clique é uma `list_games` inteira.
- **Não** há releitura ao focar a janela (não existe tratamento de foco: o único `on_window_event` é o
  `CloseRequested`, `main.rs:3641`). Um jogo instalado pela Steam aparece em até 60 s, pelo `store_time::watch`.

### 1.3 Onde fica cada coisa e quem é o dono

| Arquivo | Dono | Leitura | Gravação | Se a leitura falha |
| --- | --- | --- | --- | --- |
| `library.json` | `LocalLibrary` (cache na memória com `Mutex`) | uma vez, com recuperação (`library/recovery.rs`) | `.tmp` + `rename` | **bloqueia a gravação** e avisa (correto) |
| `favorites.json` | `favorites.rs` | a cada chamada | `.json.tmp` + `rename` | **vira lista vazia** |
| `collections.json` | `collections.rs` | a cada chamada | idem | **vira vazio** |
| `game-notes.json` | `game_notes.rs` | a cada chamada | idem | **vira vazio** |
| `art.json` | `CustomArt` (cache na memória) | **só na abertura** | idem | **vira vazio** |
| `shortcuts.vdf` | `steam_shortcuts.rs` | a cada ação | `.cerberus.tmp` + `rename`, cópia `.cerberus.bak` | recusa (correto) |
| cache de arte | `ArtCache`, `<cache>/art/<sha256[..32]>` | `is_file` | `.part` + `rename` | baixa de novo |

### 1.4 Ids

| Fonte | Id | Estável? |
| --- | --- | --- |
| Steam | `steam:<appid>` | sim |
| Epic | `epic:<app_name>` | sim |
| Xbox | `xbox:<família>`; Game Pass sem família: `xbox:<produto>` (`gamepass.rs:220`) | muda se a família aparecer depois (F16) |
| Adicionado à mão | `local:<caminho do exe>` (`library.rs:171`) | muda se a pasta mudar |
| Recomp | `recomp:<id>`, parte: `recomp:<id>--<parte>` | depende do manifesto do desenvolvedor |

Favoritos, coleções, notas, arte, tempo das lojas e atualizações pausadas são guardados por esse id. Um id que muda deixa
tudo isso órfão, sem aviso.

---

## 2. Como os outros fazem

| Tema | Playnite (MIT) | Hydra (MIT) | Heroic (GPL, só ideias) | Cerberus hoje |
| --- | --- | --- | --- | --- |
| Biblioteca | Banco persistente (LiteDB). Importação por plugin **na abertura, no F5 ou agendada**, nunca a cada listagem (`GameDatabase.ImportGames`, `Playnite/Database/GameDatabase.cs:1205`). | Banco local (LevelDB) por `shop:objectId`. Sincronização em segundo plano. | Cache por loja (electron-store), atualizado ao abrir e pelo botão Atualizar. | Refeita inteira a cada chamada, a partir das lojas. |
| Chave do jogo | `(PluginId, GameId)` mais um `Guid` próprio. Dados do jogador presos ao `Guid`. | `shop` + `objectId` | `appName` por loja | string `fonte:id`, alguns ids não estáveis |
| Jogo removido | Exclusão de importação (`ImportExclusionsCollection`, `PlayniteSDK/Models/ImportExclusionList.cs`): não volta na próxima importação. | — | — | Jogo de loja não se remove (só se oculta). Os dados do jogador ficam órfãos ao remover um jogo adicionado. |
| Jogo desinstalado | Continua no banco com `IsInstalled = false`, tempo e notas guardados. Atualiza só o que mudou (`InstallDirectory`, `Playtime`; `LastActivity` só se for mais novo). | idem | idem | Some se a conta da Steam não estiver ligada. Os dados por id continuam no JSON. |
| Lista grande | WPF virtualizado | `@tanstack/react-virtual` na biblioteca (`src/renderer/src/pages/library/library.tsx:1142`) e na barra lateral | grade com paginação e carregamento sob demanda (**não verificado**) | ~950 cartões no DOM, refeitos a cada tecla |
| Arte | Baixada uma vez para `library/files/<Guid>`, e a URL deixa de ser usada. | URL da Steam `library_600x900_2x.jpg`, imagens com várias larguras e `thumbHash` (`catalogue-images.ts`) | cache local | cópia em disco por SHA-256 da URL. Sem cache de falha. A webview também baixa a URL. |
| Favoritos e ocultos | campos do jogo no banco | campos do jogo | listas `favouriteGames` e `hiddenGames` por `appName` | JSONs à parte por id (o mesmo padrão do Heroic) |

O que vale copiar, por ordem de retorno:

1. **Não reconstruir a lista inteira a cada clique** (Playnite e Hydra): mudar favorito, oculto, coleção ou nota no
   próprio `Game` da UI e só pedir a lista de novo nas importações.
2. **Importar com ritmo** (Playnite): lojas lidas na abertura, no F5, no `store_time` e nas tarefas de fundo; entre
   elas, uma `Vec<Game>` guardada na memória do backend.
3. **Virtualizar** a grade e a barra lateral (Hydra).
4. **Exclusão de importação** (Playnite), para "tirar da biblioteca" um jogo de loja de verdade.

---

## 3. Falhas

Gravidade: **Crítica** = perde dados ou segurança; **Alta** = quebra o uso; **Média**; **Baixa**.

### F1. Favoritos, coleções, ocultos e notas apagados por uma leitura que falhou — Crítica

**Onde.** `favorites.rs:13-29`, `collections.rs:33-49`, `game_notes.rs:31-51`.

**Problema.** `load` devolve vazio para *qualquer* erro: arquivo travado pelo antivírus ou pelo OneDrive, JSON truncado,
campo desconhecido de uma versão mais nova, `GameStatus` novo que a versão antiga não conhece. O `set` seguinte parte
desse vazio e grava por cima. Resultado: todos os favoritos, todas as coleções e ocultos ou todas as notas viram um item
só. O `library.json` já tem a proteção certa (`library/recovery.rs`: bloqueia a gravação e guarda os bytes originais).
Estes três arquivos não têm. Também não há `.bak`.

**Reproduzir.**
1. Abrir `game-notes.json` e quebrar uma vírgula (ou deixar o arquivo aberto com trava exclusiva).
2. Mudar a nota de um jogo.
3. O arquivo passa a ter uma nota só.
O mesmo acontece com `collections.json` ao ocultar um jogo.

**Correção.**
- `load` devolve `Result`: `NotFound` vira vazio, qualquer outro erro vira `Err`.
- `set` e `change` recusam gravar quando a leitura falhou e guardam uma cópia `*.original-N.json`, como `recovery.rs`.
- `apply` sem os dados deixa os campos como estão e devolve um aviso para a UI (o mesmo `library_recovery_notice`).
- Serde tolerante a variantes novas: `#[serde(other)]` em `GameStatus` ou leitura por item, como o `recovery`.
- Um módulo único `player_data::JsonFile<T>` para os três (e para o `art.json`), em vez de três cópias.

**Aceite.** Com o arquivo corrompido ou travado, mudar favorito, coleção ou nota dá erro visível, e o arquivo continua
com os mesmos bytes. Depois de destravar, a gravação funciona.

**Teste.** Um teste unitário por arquivo: JSON inválido mais `set` mantém os bytes. No Windows, a trava com
`share_mode(0)`, como `locked_library_is_never_treated_as_empty` (`library.rs`).

### F2. Arte própria esquecida: `art.json` lido só na abertura, vazio se falhar — Crítica

**Onde.** `art_custom.rs:86-90` (`CustomArt::new`), `art_custom.rs:126-166` (`set`).

**Problema.** Mesmo padrão da F1, piorado pelo cache na memória. Se o `art.json` não abre na partida, o mapa fica vazio
até fechar o app. A primeira arte escolhida grava esse mapa vazio mais um item: todas as outras escolhas somem. Os
arquivos em `covers/` ficam lá, sem dono, e não são apagados.

**Reproduzir.**
1. Com 3 jogos com capa própria, deixar `art.json` travado ou inválido e abrir o Cerberus.
2. Escolher a capa de um quarto jogo.
3. Os 3 primeiros voltam à arte da loja.

**Correção.** A mesma da F1: `Result` na leitura e recusa de gravar. Ler de novo do disco em `set` quando o cache está
vazio por erro.

**Aceite e teste.** Iguais aos da F1, com `CustomArt::new` sobre um arquivo inválido.

### F3. Uma falha no `library.json` apaga a Biblioteca inteira da tela, Steam inclusive — Alta

**Onde.** `main.rs:1590` (`library.games_with_parts()?`), `ui/src/library_page.rs:451-460`, `ui/src/home.rs:128`.

**Problema.** Quando o `library.json` está travado ou não tem conserto, `recovery` faz o certo e recusa. Mas o `?` aborta
`list_games` inteira: os jogos da Steam, da Epic e do Xbox e os da conta também não aparecem. O Início mostra só o erro.
A barra lateral e a bandeja ficam vazias. Para quem tem cerca de 950 jogos de loja e 5 adicionados, a falha de 5 tira
950.

**Reproduzir.** Trocar o `library.json` por `{` e abrir: "Não foi possível carregar a biblioteca", sem nenhum jogo.

**Correção.** `list_games` segue sem os jogos locais. Devolve `LibraryList { games, problems: Vec<String> }` ou usa o
`library_recovery_notice` para avisar "Os jogos adicionados e os recomps não puderam ser lidos: …". As ações de escrita
continuam bloqueadas, como hoje.

**Aceite.** Com o `library.json` inválido, os jogos das lojas aparecem e um aviso fixo explica o problema dos outros.

**Teste.** Uma função pura `assemble(local: Result<Vec<Game>>, stores: Vec<Game>)` com teste unitário, ou teste do
comando com `LocalLibrary` apontando para um arquivo inválido.

### F4. `list_games` refaz todo o trabalho em cada clique e em rajadas de eventos — Alta

**Onde.** `main.rs:1584-1658` e §1.2. Na UI: `ui/src/main.rs:92-99`, `:313-322`, `ui/src/game_menu.rs:94, 262`,
`ui/src/library_page.rs:194, 215`.

**Problema.** Com cerca de 950 jogos, cada favorito, ocultar ou coleção custa uma montagem completa:
- 8 leituras do `settings.json`;
- o `localconfig.vdf` e o `catcache.bin` inteiros;
- 4 a 10 mil chamadas de arquivo para a arte da Steam;
- 2 aberturas de SQLite;
- cerca de 950 `Game` pelo IPC, desserializados no WASM;
- a grade inteira desenhada de novo (ver metadados §6).
Somam-se as rajadas de `catalog-changed` (até 6 depois de uma listagem quando os caches venceram) e chamadas
paralelas sem coalescência. O tempo real **não foi medido**. Pela quantidade de I/O e de serialização, é o maior
candidato a "clicar no coração demora".

**Correção, em passos.**
1. **Estado do jogador sem refazer a lista.** `set_favorite`, `set_hidden`, `set_in_collection`, `set_game_note` e
   `art_save` atualizam o `Game` na UI (`library.games.update`) e não chamam `refetch`.
2. **Cache da montagem no backend.** Guardar a última `Vec<Game>` das lojas, `Arc<Mutex<Option<(Instant, Vec<Game>)>>>`.
   As fontes de loja só são lidas de novo quando chega um `catalog-changed` ou passa um tempo curto (30 s). Favoritos,
   coleções e notas são aplicados por cima (barato).
3. **Coalescer.** Uma `list_games` em andamento é compartilhada por quem chegar durante ela (`tokio::sync::Mutex` ou
   `Shared` future). Na UI, juntar vários `catalog-changed` em 300 ms antes do `refetch`.
4. **`settings::load` uma vez** por chamada, passado por referência às funções.
5. **Arte da Steam:** listar `librarycache` uma vez (`read_dir` do diretório raiz e de cada subpasta só quando
   necessário) e montar um `HashMap<appid, (capa, fundo)>`, em vez de `local_art` por jogo. Ou guardar em cache
   pela data de alteração da pasta.
6. Ler o `owned-<id>.json` uma vez e passar para `steam_playtimes`. Guardar em cache o resultado do `localconfig.vdf`
   pela data de alteração.

**Aceite.**
- Favoritar não chama `list_games` (conferir no log de comandos).
- Uma `list_games` quente (sem mudança nas lojas) lê o `settings.json` no máximo 1 vez e não lê o `localconfig.vdf`.
- Cinco `catalog-changed` seguidos resultam em no máximo 2 montagens.
- Meta: `list_games` quente abaixo de 50 ms com 1.000 jogos (medir com `Instant` no log, antes e depois).

**Teste.** Um teste de host que monta 1.500 `Game` sintéticos e mede `assemble`. Um contador de chamadas a
`settings::load` em `cfg(test)`. Na UI, verificação manual com o painel de desempenho.

### F5. Gravações sem trava e caches na memória que podem gravar por cima de uma restauração — Média

**Onde.** `favorites.rs:17`, `collections.rs:45`, `game_notes.rs:36` (ler, mudar e gravar sem trava, e o mesmo nome
`.json.tmp` para todas as gravações). `library.rs:12-16` e `art_custom.rs:46-50` (caches na memória). `main.rs:1455-1470`
(`import_data` reinicia 1,2 s depois de gravar).

**Problema.**
- Hoje os comandos desses arquivos são síncronos e, no Windows, rodam um por vez na thread principal. Só por isso não há
  corrida. Basta um deles virar `async` (é o que a F12 pede) para dois `set` seguidos perderem um ao outro e
  disputarem o mesmo `.tmp`.
- Na restauração, os novos `library.json` e `art.json` vão para o disco. Mas `LocalLibrary` e `CustomArt` continuam com
  a versão antiga na memória por 1,2 s, mais o tempo até fechar. Uma partida que termina nesse intervalo
  (`record_session` → `commit`) grava a biblioteca antiga por cima da restaurada. **Não reproduzido** (janela curta).

**Correção.**
- Um `Mutex` por arquivo dentro do `JsonFile<T>` da F1. `.tmp` com nome único (`tempfile::NamedTempFile::persist`).
- Na restauração: bloquear `LocalLibrary` e `CustomArt` (um sinalizador "restaurando" que faz `commit` recusar) antes de
  gravar, e reiniciar.

**Aceite.** 100 `set_favorite` paralelos em threads resultam em 100 favoritos. Uma sessão que fecha durante a
restauração não grava por cima.

**Teste.** Unitário com `std::thread::scope` e 50 gravações paralelas por arquivo.

### F6. Arte que falha é pedida de novo em toda listagem, e a webview baixa a mesma URL — Média

**Onde.** `art_cache.rs:48-60` (`swap`), `art_cache.rs:82-104` (`fetch_later`), `main.rs:1655`.

**Problema.**
- Uma URL que responde 404 (muitos jogos antigos da Steam não têm `library_600x900.jpg` nem `library_hero.jpg`; vários
  da conta não instalados) volta para `missing` em toda `list_games` e é pedida de novo, um pedido por vez, para sempre.
  Não há cache de falha.
- Enquanto a cópia não existe, a UI recebe a URL `http…` e a webview baixa a mesma imagem.
- `fetch_later` não avisa a UI quando termina: a cópia só é usada na próxima listagem.
- O cache não tem limite de tamanho nem limpeza de órfãos (só "Limpar" manual).

**Reproduzir.** Com a Steam Web API ligada, observar a rede: as mesmas URLs com 404 a cada `catalog-changed`
(**não medido**).

**Correção.**
- Um arquivo `<cache>/art/failed.json` com `url → (status, quando)`. 404 espera 7 dias; erro de rede espera 1 hora.
- Pular essas URLs em `swap` (não pôr em `missing` e entregar a URL seguinte da lista à UI).
- Opcional: limite de 500 MB com remoção pelos mais antigos.

**Aceite.** Uma URL com 404 é pedida no máximo uma vez por semana. Uma segunda `list_games` não pede nada que falhou
na primeira.

**Teste.** Unitário de `swap` com um `failed.json` sintético. Unitário de que um 404 grava a falha (cliente com
servidor local).

### F7. Jogo antigo da Steam sem capa: falta o `header.jpg` como alternativa — Média

**Onde.** `steam.rs:92-104` (`not_installed`), `steam.rs:146-147` (`read_manifest`), `ui/src/art.rs:21-31`.

**Problema.** Só se tenta `library_600x900.jpg` (local e na CDN). Jogos antigos ou pequenos só têm `header.jpg` e
`capsule_231x87.jpg`. Eles caem no retângulo com o nome. Numa conta com centenas de jogos antigos, boa parte da grade
fica sem arte. O ROADMAP 6.3 ("Arte padrão quando falta") fala da ideia, mas não deste caso concreto.

**Correção.** Pôr `…/apps/<id>/header.jpg` como terceira fonte da capa: `Poster` já tenta as fontes em ordem. O
`header` é horizontal, então desenhar com `blur-fill` e `object-fit: contain`, como as caixas do N64. No fundo, usar
`library_hero` e depois `page_bg_generated_v6b.jpg` (**não verificado** se o nome é estável).

**Aceite.** Um jogo sem `library_600x900` mostra o `header` sobre o fundo desfocado, nunca o retângulo, quando a Steam
tem qualquer arte.

**Teste.** Unitário de que `not_installed` e `read_manifest` devolvem 3 fontes de capa. Conferência manual com
2 appids antigos.

### F8. "Adicionar todos à Steam" coloca os jogos ocultos — Média

**Onde.** `main.rs:3081-3101`.

**Problema.** O comentário diz "(hidden ones aside)" e o código testa `game.hidden`. Mas os jogos vêm de
`LocalLibrary::games()` e `epic::installed_games()`, onde `hidden` é sempre `false`: o oculto só é aplicado em
`list_games` (`collections::apply`). Todo jogo oculto vai para a Steam.

**Reproduzir.** Ocultar um jogo adicionado e usar "Adicionar todos à Steam": ele entra.

**Correção.** Chamar `collections::apply(&data, &mut games)` antes do laço, ou filtrar por
`collections::load(&data).hidden`.

**Aceite.** Um jogo oculto não é adicionado e não conta em "N adicionados".

**Teste.** Extrair `fn eligible(games, shelf, in_steam) -> Vec<Game>` e testar com um oculto.

### F9. O destaque do Início mostra jogos ocultos (e o pacote de um lançamento com vários jogos) — Média

**Onde.** `ui/src/home.rs:99-104` (`features`).

**Problema.** As prateleiras filtram `!g.hidden && g.part_ids.is_empty()` (linhas 139, 158 e 172), mas o destaque
"Continue jogando" não filtra. Um jogo que o jogador escondeu aparece em tamanho grande no topo do Início. Um
lançamento com vários jogos pode aparecer junto com as suas partes.

**Reproduzir.** Jogar um jogo, ocultá-lo e abrir o Início: ele está no destaque.

**Correção.** Em `features`, filtrar `!g.hidden && g.part_ids.is_empty()`.

**Aceite.** Nenhum oculto no destaque.

**Teste.** `features` é pura: um teste unitário no crate `ui` (host) com um jogo oculto.

### F10. Anotações digitadas se perdem quando a página do jogo é refeita — Média

**Onde.** `ui/src/game.rs:52-64` (`content` acompanha `library.games.get()`), `ui/src/game_notes.rs:103-112` (só grava
no `blur`).

**Problema.**
- A página inteira do jogo é refeita sempre que `library.games` muda: `catalog-changed`, `playtime-updated`, a
  `list_games` de outra ação, uma sincronização em segundo plano. O `NoteCard` volta com o texto do disco, e o que
  estava sendo digitado e ainda não gravado (o `blur` não aconteceu) some.
- Também se perde ao fechar o app pelo X ou pela bandeja sem sair do campo, e ao voltar com o botão lateral do mouse
  (**não verificado** se o `blur` dispara nesses casos).

**Reproduzir.** Abrir a página de um jogo da Steam, digitar uma anotação longa sem clicar fora e, ao mesmo tempo,
terminar uma partida de outro jogo (gera `playtime-updated`): o texto volta ao anterior. **Não reproduzido no app.**

**Correção.**
- Gravar com espera de 800 ms durante a digitação, além do `blur`.
- Fazer a página depender só do jogo dela: um `Memo` por id com `PartialEq`, para não ser refeita quando outro jogo
  muda. Hoje o `Vec<Game>` inteiro vem por `get()` e a busca é linear.
- Ao fechar a janela, gravar o rascunho pendente (`beforeunload`).

**Aceite.** Digitar, esperar 1 s e disparar `catalog-changed`: o texto continua e está no `game-notes.json`.

**Teste.** Manual com `window.__TAURI__.event.emit('catalog-changed')` no console de desenvolvimento.

### F11. Menu do jogo pelo teclado: as setas pulam as coleções e "roubam" teclas da caixa de nome — Média

**Onde.** `ui/src/game_menu.rs:128-157` (`step_items`), `:160-170` (escuta `keydown` na janela).

**Problema.**
- `step_items` só procura `[role=menuitem]`. As coleções são `role=menuitemcheckbox`, então ninguém de teclado chega
  nelas pelas setas.
- Com a caixa "Nova coleção…" aberta, o ouvinte da janela continua ativo: Home, End e as setas tiram o foco da caixa
  (com `prevent_default`), e não dá para mover o cursor no texto.

**Correção.**
- Seletor `[role^=menuitem]:not([disabled])` nos dois lugares (linhas 44 e 137).
- Em `step_items`, sair cedo quando o alvo é `input` ou `textarea`.
- Esc dentro da caixa fecha só a caixa.

**Aceite.** Seta para baixo percorre Jogar, Página, Favoritar, cada coleção e Nova coleção. Home e End funcionam dentro
da caixa.

**Teste.** Manual com teclado e com o Narrador do Windows.

### F12. Comandos síncronos com I/O pesado na thread principal — Média

**Onde.**
- `main.rs:450` `art_save`: decodifica até cerca de 33 MB de base64 e grava a imagem.
- `main.rs:2940` `create_shortcut`: quando o jogo não é local, lê as três lojas inteiras, e o COM roda numa thread que
  é esperada com `join`.
- `main.rs:3105` `remove_from_steam`: lê e grava `shortcuts.vdf` de cada conta.
- `main.rs:2051` `rename_game`: grava o `library.json` inteiro, cerca de 950 KB **(estimado)**.
- `main.rs:2865-2912`: os comandos de favoritos, coleções e notas (arquivos pequenos, risco menor).

**Problema.** No Windows, um `#[tauri::command]` síncrono roda na thread principal: a janela congela enquanto ele
trabalha. Uma arte de 20 MB ou um disco lento travam a UI por segundos. Outros comandos do mesmo arquivo já usam
`#[tauri::command(async)]`, como `remove_game`, `in_steam` e `look_status`.

**Correção.** `#[tauri::command(async)]` nesses comandos (depois da F5, para não abrir corrida). No `art_save`,
`spawn_blocking`. No `create_shortcut`, usar `any_game` e trocar a varredura por uma consulta ao cache da F4.

**Aceite.** Salvar uma capa de 20 MB não congela a animação da UI.

**Teste.** Manual. Opcional: um teste de que os comandos da lista têm o atributo (verificação com `grep` no CI).

### F13. Arte e capas guardadas com caminho absoluto se perdem ao restaurar em outro PC — Média

**Onde.** `art_custom.rs:138-141` (grava `path.to_string_lossy()` absoluto), `main.rs:3285-3293` (`copy_cover`, idem no
`library.json`), `data_backup.rs:117-122` (restaura `covers/` com outro caminho base).

**Problema.** A pasta de dados fica no perfil do usuário. Em outro PC, ou com outro nome de usuário, os caminhos do
`art.json` e o `cover_path` do `library.json` apontam para uma pasta que não existe. `apply` só confere `is_file` e cai
em silêncio na arte da loja. O backup diz que leva "a arte própria", mas ela não volta.

**Reproduzir.** Fazer o backup, restaurar com outro `%APPDATA%` (outro usuário) e abrir um jogo com capa própria: a arte
da loja aparece. **Não reproduzido.**

**Correção.** Gravar só o nome do arquivo (relativo a `covers/`) e montar o caminho ao aplicar. Ao ler, migrar os
absolutos que começam com qualquer `…\covers\` para o nome só.

**Aceite.** Restaurar em outra pasta de dados mantém todas as capas próprias.

**Teste.** Unitário: `set` numa pasta, copiar a pasta de dados para outra e aplicar (o caminho resolve).

### F14. "Sua biblioteca está vazia" com clássicos nas pastas — Baixa

**Onde.** `ui/src/library_page.rs:471-480`.

**Problema.** A verificação de `games.is_empty()` vem antes de juntar os clássicos. Quem só tem ROMs (ou ainda não ligou
as lojas) vê "Sua biblioteca está vazia", e a aba Todos não mostra os clássicos.

**Correção.** Calcular `classic_rows` antes e mostrar o vazio só se as duas listas estiverem vazias.

**Aceite.** Com só ROMs, a aba Todos lista os clássicos.

**Teste.** Manual.

### F15. `shortcuts.vdf`: cópia original sobrescrita, nomes fora de UTF-8 alterados, arte órfã — Baixa

**Onde.** `steam_shortcuts.rs:280-292` (`save`), `:48` (`from_utf8_lossy`), `:222-227` (`add`).

**Problema.**
- A cópia `shortcuts.vdf.cerberus.bak` é refeita a cada operação com mais de 10 min de distância. Depois de duas
  mudanças, a versão de antes do Cerberus se perde.
- Um nome de outro programa gravado fora de UTF-8 (editado à mão, ferramentas antigas) volta com `U+FFFD`: o Cerberus
  muda um atalho que não é dele (**não verificado** se a Steam grava algo fora de UTF-8).
- Renomear um jogo e adicioná-lo de novo troca o `appid` (CRC de exe + nome). A arte em `grid/` com o número antigo fica
  órfã.
- `put_in_steam` ignora o resultado de `place_art`: diz "com sua arte" mesmo sem imagem.

**Correção.**
- Guardar `Vec<u8>` nos textos e só comparar como UTF-8.
- Manter `shortcuts.vdf.cerberus-original.bak` criado uma vez só, mais o de rodízio.
- Em `add`, antes de trocar a entrada, apagar a arte do `appid` antigo.
- Avisar quando nenhuma arte foi copiada.

**Aceite.** Um nome com bytes Latin-1 de outro programa continua com os mesmos bytes depois de adicionar um jogo. Depois
de renomear e adicionar de novo, `grid/` não tem arquivos do `appid` antigo.

**Teste.** Unitário com `parse` e `write` de bytes não UTF-8. Unitário de que `add` devolve os `appid` antigos para
`remove_art`. Um `shortcuts.vdf` real anonimizado como dado de teste (hoje só há árvores montadas à mão).

### F16. Ids que mudam ou se repetem: dados órfãos e jogo duplicado — Baixa

**Onde.** `gamepass.rs:220`, `library.rs:171`, `library.rs:89-95` (`add`), `main.rs:2029-2047` (`add_local_game`).

**Problema.**
- Um jogo do Game Pass sem família usa `xbox:<produto>`. Quando é instalado, passa a `xbox:<família>`, e favorito,
  nota e coleção ficam no id velho (**não verificado** com que frequência o catálogo vem sem família).
- Um jogo adicionado à mão é `local:<caminho>`: ao mover a pasta e adicionar de novo, perde tudo. Não há "trocar o
  programa" que mantenha o id.
- Adicionar um `.exe` que já está na biblioteca: `add` volta `Ok` em silêncio, e o editor abre com um `Game` novo (nome
  e capa da Steam) que não foi gravado. A UI mostra dados que não existem.
- Adicionar o `.exe` de um jogo instalado pela Steam cria uma segunda entrada (`local:` e `steam:`).

**Correção.**
- `add` devolve o jogo existente. `add_local_game` diz "Esse jogo já está na biblioteca" e abre esse.
- Recusar ou oferecer "usar o da Steam" quando o exe está dentro de `steamapps\common\<installdir>` de um `steam:`
  conhecido.
- Um `aliases` por id no `library.json` (id antigo → novo) usado em `apply` de favoritos, coleções e notas, e uma ação
  "Localizar programa" para jogo adicionado que mantém o id.
- Game Pass: chavear pelo produto sempre que ele existe.

**Aceite.** Adicionar duas vezes o mesmo exe não duplica e não mostra dados falsos. Mover um jogo adicionado e
localizá-lo mantém favorito e nota.

**Teste.** Unitário de `add` duplicado. Unitário de `gamepass::merge` antes e depois da família.

### F17. Remover um jogo deixa favoritos, coleções, notas e arte própria para trás — Baixa

**Onde.** `main.rs:2105-2139` (`remove_game`).

**Problema.** Só o `library.json` e a capa copiada são limpos. As entradas em `favorites.json`, `collections.json`,
`game-notes.json` e `art.json` ficam, e os arquivos de arte própria em `covers/` ficam no disco e no backup.
(Readicionar o mesmo exe recupera tudo, o que pode ser bom, mas ninguém decidiu isso.)

**Correção.** Decidir: limpar tudo, ou guardar por 30 dias e limpar depois. Na limpeza, chamar `CustomArt::set(id, kind,
None)` para as três artes.

**Aceite.** Depois de remover, `covers/` não tem arquivos daquele id (ou o comportamento decidido está escrito).

**Teste.** Unitário de uma função `forget_player_data(data, id)`.

### F18. Acessibilidade e teclado — Baixa

**Onde e problema.**
- `ui/src/game_notes.rs:88-100`: as estrelas não têm `aria-pressed` nem forma de anunciar a nota atual. Melhor um grupo
  de rádio "Nota: 3 de 5".
- `ui/src/tiles.rs` (`fav-mark`): o favorito só tem `title`. O leitor de tela não anuncia "Favorito" no cartão.
- `ui/src/library_page.rs:360-365`: `role=tablist` e `role=tab` sem `tabpanel`, sem `aria-controls` e sem setas entre
  as abas.
- `ui/src/shell.rs` (`sg-search`): a caixa "Filtrar jogos" só tem `placeholder`, sem `aria-label`.
- `ui/src/game_notes.rs` (`RandomPick`): Esc não fecha, o foco não fica preso no diálogo, e o "Nada para sortear" não
  tem `role=dialog`.
- `ui/src/game_menu.rs:21`: altura fixa de 420 px para posicionar o menu. Com muitas coleções, ele passa da borda de
  baixo (**não verificado** se o CSS limita).
- Os atalhos de teclado do ROADMAP 6.3 (F5, Ctrl+F) já estão listados lá.

**Correção.** Itens pequenos, um por linha.

**Aceite.** O Narrador anuncia a nota, o favorito e a aba atual. Esc fecha o sorteio.

**Teste.** Manual com o Narrador e só com teclado.

### F19. Erros engolidos — Baixa

**Onde e problema.**
- `art_cache.rs:99`: `let _ = cache.fetch(...)`. A falha não fica em lugar nenhum (é a F6 sem log).
- `main.rs:1960-1975` (`xbox_art`): quem não é achado na Steam vira `None` para sempre e nunca é procurado de novo,
  mesmo quando a Steam passa a ter o jogo.
- `favorites::load`, `collections::load` e `game_notes::load`: o erro some (é a F1).
- `put_in_steam`: ignora a contagem de `place_art` (F15).
- `fetch_later` e `owned_not_installed` só emitem quando algo muda. Uma falha de rede não aparece no `trail` nem no
  cartão da conta (parte disso está em lojas L8).

**Correção.** Registrar no `trail` (uma linha por fonte, não por imagem). Para o Xbox, `None` com data e nova tentativa
em 30 dias.

**Aceite.** Uma falha de rede ao buscar arte aparece uma vez no histórico. Um jogo Xbox sem arte é procurado de novo
depois de 30 dias.

**Teste.** Unitário de `xbox::art` com a data da falha.

### F20. Testes que faltam — Baixa

Nada testa a montagem de `list_games` (ordem, ids únicos, `hidden` e `favorite` aplicados, jogos da conta somados aos
instalados). Também faltam testes para:
- arquivo do jogador corrompido (F1 e F2);
- `shortcuts.vdf` real;
- `owned_not_installed` com um jogo já instalado;
- `features` do Início;
- os filtros e as contagens da Biblioteca (`Category::keeps`, `Filter::keeps`, `counts`).

**Correção.** Separar uma função pura `assemble(inputs) -> Vec<Game>` de `list_games` (os leitores de loja passam a ser
entradas) e escrever:
- teste de ids únicos, sem duplicata entre instalado e conta;
- teste de que favorito, oculto, coleção e nota chegam ao `Game`;
- teste de que um id da Steam instalado não aparece de novo como não instalado;
- teste de 1.500 jogos com tempo medido (base para a F4).
Na UI, mover `Category::keeps`, `Filter::keeps` e `features` para funções puras testáveis no host.

**Aceite.** `cargo test` cobre `assemble` e as falhas F1, F2, F8, F9 e F16.

---

## 4. Lacunas (o que falta para a Biblioteca ficar completa, sem ser defeito)

- **Tirar da biblioteca um jogo de loja de verdade** (exclusão de importação, como o Playnite): hoje só dá para ocultar.
- **"Atualizar biblioteca" visível** (F5 ou botão) com "atualizado há…" e o que falhou por loja. Hoje não existe, e o
  jogador não sabe se a lista está fresca.
- **Jogos da Steam sem a Web API**: sem chave, os não instalados não aparecem. O plano
  [steam-sem-chave](steam-sem-chave-e-gerenciar-lojas.md) cobre isso. Lembrar aqui que um jogo desinstalado some da
  Biblioteca (os dados ficam no JSON).
- **Virtualizar a grade e a barra lateral** (Hydra), se a medição do plano de metadados mostrar necessidade.
- **Backup automático dos arquivos do jogador** com rodízio (ROADMAP 6.8 e 7.x). Depois da F1, é a segunda linha de
  defesa.
- **Menu do jogo completo**: "Adicionar à Steam", "Remover", "Editar" e "Personalizar arte" pelo clique direito, como no
  Hydra. Hoje só pela página.
- **Busca que ignora acentos e sinais de marca** ("Pokemon" acha "Pokémon", "Halo" acha "Halo®"). Está no índice
  proposto em metadados §6. Juntar à `names::key` da lojas L13.
- **SteamGridDB pelo appid** (`/games/steam/<appid>`) para jogos da Steam, em vez da busca por nome, que erra em nomes
  repetidos.

---

## 5. Ordem sugerida

| Ordem | Falhas | Por quê | Tamanho |
| --- | --- | --- | --- |
| 1 | F1 + F2 + F5 (um `JsonFile<T>` com `Result`, trava e cópia) | perde dados do jogador; uma peça só resolve as três | M |
| 2 | F3 | um arquivo ruim esconde 950 jogos | P |
| 3 | F8, F9, F14 | correções de uma linha | P |
| 4 | F4, passos 1 e 4 (sem `refetch` nas ações do jogador; `settings` uma vez) | o maior ganho percebido com pouco código | P/M |
| 5 | F10, F11 | anotações perdidas e menu inacessível | P |
| 6 | F12 (depois da F5) | congelamentos | P |
| 7 | F4, passos 2, 3, 5 e 6 (cache, coalescência, arte da Steam por pasta) + F20 (`assemble` testável) | desempenho de verdade, com medida antes e depois | M |
| 8 | F6, F7, F19 | arte | P/M |
| 9 | F13, F15, F16, F17 | dados entre PCs, Steam e ids | M |
| 10 | F18 | acessibilidade | P |

Contagem: **Crítica 2** (F1, F2) · **Alta 2** (F3, F4) · **Média 9** (F5 a F13) · **Baixa 7** (F14 a F20).
