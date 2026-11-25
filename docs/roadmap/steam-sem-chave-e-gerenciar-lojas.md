# Steam sem chave, biblioteca da família e gerenciar jogos das lojas

Plano para os itens 3 e 4 do Mapa rápido do `ROADMAP.md`, com detalhes nas seções 6.1, 6.2 e 7.2. Pesquisa feita em
25/11/2026 sobre o código da branch `ajustes-rapidos` (commit `a9f76c2`), o checkout local do Hydra e fontes da web.
O que não pôde ser confirmado leva a marca **(não verificado)** e tem um teste próprio no plano.

---

## 1. Objetivo

**A. Steam sem chave.** Quem joga entra na própria loja da Steam numa janela do Cerberus, como já faz com Epic e Xbox,
e pronto: a Biblioteca passa a mostrar todos os jogos da conta, inclusive os não instalados, com tempo de jogo, e o hub
recebe as conquistas. Não é preciso criar uma chave da Web API nem informar o SteamID. A chave continua existindo como
alternativa (modo avançado). Junto vem a **biblioteca da família Steam**: os jogos que outros membros do grupo familiar
compartilham entram na Biblioteca marcados como "da família", com o tempo de jogo de quem joga.

**B. Gerenciar jogos das lojas sem sair do Cerberus.** Na página do jogo, a aba Gerenciar passa a oferecer, para jogos
da Steam, da Epic e do Xbox, o que cada loja permite: desinstalar, verificar arquivos, mover de disco, opções de
inicialização, tamanho instalado e disco. A ação é sempre feita **pela própria loja** (link ou janela dela). O Cerberus
nunca apaga nem move arquivos de loja sozinho. Ele percebe quando a loja terminou e atualiza a Biblioteca.

---

## 2. Pesquisa

### 2.1 Como o Cerberus está hoje

| Parte | Onde | Situação |
| --- | --- | --- |
| Conta Steam | `ui/src/settings_page.rs` (`SteamAccountCard`, ~l. 1975) | O jogador cola a chave de 32 caracteres hexadecimais e, se quiser, o perfil. O SteamID vem de `loginusers.vdf` quando é omitido. |
| Comandos | `src-tauri/src/main.rs` `steam_connect` / `steam_disconnect` / `steam_account` (~l. 1032–1090) | Guardam `steam_key` e `steam_id` em `settings.json`. |
| Chamadas | `src-tauri/src/achievements/steam_api.rs` | Todas montam `key=…` na query. São elas: `ResolveVanityURL`, `GetPlayerSummaries`, `GetOwnedGames`, `GetTopAchievementsForGames`, `GetPlayerAchievements` e `GetSchemaForGame`. |
| Não instalados | `main.rs` `owned_not_installed` (~l. 1998) | Lê `cache/achievements/steam-api/owned-<id>.json` e pede de novo em segundo plano quando o arquivo passa de 1 h. |
| Logins com janela | `main.rs` `epic_login` (~l. 1705) e `xbox_login` (~l. 1834) | Janela `incognito(true)`, `on_navigation` só aceita `https` e intercepta o endereço de volta (`cerberus.invalid`), e `on_page_load` + `window.eval` carregam o código até esse endereço. |
| Capacidades | `src-tauri/capabilities/default.json` | Só `main`, `overlay` e `unlock` recebem permissões. Uma janela com outro rótulo não tem IPC. |
| Jogar | `main.rs` `play` (~l. 2486) | Steam usa `steam://install/<id>` ou `steam://rungameid/<id>`. Epic abre o launcher com `?action=install` ou `?action=launch&silent=true`. Xbox usa `msxbox://game/?productId=` ou `xbox::launch`. |
| Opções de inicialização | `main.rs` `set_launch_args` → `library.update` | Só jogos da `LocalLibrary` (adicionados e recomps). Os jogos de loja são listados na hora e não estão nela. |
| Gerenciar | `ui/src/game.rs` `Manage` (~l. 1237) | Oferece abrir a pasta, mover (só recomps), atalho e adicionar à Steam. Não há nada para lojas. |
| Disco | `src-tauri/src/disk.rs` | `fs2::available_space` já existe. |

**Achado lateral (corrigir antes de tudo):** `src-tauri/src/data_backup.rs:33` define
`SECRETS = ["steam_key", "ra_key", "ra_token", "steamgriddb_key"]`. **`epic_refresh` e `xbox_refresh` não estão na
lista.** O backup grava `settings.json` sem os `SECRETS`, então hoje a cópia leva os refresh tokens da Epic e da conta
Microsoft. Isso contraria a regra "a chave fica só neste PC". Também não estão em `THIS_PC`, o que faz uma
restauração trocar a conta deste PC pela do backup.

**Risco de vazamento a observar:** em `src-tauri/src/net.rs`, `Failed::Network(reqwest::Error)` mostra o erro do
reqwest, e esse erro **inclui a URL**. Com `access_token=` na query, qualquer `tr!("…{}", e)` ou `trail::note` com esse
erro grava o token em disco. Hoje `steam_api::get` troca o erro por uma frase fixa, mas o caminho novo precisa garantir
isso (`e.without_url()` ou mensagem fixa).

### 2.2 Como o Hydra faz (MIT, pode ser portado)

Arquivos em `C:\Users\roger\Projects\OpenSource\Games\hydra\src\main\services\steam-integration\` (no GitHub,
`hydralauncher/hydra` em `6b7ad7bf2`):

- **`steam-store-session.ts`**: há uma **partição persistente** `persist:steam` só para a Steam, com `sandbox: true`,
  `nodeIntegration: false` e `contextIsolation: true`, e o user agent do Chrome (`STEAM_CHROME_USER_AGENT`). O login
  acontece numa janela modal dessa partição (no Hydra ele passa pelo OpenID do backend deles, que o Cerberus não tem;
  o que importa é que os cookies da Steam ficam na partição). A cada sincronização, `getSteamWebApiToken` abre uma
  **janela oculta** em `https://store.steampowered.com/explore/` e roda:
  ```js
  const el = document.getElementById("application_config");
  return { userInfoRaw: el.getAttribute("data-userinfo"),
           storeConfigRaw: el.getAttribute("data-store_user_config") };
  ```
  A janela é destruída logo depois. **O token não é gravado**: ele é lido de novo a cada sincronização.
  `refreshSteamCommunityLogin` abre `https://steamcommunity.com/my/` oculto antes de copiar cookies. O comentário do
  código diz que o `steamLoginSecure` dura cerca de um dia e que carregar uma página da Community deixa a própria Steam
  renová-lo por `login.steampowered.com/jwt/refresh`.
- **`steam-store-session-config.ts`** (`parseSteamStoreSessionConfig`): falha com `SteamSessionRequiredError` se a URL
  contém `/login`, se faltar um dos atributos, se o JSON for inválido, se `userInfo.logged_in !== true`, se `steamid`
  não tiver 17 dígitos ou se `webapi_token` estiver vazio. Devolve `{ steamId64, accessToken }`. Também traz um
  detector correto da página de login: `login.steampowered.com`, ou caminho `/login` ou `/login/…`. Perfis vaidosos
  como `/id/loginking/` não podem disparar.
