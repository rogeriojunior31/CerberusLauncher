# GOG e as outras lojas (Onda 7.2, item 2 do Mapa rápido)

Plano de implementação. Pesquisa feita em 2026-11-25. Nenhuma das lojas estava instalada no PC da pesquisa
(sem GOG Galaxy, Ubisoft Connect, EA app, Battle.net nem Amazon Games), então **nada aqui foi testado contra
um cliente real**. O que veio de código de terceiros lido diretamente leva a fonte; o que não foi confirmado
está marcado **(não verificado)**.

Fontes lidas:

- Playnite Extensions (MIT, pode portar): `GogLibrary`, `AmazonGamesLibrary`, `UplayLibrary`, `BattleNetLibrary`
  em `github.com/JosefNemec/PlayniteExtensions/source/Libraries`. O repositório local do Playnite
  (`C:\Users\roger\Projects\OpenSource\Games\Playnite`) **não tem** as bibliotecas das lojas; elas ficam nesse
  outro repositório. Não existe mais biblioteca EA/Origin no Playnite.
- Heroic (`HeroicGamesLauncher\src\backend\storeManagers\gog` e `nile`, GPL-3.0): **só ideias**. O login,
  o token e o download moram no `heroic-gogdl` (Python, GPL-3.0) e no `nile` (GPL-3.0); também só ideias.
- Hydra (MIT): nada de GOG além de reconhecer as pastas "GOG Games"/"GOG Galaxy" como pastas de jogos.
- Documentação não oficial da API da GOG: `gogapidocs.readthedocs.io` (auth e account).
- GameFinder wiki (`github.com/erri120/GameFinder/wiki`, projeto GPL-3.0): só ideias.
- Script `AB1908/GOG-Galaxy-Export-Script` (leitura do `galaxy-2.0.db`).

---

## Objetivo

1. Os jogos instalados pelo **GOG Galaxy** (e os instalados pelos instaladores offline da GOG, sem Galaxy)
   aparecem na Biblioteca com selo GOG, capa, e abrem pelo Galaxy ou direto pelo executável.
2. Com **login GOG** (opcional, cartão em Configurações › Contas), entram os jogos da conta não instalados
   (instalar pelo Galaxy), as **conquistas GOG** no hub de conquistas e o **tempo de jogo** da GOG.
3. Depois, no mesmo molde: **Amazon Games**, **Ubisoft Connect**, **EA app** e **Battle.net**, primeiro só os
   instalados (sem conta). Contas só onde houver um login por página da própria loja (OAuth/código) e tokens;
   onde a única API pede a senha, não se faz.

Fora do escopo: baixar/instalar jogos pelo próprio Cerberus (o Heroic faz isso com o gogdl; aqui a loja oficial
instala), salvar na nuvem da GOG, comprar.

---

## O que o Cerberus já tem para imitar

| Peça | Onde | O que copiar |
|---|---|---|
| Instalados sem rede | `src-tauri/src/epic.rs` (`installed_games`, `launch_url`, `launcher_found`) | um módulo por loja que só lê disco/registro e devolve `Vec<Game>`; checa se a pasta existe |
| Login por janela privada | `epic_login`/`finish_epic_login` em `main.rs` | `WebviewWindowBuilder ... .incognito(true).on_navigation(...)`; o código é lido da navegação e a página nunca toca o app |
| Token e biblioteca | `epic_account.rs` (`token`, `library`, `Owned`, `cached`/`stale`/`keep`/`forget`, `merge`, `install_url`) | cache em `<data>/<loja>-library.json` com `at`, `FRESH_FOR = 3600`, escrita atômica `.tmp` + `rename` |
| Atualização em segundo plano | `epic_library` em `main.rs` | `AtomicBool ASKING`, renova o refresh token, `emit("catalog-changed")`, erro vai para `trail::note` |
| Conquistas | `achievements/epic.rs` + `sync_epic`, `sync_accounts`, `hub`, `account_times` em `achievements/mod.rs`; banco `achievements/store.rs` | `Scope::new(AchievementSource::X, user)`, `db.due/synced/refused`, pausa entre pedidos, só pede a lista de um jogo quando o tempo dele cresceu |
| Tempo contado aqui | `store_time.rs` (`is_store`, vigia processos na pasta de instalação) | incluir as novas `Source` em `is_store` |
| Selo | `ui/src/tiles.rs` (`Mark::Store("epic")`, `source_name`), `ui/styles.css` `.store-logo.<nome>`, SVGs em `ui/brand/stores/` + `LICENSE.md` | um SVG por loja, usado como máscara (cor do tema) |
| Filtro | `ui/src/library_page.rs` (`Filter::Epic`...) | um filtro por loja que tiver jogos |
| Cartão de conta | `ui/src/settings_page.rs` (`Section::Epic`, cartão "Epic Games") | mesmo cartão, mesmo texto de "o que a conta traz" |
| Dedupe entre lojas | `gamepass::drop_owned_elsewhere` | o catálogo "não instalado" some se o jogador já tem o mesmo nome noutra loja |

Crates já no projeto: `winreg 0.55` (registro), `rusqlite 0.32 bundled` (SQLite), `reqwest`, `serde_json`.
Não precisa de dependência nova para GOG, Amazon, Ubisoft (registro) nem Battle.net (registro). Só os
caminhos "ricos" de Ubisoft e Battle.net precisariam de protobuf (`prost`); este plano **evita** esses caminhos.

---

## GOG

### Sem login (o que dá para ler do PC)

**1. Registro do GOG (Galaxy e instaladores offline)**

- Chave: `HKLM\SOFTWARE\GOG.com\Games\<productId>` lida na **vista 32 bits** (no Windows 64 ela fica em
  `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games`). Com `winreg`: `RegKey::predef(HKEY_LOCAL_MACHINE)
  .open_subkey_with_flags(r"SOFTWARE\GOG.com\Games", KEY_READ | KEY_WOW64_32KEY)`. Cada subchave é um jogo e o
  nome dela é o id do produto (GameFinder wiki).
- Valores por jogo (conhecidos da comunidade, **não verificados** contra um PC real): `gameID`, `gameName`,
  `path` (pasta), `exe`, `exeFile`, `launchCommand`, `launchParam`, `workingDir`, `dependsOn` (preenchido em
  DLC: id do jogo base), `buildId`, `productID`, `uninstallCommand`, `ver`, `language`.
  Exemplo esperado:

  ```
  HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\1207658924
      gameName    = "Unreal Tournament 2004 Editor's Choice Edition"
      path        = "C:\GOG Games\Unreal Tournament 2004"
      exe         = "C:\GOG Games\Unreal Tournament 2004\System\UT2004.exe"
      dependsOn   = ""
  ```

- Regra: descartar se `dependsOn` não estiver vazio (DLC) ou se a pasta não existir.

**2. Chaves de desinstalação (pega instaladores offline mesmo sem a chave acima)** — Playnite `GogLibrary.cs`:

- Percorrer `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall` (vistas 32 e 64) e `HKCU\...\Uninstall`.
- A subchave precisa casar `^(\d+)_is1$` (o número é o productId), `Publisher == "GOG.com"`, e não começar
  com `GOGPACK`. Pasta em `InstallLocation`, nome em `DisplayName` (sem ™/®).

**3. `goggame-<productId>.info` na pasta do jogo** (JSON; Playnite e Heroic leem):

```json
{
  "gameId": "1207658924",
  "rootGameId": "1207658924",
  "name": "Unreal Tournament 2004",
  "clientId": "4936...",
  "playTasks": [
    { "category": "game", "isPrimary": true, "type": "FileTask",
      "path": "System\\UT2004.exe", "arguments": "", "workingDir": "System", "name": "Unreal Tournament 2004" }
  ]
}
```

- Sem `playTasks` ou com `rootGameId` diferente de `gameId`: é DLC, fica de fora (regra do Playnite).
- A `playTask` com `isPrimary: true` é o executável para abrir sem Galaxy; o caminho é relativo à pasta.
- `clientId` (quando existe) é o id do jogo na Galaxy, usado pelas conquistas (ver abaixo). Formato exato do
  campo e presença em jogos antigos: **não verificado** (o Heroic o lê do `.info` como plano B).

**4. Banco do Galaxy: `C:\ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db` (SQLite)**

Tabelas confirmadas pelo script de exportação (AB1908): `GamePieces(releaseKey, gamePieceTypeId, value)`,
`GamePieceTypes(id, type)` com tipos `title`, `originalTitle`, `sortingTitle`, `summary`, `originalImages`,
`meta`, `originalMeta`, `dlcs`, `osCompatibility`, `allGameReleases`, `myRating`;
`GameTimes(releaseKey, minutesInGame)` (**minutos**); `LastPlayedDates(gameReleaseKey, lastPlayedDate)`;
`ProductPurchaseDates(gameReleaseKey)`; `UserReleaseTags(releaseKey, tag)`;
`UserReleaseProperties(releaseKey, isHidden)`.

- `releaseKey` tem o prefixo da plataforma: `gog_<productId>`, `steam_<appid>`, `epic_<...>`, `xboxone_...`
  (o Galaxy integra outras lojas). **Só ler `gog_*`**; o resto o Cerberus já lê das próprias lojas.
- O que dá sem login nenhum no Cerberus (o jogador entrou no Galaxy):
  - **tempo de jogo** e **última vez jogado** dos jogos GOG (`GameTimes`, `LastPlayedDates`);
  - **a biblioteca inteira da conta GOG**, inclusive não instalados (`ProductPurchaseDates` + `GamePieces`
    title), com capa em `originalImages` (JSON com `verticalCover`, `background`, `squareIcon` —
    **não verificado**);
  - "oculto" e tags do Galaxy.
  - Possível tabela de instalados (`InstalledBaseProducts(productId, ...)`) e `LimitedDetails`
    (**não verificado**: não usar como única fonte; o registro é a fonte dos instalados).
- Formato de `lastPlayedDate` (texto ISO ou número): **não verificado**; o leitor aceita os dois.
- Abrir **só leitura** (`OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX`, `busy_timeout` 2 s). Se o
  Galaxy segura o arquivo, copiar `galaxy-2.0.db` (+ `-wal`/`-shm` se existirem) para
  `<data>/cache/gog/` e ler a cópia. Nunca escrever nele.