- **`steam-web-api.ts`**: `api.steampowered.com/<path>?access_token=…&steamid=…&format=json`, e parâmetros complexos vão
  em `input_json`. Endpoints usados com o token:
  - `IPlayerService/GetOwnedGames/v1/` (`include_appinfo`, `include_played_free_games`, `appids_filter` via `input_json`)
  - `IPlayerService/GetGameAchievements/v1/` (esquema com `appid` e `language`, sem `steamid`)
  - `IPlayerService/ClientGetLastPlayedTimes/v1/` (tempo e última vez de cada jogo, sem `steamid`)
  - `IFamilyGroupsService/GetFamilyGroupForUser/v1/` (sem `steamid`)
  - `IFamilyGroupsService/GetSharedLibraryApps/v1/` (`family_groupid`, `include_own=0`, `include_excluded=0`,
    `include_free=0`, `include_non_games=0`, `language`)
  - `IFamilyGroupsService/GetPlaytimeSummary/v1/` (`family_groupid`, repetido em `input_json`)
- **As conquistas do jogador** o Hydra **não** pede à Web API com o token. Ele lê as páginas da Community com os
  cookies da sessão (`steam-community-achievements.ts`: `/profiles/<id>/stats/<app>/?xml=1` e `/achievements`). Isso
  sugere que `ISteamUserStats/GetPlayerAchievements` não aceita o token, ou não resolve o perfil privado **(não
  verificado)**.
- **`steam-family-library.ts`**: `family_groupid` vem de `response.family_groupid`, e `is_not_member_of_any_group` ou
  `"0"` significam que não há família. Um app compartilhado só entra com `exclude_reason == 0` e `app_type == 1`
  (jogo). `rt_playtime` é tratado como **minutos** e `rt_last_played` como Unix em segundos. `GetPlaytimeSummary`
  traz `entries` e `entries_by_owner` com `steamid`, `appid`, `seconds_played`, `first_played` e `latest_played`, e só
  as entradas do `steamid` conectado contam. Na fusão, os jogos próprios vêm primeiro e a família só acrescenta app
  ids que faltam. Entre dois tempos, fica o maior.
- **`steam-source-retry.ts`**: 502 tem 3 tentativas (500 ms ×2). 429 e 420 têm 5 tentativas (1 s ×2, teto de 30 s),
  respeitando `Retry-After`. 403 na biblioteca é fatal.
- **`steam-app-action.ts`** + **`steam-install-watcher.ts`**: `steam://install/<id>` e `steam://uninstall/<id>` pelo
  shell. Depois o Hydra observa as pastas `steamapps` com `fs.watch` e confere 3 s e 30 s após cada mudança de
  `appmanifest_*.acf`. Também observa `libraryfolders.vdf` para enxergar bibliotecas novas.
- **`steam-installation-core.ts`** (`buildSteamGameLaunchUrl`): sem argumentos usa `steam://rungameid/<id>`. Com
  argumentos usa `steam://run/<id>//<encodeURIComponent(args)>/`, e quando há `%command%` só conta o que vem depois
  dele. Considera instalado quando `StateFlags & 4` está ligado e a pasta existe.

### 2.3 O token da loja