- O esquema é interno e muda entre versões do Galaxy: toda consulta tolera tabela/coluna ausente
  (resultado vazio, sem erro na tela).

**5. Onde está o Galaxy**

- `HKLM\SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths` valor `client` (pasta), `GalaxyClient.exe` dentro
  (Playnite `Gog.cs`; fallback `HKLM\SOFTWARE\GOG.com\GalaxyClient\paths`).
- "Galaxy aberto": o Playnite olha o processo `GOG Galaxy Notifications Renderer`.
- Pasta de travas da instalação: `C:\ProgramData\GOG.com\Galaxy\lock-files` (o Playnite vigia
  `GOG Galaxy Notifications Renderer.exe.lock` para saber quando a instalação terminou).

**Capas sem login** (endpoints públicos, sem token):

- `https://api.gog.com/products/<id>?expand=description` → `title`, `images.background`, `images.logo2x`,
  `images.icon`, `game_type` (`game`/`dlc`/`pack`) (Heroic usa; campos **não verificados** um a um).
- `https://api.gog.com/products?ids=<id1>,<id2>,...` (lote, até ~50) — **não verificado**.
- `https://gamesdb.gog.com/platforms/gog/external_releases/<id>` → `game.vertical_cover.url_format`,
  `game.background.url_format`, `game.logo.url_format` (substituir `{formatter}`/`{ext}`). O Heroic manda o
  cabeçalho `X-GOG-Library-Cert` quando tem, mas a rota responde sem ele (**não verificado**).
- Quando tudo falha, o `art_cache` atual (SteamGridDB pelo nome) cobre.

### Como abrir e instalar

| Ação | Como | Fonte |
|---|---|---|
| Jogar pelo Galaxy | `"<GalaxyClient.exe>" /command=runGame /gameId=<id> /path="<pasta>"` (o Playnite passa também `/launchViaAutostart`) | Playnite |
| Jogar sem Galaxy | executar a `playTask` primária do `.info` (ou `exe` do registro) por `sessions::start`, como os jogos locais | Playnite |
| Página do jogo / instalar | `goggalaxy://openGameView/<id>` | Playnite (`GogGameController.cs`) |
| Instalar direto | `"<GalaxyClient.exe>" /gameId=<id> /command=installGame`, esperar ~2,5 s e mandar o `openGameView` (o Galaxy perde dois comandos seguidos) | Playnite |
| Sem Galaxy instalado | abrir `https://www.gog.com/account` (baixar o instalador offline) | Playnite |
| Desinstalar | `unins000.exe` na pasta (Inno Setup) ou o Galaxy | Playnite |

Escolha de padrão: **pelo Galaxy quando ele existe** (conquistas GOG e tempo do Galaxy só contam com ele
aberto, porque os jogos usam o SDK do Galaxy), direto quando não existe. Uma opção por jogo
("Abrir sem o GOG Galaxy") pode vir depois; jogos GOG são sem DRM, então funciona.

### Com login GOG

**OAuth** (gogapidocs; os mesmos valores aparecem no gogdl e no lgogdownloader; são do próprio GOG Galaxy):

```
CLIENT_ID     = 46899977096215655
CLIENT_SECRET = 9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9
REDIRECT      = https://embed.gog.com/on_login_success?origin=client
```

1. Janela privada em
   `https://auth.gog.com/auth?client_id=46899977096215655&redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient&response_type=code&layout=galaxy`
   (o Heroic usa `layout=galaxy`; o gogapidocs, `layout=client2`).
2. `on_navigation`: quando `host == "embed.gog.com"` e `path == "/on_login_success"`, ler `code` da query,
   **devolver `false`** (não carrega) e seguir. Mais simples que a Epic: o código vem na própria URL, sem
   `eval`. Validar o código (letras, dígitos, `-`, `_`, tamanho 16..1024).
3. Trocar o código: `GET https://auth.gog.com/token?client_id=…&client_secret=…&grant_type=authorization_code&code=<code>&redirect_uri=<REDIRECT codificado>`.
   Resposta: `access_token`, `expires_in` (3600), `token_type: bearer`, `refresh_token`, `user_id`,
   `session_id`, `scope`.
4. Renovar: `GET https://auth.gog.com/token?client_id=…&client_secret=…&grant_type=refresh_token&refresh_token=<r>`.
   A resposta traz **outro** refresh token: guardar o novo (como a Epic).
5. Nome do jogador: `GET https://users.gog.com/users/<user_id>` com `Authorization: Bearer` → `username`
   (Heroic). Alternativa: `https://embed.gog.com/userData.json` → `username`, `userId` (**não verificado** com
   bearer; no navegador funciona por cookie).

Guardar em `settings.json`: `gog_user` (nome), `gog_user_id`, `gog_refresh`. **Nunca** o access token em
disco nem senha. O access token vive só na memória da tarefa (dura 1 h).

**Biblioteca (jogos da conta, inclusive não instalados)**