- **Onde está.** No HTML de qualquer página da loja com sessão ativa, `<div id="application_config"
  data-store_user_config='{"webapi_token":"…",…}' data-userinfo='{"logged_in":true,"steamid":"7656…",…}'>`.
  Confirmado pelo código do Hydra, que está em produção. Uma alternativa citada é
  `store.steampowered.com/pointssummary/ajaxgetasyncconfig`, que devolveria `webapi_token` em JSON
  ([Heronite #192](https://github.com/RafaelGoulartB/Heronite/issues/192)). A fonte é uma resposta de IA colada numa
  issue, então fica **(não verificado)**.
- **O que é.** Um access token de sessão web, o mesmo tipo que as páginas da loja e da Community usam no lugar da chave
  ([xpaw: "Access tokens are used by Steam's own web pages (Store and Community) for authenticated users instead of Web
  API keys"](https://steamapi.xpaw.me/)). É um JWT **(formato não verificado neste token específico)**: o campo `exp`
  do payload diz quando vence e `sub` traz o SteamID64
  ([Doctor McKay: "Decode the JWT and check the exp parameter"](https://dev.doctormckay.com/topic/4607-access-tokens-and-refreshtoken-lifetime)).
- **Quanto dura.** Os relatos falam em cerca de 24 h para o access token web
  ([SteamKit #1125](https://github.com/SteamRE/SteamKit/issues/1125); o fórum do McKay fala em "about 23–24 hours").
  O cookie de login "lembrar de mim" dura muito mais, e o mesmo relato cita uns 207 dias. A Valve pode invalidar o
  token antes, por exemplo quando o IP muda. **A renovação é feita pela própria página:** ao abrir uma página da Steam
  na mesma sessão, a Steam redireciona por `login.steampowered.com/jwt/refresh` e entrega um token novo (comentário
  do Hydra em `refreshSteamCommunityLogin`). Daí a regra: **nunca guardar o token, pegar outro quando precisar.**
- **Quais endpoints aceitam.** O xpaw avisa: "Not all APIs support access tokens. Some only work with API keys, and some
  only work with access tokens." A tabela abaixo junta o que se sabe:

  | Endpoint | Com `access_token` | Fonte |
  | --- | --- | --- |
  | `IPlayerService/GetOwnedGames` | Sim | Hydra |
  | `IPlayerService/GetGameAchievements` (esquema e % global) | Sim | Hydra |
  | `IPlayerService/ClientGetLastPlayedTimes` | Sim (só com token) | Hydra |
  | `IFamilyGroupsService/*` | Sim (só com token) | Hydra |
  | `IPlayerService/GetTopAchievementsForGames` | **(não verificado)** | — |
  | `ISteamUserStats/GetPlayerAchievements` | **(não verificado)**, provavelmente não | O Hydra usa a Community em vez dela |
  | `ISteamUserStats/GetSchemaForGame` | **(não verificado)** | Substituível por `IPlayerService/GetGameAchievements` |
  | `ISteamUser/GetPlayerSummaries` | **(não verificado)** | Nome e avatar também estão em `data-userinfo` |
  | `ISteamUser/ResolveVanityURL` | Desnecessário | O SteamID vem da sessão |

- **Perfil privado.** Com a chave do próprio dono, `GetOwnedGames` já ignora a privacidade (comentário em
  `steam_api::account` e a documentação de clientes:
  ["unless you are asking for your own personal details"](https://pkg.go.dev/github.com/xemkayx/steam-api/pkg/steamclient)).
  O que a privacidade esconde são as **conquistas** (`GetPlayerAchievements` responde "not public"). Com a sessão, as
  páginas da Community mostram as conquistas do próprio dono mesmo com o perfil privado, que é o caminho do Hydra.
  Então: **com a sessão, o perfil privado deixa de ser problema para as conquistas se elas forem lidas da Community
  com os cookies (como o Hydra faz)**. Se for preciso manter `GetPlayerAchievements`, isso vira **(não verificado)**.
  A sessão só lê a conta de quem entrou. Ler a biblioteca privada de outra pessoa está fora de questão.
- **Limites de uso.** Os limites "variam por método e não são documentados", e a recusa vem como 429 ou com
  `x-eresult` 25 (LimitExceeded) ou 84 (RateLimitExceeded) ([xpaw](https://steamapi.xpaw.me/)). Os termos da Web API
  falam em 100.000 chamadas por dia **por chave** ([termos](https://steamcommunity.com/dev/apiterms), **(não
  verificado para o token)**). O `net.rs` do Cerberus já faz a espera exponencial por host, e o banco de conquistas
  grava as esperas. Basta tratar também `x-eresult` 25 e 84 como "ocupado".
- **Sensível?** Sim. Quem tem o token age como a conta nas APIs web (loja, carrinho, família, amigos) até ele vencer.
  Os cookies da sessão (`steamLoginSecure`, `steamRefresh_steam`) valem mais ainda, porque renovam o token por meses.
  Por isso: o token fica **só na memória**, nunca vai para log, toast, `trail.jsonl`, `settings.json`, cache ou backup.
  Os cookies ficam no perfil próprio do WebView2 da Steam, numa pasta que o backup não copia.

### 2.4 Família Steam (`IFamilyGroupsService`)

Fontes: [xpaw IFamilyGroupsService](https://steamapi.xpaw.me/IFamilyGroupsService) e os protobufs da Valve
([`service_familygroups.proto`](https://raw.githubusercontent.com/SteamDatabase/Protobufs/master/webui/service_familygroups.proto)).

- `GetFamilyGroupForUser`: `steamid` é opcional e deve ser omitido fora da ferramenta de suporte. A resposta traz
  `family_groupid`, `is_not_member_of_any_group`, `role`, `cooldown_seconds_remaining` e, com
  `include_family_group_response`, o grupo com os membros.
- `GetSharedLibraryApps`: recebe `family_groupid` (fixed64), `include_own`, `include_excluded`, `include_free`,
  `language`, `max_apps`, `include_non_games` e `steamid`. Cada item de `apps` traz `appid`, `owner_steamids[]`,
  `name`, `sort_as`, `capsule_filename`, `img_icon_hash`, `exclude_reason`, `rt_time_acquired`, `rt_last_played`,
  `rt_playtime`, `app_type` e `content_descriptors`, e a resposta traz `owner_steamid`.
- `GetPlaytimeSummary`: recebe `family_groupid`. Devolve `entries[]` e `entries_by_owner[]` com `steamid`, `appid`,
  `first_played`, `latest_played` e `seconds_played`.
- Tempo de jogo: `rt_playtime` é o tempo **de quem pediu** em minutos, segundo o Hydra **(unidade não verificada nos
  protobufs)**. `GetPlaytimeSummary` dá segundos por membro. A regra do Hydra é ficar com o maior dos dois.
- Valor dos enums de `exclude_reason` **(não verificado)**: o Hydra só aceita `0`. Um app excluído pode ser, por
  exemplo, um jogo que o dono marcou como privado ou que não pode ser compartilhado.
- Instalar um jogo da família usa o mesmo `steam://install/<id>`. Só um membro joga por vez, e o dono tem prioridade.
  Isso é comportamento da Steam, que o Cerberus só informa.

### 2.5 URIs e ações das lojas

**Steam** ([wiki da Valve](https://developer.valvesoftware.com/wiki/Steam_browser_protocol). A página nega acesso a
robôs, então as descrições abaixo vêm das buscas, do Playnite e do Hydra. A própria wiki avisa que "some commands are
no longer functional".)

| URI | O que faz | Confiança |
| --- | --- | --- |
| `steam://install/<id>` | Abre a janela de instalação | Em uso no Cerberus, no Hydra e no Playnite |
| `steam://uninstall/<id>` | Abre a **confirmação de desinstalação** da própria Steam | Hydra e [Playnite `SteamGameController.cs`](https://raw.githubusercontent.com/JosefNemec/PlayniteExtensions/master/source/Libraries/SteamLibrary/SteamGameController.cs). A confirmação não foi conferida num PC: **(não verificado)** |
| `steam://validate/<id>` | Verifica a integridade dos arquivos | Wiki. Se mostra progresso na janela de downloads: **(não verificado)** |
| `steam://rungameid/<id>` | Abre o jogo com as opções da Steam | Em uso |
| `steam://run/<id>//<args>/` | Abre o jogo e passa `<args>` | Wiki e Hydra. A Steam mostra a cada vez a caixa **"Launch game with custom arguments"** ("If I did not request this launch… select cancel"), segundo [relatos](https://steamcommunity.com/discussions/forum/0/4625856213666517789/). Há relato de que `run` ignora as Launch Options da Steam ([fórum](https://steamcommunity.com/discussions/forum/1/3830914462336948405)) |
| `steam://launch/<id>/Dialog` | Abre o jogo respeitando as Launch Options e a escolha de modo | Playnite, rodando `steam.exe -silent "steam://launch/<id>/Dialog"` |
| `steam://backup/<id>` | Assistente de backup antigo | Wiki. Não oferecer |
| `steam://nav/games/details/<id>` | Página do jogo na Biblioteca da Steam, onde ficam Propriedades › Arquivos instalados › **Mover pasta** e as Opções de inicialização | **(não verificado)** |
| Mover de disco | **Não há URI conhecida.** Na Steam fica em Propriedades › Arquivos instalados › Mover pasta de instalação, ou em Configurações › Armazenamento | — |

Opções de inicialização "permanentes" da Steam ficam em `userdata/<conta>/config/localconfig.vdf`, no caminho
`UserLocalConfigStore › Software › Valve › Steam › apps › <id> › LaunchOptions`. Escrever ali **exige a Steam
fechada**, porque a Steam ignora e sobrescreve o arquivo. A sincronização em nuvem da Steam pode desfazer a mudança, e há
relatos de arquivo corrompido ([gist](https://gist.github.com/Aetopia/4f1700e58f76b2a3704148aabc721136),
[fórum](https://steamcommunity.com/discussions/forum/0/1747893292776876733)). **Ler** o valor é seguro.

Tamanho e disco: o `appmanifest_<id>.acf` tem `SizeOnDisk` em bytes, que o Hydra não usa e o Cerberus ainda não lê
**(não verificado neste PC; conferir num manifest real)**, e `StateFlags` (bit 4 = instalado). A pasta da biblioteca
vem de `libraryfolders.vdf`, e o espaço livre de `disk::available`.

**Epic** ([Playnite `EpicLauncher.cs`](https://raw.githubusercontent.com/JosefNemec/PlayniteExtensions/master/source/Libraries/EpicLibrary/EpicLauncher.cs),
[`EpicGameController.cs`](https://raw.githubusercontent.com/JosefNemec/PlayniteExtensions/master/source/Libraries/EpicLibrary/EpicGameController.cs))

| URI | O que faz | Confiança |
| --- | --- | --- |
| `com.epicgames.launcher://apps/<ns>%3A<item>%3A<app>?action=launch&silent=true` | Abre o jogo | Em uso. É o mesmo do Playnite |
| `com.epicgames.launcher://apps/<…>?action=install` | Instala | Em uso no Cerberus. O Playnite **comentou** essa linha e passou a abrir a biblioteca, sem dizer por quê |
| `com.epicgames.launcher://store/library` | Abre a Biblioteca do launcher | O Playnite usa isso para **desinstalar** |
| `?action=uninstall`, `?action=verify`, mover | **Nada documentado.** Nenhuma fonte encontrada | **(não verificado, provavelmente não existe)** |

Desinstalar, verificar e mover na Epic ficam no menu "…" do jogo dentro do launcher, e as opções de inicialização em
Gerenciar › "Argumentos adicionais de linha de comando". Onde o launcher guarda esses argumentos **(não verificado)**.
Abrir o `.exe` do jogo direto com argumentos quebra a autenticação ou o DRM de muitos jogos da Epic, então não fazer.
Tamanho: o manifest `.item` em `EpicGamesLauncher\Data\Manifests` tem `InstallSize` **(não verificado; o Cerberus já
lê esses arquivos em `epic.rs`)**.

**Xbox / Microsoft Store** ([Launch Windows Settings, Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings))

| URI | O que faz | Confiança |
| --- | --- | --- |
| `msxbox://game/?productId=<id>` | Página do jogo no app Xbox, com instalar e o menu Gerenciar | Em uso para instalar. O menu Gerenciar com desinstalar, verificar e reparar e mover **(não verificado)** |
| `ms-settings:appsfeatures-app?<PackageFamilyName>` | Opções avançadas do app nas Configurações do Windows: "Reset, manage add-on & downloadable content", e em geral também Reparar, Encerrar e Desinstalar | Documentado. Que funcione com jogos GDK em `XboxGames` **(não verificado)** |
| `ms-settings:appsfeatures` | Lista de apps instalados | Documentado |

Argumentos de inicialização em jogos MSIX/GDK: o Windows abre pelo AUMID sem argumentos do usuário. **Não oferecer.**
Tamanho: somar a pasta `install_dir` em segundo plano, com cache.

### 2.6 Como saber que a loja terminou

- O **Playnite** não tem callback e faz polling da lista de instalados: Steam a cada 5 s, Epic a cada 2 s para
  desinstalar e 10 s para instalar, **sem limite de tempo**, até o id sumir ou aparecer.
- O **Hydra** usa `fs.watch` nas pastas `steamapps` com nova conferência 3 s e 30 s depois.
- Para o Cerberus, o mais simples é **polling com prazo** só enquanto há uma ação pendente: a cada 3 s nos primeiros
  2 min e a cada 10 s até 30 min. Ao acabar o prazo, desiste em silêncio, porque o jogador pode ter cancelado a caixa
  da loja. Quando a condição é atendida, emite `catalog-changed`. Não é preciso crate novo de watcher.

### 2.7 Tauri (2.12.7, versão do `Cargo.lock`)

- `WebviewWindowBuilder` tem `data_directory`, `incognito`, `visible`, `on_navigation`, `on_page_load`, `on_new_window`
  e `initialization_script` ([docs.rs](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html)).
  `new` "deadlocks when used in a synchronous command" no Windows. O Cerberus já usa comandos `async` por isso.
- `WebviewWindow::eval_with_callback`: "the evaluation result will be serialized into a JSON string and passed to the
  callback". As exceções são ignoradas no Windows, então o script precisa ter `try/catch` próprio e devolver string.
  `cookies_for_url` / `cookies`: deadlock em comando síncrono no Windows, e houve relato de travamento
  ([PR #12665](https://github.com/tauri-apps/tauri/pull/12665)).
  `clear_all_browsing_data` existe ([docs.rs](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)).
- IPC: uma janela só fala com o app se estiver numa capability. Páginas remotas ainda precisam de `remote.urls`. Com
  rótulos fora de `default.json`, a janela da Steam **não tem nenhum comando**. Isso já vale para `epic-login` e
  `xbox-login`.
- `data_directory` diferente da janela principal cria um ambiente WebView2 separado (cookies isolados). Que duas pastas
  de dados convivam no mesmo processo sem erro de "ambiente já criado com outras opções" **(não verificado no Cerberus;
  primeiro teste do plano)**.

---

## 3. Desenho

### 3.1 Visão geral

```
Configurações › Steam ──steam_login──▶ janela "steam-login" (perfil WebView2 próprio, sem IPC)
                                          │ on_page_load(Finished, host da loja, fora de /login)
                                          ▼
                                   eval_with_callback(lê #application_config)
                                          │ JSON {steamid, logged_in, token}
                                          ▼
                          steam_session::Token (memória: steamid, token, vence_em)
                                          │
          steam_api::Auth::Token ◀────────┘     Auth::Key (chave, como hoje)
                 │
      GetOwnedGames, ClientGetLastPlayedTimes, GetGameAchievements, família…
```

Precisando de token (o anterior venceu ou falta menos de 10 min), `steam_session::token()` abre uma janela **oculta**
`steam-session` no mesmo perfil, em `https://store.steampowered.com/explore/`, lê o token do mesmo jeito e fecha. Isso
é o que o Hydra faz, e a própria Steam renova o cookie no caminho. Se cair na página de login, a sessão acabou:
`Err(SessaoExpirada)`, um aviso em Configurações e nenhuma janela aparece sozinha.

### 3.2 Módulos

- **`src-tauri/src/steam_session.rs`** (novo):
  - `pub fn login_url() -> Url`, que aponta para `https://store.steampowered.com/login/?redir=explore%2F` (o parâmetro
    `redir` é **(não verificado)**; se falhar, abre `/login/` e espera a navegação para fora de `/login`).
  - `pub fn is_login_page(url: &Url) -> bool`, port do `isSteamCommunityLoginPageUrl` do Hydra.
  - `pub fn allowed(url: &Url) -> bool`, que exige `https` e host em `store.steampowered.com`,
    `login.steampowered.com`, `steamcommunity.com`, `help.steampowered.com` ou `checkout.steampowered.com`. O resto é
    negado na janela. As imagens e os scripts do CDN não passam por `on_navigation`, que só trata navegação de frame
    **(não verificado para iframes de captcha; testar)**.
  - `const READ_CONFIG: &str`, o script que devolve só o necessário:
    ```js
    (function(){try{var e=document.getElementById('application_config');if(!e)return '';
      var u=JSON.parse(e.getAttribute('data-userinfo')||'{}'),s=JSON.parse(e.getAttribute('data-store_user_config')||'{}');
      return JSON.stringify({in:u.logged_in===true,id:String(u.steamid||''),name:String(u.persona_name||''),t:String(s.webapi_token||'')});
    }catch(x){return '';}})()
    ```
    O nome do campo `persona_name` em `data-userinfo` é **(não verificado)**. Se não existir, o nome vem de
    `GetPlayerSummaries` ou de `loginusers.vdf`.
  - `pub fn parse(json: &str, url: &Url) -> Result<Token, Falha>`, uma função pura com testes. Recusa página de login,
    `in != true`, `id` sem 17 dígitos, token vazio, token fora de `[A-Za-z0-9._-]` ou com mais de 4 KB. Calcula
    `vence_em` decodificando o `exp` do JWT. Se não der para decodificar, usa agora + 1 h.
  - `pub struct Token { steamid: String, segredo: Secret }`, onde `Secret` é um newtype cujo `Debug` e `Display`
    imprimem `"***"`. Não é `Serialize`.
  - `pub struct Sessao(Mutex<Option<Token>>)` no `State`. `pub async fn token(app) -> Result<Token, Falha>` usa um
    `AtomicBool` "lendo", para evitar duas janelas ocultas ao mesmo tempo, e espera no máximo 20 s.
  - `pub async fn sair(app)`: limpa a memória, abre o perfil oculto e chama `clear_all_browsing_data` (ou apaga
    `webview/steam/` com a janela fechada), e então `settings.steam_session = false`.
- **`src-tauri/src/achievements/steam_api.rs`**: troca `key: &str` por `auth: &Auth`:
  ```rust
  pub enum Auth { Key(String), Token(steam_session::Token) }
  impl Auth { fn query(&self) -> String; fn steamid(&self) -> &str; fn kind(&self) -> AuthKind }
  ```
  - Para cada endpoint vale a tabela da 2.3. Com o token, `GetSchemaForGame` dá lugar a
    `IPlayerService/GetGameAchievements` e `GetPlayerAchievements` dá lugar às páginas da Community com cookies (etapa
    A5). Até lá, com o token e conquistas privadas, mostra-se o mesmo aviso de hoje.
  - Em `read_reply`: com o token, `401` ou `403` em JSON quer dizer "sessão vencida". Isso invalida o token na memória e
    tenta **uma** vez com um token novo. `x-eresult` 25 ou 84 viram `BUSY`.
  - Erro de rede: só mensagem fixa (`e.without_url()` se for preciso logar).
- **`src-tauri/src/achievements/steam_family.rs`** (novo; port de `steam-family-library.ts`): `group_id`,
  `shared_apps` e `playtime_summary`, com as funções `parse_*` puras. Cache em
  `cache/achievements/steam-api/family-<steamid>.json` com `{ group, apps: [{app, name, owners, minutes, last}] }`,
  renovado de hora em hora como o `owned-*.json`.
- **`main.rs`**:
  - `steam_login` (async): cópia do `epic_login` com rótulo `steam-login`, `data_directory(data/webview/steam)`,
    **sem** `incognito`, `on_new_window` → `Deny` para hosts fora de `allowed` (os de dentro abrem na mesma janela) e
    `on_navigation(allowed)`. Em `on_page_load(Finished)` num host da loja fora do login, chama
    `eval_with_callback(READ_CONFIG)` → `finish_steam_login`.
  - `finish_steam_login`: `parse`, guarda o token na `Sessao`, grava `steam_id`, `steam_user` e
    `steam_session = true`, fecha a janela, faz o toast "Steam conectada como X", emite `accounts-changed` e
    `catalog-changed`, e dispara em segundo plano `owned` e a família.
  - `steam_api_account` passa a devolver `Option<Auth>`: a sessão ligada vem primeiro, senão a chave.
  - `owned_not_installed` usa `Auth` e acrescenta os jogos da família ainda não listados, com `family_owner`.
  - `steam_disconnect` → `steam_session::sair` + limpar a chave.
- **`src-tauri/src/store_manage.rs`** (novo; parte B):
  ```rust
  pub enum Acao { Desinstalar, Verificar, Mover, PaginaDaLoja, OpcoesNaLoja }
  pub fn url(source: &Source, acao: Acao) -> Option<String>   // pura, testada; None = não oferecida
  pub fn capacidades(source: &Source) -> Vec<Acao>            // o que a UI mostra
  pub fn instalado(game: &Game) -> Option<Instalado>          // tamanho, disco, livre
  pub fn vigiar(app, game_id, espera: Espera)                 // polling com prazo, emite catalog-changed
  ```
  Tabela de `url`:

  | Fonte | Desinstalar | Verificar | Mover | Página/Opções |
  | --- | --- | --- | --- | --- |
  | Steam | `steam://uninstall/<id>` | `steam://validate/<id>` | `steam://nav/games/details/<id>` + instrução | idem |
  | Epic | `com.epicgames.launcher://store/library` + instrução "… › Desinstalar" | biblioteca + "… › Gerenciar › Verificar" | biblioteca + "… › Gerenciar › Mover" | biblioteca |
  | Xbox | `ms-settings:appsfeatures-app?<family>` | mesma página (Reparar) | `msxbox://game/?productId=<p>` (Gerenciar) | `msxbox://…` |

  `Espera::Sumir` (desinstalar) volta `true` quando o jogo não está mais em `steam::installed_games()` /
  `epic::installed_games()` / `xbox::installed_games()`. `Espera::Nada` serve para verificar e mover, em que a UI só
  diz "acompanhe na Steam".
- **Opções de inicialização de jogos de loja**: um arquivo novo `store-args.json` (`{ "steam:620": "-novid" }`) num
  módulo pequeno no molde de `game_notes.rs` (`load`, `set`, `apply` em `list_games`). `set_launch_args` decide: jogo da
  `LocalLibrary` → `library.update`; `Source::Steam` → `store_args::set`; Epic e Xbox → erro "a loja não aceita
  opções por aqui". Em `play`, se `Source::Steam` e `launch_args` não estiver vazio, usa
  `steam://run/<id>//<percent-encode(args)>/` (port de `buildSteamGameLaunchUrl`, com `%command%`). Senão, segue o
  `rungameid` de hoje. Entra no backup (`FILES`) porque não é segredo.
- **`shared/src/lib.rs`**:
  - `SteamAccount` ganha `via: SteamVia { Sessao, Chave }` e `family: Option<FamilySummary { members: u8, shared: u32 }>`.
  - `Game` ganha `family_owner: Option<String>` (`#[serde(default)]`): o nome ou SteamID de quem compartilha.
  - `StoreManage { actions: Vec<StoreAction>, size: Option<u64>, drive: Option<String>, free: Option<u64>,
    launch_args_supported: bool, steam_launch_options: Option<String> }`, este último lido de `localconfig.vdf`, só
    leitura.
- **Comandos novos**: `steam_login`, `steam_session_status`, `store_manage(id) -> StoreManage` e
  `store_action(id, action)`. Os de hoje `steam_connect` e `steam_disconnect` continuam.

### 3.3 O que guardar e onde

| Dado | Onde | Backup? | Observação |
| --- | --- | --- | --- |
| Senha da Steam | **Em lugar nenhum** | — | É digitada na página da Steam. O Cerberus não injeta script na página de login |
| `webapi_token` | Só na memória (`Sessao`) | Não | Some ao fechar o app. É lido de novo quando preciso |
| Cookies da sessão | `<data>/webview/steam/` (perfil WebView2) | **Não** (o backup só copia `FILES`) | Pasta exclusiva da Steam, nunca usada pela janela principal |
| `steam_session: bool`, `steam_id`, `steam_user` | `settings.json` | `steam_id` em `THIS_PC`; `steam_user` e `steam_session` também | Não são segredos, mas são da conta deste PC |
| `steam_key` | `settings.json` (como hoje) | Fora (`SECRETS`) | Modo avançado |
| Listas (`owned-*`, `family-*`, `tops-*`, `mine-*`) | `cache/achievements/steam-api/` | Não | Sem token dentro |
| `store-args.json` | data | Sim | Opções do jogador |

### 3.4 Segurança da janela de login

1. **Sem capacidades**: o rótulo `steam-login` / `steam-session` fica fora de `default.json`, e um teste garante que
   nenhuma capability usa `"windows": ["*"]` nem `remote`.
2. **Navegação presa à Steam** (`allowed`): http, `file:`, `javascript:` e esquemas próprios são negados. Pop-ups fora
   da lista são negados, porque não há login federado na Steam que precise deles. Se o captcha precisar, entram os
   hosts dele **(testar)**.
3. **Nenhum `initialization_script`** e nenhum `eval` em `login.steampowered.com` ou em `/login`. O `eval_with_callback`
   só roda depois que a URL já está numa página da loja logada, e o script só **lê** atributos.
4. **Validação no Rust** de tudo que volta (`parse`): a página pode mudar, e o que vier fora do formato é descartado.
5. **Segredo opaco**: `Secret` sem `Debug`, `Display` nem `Serialize` legíveis. O token só sai em `Auth::query()`, que
   monta a URL, e a URL nunca vai para log nem para mensagem de erro.
6. **Perfil separado** (`data_directory`): a sessão da Steam não vaza para a janela principal (que é `tauri://`), e
   "Sair" apaga tudo.
7. **User agent**: o Hydra força o UA do Chrome porque a Steam recusa alguns UAs de Electron. O WebView2 já se
   apresenta como Edge, então não deve ser preciso **(não verificado)**.

---

## 4. UI

### 4.1 Configurações › Steam (`SteamAccountCard`)

- **Desconectado:**
  - Título "Steam", texto curto: "Entre na sua conta da Steam para ver todos os seus jogos, inclusive os não instalados
    e os da família, com tempo de jogo e conquistas de qualquer PC. O Cerberus nunca vê a sua senha."
  - Botão principal **Entrar na Steam** (`steam_login`). Se houver conta em `loginusers.vdf`: "Conta da Steam neste PC:
    Fulano" como dica.
  - Bloco recolhido **Usar chave da Web API (avançado)** com o formulário de hoje (chave + perfil).
- **Entrando:** o botão vira "Janela da Steam aberta…" e um segundo clique foca a janela, como no Epic.
- **Conectado (sessão):** avatar, nome, "Conectada pela sessão da loja" e o link "Ver perfil". Uma linha sobre a família:
  "Família Steam: N jogos compartilhados por M membros" ou "Sem família Steam". Botões **Sincronizar agora** e
  **Sair**. Sair pergunta "Sair também apaga a sessão da Steam guardada neste PC."
- **Conectado (chave):** como hoje, mais "Trocar para o login da loja" (não precisa mais de perfil público para os
  jogos).
- **Sessão vencida:** faixa amarela "A sessão da Steam expirou. Entre de novo para atualizar jogos e conquistas.",
  botão **Entrar de novo**, e a Biblioteca continua com o cache. Nada abre sozinho.
- **Erro da Steam ocupada:** a mensagem `BUSY` de hoje.
- Nas buscas da página (`Section::Steam`, `settings_page.rs:67`), acrescentar "Entrar na Steam" e "Família Steam".

### 4.2 Biblioteca

- Os jogos da família aparecem como não instalados com o selo de origem Steam e um chip "Família" (tooltip:
  "Compartilhado por Fulano"). O filtro de plataforma Steam inclui esses jogos, e o filtro "não instalados" também.
- Um jogo próprio nunca é duplicado pelo da família. O próprio vence, como na fusão do Hydra.

### 4.3 Página do jogo › Gerenciar (jogos de loja)

Uma seção **"Na Steam"** / **"Na Epic"** / **"No app Xbox"** no `Manage`, com o mesmo `row-btn` de hoje:

- **Tamanho e disco** (só leitura): "12,4 GB em D: (SteamLibrary) · 210 GB livres". Fica oculto quando é
  desconhecido.
- **Desinstalar** → "A Steam vai pedir confirmação." Depois do clique, o estado vira "Aguardando a Steam…" com
  "Cancelar acompanhamento". Quando o jogo some: toast "X foi desinstalado" e o jogo volta a "Instalar" (se for da conta)
  ou sai da lista.
- **Verificar arquivos** (Steam) → "A Steam verifica e baixa o que faltar. Acompanhe em Downloads da Steam."
- **Mover para outro disco** → abre a página do jogo na Steam, com o texto "Em Propriedades › Arquivos instalados ›
  Mover pasta de instalação". Não há botão que finja mover.
- **Abrir na loja** (Epic e Xbox) com o texto do caminho no launcher: "… › Gerenciar".
- **Opções de inicialização** (`LaunchOptions`, só Steam): o mesmo formulário de hoje com outro texto. "Passadas pela
  Steam ao abrir pelo Cerberus. A Steam pede confirmação a cada partida com opções. Para opções fixas, use as
  Propriedades do jogo na Steam." Mostra, só para leitura, as Launch Options gravadas na Steam
  (`steam_launch_options`), se houver.
- Epic e Xbox **não** mostram o campo de opções (`launch_args_supported = false`).
- Jogo não instalado: só "Instalar" (como hoje) e "Abrir na loja".
- Jogo aberto (`sessions.is_running_any`): Desinstalar e Mover ficam desligados com "Feche o jogo antes".

### 4.4 Estados a cobrir na UI

Desconectado · janela aberta · conectado por sessão · conectado por chave · sessão vencida · Steam ocupada (429) · sem
internet (usa o cache) · sem família · família sem jogos para compartilhar · ação pendente (aguardando a loja) · ação
terminou · prazo encerrado sem mudança (some sem aviso) · loja não instalada (o link não abre: "Instale ou abra a Steam").

---

## 5. Casos de borda

- **Conta da sessão diferente da conta deste PC** (`loginusers.vdf`): permitido. O `steam_id` passa a ser o da sessão.
  O tempo de jogo local (`librarycache`) e as conquistas ao vivo continuam vindo da conta local, como hoje. Avisar
  numa linha: "A Steam deste PC está em outra conta (Y)". O Hydra bloqueia com `SteamAccountMismatchError` porque tem
  conta vinculada no servidor, coisa que o Cerberus não tem.
- **Chave e sessão ao mesmo tempo**: a sessão vence. A chave fica guardada como reserva e é usada se a sessão vencer
  e o jogador não entrar de novo.
- **Token vencido no meio de uma sincronização longa** (centenas de `mine-*`): `401` invalida o token, pede um novo uma
  vez e, se falhar, para e grava a espera.
- **Duas leituras simultâneas** (Biblioteca + hub): um `AtomicBool` + `Notify`, e quem chega depois espera o mesmo
  token.
- **Janela oculta não carrega** (sem internet, Steam fora): `Err(Offline)` dentro de 20 s, a janela é destruída e o
  cache é usado.
- **Steam Guard e QR**: a página da Steam cuida disso. O fluxo do Cerberus só termina quando aparece uma página da
  loja logada.
- **Jogador fecha a janela de login**: nada muda, e o botão volta ao estado inicial (`WindowEvent::Destroyed`).
- **Família**: o jogador sai do grupo, o que esvazia o cache dos jogos da família. Um membro desativa o
  compartilhamento de um jogo (`exclude_reason != 0`), e o jogo sai. Um jogo da família também é próprio: fica o
  próprio. Família com conta infantil: as restrições de conteúdo são da Steam. Tempo de jogo da família: só o do
  jogador conectado (`steamid` igual).
- **`steam://uninstall` com a Steam fechada**: a Steam abre primeiro, o que pode demorar e cair no login, e o prazo de
  30 min cobre isso.
- **O jogador cancela a confirmação**: o prazo acaba e nada é dito.
- **Jogo em duas bibliotecas** (manifests em C: e D:): o tamanho vem do manifest válido (`StateFlags & 4`).
- **Opções de inicialização com espaços, aspas e `%command%`**: percent-encode completo e o corte em `%command%` como
  no Hydra. Linha vazia volta ao `rungameid`.
- **Caixa "custom arguments" da Steam em Big Picture e na TV**: avisar no texto e testar com controle.
- **Epic sem o launcher instalado**: o link não abre e o erro de hoje ("Não foi possível abrir o launcher da loja")
  aparece.
- **Xbox com `family` vazio** (jogo da conta não instalado): só "Abrir no app Xbox".
- **Restauração de backup em outro PC**: não traz sessão, chave nem refresh tokens. O jogador entra de novo.

---

## 6. Plano em etapas

Cada etapa é pequena, entra num commit próprio e deixa o app funcionando. "Teste" quer dizer teste de unidade em Rust
(`#[cfg(test)]`) mais a verificação manual descrita.

### Etapa 0. Segredos fora do backup (pré-requisito, P)
- `data_backup.rs`: incluir `epic_refresh` e `xbox_refresh` em `SECRETS`. Incluir `epic_user`, `epic_refresh`,
  `xbox_user` e `xbox_refresh` em `THIS_PC`, além dos novos `steam_session` e `steam_user`.
- **Aceite:** uma cópia nova não contém nenhum `*_refresh`, e restaurar não troca as contas deste PC.
- **Teste:** estender `secrets_never_leave` / os testes existentes de `data_backup` com `epic_refresh` e `xbox_refresh`.

### Etapa A0. Spike: perfil separado e leitura do token (P, descartável)
- Comando de teste que abre `steam-login` com `data_directory` próprio, faz o login e lê `application_config` por
  `eval_with_callback`. Loga **só** `token.len()`, `exp` e o `steamid`.
- Responder aqui: (1) duas pastas de dados WebView2 convivem? (2) `exp` do token; (3) `persona_name` existe?
  (4) `ajaxgetasyncconfig` funciona? (5) depois de fechar e reabrir o app, a janela oculta ainda pega o token?
  (6) e depois de 25 h? (7) quais endpoints da tabela 2.3 aceitam o token: `GetTopAchievementsForGames`,
  `GetPlayerAchievements` com perfil privado, `GetPlayerSummaries`.
- **Aceite:** respostas escritas neste documento, trocando as marcas "(não verificado)".

### Etapa A1. `Auth` no `steam_api.rs` sem mudar o comportamento (P)
- `enum Auth { Key, Token }`, e todas as funções recebem `&Auth`. Os chamadores de hoje passam `Auth::Key`.
- `Secret` com `Debug` mascarado. `read_reply` aprende `x-eresult` 25 e 84 e o "token vencido".
- **Aceite:** tudo como antes com a chave, e `cargo test` verde.
- **Teste:** `Auth::query` para os dois tipos. `format!("{:?}", token)` não contém o segredo. `read_reply` com
  401 + token devolve `SessaoVencida`.

### Etapa A2. `steam_session.rs`: parse e regras de navegação (P)
- Funções puras: `parse`, `is_login_page`, `allowed` e decodificar `exp`.
- **Teste:** os casos do `steam-store-session.test.ts` do Hydra portados: URL de login, `logged_in` falso, JSON
  quebrado, steamid inválido, token vazio, `/id/loginking/` **não** é login, host estranho negado, `http` negado.

### Etapa A3. Login na janela e token em memória (M)
- `steam_login`, `finish_steam_login`, `Sessao`, `token()` com a janela oculta e `sair()`. Settings
  `steam_session` e `steam_user`.
- UI: o botão **Entrar na Steam** e os estados conectado, sessão vencida e Sair. O formulário da chave vai para
  "avançado".
- **Aceite:** entrar com uma conta real (com Steam Guard) conecta sem chave. Fechar e abrir o Cerberus mantém a
  conexão. "Sair" apaga `webview/steam/` e a próxima entrada pede login. `rg` no data dir e nos logs não acha o token
  (procurar pelos 12 primeiros caracteres anotados no spike, depois apagar a anotação).
- **Teste:** um teste que lê `capabilities/*.json` e falha se `steam-login` ou `steam-session` aparecerem ou se houver
  `"*"`.

### Etapa A4. Biblioteca pelo token (P)
- `owned_not_installed` e `achievements::steam_playtimes` usam `Auth`. Com o token, pedir
  `ClientGetLastPlayedTimes` e somar o tempo (o maior vence).
- **Aceite:** com perfil **privado** e sem chave, a Biblioteca mostra todos os jogos da conta com tempo de jogo.
- **Teste:** fusão do tempo (`pickPreferredPlaytime` portado).

### Etapa A5. Conquistas com o token (M)
- Esquema por `IPlayerService/GetGameAchievements`. Conquistas do jogador por `GetPlayerAchievements` se o spike
  confirmar que aceita o token, senão pelas páginas da Community (`/profiles/<id>/stats/<app>/?xml=1`) com os cookies
  do perfil. Ler os cookies com `cookies_for_url` só em comando async, ou carregar a página na janela oculta e ler o
  XML por `eval_with_callback`. A segunda opção não tira cookies do WebView.
- **Aceite:** o hub mostra as conquistas de um jogo jogado em outro PC com o perfil privado. O banco
  `achievements.db` continua sem perder desbloqueios.
- **Teste:** parse do XML de estatísticas (fixture), e "sessão vencida" grava a espera.

### Etapa A6. Família Steam (M)
- `steam_family.rs` com parse e cache. Fusão em `owned_not_installed` com `family_owner`. Chip "Família" na grade.
  Linha da família em Configurações.
- **Aceite:** numa conta com família, os jogos compartilhados aparecem como não instalados, "Instalar" abre a Steam e,
  depois de instalado, o jogo vira instalado como qualquer outro. Sem família, nada muda.
- **Teste:** os casos de `steam-family-library.test.ts`: `is_not_member_of_any_group`, `"0"`, `exclude_reason`,
  `app_type`, nome vazio, tempo só do `steamid` conectado, próprio vence a família.

### Etapa B1. `store_manage.rs`: URLs e capacidades (P)
- `url(source, acao)` e `capacidades(source)`, puras.
- **Teste:** uma tabela por loja (os textos exatos de cada URI). `Local` e `Recomp` devolvem `None`.

### Etapa B2. Tamanho e disco (P)
- Ler `SizeOnDisk` do appmanifest (estender `steam::read_manifest`) e `InstallSize` do `.item` da Epic. Para o Xbox,
  somar a pasta em `spawn_blocking` com cache de 24 h. Disco e espaço livre por `disk::available`.
- **Aceite:** os números batem com Propriedades da Steam e da Epic (±1%).
- **Teste:** manifest de exemplo com `SizeOnDisk`, e ausência devolve `None`.

### Etapa B3. Desinstalar e verificar com acompanhamento (M)
- `store_action` abre a URL pelo `opener` e chama `vigiar`. A UI mostra "Aguardando a Steam…".
- **Aceite:** desinstalar um jogo pequeno pela Steam faz o card voltar a "Instalar" em até 10 s sem reabrir a
  página. Cancelar a confirmação não deixa o card preso: há "Cancelar acompanhamento" e o prazo de 30 min.
- **Teste:** `vigiar` com uma função de leitura injetada (sumiu na 3ª leitura → emite uma vez e para; nunca some →
  para no prazo).

### Etapa B4. Opções de inicialização da Steam (P)
- `store-args.json`, `set_launch_args` para `Source::Steam`, e `play` usando `steam://run/<id>//<args>/`. Leitura
  somente das Launch Options de `localconfig.vdf`.
- **Aceite:** `-novid` num jogo Source chega ao jogo (a Steam mostra a confirmação). Sem opções, o comportamento é o
  de hoje.
- **Teste:** `launch_url` com espaços, aspas, `%command%` e vazio (casos do Hydra).

### Etapa B5. Mover e abrir na loja (P)
- Botões que abrem a página ou a biblioteca da loja com o texto do caminho. Conferir `steam://nav/games/details/<id>`;
  se não funcionar, usar `steam://open/games` + o nome do jogo no texto.
- **Aceite:** cada botão abre o lugar certo nas três lojas.

### Etapa B6. Xbox (P)
- `ms-settings:appsfeatures-app?<family>` para desinstalar e reparar, e `msxbox://game/?productId=` para gerenciar.
- **Aceite:** num jogo do Game Pass instalado, a página do Windows certa abre. Se não abrir para jogos GDK, cair em
  `ms-settings:appsfeatures`.

Ordem sugerida: 0 → A0 → A1 → A2 → A3 → A4 → B1 → B2 → B3 → B4 → A6 → A5 → B5 → B6. A família e as conquistas pela
Community ficam para depois do caminho básico porque dependem das respostas do spike.

---

## 7. Riscos

| Risco | Probabilidade | Mitigação |
| --- | --- | --- |
| A Valve muda `application_config` ou o nome `webapi_token` | Média | `parse` isolado e testado, mensagem "a Steam mudou o login; use a chave" e a chave continua funcionando |
| Endpoint deixa de aceitar o token | Média | `Auth` por endpoint, com volta para a chave quando existir |
| Duas pastas WebView2 no mesmo processo dão erro | Baixa/média | Spike A0. Plano B: a janela de login sem `data_directory`, mas com rótulo próprio, o que mistura os cookies com o perfil padrão do app, aceitável porque a janela principal é local |
| Janela oculta periódica pesa ou aparece | Baixa | Só quando o token falta ou vence (≈ 1 vez por dia), timeout de 20 s, `visible(false)` |
| Token ou cookie vaza em log | Média sem cuidado | `Secret` mascarado, erro do reqwest sem URL, teste de `Debug`, busca por `access_token` em `trail.jsonl` na revisão |
| Caixa "custom arguments" incomoda | Alta | Texto claro e indicação das Propriedades da Steam para opções fixas. Não escrever `localconfig.vdf` |
| `steam://uninstall` ou `validate` mudam de comportamento | Baixa | São links da própria Steam, e o pior caso é nada acontecer: o prazo acaba |
| Termos da Steam sobre automação da sessão | Baixa | Só leitura da conta de quem entrou e na frequência do app. Nada de compras, trocas ou ações sociais |
| Acompanhamento preso | Baixa | Prazo fixo + cancelamento + só um acompanhamento por jogo |

---

## 8. O que NÃO fazer

- **Não** pedir, digitar, ler ou guardar a senha da Steam. Nada de `initialization_script` ou `eval` nas páginas de
  login.
- **Não** gravar o `webapi_token` em `settings.json`, cache, `trail.jsonl`, toast, backup ou log, nem em `{:?}` de erro.
- **Não** dar capability à janela `steam-login` / `steam-session` e não usar `"windows": ["*"]`.
- **Não** usar `incognito` na janela da Steam (perderia a renovação). Também não reaproveitar o perfil da janela
  principal se o `data_directory` próprio funcionar.
- **Não** apagar, mover ou verificar arquivos de jogos de loja por conta própria (`remove_dir_all`, cópia entre
  discos). Tudo passa pela loja.
- **Não** escrever `localconfig.vdf` nem arquivos de configuração da Epic.
- **Não** abrir o `.exe` de jogo da Epic ou do Xbox direto para passar argumentos.
- **Não** desinstalar apps do Xbox por `PackageManager` ou `Remove-AppxPackage`.
- **Não** usar a sessão para ler a conta de outra pessoa nem para nada além de leitura (sem carrinho, lista de desejos,
  amigos ou mensagens).
- **Não** copiar código do RecompLauncher (GPL) nem do Legendary (GPL). Do Hydra e do Playnite (MIT) é permitido
  portar com crédito no comentário.
- **Não** abrir a janela de login sozinho quando a sessão vence. Só avisar.