- Simples: `GET https://embed.gog.com/user/data/games` (bearer) → `{"owned":[1207658924, ...]}` (só ids;
  inclui DLC, filmes e pacotes, que precisam sair pelo `game_type`).
- Mais rica (o que o Galaxy usa, ideia do Heroic):
  `GET https://galaxy-library.gog.com/users/<user_id>/releases[?page_token=…]` (bearer) → `items[]` com
  `platform_id`, `external_id`, `owned`, `date_created`, `owned_since`, `certificate`, e `next_page_token`.
  Filtrar `platform_id == "gog"` (o resto são lojas integradas no Galaxy).
- Nome e capas: gamesdb/`api.gog.com/products` acima, com pausa de 250 ms e **só para ids novos** (guardar o
  que já foi resolvido no cache).
- Alternativa usada pelo Playnite (cookies, não token): `https://www.gog.com/account/getFilteredProducts?hiddenFlag=0&mediaType=1&page=<n>&sortBy=title`
  (`mediaType=1` = jogos; traz `title`, `image`, `isHidden`, `totalPages`). Com bearer: **não verificado**.

**Tempo de jogo**

- Fonte principal: `GameTimes.minutesInGame` do `galaxy-2.0.db` (sem login no Cerberus; precisa do Galaxy).
- Com login, sem Galaxy: o Playnite usa `https://www.gog.com/u/<username>/games/stats?sort=recent_playtime&order=desc&page=<n>`
  (cookies; `stats.<chave>.playtime` em minutos — o Playnite multiplica por 60 — e `lastSession`). Com bearer
  em vez de cookie: **não verificado**. Um fórum da GOG diz que não há API de tempo por token
  (**não verificado**). Se não funcionar com bearer, fica só o banco do Galaxy + o que o Cerberus conta.
- O Heroic **envia** sessões para a GOG (`POST https://gameplay.gog.com/games/<id>/users/<user_id>/sessions`
  com `{session_date, time}`). O Cerberus **não** deve fazer isso (ver "O que não fazer").
- Combinação: `max(tempo GOG, tempo contado pelo Cerberus)`, como `account_times` faz para Epic/Xbox.

**Conquistas**

- `GET https://gameplay.gog.com/clients/<client_id>/users/<user_id>/achievements` com
  `Authorization: Bearer <access>` e `X-Gog-Lc: pt-BR` (Heroic). Resposta `items[]`:
  `achievement_id`, `achievement_key`, `visible`, `name`, `description`, `image_url_unlocked`,
  `image_url_locked`, `rarity` (porcentagem), `rarity_level_slug`, `rarity_level_description`,
  `date_unlocked` (texto ou `null`). Também `total_count`/`limit` (**não verificado**).
- O `<client_id>` aqui é o do **jogo** na Galaxy (o `clientId` do `.info`, ou do manifesto do build em
  `https://content-system.gog.com/products/<id>/os/windows/builds?generation=2` → `items[0].link` → manifesto
  com `clientId`). O Heroic passa o **productId** nesse lugar e mostra conquistas, então a rota parece aceitar
  os dois (**não verificado**). Plano: tentar `clientId` quando conhecido, senão productId; lembrar qual
  funcionou por jogo.
- O Galaxy usa um token por jogo (renova com o client do jogo e `without_new_session=1`, ideia do gogdl).
  O Heroic usa o token principal. **Não verificado** se o token principal sempre basta; começar com ele.
- Mapear para `cerberus_shared::Achievement` como em `achievements/epic.rs::read`: id = `achievement_key`,
  oculta = `!visible`, ícones, raridade, data em segundos Unix.
- Gentileza: só pedir a lista de um jogo quando ele é novo no banco ou o tempo dele cresceu
  (mesma regra da Epic); 250 ms entre pedidos; `429` → `db.refused(scope)` e mensagem "a GOG pediu uma pausa".
  Jogo sem conquistas é lembrado como tal.

**Limites de uso**: a GOG não publica limites (nenhuma fonte encontrou números). Tratar `429` e `5xx` com recuo,
cache de 1 h na biblioteca, conquistas só por jogo jogado.

---

## Amazon Games (Prime Gaming)

**Sem login** (Playnite `AmazonGamesLibrary.cs`):

- Banco: `%LOCALAPPDATA%\Amazon Games\Data\Games\Sql\GameInstallInfo.sqlite`,
  `SELECT Id, InstallDirectory, ProductTitle FROM DbSet WHERE Installed = 1;` (descartar se a pasta não existe).
- Cliente: `Amazon Games.exe`; pasta pelo registro de desinstalação "Amazon Games" (**não verificado** o nome
  exato da chave).
- Abrir: `amazon-games://play/<Id>`. Instalar: `amazon-games://install/<Id>`.
- O Playnite também abre direto pelo `fuel.json` da pasta do jogo (`Main.Command`, `Main.Args`,
  `Main.WorkingSubdirOverride`) quando o jogo não exige o cliente (**não verificado** o nome dos campos).
- Capas: o banco não tem. SteamGridDB pelo nome (o `art_cache` atual).

**Com login** (Playnite `AmazonAccountClient.cs`, MIT; o Heroic usa o `nile`, GPL):

- É um **registro de dispositivo** da Amazon com PKCE: página `https://www.amazon.com/ap/signin?...` com
  `openid.oa2.client_id=device:<hex>`, `device_type=A2UMVHOX7UP4V7`; depois
  `POST https://api.amazon.com/auth/register` (`User-Agent: AGSLauncher/1.0.0`), renovação
  `POST https://api.amazon.com/auth/token` (`app_name: AGSLauncher`, `source_token_type: refresh_token`).
- Biblioteca: `POST https://gaming.amazon.com/api/distribution/entitlements` com
  `X-Amz-Target: com.amazon.animusdistributionservice.entitlement.AnimusEntitlementsService.GetEntitlements`,
  `x-amzn-token`, `Content-Encoding: amz-1.0`, corpo `{keyId, hardwareHash, nextToken}`; descartar
  `productLine == "Twitch:FuelEntitlement"`.
- Sem conquistas. Sem tempo de jogo conhecido.
- Avaliação: viável, mas o login cria um "dispositivo" na conta Amazon do jogador (a mesma das compras) e o
  formato muda de tempos em tempos. **Deixar para o fim**, atrás de aviso claro.

## Ubisoft Connect

**Sem login**:

- Instalados: `HKLM\SOFTWARE\Ubisoft\Launcher\Installs\<gameId>` (vista 32 bits primeiro, depois 64), valor
  `InstallDir` (com `/`; trocar por `\`). Descartar pasta inexistente (Playnite `UplayLibrary.cs`).
- Nome: o Playnite tira do cache `%LOCALAPPDATA%\Ubisoft Game Launcher\cache\configuration\configurations`
  (protobuf com YAML dentro: `root.name`, `root.thumb_image`, `root.start_game`, `root.addons`,
  `root.third_party_platform`, `root.is_ulc`). Para não depender de protobuf: nome pela chave de desinstalação
  `Uplay Install <gameId>` → `DisplayName` (**não verificado**), senão o nome da pasta.
- O cache `configurations` também lista **jogos da conta não instalados** sem login no Cerberus (o jogador já
  entrou no Ubisoft Connect). Etapa opcional com um leitor protobuf mínimo; descartar itens com
  `third_party_platform` (comprados na Steam/Epic), `is_ulc` (DLC), sem `start_game`.
- Configuração do cliente: `%LOCALAPPDATA%\Ubisoft Game Launcher\settings.yml` (`misc.game_installation_path`:
  só a pasta padrão de instalação, não a lista de jogos) (**não verificado**).
- Abrir: `uplay://launch/<gameId>` (Playnite; muitos atalhos usam `uplay://launch/<id>/0`, **não verificado**).
  Instalar: `uplay://install/<gameId>`. Desinstalar: `uplay://uninstall/<gameId>`.

**Com login**: a API da Ubisoft (`public-ubiservice.ubi.com/v3/profiles/sessions`) entra com **e-mail e
senha** em Basic Auth. **Não fazer.** Conquistas da Ubisoft: fora.

## EA app

**Sem login**:

- O arquivo de instalados do EA app (`C:\ProgramData\EA Desktop\530c1147…\IS`) é **cifrado** (AES-256-CBC com
  chave derivada do hardware, segundo o GameFinder). **Não decifrar** (frágil e é contornar proteção do app).
- Caminho limpo: cada jogo EA tem `__Installer\installerdata.xml` na pasta; ele aponta uma chave em
  `HKLM\SOFTWARE\<...>` com o valor `Install Dir`, e traz `contentIDs` (GameFinder wiki; **não verificado**
  o formato). Achar as pastas: chaves de desinstalação com `Publisher` "Electronic Arts" (**não verificado**) e as
  pastas de biblioteca já varridas (`C:\Program Files\EA Games`, a escolhida pelo jogador).
- Abrir: `origin2://game/launch?offerIds=<contentId>` ou `link2ea://launchgame/<contentId>` (ambos usados pela
  comunidade; **não verificado** com o EA app atual). Se nenhum funcionar, abrir o `.exe` direto (o EA app
  sobe sozinho para a checagem de licença).
- Avaliação: a mais frágil das lojas. Fazer depois de Ubisoft, como "detecção de melhor esforço".

**Com login**: só existem APIs internas não documentadas (`accounts.ea.com`, GraphQL do EA app)
(**não verificado**). Não planejar agora.

## Battle.net

**Sem login** (Playnite `BattleNetLibrary.cs`, `BattleNet.cs`, `BattleNetGameController.cs`):

- Instalados: chaves de desinstalação cujo `UninstallString` casa `Battle\.net.*--uid=(.*?)\s`; descartar
  `DisplayName` terminando em "Test" ou "Beta". O `uid` (ex.: `wow`, `s2`, `diablo3`) mapeia para o código de
  abrir (ex.: `WoW`, `S2`, `D3`) por uma **tabela pequena escrita por nós** (o Playnite tem `BattleNetGames.cs`;
  são fatos, mas escrever a nossa, com os jogos atuais).
- Plano B: `C:\ProgramData\Battle.net\Agent\product.db` (protobuf). Evitar na primeira versão.
- Cliente: chave de desinstalação com `-uid=battle.net` → pasta → `Battle.net.exe`.
- Abrir: `"<Battle.net.exe>" --exec="launch <código>"` (Playnite). `battlenet://<código>` também é usado
  (**não verificado**). Instalar: abrir o Battle.net na página do jogo (sem URI confirmada).
- Clássicos (Diablo II, Warcraft III antigos): `Publisher == "Blizzard Entertainment"`, abrir pelo exe.

**Com login**: a API oficial da Blizzard (OAuth em `develop.battle.net`) dá perfis de WoW/Diablo/SC, **não** a
biblioteca da conta; o Playnite lê a biblioteca por uma página logada (cookies). Não planejar.

---

## Desenho no Cerberus

### `Source` novas (`shared/src/lib.rs`)

```rust
/// A GOG game, by its product id. Installed when `install_dir` is set; otherwise owned, to install through Galaxy.
Gog { product_id: String },
/// Amazon Games, by its entitlement/product id.
Amazon { id: String },
/// Ubisoft Connect, by its launcher game id.
Ubisoft { id: String },
/// EA app, by the content id read from `__Installer\installerdata.xml` (empty when unknown: open the exe).
Ea { content_id: String, #[serde(default)] exe: String },
/// Battle.net, by its launch code ("WoW", "S2").
BattleNet { code: String },
```

- Ids de jogo: `gog:<productId>`, `amazon:<id>`, `ubisoft:<id>`, `ea:<contentId ou pasta>`, `bnet:<code>`.
  Favoritos, coleções, notas e `store-playtime.json` usam o id: ele precisa ser estável.
- `AchievementSource::Gog` (código `"gog"`), com o par em `code()`/`from_code()` e o nome "GOG" em
  `ui/src/achievements.rs`.
- Adicionar uma `Source` quebra todo `match` exaustivo; lista do que tocar: `tiles.rs` (`source_name`, marcas),
  `library_page.rs` (`Filter`), `main.rs` (`play`, uninstall, `ui/src/main.rs` mensagem de instalar),
  `store_time::is_store`, `game.rs` (página na loja), `first_run.rs` (contagem), `achievements::account_times`.
  Fazer **cada loja num PR** com essa lista.

### Módulos (`src-tauri/src/`)

| Arquivo | Conteúdo |
|---|---|
| `gog.rs` | `installed_games()` (registro GOG + desinstalação + `.info`), `galaxy_path()`, `launch(game)`, `install_url(id)`, `galaxy_db()` → `GalaxyData { owned: Vec<Owned>, times: HashMap<id,(secs,last)> }` só leitura |
| `gog_account.rs` | `login_url`, `code_in(url)`, `token(Grant)`, `user_name`, `library`, `Owned`, cache `gog-library.json` (`cached/stale/keep/forget`), `merge` |
| `achievements/gog.rs` | `game_list(http, access, user_id, client_or_product)`, `read()` e testes com JSON gravado |
| `amazon.rs`, `ubisoft.rs`, `ea.rs`, `battlenet.rs` | só `installed_games()` + `launch_url`/`install_url` |
| `uninstall_keys.rs` (novo, pequeno) | lê as chaves de desinstalação (HKLM 32/64, HKCU) uma vez: `Vec<Entry{key, name, publisher, location, uninstall}>`; usado por GOG, Ubisoft, EA, Battle.net |

Tudo que lê disco roda dentro do `spawn_blocking` que já existe em `list_games` (ao lado de
`steam::installed_games()`).

### Cache

- `gog-library.json` (`{at, games: [Owned{product_id, title, cover, hero, logo}]}`), fresco por 1 h, como a Epic.
- `cache/gog/meta.json`: id → título/capas/`game_type` já resolvidos (não perguntar de novo à GOG).
- `cache/gog/galaxy-2.0.db` (cópia temporária quando o original está preso).
- Conquistas e tempo da conta: no `achievements.db` existente (`Scope { Gog, user }`), tabela `played` com chave
  = productId.

### Merge e dedupe (`list_games`)

1. Instalados (registro) entram no `scan`.
2. `gog_library(&app, &http, &mut games)` (cópia de `epic_library`): junta o cache; o que não está instalado
   vira `NextStep::Install`; capas do catálogo preenchem só o que falta.
3. **Sem login, com Galaxy**: o `galaxy-2.0.db` faz o papel da conta (biblioteca + tempo). Uma configuração
   "Mostrar jogos GOG não instalados" (padrão ligado quando o Galaxy existe). Com login, a conta vence; o banco
   só completa o tempo.
4. Dedupe dentro da GOG: pelo productId (registro, desinstalação, `.info` e banco apontam o mesmo id).
5. Dedupe entre lojas: estender `gamepass::drop_owned_elsewhere` para uma regra geral: **um jogo não instalado
   some quando o mesmo nome (normalizado) está instalado em outra loja**. Dois instalados em lojas diferentes
   ficam os dois (o jogador escolhe).
6. Tempo: `achievements::account_times` ganha o ramo `Source::Gog` (max com o contado aqui); o banco do Galaxy
   entra como mais uma fonte no mesmo `max`.

### Selos (logos)

- SVGs monocromáticos do **Simple Icons** (`simple-icons`, licença **CC0 1.0** para os desenhos), conferido que
  existem: `gogdotcom.svg`, `amazongames.svg`, `ubisoft.svg`, `ea.svg`, `battledotnet.svg` (também
  `itchdotio.svg`). Baixar de uma versão fixa, por exemplo
  `https://cdn.jsdelivr.net/npm/simple-icons@13.21.0/icons/gogdotcom.svg`, e salvar em `ui/brand/stores/`
  como `gog.svg`, `amazon.svg`, `ubisoft.svg`, `ea.svg`, `battlenet.svg`.
- `ui/brand/stores/LICENSE.md`: acrescentar a origem (Simple Icons, CC0) e a frase de marca registrada que já
  existe ("trademarks of …; shown only to say where a game comes from").
- `ui/styles.css`: `.store-logo.gog { --logo: url("brand/stores/gog.svg"); }` etc.
- `tiles.rs`: `Source::Gog { .. } => one(Mark::Store("gog"), "GOG")`, e os outros.

---

## UI

- **Biblioteca**: filtros novos só aparecem quando há pelo menos um jogo daquela loja (para não lotar a barra);
  selo no canto da capa; jogo não instalado com o botão "Instalar pelo GOG Galaxy" (ou "Baixar na GOG" quando
  não há Galaxy).
- **Página do jogo**: "Abrir no GOG Galaxy" (`goggalaxy://openGameView/<id>`), "Abrir a pasta", e quando houver
  Galaxy, a opção "Abrir sem o GOG Galaxy" (o tempo e as conquistas GOG só contam pelo Galaxy: dizer isso).
- **Configurações › Contas › GOG** (novo `Section::Gog`, mesmo cartão da Epic):
  - sem conta: "Os jogos instalados vêm do que a GOG já guarda no PC, sem conta. Com o GOG Galaxy, a biblioteca e o
    tempo de jogo dele também. Entre para trazer as conquistas e os jogos da conta neste PC." + botão
    "Entrar na GOG";
  - com conta: "Conectada como <nome>", "Sair" (esquece token, cache e as conquistas do hub, como
    `epic_logout`).
- **Configurações › Lojas** (ou junto de "Pastas de jogos"): uma linha por loja detectada (Amazon, Ubisoft, EA,
  Battle.net) com "Encontrados: N jogos" e um interruptor "Mostrar na Biblioteca". Sem contas para essas.
- **Conquistas**: plataforma "GOG" no hub com o mesmo visual de Epic/Xbox.
- Primeiro uso (`first_run.rs`): contar GOG junto de Steam/Epic.
- Textos novos em pt-BR e en (`tr!`/`tk!`).

---

## Plano em etapas

Cada etapa é um PR pequeno, com testes unitários (sem rede, JSON/registro gravados como texto) e conferência no
app com captura de tela (regra do projeto: comparar com o Playnite/Hydra).

**G1. GOG instalados (registro + desinstalação + `.info`)** — P
- `Source::Gog`, `gog.rs::installed_games`, `uninstall_keys.rs`, selo, filtro, `play` pelo Galaxy ou direto.
- Testes: parser do `.info` (primário, DLC por `rootGameId`, sem `playTasks`); regex `^(\d+)_is1$` + `GOGPACK`;
  dedupe dos três lugares pelo id; caminho relativo da `playTask`.
- Aceite: num PC com Galaxy e um jogo grátis da GOG instalado, o jogo aparece com selo GOG e capa, abre pelo
  Galaxy; um jogo do instalador offline (sem Galaxy) aparece e abre direto; o tempo conta (`store_time`).

**G2. Banco do Galaxy (sem login)** — P/M
- `gog::galaxy_db` só leitura (com cópia quando preso); biblioteca `gog_*` não instalada + `GameTimes`.
- Testes: um `galaxy-2.0.db` mínimo criado no teste (tabelas acima), tabela ausente → vazio, só `gog_` entra,
  minutos → segundos.
- Aceite: com Galaxy logado, os jogos da conta aparecem como "Instalar" e o tempo bate com o do Galaxy; com o
  Galaxy aberto, nada trava; sem Galaxy, nada muda.

**G3. Capas e nomes pela GOG** — P
- `api.gog.com/products` / gamesdb, cache `meta.json`, só ids novos, 250 ms.
- Aceite: capa vertical oficial nos jogos GOG; segunda abertura não faz pedido.

**G4. Login GOG** — M
- `gog_login` (janela privada, código por navegação), `gog_account::token`, `gog_user*` em settings, cartão,
  `gog_logout`, `gog_library` em segundo plano com cache de 1 h.
- Testes: `code_in` aceita só `https://embed.gog.com/on_login_success?...code=…`; código inválido recusado;
  `login_url` codificado; `merge` (instalado ganha capa; não instalado entra com `NextStep::Install`).
- Aceite: entrar, ver "Conectada como", fechar e abrir o Cerberus (renova sem pedir login), "Sair" limpa tudo.

**G5. Conquistas GOG** — M
- `AchievementSource::Gog`, `achievements/gog.rs`, `sync_gog` em `sync_accounts` e `hub`, regra "só jogado desde".
- Testes: `read()` com JSON gravado (desbloqueada, bloqueada, oculta, `date_unlocked: null`), recusa 429.
- Aceite: num jogo GOG com conquistas jogado pelo Galaxy, o hub mostra as mesmas que o Galaxy, com data e raridade.
  Anotar no PR se `clientId` ou productId funcionou (fecha o "não verificado").

**G6. Tempo pela conta sem Galaxy** — P (só se o endpoint por bearer funcionar; senão, registrar e encerrar)

**A1. Amazon instalados** — P: `GameInstallInfo.sqlite`, `amazon-games://play|install`. Teste com sqlite criado no teste.

**U1. Ubisoft instalados** — P: registro `Installs`, nome pela desinstalação, `uplay://launch|install`.

**B1. Battle.net instalados** — P: desinstalação `--uid=`, tabela uid→código, `--exec="launch X"`.

**E1. EA instalados (melhor esforço)** — P/M: `installerdata.xml` + `Install Dir`, `origin2://`/exe.

**U2 (opcional). Ubisoft não instalados pelo cache `configurations`** — M (leitor protobuf mínimo).

**A2 (opcional, por último). Conta Amazon** — M/G, com aviso de que cria um dispositivo na conta.

Generalização depois de G1 e A1/U1: o dedupe entre lojas (item 5 do merge) num PR próprio, com testes.

---

## Riscos

- **APIs não oficiais**: tudo da conta GOG (auth, galaxy-library, gameplay) é interno do Galaxy, documentado só
  pela comunidade. Pode mudar ou fechar sem aviso. Mitigação: cache, erro discreto no `trail`, nada quebra a
  Biblioteca; os instalados continuam pelo registro.
- **Client id/secret de terceiros**: os valores são do próprio GOG Galaxy e publicados há anos (gogapidocs,
  gogdl, lgogdownloader); usar faz o Cerberus se apresentar como o Galaxy. A GOG tolera Heroic, Playnite e
  lgogdownloader, mas pode revogar. Os termos de uso da GOG não foram lidos para isto (**não verificado**).
  Mesma situação do client da Legendary já usado na Epic. Documentar no código e no cartão ("o Cerberus usa o
  login do GOG Galaxy").
- **Licença**: Heroic, gogdl, nile e GameFinder são GPL-3.0: só ideias, nada de copiar código nem estrutura
  linha a linha. Playnite Extensions é MIT: pode portar com crédito no cabeçalho do módulo (como `epic.rs` dá
  crédito ao Hydra). Constantes de protocolo (URLs, ids) são fatos, não código.
- **`galaxy-2.0.db`**: esquema interno, muda; arquivo preso pelo Galaxy; ler só leitura e tolerar faltas.
- **Token em texto puro** no `settings.json` (igual Epic/Xbox hoje). Melhorar depois para o Gerenciador de
  Credenciais do Windows, para todas as contas de uma vez (fora deste plano).
- **Conta Amazon**: login sensível (mesma conta das compras); registra um dispositivo.
- **EA**: detecção frágil; o arquivo bom é cifrado.
- **Nomes e logos**: marcas registradas; usar só como selo de origem, monocromático, como hoje.
- **Galaxy SDK**: conquistas e tempo GOG só contam com o Galaxy aberto; abrir direto pode "perder" conquistas
  do lado da GOG. Explicar na opção.
- **Nada testado num cliente real** nesta pesquisa: cada etapa começa conferindo as chaves/arquivos num PC
  com a loja instalada e corrigindo este documento.

## O que NÃO fazer

- Nada de pirataria: não baixar instaladores fora da conta do jogador, não usar "extras"/chaves de outros, não
  contornar DRM. Não decifrar o `IS` do EA app.
- Não guardar senha nenhuma. Ubisoft (Basic Auth com senha) e qualquer loja que só entre por senha: não fazer.
  Só tokens, e só o refresh token em disco.
- Não copiar código GPL (Heroic, gogdl, nile, GameFinder).
- Não escrever no `galaxy-2.0.db`, no registro das lojas nem nos arquivos delas.
- Não enviar sessões/tempo para a GOG (`POST .../sessions`) nem desbloquear conquistas por API (existem projetos
  que fazem; é trapaça e risco para a conta).
- Não baixar/instalar jogos pelo Cerberus (gogdl-like) nesta onda: a loja oficial instala.
- Não perguntar à GOG a lista de conquistas de todos os jogos a cada abertura: só os jogados desde a última vez.
- Não usar protobuf (Ubisoft/Battle.net) na primeira versão; registro primeiro.
- Não puxar a conta Battle.net ou EA por páginas logadas com cookies.

---

## Ordem recomendada

1. **GOG** (G1 → G2 → G3 → G4 → G5, G6 se der): maior ganho, melhor documentada, DRM-free.
2. **Ubisoft Connect instalados** (U1): registro simples e URIs confirmadas pelo Playnite.
3. **Battle.net instalados** (B1): registro + tabela de códigos.
4. **Amazon Games instalados** (A1): um SQLite simples; conta (A2) só no fim.
5. **EA app instalados** (E1): melhor esforço, por último entre os instalados.
