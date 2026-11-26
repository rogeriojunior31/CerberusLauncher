# Saves de qualquer jogo de PC

Plano de implementação do item 1 do Mapa rápido do `ROADMAP.md` (também 5.4, 6.8 e 7.1). Pesquisa de 25/11/2026.
Escrito para quem vai implementar: cada etapa fecha com teste, não só com compilação.

**Decisão principal:** baixar sob demanda o manifesto oficial do Ludusavi, da fonte original e com ETag, e ler esse
arquivo com código próprio em Rust. Ele não entra no repositório nem no instalador. O programa Ludusavi não é
usado: o backup, o rodízio e a nuvem por pasta continuam sendo os de `recomp/saves.rs` e `recomp/cloud.rs`,
generalizados. A seção 2 explica por quê.

---

## 1. Objetivo e escopo

### Objetivo

Para jogos de PC instalados pela Steam, Epic, app Xbox/Game Pass, GOG ou adicionados à mão, o Cerberus deve:

1. **Achar** onde o jogo guarda os saves, pelos locais conhecidos do manifesto e por pastas que o jogador escolher.
2. **Mostrar** esses locais antes de copiar: caminho, número de arquivos, tamanho e de onde veio a informação.
3. **Fazer backup** sozinho depois de jogar e sempre que o jogador pedir, com rodízio dos 10 mais recentes, como os
   recomps.
4. **Restaurar** qualquer backup, guardando antes uma cópia dos saves atuais.
5. **Sincronizar entre PCs** pela mesma pasta do OneDrive, Google Drive ou Dropbox que os recomps já usam, sem conta e
   sem servidor. Quando os dois PCs mudaram os saves, o jogador escolhe quais ficam.

### Dentro do escopo

- Windows primeiro. O Cerberus hoje só lê lojas no Windows.
- Lojas que o Cerberus já lê: Steam (`steam.rs`), Epic (`epic.rs`), Xbox (`xbox.rs`) e jogos locais (`Source::Local`).
- GOG entra quando a etapa 7.2 trouxer `Source::Gog`. O desenho já prevê `<storeUserId>` e o id do GOG no manifesto.
- Saves em arquivo. Saves no registro do Windows ficam numa etapa à parte (E11).

### Fora do escopo agora

- Linux, Proton e Wine. O manifesto tem as regras, mas o Cerberus ainda não roda lá (5.6).
- Mexer na nuvem da Steam, da Epic, do GOG ou da Xbox. O Cerberus copia, não sincroniza com elas.
- Converter saves entre lojas, como Game Pass para Steam. Ferramentas como GPSaveConverter fazem isso, mas é frágil e
  fica fora.
- Conta própria e servidor (5.6, 6.9).

---

## 2. Fontes de dados e licenças

### 2.1 Ludusavi Manifest (a base de locais)

| Item | O que se sabe | Fonte |
|---|---|---|
| Repositório | `mtkennerly/ludusavi-manifest` | https://github.com/mtkennerly/ludusavi-manifest |
| Licença do repositório | **MIT**, "Copyright (c) 2020 Matthew T. Kennerly". O GitHub informa `spdx_id: MIT`. | https://github.com/mtkennerly/ludusavi-manifest/blob/master/LICENSE |
| Origem dos dados | "compilado a partir de dados da **PCGamingWiki**", mais a API da Steam para o nome da pasta de instalação | README do repositório |
| Licença da PCGamingWiki | **CC BY-NC-SA 3.0**, segundo a Wikipedia e seus espelhos. A página `PCGamingWiki:Copyrights` recusou o acesso (403). **Não verificado na fonte primária.** | https://en.wikipedia.org/wiki/PCGamingWiki |
| Arquivo | `data/manifest.yaml` no ramo **`data`**. O README avisa que a cópia em `master` vai parar de ser atualizada. | https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/refs/heads/data/data/manifest.yaml |
| Tamanho (25/11/2026) | 17.817.430 bytes em YAML, ou **2.420.079 bytes com gzip** (`Accept-Encoding: gzip`). Cerca de 53 mil entradas no topo. | `curl -I` medido |
| Atualização | O README pede **ETag**: enviar `If-None-Match`, tratar 304 como "sem novidade" e 200 como versão nova. A frequência não é informada. | README |
| Esquema | `data/schema.yaml` (geral) e `data/schema.strict.yaml` (com enums) | mesmo repositório |

**A licença dos dados é ambígua.** O arquivo é MIT, mas deriva de texto CC BY-NC-SA. O repositório não diz se os
termos NC e SA passam para o manifesto (**não verificado**; nenhuma declaração foi encontrada). Por isso:

- **Não embutir.** O manifesto não entra no git, no instalador nem em artefato de release do Cerberus. Assim o
  Cerberus não o redistribui.
- **Baixar sob demanda**, no PC do jogador e da fonte original, como o próprio Ludusavi faz. O índice derivado fica só
  no cache local.
- **Dar crédito sempre:** "Locais de saves: Ludusavi Manifest (MIT), a partir da PCGamingWiki (CC BY-NC-SA)", em Sobre
  e na prévia de saves.
- O Cerberus é gratuito e MIT. Mesmo sob NC-SA, o uso local e não comercial com crédito parece permitido (**não
  verificado juridicamente**). Se o Cerberus um dia tiver algo pago, revisar este ponto antes.

### 2.2 Formato do manifesto (o que o código precisa ler)

Exemplo real, encurtado, do ramo `data`:

```yaml
Balatro:
  cloud:
    steam: true
  files:
    "<winAppData>/Balatro":
      tags: [save]
      when: [{ os: windows, store: steam }]
    "<winAppData>/Balatro/settings.jkr":
      tags: [config]
      when: [{ os: windows }]
    "<winLocalAppData>/Packages/PlayStack.Balatro_3wcqaesafpzfy/SystemAppData/wgs":
      tags: [save]
      when: [{ os: windows, store: microsoft }]
  steam:
    id: 2379780
Grounded:
  installDir: { Grounded: {} }
  id: { steamExtra: [2161390], lutris: grounded }
  steam: { id: 962130 }
```

Campos do esquema: `files`, `registry` (chaves `HKEY_CURRENT_USER/...`), `installDir`, `launch`, `steam.id`, `gog.id`,
`id` (`flatpak`, `gogExtra`, `lutris`, `steamExtra`), `cloud` (`epic`, `gog`, `origin`, `steam`, `uplay`), `alias`
(apelido: os outros campos são ignorados) e `notes[].message`. Enums do esquema estrito: os `dos|linux|mac|windows`,
store `discord|epic|gog|microsoft|origin|steam|uplay`, tag `config|save`.

Regras do README:

- Primeiro trocar as variáveis, depois avaliar o resultado como **glob**. O manifesto usa `*` e `**`, como em
  `<winLocalAppData>/XV83/Saved/SaveGames/**/*.sav`.
- Um caminho que aponta para uma pasta inclui tudo dentro dela, recursivamente.
- Ignorar caminhos relativos (`./`, `../`) e limitar `alias` recursivo.
- Registro: a chave inclui as subchaves.

Contagem no arquivo de hoje: `<base>` 78.130 usos, `<home>` 13.648, `<winLocalAppData>` 6.861, `<winDocuments>` 4.434,
`<storeUserId>` 4.035, `<winAppData>` 3.607, `<root>` 1.559, `<winProgramData>` 149, `<winDir>` 53, `<winPublic>` 44 e
`<osUserName>` 11. Há 391 caminhos com `SystemAppData/wgs`, 2.859 jogos com `registry`, 1.133 `alias` e 14.688 jogos com
`cloud.steam: true`. `<winLocalAppDataLow>` não aparece: o manifesto escreve `<home>/AppData/LocalLow`.

### 2.3 Variáveis de caminho no Windows, por loja

| Variável | Resolver como | Observação |
|---|---|---|
| `<home>` | `%USERPROFILE%` (Known Folder `Profile`) | |
| `<winAppData>` | Known Folder `RoamingAppData` (`%APPDATA%`) | |
| `<winLocalAppData>` | Known Folder `LocalAppData` | |
| `<winDocuments>` | `standalone::documents()` (`SHGetKnownFolderPath(FOLDERID_Documents)`) | Já acerta Documentos dentro do OneDrive |
| `<winPublic>` | Known Folder `Public` | |
| `<winProgramData>` | Known Folder `ProgramData` | Escrever ali pode exigir administrador |
| `<winDir>` | Known Folder `Windows` | Só leitura na prática. Restaurar ali: recusar. |
| `<osUserName>` | `%USERNAME%` | |
| `<xdgData>`, `<xdgConfig>` | Não resolver no Windows: regra ignorada | |
| `<root>` | Steam: a pasta da Steam com `userdata` (`steam::root()`). Demais lojas: a biblioteca que contém o jogo. | |
| `<game>` | `installDir` do manifesto, senão o nome da entrada | Também serve para procurar pastas |
| `<base>` | Pasta de instalação real do jogo (`Game.install_dir`) | Steam: `steamapps/common/<pasta>` vem do `appmanifest`, então não precisa de `<root>/<game>` |
| `<storeGameId>` | Steam: app id. GOG: id do produto. | Pouco usado |
| `<storeUserId>` | **Steam:** pasta `userdata/<conta>`, com conta = SteamID64 − 76561197960265728. O Cerberus já lê `loginusers.vdf` em `achievements/steam_api.rs`. **Epic:** `account_id` da sessão (`epic_account.rs`), quando logado. **GOG:** id do usuário do Galaxy (**não verificado** onde fica). **Outros:** `*` | Com várias contas no PC: usar a do último login e listar as outras na prévia. O Hydra troca por `*` sempre. |
| `%APPDATA%`, `%LOCALAPPDATA%` | Iguais às variáveis acima | O Hydra aceita os dois |
| qualquer outra `<...>` ou `%X%` | Regra **ignorada**, e a prévia diz isso | Nunca copiar um caminho meio resolvido |

Condições `when`: uma regra vale se não tiver `when` ou se algum item combinar (`os` ausente ou `windows`, e `store`
ausente ou igual à loja do jogo). Correspondência de loja: Steam → `steam`, Epic → `epic`, Xbox/Game Pass →
`microsoft`, GOG → `gog`, Local → aceitar só regras **sem** `store`.

### 2.4 Hydra (MIT, pode adaptar com crédito)

Lido em `C:\Users\roger\Projects\OpenSource\Games\hydra`:

- `src/main/services/ludusavi.ts` chamava o **binário do Ludusavi** (`backup <id> --api --force [--preview]`) com um
  `config.yaml` próprio. Esse config desligava o manifesto principal e usava um **secundário do Hydra**
  (`https://cdn.losbroxas.org/manifest.yaml`, 23,9 MB, com chaves pelos ids do Hydra). O binário vem do
  `scripts/postinstall.cjs` (Ludusavi 0.29.0) e é empacotado no app.
- O caminho novo é nativo em Rust: `native/hydra-native/src/cloud_save/`, com cerca de 13 mil linhas.
  - `manifest/indexer.rs` transforma o YAML (`serde_yaml_ng`) num índice JSON com `files`, `tags` e `when`.
  - `manifest/cache.rs` guarda o índice por **24 h**, grava de forma atômica e usa timeout de 30 s.
  - `manifest/lookup.rs` procura pelo id, depois pelo id remoto e depois pelo título **normalizado** (NFKD, sem acentos,
    só letras e números, minúsculas).
  - `path_resolution/tokens.rs` traz a tabela de variáveis da seção 2.3, a detecção de variável não resolvida e o escape
    de `{` e `}` para o glob.
  - `path_resolution/context.rs` usa Known Folders e acha a raiz da Steam por `/steamapps/common/`.
  - `local_snapshot/guardrails.rs` limita a **500 arquivos e 2 GiB** por cópia.
- Em `src/main/services/cloud-save/`, as ideias de sincronização são:
  - **Âncora** (`sync-anchor.ts`): o hash da última versão sincronizada.
  - **Estado** local à frente, nuvem à frente ou conflito (`sync-game/policy.ts`). Antes de abrir o jogo só restaura;
    depois de fechar só envia.
  - **Trava de abertura** (`launch-guard.ts`): em conflito, o jogo não abre até o jogador escolher, e o envio depois da
    partida só acontece se a checagem antes dela foi segura.
  - **Pasta personalizada** do jogador com aprovação, reassociação e detecção de sobreposição (`custom-path-*.ts`).
- **O que aproveitar:** índice, busca, tabela de variáveis, limites, âncora e trava de abertura, com crédito "adaptado do
  Hydra (MIT)" no topo de cada arquivo.
- **O que não copiar:** a URL do CDN do Hydra, porque é de terceiro, tem termos desconhecidos e chaves que não são as
  nossas. Também não copiar o envio por servidor.

### 2.5 Heroic (GPL-3, só ideias)

Lido em `HeroicGamesLauncher/src/backend/cloud_saves/` e `save_sync.ts`:

- O local de save da Epic vem do **modelo da própria loja** (`save_folder`, com `{epicid}`, `{installdir}`,
  `{appdata}`...). Sem a conta, o modelo com `{epicid}` não é usado. Ideia: quando o Cerberus tiver esse dado da Epic,
  ele vale como segunda fonte (**não verificado** se o `.item` local traz o modelo).
- O GOG traz os locais da configuração remota do jogo.
- Backup local: um manifesto por cópia com `size`, `mtime` e hash de cada arquivo. O hash é **reaproveitado** quando
  tamanho e mtime não mudaram. O rodízio é por **tamanho total** (1 GiB por jogo, configurável).
- Na restauração, os arquivos mais novos que não estão no backup são apagados. Aqui fica como opção, não como padrão
  (seção 4).

### 2.6 Ludusavi, o programa (alternativa avaliada)

- **Licença MIT** (GitHub `spdx_id: MIT`). Versão mais recente: **v0.31.0** (04/04/2026). `ludusavi-v0.31.0-win64.zip`
  tem 13,8 MB.
- Na linha de comando, `--api` devolve JSON (`errors.someGamesFailed`, `overall`, `games[nome].decision/files/registry`).
  O comando `api` aceita JSON na entrada. Se o JSON vier vazio, a documentação manda ler o stderr.
- Ele resolve o registro, o rodízio completo e diferencial e a nuvem via rclone.

**Por que não usar o programa:**

- Ele faria um segundo backup, com formato e rodízio próprios, e mais 14 MB a baixar.
- Seria um segundo arquivo de configuração para manter em sincronia, e o Cerberus já tem zip, rodízio, nuvem por pasta,
  histórico e UI testados.
- Ele identifica jogos pelo título do manifesto, não pelos ids das lojas que o Cerberus já conhece.

**Fica como plano B:** se o índice próprio der errado ou ficar caro, chamar `ludusavi backup --preview --api` só para
**descobrir** os caminhos. O backup continua sendo do Cerberus.

### 2.7 Saves do app Xbox / Game Pass

Fontes: Microsoft Learn, "XGameSaveFiles API overview" e "xgamesaveutil".

- **Onde ficam:** `%LOCALAPPDATA%\Packages\<PFN>\SystemAppData\wgs` para `XGameSave`, e `...\SystemAppData\xgs\<HexXuid>_<SCID>\`
  para `XGameSaveFiles`. No PC do usuário, a pasta `wgs` tem subpastas `<hex>_<hex>` (a primeira parte identifica a
  **conta Xbox**), cada uma com `containers.index` e pastas com nome de GUID, além de uma pasta `t`. O usuário consegue
  ler a pasta (conferido no PC, só leitura).
- **Como sincroniza:** a sincronização com a nuvem acontece **ao abrir o jogo**, com trava de um aparelho por vez.
  Depois que o jogo fecha, "o sistema tenta enviar os dados para a nuvem cerca de **10 a 30 segundos** depois".
  Conflitos aparecem num diálogo do próprio sistema.
- **Usar a ferramenta da Microsoft:** a documentação diz "não use enquanto o jogo roda; pode deixar os saves num estado
  indeterminado". O `import` "grava direto no armazenamento local, não sincroniza com a nuvem" e só envia na próxima
  abertura. O formato XML "não é de arquivo". `xgamesaveutil` faz parte do GDK e não vem com o Windows (**não
  verificado** se ela existe no PC do jogador).

**Conclusão:**

- **Backup é seguro** se for cópia de leitura da pasta `wgs` (ou `xgs`) **inteira**, feita com o jogo fechado e **mais
  de 60 s depois** do fim da partida, para não pegar o envio pela metade.
- **Restaurar é arriscado.** Pode funcionar só com o jogo fechado, no mesmo PC e na mesma conta Xbox (a subpasta
  `<xuid>_` precisa existir igual). Na próxima abertura, o app Xbox pode trocar os arquivos pela nuvem ou abrir o diálogo
  de conflito (**não verificado** qual dos dois acontece).
- Por isso, nos jogos Xbox:
  - Só backup automático.
  - Restauração manual, com aviso forte, e só para a mesma conta.
  - Nunca puxar da nuvem do Cerberus sozinho.
  - Nunca levar para outra conta.

---

## 3. Desenho

### 3.1 Visão geral

```
manifesto (ramo data, ETag) ──► pc_saves/manifest.rs ──► índice local (JSON compacto, só Windows)
                                                           │
Game (Steam/Epic/Xbox/Local) ──► pc_saves/matching.rs ─────┤ entrada do manifesto
                                                           ▼
                         pc_saves/paths.rs (variáveis + when + glob) ──► SavePlan (prévia)
                                                           │
             recomp/saves.rs generalizado (zip, impressão, rodízio, restore em etapas)
                                                           │
             recomp/cloud.rs (fila .upload, pasta "Cerberus Saves/pc/<chave>") + âncora de sincronização
```

### 3.2 Módulos novos

`src-tauri/src/pc_saves/`:

| Arquivo | Responsabilidade |
|---|---|
| `mod.rs` | API pública: `plan(game)`, `backup(game, reason)`, `restore(game, name)`, `before_play(game)`, `after_play(game)`, `list(game)`. Escolhe a chave e as pastas de backup. |
| `manifest.rs` | Baixar com `reqwest` (já é dependência), `Accept-Encoding: gzip`, `If-None-Match`, timeout de 60 s e limite de 64 MiB. Montar o índice e gravar de forma atômica. Manter o último índice válido se o novo falhar. |
| `index.rs` | Tipos do índice: `Entry { key, steam: Vec<u32>, gog: Vec<u64>, families: Vec<String>, install_dirs, files: Vec<Rule>, registry: Vec<Rule>, cloud: StoreCloud, notes }`. Busca por id, família Xbox, `installDir` e título normalizado. Resolve `alias` com no máximo 1 salto. |
| `paths.rs` | Contexto por loja (seção 2.3), troca de variáveis, filtro `when`, recusa de variável não resolvida, de caminho relativo e de raiz larga demais. Escape de `[`, `{`, `}` vindos de caminhos reais. |
| `scan.rs` | Expande globs a partir do prefixo literal, com profundidade máxima de 12, sem seguir links e com limites (seção 4). Devolve `Found { place, relative, absolute, size, mtime }`. |
| `custom.rs` | Pastas escolhidas pelo jogador e exclusões por jogo, em `pc-saves/custom.json`. O caminho escolhido é guardado como modelo portátil (`C:\Users\x\AppData\Roaming\Foo` vira `<winAppData>/Foo`). |
| `sync.rs` | Âncora e decisão antes de jogar (seção 3.6). |

Dependências novas (conferir licença na hora de adicionar):

- `serde_yaml_ng` (MIT/Apache, usada pelo Hydra) para ler o YAML.
- `globset` (MIT/Unlicense) para os padrões.
- `unicode-normalization` (MIT/Apache) para o título.
- `winreg` já existe e serve para E11.
- `known-folders`, ou o próprio `windows` crate com `SHGetKnownFolderPath`, que `standalone.rs` já usa.

### 3.3 Módulos alterados

| Arquivo | Mudança |
|---|---|
| `recomp/saves.rs` | **Generalizar sem quebrar os zips antigos.** (1) `Manifest` ganha `#[serde(default)] format: u8` (1 = modelos `{local}`, 2 = modelos do Ludusavi), `machine: String` (nome do PC, para o diálogo de conflito), `game_key` e `source` (`"manifest"`, `"custom"`). (2) Os lugares passam a ser resolvidos por um `trait Resolver` (o atual, que usa `{install}`, e o novo do `pc_saves`). (3) Glob completo (`**`, `*` no meio) no lugar de `split_pattern` só no último segmento; os arquivos ficam relativos ao prefixo literal do glob. (4) **A impressão digital passa a ser em fluxo** (`io::copy` para o `Sha256`, como `cloud::digest`), sem o `read_to_end` atual, que põe o arquivo inteiro na memória. Cache opcional por (tamanho, mtime). (5) `MAX_FILE` (64 MiB) e `NOT_SAVES` valem só para o palpite `{install}`. Nos lugares do manifesto, os limites são os da seção 4. (6) `KEEP` e `CLOUD_KEEP` viram parâmetros, com padrão 10 e 20. |
| `recomp/cloud.rs` | `retry_game` e `games()` passam a percorrer também `saves/pc/<chave>`. Destino na nuvem: `Cerberus Saves/pc/<chave>`. A fila `.upload` continua igual. |
| `recomp/mod.rs` | `pull_from_cloud` passa a usar a âncora de `pc_saves::sync` (E9). Hoje ele compara só `created` e não detecta quando **os dois** PCs mudaram. |
| `store_time.rs` | No fim de partida (`ended`, ≥ 30 s), chamar `pc_saves::after_play` numa thread, **60 s depois** que o processo sumiu (para a Steam ou o app Xbox terminarem de gravar e enviar), e só se o jogo não voltou a rodar. Anotar no `trail`. |
| `sessions.rs` | `close_session`: para `Source::Local`, chamar `pc_saves::after_play`. `start`: para `Source::Local`, chamar `pc_saves::before_play`, como já é feito com `pull_from_cloud`. |
| `main.rs` (`play`) | Antes de abrir `steam://rungameid`, o link da Epic ou `xbox::launch`, chamar `pc_saves::before_play`. Em conflito, devolver um erro que a UI transforma no diálogo de escolha (seção 5). Novos comandos: `pc_save_plan`, `pc_backup_now`, `pc_list_backups`, `pc_restore_backup`, `pc_add_save_folder`, `pc_remove_save_folder`, `pc_resolve_conflict`, `pc_saves_overview`, `pc_manifest_status`, `pc_manifest_refresh`. |
| `shared/src/lib.rs` | Tipos novos: `SavePlan { key, places: Vec<SavePlace>, store_cloud: Option<String>, registry_only: bool, notes, manifest_age }`, `SavePlace { template, resolved, files, bytes, origin, tags, problem: Option<String> }`, `SaveConflict { local: Backup, cloud: Backup, cloud_machine }`, `PcSavesOverview`. `Backup` ganha `#[serde(default)] machine: String` e `label: Option<String>` (versões com nome, 7.1). |
| `settings.rs` | `pc_saves: PcSavesSettings { enabled: bool (padrão true), include_config: bool (false), cloud_for_store_cloud_games: bool (false), max_backup_mb: u32 (1024) }`. |
| `ui/src/backups.rs` | Trocar `emulator: bool` por `kind: BackupKind { Recomp, Emulator, Pc }`. Mostrar `machine` e `label` em cada linha. |
| `ui/src/game.rs` | Aba Saves também para jogos Steam, Epic, Xbox e Local (`has_saves`). |
| `ui/src/pages.rs` | Página Saves: nova seção "Jogos de PC" (seção 5). |
| `docs/guide/saves.md` | Guia do jogador para os jogos de PC (E12). |

### 3.4 Chave do jogo e pastas

A chave precisa ser **igual em todos os PCs** e servir de nome de pasta (o `:` do `Game.id` não serve no Windows).

| Origem | Chave | Exemplo |
|---|---|---|
| Steam | `steam-<appid>` | `steam-2379780` |
| Epic | `epic-<app_name>` | `epic-Fortnite` |
| Xbox | `xbox-<família>` | `xbox-PlayStack.Balatro_3wcqaesafpzfy` |
| GOG | `gog-<id>` | `gog-1207658924` |
| Local | `local-<slug da entrada do manifesto>`, ou `local-<slug do nome>` sem entrada | `local-hollow-knight` |

- O slug usa só `[a-z0-9-]`, com no máximo 80 caracteres. Se houver colisão, acrescentar `-2`.
- O mesmo jogo em lojas diferentes tem **chaves diferentes de propósito**: o save do Game Pass não é o da Steam.
- Pastas locais: `<dados>/saves/pc/<chave>/` para os zips e `<dados>/pc-saves/` para índice, `custom.json` e
  `sync/<chave>.json`.
- Nuvem: `<nuvem>/Cerberus Saves/pc/<chave>/`. Os recomps continuam em `Cerberus Saves/<recomp_id>`, sem migração.

### 3.5 Índice local do manifesto

- Arquivo `pc-saves/manifest-index.json` com `{ version, etag, fetched_at, source_url, entries }`. Na montagem, ficam só
  regras que podem valer no Windows (`when` com os `windows` ou sem os) e entradas com pelo menos uma regra de arquivo ou
  registro. O tamanho esperado do índice é bem menor que 17,8 MB (**não verificado**, medir em E1).
- Atualização:
  - Na primeira vez que o jogador abre um save de jogo de PC, ou com o recurso ligado, em segundo plano até 1 vez por
    dia.
  - Também pelo botão "Atualizar locais" em Configurações.
  - Usa `If-None-Match`: 304 só renova `fetched_at`.
  - Respeita o modo sem internet (5.4).
- Falha de rede ou YAML inválido: continua usando o índice anterior e anota no `trail` ("Locais de saves"). Sem índice
  nenhum, a prévia diz "Ainda não baixamos a lista de locais de saves" e oferece "Baixar agora" e "Escolher pasta".
- O YAML bruto é apagado depois da montagem, porque só o índice é necessário.
- Montar o índice numa thread de baixa prioridade. Medir o tempo (**não verificado**; meta: menos de 3 s num PC comum).

### 3.6 Fluxo antes e depois de jogar

**Depois de jogar** (`after_play`), para todas as lojas:

1. Esperar o processo sair: 60 s para jogos de loja, 5 s para os locais. Se ele voltar nesse tempo, cancelar.
2. `plan(game)`. Sem lugar válido, anotar "sem local conhecido" e parar sem incomodar.
3. `saves::backup(...)` com motivo "Depois de jogar". Se nada mudou, não faz nada (a impressão já cuida disso).
4. Enviar para a nuvem pela fila `.upload`, se a nuvem estiver ligada **e** o jogo puder ir para ela (seção 3.7). Depois
   do envio, gravar a âncora = impressão enviada.
5. Anotar no `trail`. Em falha, mostrar aviso como hoje ("Falha no backup de {jogo}: ...").

**Antes de jogar** (`before_play`), só quando o jogo é aberto **pelo Cerberus** (Jogar):

1. Sem nuvem, ou com jogo fora da nuvem do Cerberus (3.7): nada.
2. Ler a cópia mais nova na nuvem (`newest_with_fingerprint`), a impressão atual (`current_fingerprint`) e a âncora
   (`sync/<chave>.json`: `{ fingerprint, backup_name, at }`).
3. Decidir:

   | Local == âncora? | Nuvem == âncora? | Ação |
   |---|---|---|
   | sim | sim | nada |
   | sim | não | **trazer da nuvem** (backup "Antes de trazer da nuvem", que fica só local), restaurar e âncora = nuvem |
   | não | sim | nada: este PC está à frente e o envio vem depois de jogar |
   | não | não, e local == nuvem | âncora = nuvem |
   | não | não | **conflito**: não abrir o jogo; mostrar o diálogo (seção 5) |
   | sem âncora (primeira vez) | | se local estiver vazio, trazer; se só a nuvem estiver vazia, nada; se os dois tiverem saves e forem diferentes, **conflito** |

4. Se o jogador escolher "abrir mesmo assim", o jogo abre e **o envio depois da partida fica suspenso** até o conflito
   ser resolvido, como a trava de abertura do Hydra. O backup local continua.

**Jogo aberto fora do Cerberus** (pela Steam, pelo menu Iniciar): o `store_time` só percebe o jogo depois que ele já
abriu, então não dá para trazer antes. Ao perceber, se a nuvem estiver à frente da âncora, avisar uma vez: "{jogo} tem
saves mais novos de {PC} na nuvem. Feche o jogo e abra pelo Cerberus para trazê-los." O backup depois da partida
**não envia** se a nuvem mudou desde a âncora. Fica só local e marcado como conflito.

### 3.7 Nuvem: o que vai e o que não vai

| Situação | Backup local | Envio para a pasta da nuvem | Trazer da nuvem antes de jogar |
|---|---|---|---|
| Jogo local ou sem nuvem da loja | sim | sim | sim |
| Steam, Epic ou GOG com `cloud.<loja>: true` no manifesto | sim | **só com a chave ligada** (`cloud_for_store_cloud_games`, desligada por padrão) | **nunca sozinho**: só pelo botão Restaurar |
| Xbox / Game Pass | sim (pasta `wgs`/`xgs` inteira) | **não** | **nunca** |
| Saves no registro (E11) | sim | sim | sim, com confirmação |

Assim o Cerberus **não briga com a Steam Cloud**. Se as duas nuvens escrevessem no mesmo arquivo, a Steam veria uma
mudança fora dela e abriria o próprio diálogo de conflito, ou trocaria os arquivos pelos dela (**não verificado** qual
dos dois). Com a loja dona da sincronização, o Cerberus só guarda versões.

### 3.8 Formato do zip (versão 2)

Continua sendo o de hoje: `<n>/<caminho relativo>` mais `cerberus-backup.json`. Campos novos com `serde(default)`, para
que versões antigas do Cerberus leiam ou ignorem:

```json
{
  "format": 2,
  "game_key": "steam-2379780",
  "places": ["<winAppData>/Balatro"],
  "origins": ["manifest"],
  "created": 1764086400,
  "reason": "Depois de jogar",
  "machine": "DESKTOP-SALA",
  "files": 3,
  "bytes": 48213,
  "fingerprint": "…",
  "manifest_etag": "\"2d35…\""
}
```

- Na restauração, cada `places[n]` é **resolvido de novo no PC atual**, como os recomps já fazem com `{local}`. Isso
  vale para outro usuário do Windows, outra pasta da Steam e Documentos dentro ou fora do OneDrive.
- `<storeUserId>` resolvido para uma conta é guardado **já trocado** no `places` (`<root>/userdata/123/2379780/remote`).
  Num PC com outra conta, a restauração para em vez de gravar num lugar errado (seção 4).
- `target_for` continua recusando `..` e caminhos absolutos. Mais uma checagem: o destino final, normalizado, precisa
  começar pelo lugar resolvido.

---

## 4. Casos de borda

| Caso | Comportamento |
|---|---|
| **Jogo aberto durante o backup** | Não fazer o backup automático com o jogo aberto: `store_time::running` diz se está. O manual com o jogo aberto pede confirmação ("O jogo está aberto; o save pode estar pela metade"). Arquivo travado (`ERROR_SHARING_VIOLATION`): tentar 3 vezes com 2 s de intervalo. Se continuar travado, **abortar a cópia inteira** (nunca zip parcial) e anotar. |
| **Jogo aberto durante a restauração** | Recusar: "Feche {jogo} antes de restaurar." |
| **Steam ou app Xbox gravando a própria nuvem no fim da partida** | Esperar 60 s depois do fim (Microsoft: envio 10 a 30 s depois). |
| **Saves enormes** | Avisar na prévia acima de 200 MB. Acima de `max_backup_mb` (1 GiB), não fazer backup automático: "Saves de {jogo} têm {n} GB; faça o backup manual se quiser". Limite de 20.000 arquivos por jogo e 4 GiB por zip (o crate `zip` 2 usa zip64, conferir). Para a nuvem, limite de 500 arquivos e 2 GiB, como o Hydra. Rodízio também por tamanho total: se 10 cópias passarem de 5 GiB, apagar as mais antigas até caber. |
| **Regra larga demais** | Recusar um lugar resolvido que seja raiz de disco, `<home>`, `<winDocuments>`, `<winAppData>`, `<winLocalAppData>`, `<winProgramData>` ou `<winDir>` **inteiros**, `<root>` inteiro, `C:\Program Files` ou a própria pasta de dados do Cerberus. A prévia mostra "Ignorado: pasta ampla demais". |
| **`<base>` com o jogo inteiro** | Regras com `<base>` sem tag `save` não entram. Com tag `save`, passam pelos limites acima. Arquivos `exe`, `dll`, `pak` e similares só entram se a regra apontar para eles diretamente. |
| **Tags** | Copiar só `save` por padrão. `config` entra com a chave `include_config`, porque uma configuração gráfica de outro PC pode atrapalhar. Regras sem tag: tratar como `save`. |
| **Exclusões** | O jogador desmarca lugares ou arquivos na prévia. Isso vai para `custom.json` como `exclude: [modelo]` e vale para backup e restauração. Ignorar sempre `*.tmp`, `*.log`, `*.dmp`, `desktop.ini`, `Thumbs.db`, `.cerberus-*.tmp` e pastas `shadercache` ou `logs`. |
| **Várias contas da Steam no PC** | Usar a do último login (`MostRecent` em `loginusers.vdf`). A prévia lista as outras ("Outras contas neste PC: 2"). Não misturar contas num mesmo zip. |
| **Outra conta da Steam ou do Xbox no PC de destino** | Se o modelo tem a conta já trocada e ela não existe aqui, avisar: "Este backup é de outra conta da Steam (…123). Restaurar para a conta atual?" Para Xbox, **recusar**. |
| **Steam Cloud ou Epic Cloud do jogo** | Ver 3.7. Ao restaurar manualmente num jogo com nuvem da loja, avisar: "A Steam também guarda esses saves. Abra o jogo pela Steam depois e, se ela perguntar, escolha os arquivos deste PC." O diálogo da Steam nesse caso é **não verificado**. |
| **Xbox / Game Pass** | Ver 2.7. Backup só da pasta inteira. Restauração manual com o jogo e o app Xbox fechados, na mesma conta, com aviso. |
| **OneDrive "arquivos sob demanda"** | Saves em Documentos dentro do OneDrive podem ser só marcadores. Ler o arquivo força o download e pode falhar sem internet. Atributo `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: contar à parte e avisar "{n} arquivos só na nuvem do OneDrive", sem forçar o download no automático (**não verificado** como cada jogo se comporta). |
| **Pasta da nuvem do Cerberus fora do ar** (disco removido, OneDrive pausado) | Já tratado: a fila `.upload` espera e `pending_count` aparece na página. |
| **Dois PCs gravando juntos na nuvem** | O nome com milissegundos e `persist_noclobber` já preservam as duas cópias. A âncora decide qual é a "mais nova" pelo conteúdo, não pelo relógio. Relógio errado num dos PCs não causa perda, só um conflito a mais. |
| **Cópia pela metade na nuvem** (sincronizador ainda baixando) | O zip é aberto e o manifesto lido antes de restaurar (`restore_open` já lê tudo antes de trocar). Se o zip estiver inválido, tratar como "ainda chegando" e tentar na próxima. |
| **Permissões** | Restaurar em `Program Files` ou `ProgramData` pode exigir administrador. Nunca pedir elevação sozinho: "Sem permissão para gravar em {pasta}. Os saves continuam no backup {nome}." Leitura sem permissão: o lugar aparece com "Sem acesso". |
| **Caminhos longos e Unicode** | Usar `\\?\` só se precisar (**não verificado** se `std::fs` já cobre). No zip, guardar o caminho relativo em UTF-8 com `/`. |
| **Jogo desinstalado** | Os backups ficam (como nos recomps). `<base>` não resolve: o lugar aparece como "jogo não instalado" e a restauração desse lugar espera a reinstalação. |
| **Entrada errada do manifesto** (título parecido) | Jogo local: a correspondência só pelo título é **sugestão**. A prévia pergunta "É este jogo?" e só faz backup automático depois do "sim". Steam pelo app id e Xbox pela família no caminho `wgs` são exatas e não pedem confirmação. |
| **Manifesto mudou entre backup e restauração** | A restauração usa os `places` guardados no zip, não o manifesto atual. |
| **Saves no registro** | Até E11, a prévia diz "Este jogo guarda o progresso no registro do Windows; o Cerberus ainda não copia isso" e mostra as chaves. |
| **Antivírus segurando o arquivo** | Mesmo tratamento de arquivo travado. Já existe "Repetir quando o Windows ou o antivírus prende um arquivo" (6.x). |
| **Restauração exata** (apagar arquivos que não estão no backup) | Padrão: **não apagar**, só sobrepor, como hoje. Opção "Restaurar exatamente como estava" move os arquivos extras para o backup "Antes de restaurar" antes de apagar. Ideia do Heroic, implementação nossa. |

---

## 5. Interface

### 5.1 Onde aparece

1. **Página do jogo, aba Saves** (`ui/src/game.rs`), agora também para Steam, Epic, Xbox e Local:
   - Cartão **"Onde ficam os saves"** (prévia): cada lugar com caminho copiável, arquivos, tamanho, origem
     ("Lista de locais (PCGamingWiki)" ou "Escolhido por você") e caixa para incluir ou excluir. Botões "Abrir pasta",
     "Adicionar pasta de save" e "Atualizar".
   - Cartão **"Backups de saves"**: o componente `Backups` atual (`kind=Pc`), com o PC de origem em cada linha e "Dar
     nome" (versão com nome, 7.1).
   - Selo de nuvem: "Nuvem da Steam cuida da sincronização; o Cerberus guarda versões" / "Na nuvem do Cerberus" / "Só
     neste PC".
2. **Página Saves** (`ui/src/pages.rs`): o cartão da nuvem fica no topo, como hoje. Depois vêm as seções **Recomps**,
   **Jogos de PC** (nova) e **Emuladores**. Cada jogo de PC é uma linha `SaveRow` com o estado ao lado. A seção tem os
   filtros "Com saves", "Sem local conhecido" e "Com conflito".
3. **Diálogo de conflito**, que abre no Jogar ou pela página Saves.
4. **Configurações → Saves**: ligar ou desligar, incluir configurações, nuvem para jogos com nuvem da loja, limite de
   tamanho, "Atualizar locais de saves" (com a data da lista) e o crédito.

### 5.2 Estados de uma linha "Jogos de PC"

| Estado | Pílula | Linha de baixo |
|---|---|---|
| Achado, com backup | `ok` "Em dia" | "Último backup: hoje, 14:02 · 3 arquivos · 48 KB" |
| Achado, sem backup ainda | `none` "Ainda sem backup" | "Um backup é feito quando você fecha o jogo." |
| Nuvem da loja | `info` "Nuvem da Steam" | "A Steam sincroniza; o Cerberus guarda versões neste PC." |
| Xbox | `info` "Só backup" | "O app Xbox sincroniza estes saves. O Cerberus guarda cópias neste PC." |
| Sem local conhecido | `warn` "Sem local conhecido" | "Escolha a pasta onde o jogo salva." + botão |
| Precisa confirmar (jogo local) | `warn` "Confirmar jogo" | "Achamos \"{entrada}\" na lista. É este jogo?" [Sim] [Não] |
| Grande demais | `warn` "Saves grandes" | "{n} GB: sem backup automático. Use Fazer backup agora." |
| Registro | `none` "No registro" | "Este jogo guarda o progresso no registro do Windows; ainda não copiamos." |
| Conflito | `bad` "Conflito" | "Mudou neste PC e em {PC} desde a última sincronização." [Resolver] |
| Esperando a nuvem | `wait` "Enviando" | "1 cópia esperando a pasta da nuvem." |

### 5.3 Textos principais (pt-BR, via `tr!`)

- Prévia vazia sem lista: "Ainda não baixamos a lista de locais de saves (cerca de 2,4 MB). [Baixar agora] ou [Escolher
  pasta]".
- Crédito: "Locais de saves: Ludusavi Manifest, a partir da PCGamingWiki (CC BY-NC-SA). Corrija ou acrescente um local na
  PCGamingWiki e ele chega aqui."
- Diálogo de conflito:
  - Título: "Saves diferentes em dois PCs"
  - Corpo: "Os saves de {jogo} mudaram neste PC ({data local}) e em {PC} ({data nuvem}) desde a última sincronização.
    Escolha quais ficam. Os outros continuam num backup, nada se perde."
  - Botões: [Usar os de {PC}] [Manter os deste PC] [Decidir depois]. "Decidir depois" abre o jogo sem enviar nada.
- Antes de restaurar um jogo Xbox: "Restaurar saves do app Xbox é arriscado: ao abrir o jogo, o app Xbox pode trocá-los
  pelos da nuvem da Microsoft. Feche o jogo e o app Xbox antes. Só funciona na mesma conta Xbox. [Restaurar mesmo assim]".
- Jogo aberto: "Feche {jogo} antes de restaurar."
- Trazido da nuvem: o texto atual de `pull_from_cloud` ("Saves mais novos de outro PC foram trazidos da nuvem; os deste
  PC ficaram num backup.").
- Lugar ignorado: "Ignorado: {motivo}" (pasta ampla demais / variável desconhecida / outra loja / outro sistema).

Seguir o padrão da UI atual: confirmação com segundo clique ("Confirmar: substituir os saves atuais"), avisos por
`toast` e erros no `report`. Conferir em 640×480 e no modo TV.

---

## 6. Plano em etapas

Cada etapa é um PR pequeno. "Teste" = `cargo test` no crate citado; "No app" = conferência real com print, como manda a
regra do projeto.

### E0. Licença e crédito (P)

- Tarefa: registrar a decisão deste documento no `ROADMAP.md` (7.1) e acrescentar o crédito em Sobre e em
  `docs/development/architecture.md`.
- Aceite: nenhum arquivo do manifesto no git (`git ls-files | grep -i manifest.yaml` vazio). O crédito aparece em Sobre.

### E1. Baixar e indexar o manifesto (M)

- Tarefa: criar `pc_saves/manifest.rs` e `index.rs`. Download com gzip, ETag, limite e gravação atômica. Índice só com
  regras de Windows. Busca por Steam id, `steamExtra`, GOG id, família Xbox (extraída de `Packages/<família>/` nas regras
  `wgs`/`xgs`), `installDir` e título normalizado. `alias` com 1 salto.
- Testes:
  - O índice montado de um YAML de exemplo (Balatro, Grounded e um alias) bate com o esperado.
  - Regra só de `mac` sai do índice.
  - `If-None-Match` com 304 mantém o índice e renova a data (servidor falso local ou leitor injetado).
  - YAML truncado mantém o índice anterior.
  - Corpo acima do limite é recusado.
- Aceite no app: Configurações mostra "Lista de locais de 25/11/2026". Um segundo "Atualizar" no mesmo dia devolve 304
  (anotado no `trail`). Medir e anotar no PR o tamanho do índice e o tempo de montagem.

### E2. Resolver caminhos (M)

- Tarefa: criar `pc_saves/paths.rs` com o contexto por loja, `when`, a recusa de variável desconhecida, de caminho
  relativo e de raiz larga, e `<storeUserId>` da Steam a partir de `loginusers.vdf`.
- Testes:
  - Tabela de casos com contexto falso: cada variável da seção 2.3.
  - `store: steam` não vale para Epic.
  - `<xdgData>` é ignorada no Windows.
  - `<home>` sozinho é recusado.
  - `..` é recusado.
  - Escape de `[`/`{` num caminho real.
  - Teste `#[cfg(windows)]` em que `<winDocuments>` == `standalone::documents()`.
- Aceite: `cargo test -p cerberus` verde. Nenhuma chamada de rede nos testes.

### E3. Ligar jogo à entrada (P)

- Tarefa: criar `pc_saves::matching`.
- Testes:
  - Steam pelo app id e pelo `steamExtra`.
  - Xbox pela família.
  - Epic pelo título normalizado ("Hades" e "HADES™").
  - Local com título ambíguo vira "sugestão", não automático.
  - Jogo sem entrada devolve `None`.
- Aceite no app: um log de depuração lista as entradas achadas para os jogos instalados. Anotar no PR quantos dos jogos
  reais do PC acharam entrada.

### E4. Generalizar `recomp/saves.rs` (M)

- Tarefa: `Manifest` v2 com `serde(default)`, `trait Resolver`, glob completo, impressão digital em fluxo, limites por
  parâmetro e checagem do destino final.
- Testes:
  - **Todos os testes atuais de `saves.rs`, `cloud.rs`, `emulator_saves.rs` e `recomp/mod.rs` passam sem mudança.**
  - Um zip v1 gravado antes da mudança (fixture) restaura.
  - Glob `**/*.sav` cria e restaura a estrutura.
  - Arquivo de 300 MB gera impressão sem passar de ~10 MB de memória (medir no teste ou anotar).
  - Zip com destino fora do lugar é recusado.
- Aceite: nenhum comportamento visível muda para recomps e emuladores (backup e restauração manual de um recomp no app).

### E5. Prévia na aba Saves (M)

- Tarefa: comando `pc_save_plan`, tipos em `shared` e aba Saves com o cartão "Onde ficam os saves" (somente leitura) para
  Steam, Epic, Xbox e Local.
- Testes: `plan` com contexto falso devolve lugares, arquivos e bytes, e marca os ignorados com motivo.
- Aceite no app (com print): Balatro (Steam) mostra `%APPDATA%\Balatro` e o selo "Nuvem da Steam". Um jogo Game Pass do
  PC mostra a pasta `wgs` e "Só backup". Um jogo local sem entrada mostra "Sem local conhecido".

### E6. Backup e restauração manuais (M)

- Tarefa: `pc_backup_now`, `pc_list_backups` e `pc_restore_backup`, com o componente `Backups` (`kind=Pc`). Recusar
  restauração com o jogo aberto. Mensagem de permissão.
- Testes:
  - Ida e volta em pasta temporária com contexto falso.
  - Cópia "Antes de restaurar" criada.
  - Restauração recusada com o jogo "rodando" (função `running` injetada).
  - Pasta sem escrita dá o erro com o nome do backup.
  - Rodízio por quantidade e por tamanho.
- Aceite no app: fazer backup de um jogo Steam real, apagar uma cópia de um arquivo de save (num jogo de teste),
  restaurar e conferir que o jogo carrega o progresso.

### E7. Backup automático depois de jogar (P)

- Tarefa: ligar `after_play` no `store_time` (espera de 60 s, cancela se voltar a rodar) e no `sessions::close_session`
  para Local. Anotar no `trail`.
- Testes:
  - A lógica de espera e cancelamento com relógio injetado.
  - `close_session` de Local chama o backup, seguindo o teste já existente de recomp.
- Aceite no app: jogar 2 min um jogo Steam aberto pela Steam, fechar e ver, em até 90 s, a linha "Backup depois de jogar"
  no histórico e o backup na aba Saves. Repetir sem mudar o save: nenhuma cópia nova.

### E8. Pastas do jogador e exclusões (P)

- Tarefa: `custom.rs`, "Adicionar pasta de save" (seletor de pasta), conversão para modelo portátil, desmarcar lugar ou
  arquivo, "É este jogo?" para Local.
- Testes:
  - `C:\Users\x\AppData\Roaming\Foo` vira `<winAppData>/Foo` e volta.
  - Pasta escolhida que seja a raiz larga é recusada.
  - Pasta escolhida que já faz parte de outro lugar não é contada duas vezes (sobreposição, ideia do Hydra).
  - Exclusão vale no backup e na restauração.
- Aceite no app: jogo local sem entrada passa a ter backup depois de escolher a pasta.

### E9. Nuvem por pasta, com âncora e conflito (M)

- Tarefa: envio para `Cerberus Saves/pc/<chave>`, `sync/<chave>.json`, `before_play` no `play` e em `sessions::start`,
  diálogo de conflito, envio suspenso em conflito, aviso para jogo aberto fora do Cerberus e regra 3.7 por loja. Depois,
  num PR à parte, levar a âncora também para `recomp::pull_from_cloud`.
- Testes, com duas pastas de dados simulando dois PCs e uma pasta de nuvem comum, seguindo os testes existentes de
  `pull_from_cloud`:
  - A grava e B traz.
  - B grava e A traz.
  - Os dois gravam e dá conflito, sem perder nenhuma cópia.
  - "Usar os de B" restaura e acerta a âncora.
  - "Manter os deste PC" envia e acerta a âncora.
  - Jogo com `cloud.steam` não é enviado com a chave desligada.
  - Xbox nunca é enviado.
  - Zip da nuvem truncado é tratado como "ainda chegando".
- Aceite no app (dois PCs reais ou duas contas do Windows apontando para a mesma pasta do OneDrive): jogo local salvo no
  PC A, Jogar no PC B traz o save; mudar nos dois sem abrir pelo Cerberus e depois Jogar abre o diálogo.

### E10. Xbox / Game Pass (P)

- Tarefa: pasta `wgs` e `xgs` inteira (achar `xgs` mesmo quando o manifesto só cita `wgs`), espera de 60 s, restauração
  com o aviso da seção 5.3, recusa quando a subpasta `<xuid>_` do backup não existe no destino.
- Testes:
  - Estrutura falsa `wgs/<a>_<b>/containers.index` com ida e volta.
  - Conta diferente é recusada.
  - Nunca há envio para a nuvem.
- Aceite no app: backup de um jogo Game Pass real depois de jogar. Restaurar nesse PC e abrir o jogo. Anotar o que o app
  Xbox fez (manteve ou trocou pelo da nuvem) e atualizar este documento, que hoje diz **não verificado**.

### E11. Saves no registro (M, depois)

- Tarefa: exportar as chaves `HKEY_CURRENT_USER/...` do manifesto com `winreg`, em JSON próprio dentro do zip
  (`registry.json`: chave, nome, tipo, valor em base64). Importar com confirmação. Nunca tocar em `HKEY_LOCAL_MACHINE`
  sem pedido explícito, e nunca pedir elevação.
- Testes: ida e volta numa chave de teste `HKCU\Software\CerberusTest\…`, apagada no fim. Tipos `REG_SZ`, `REG_DWORD`,
  `REG_BINARY` e `REG_MULTI_SZ`.
- Aceite: um jogo real com save no registro (achar um entre os 2.859) volta depois da restauração.

### E12. Página Saves, guia e roadmap (P)

- Tarefa: seção "Jogos de PC" na página Saves com estados e filtros, `docs/guide/saves.md` (e a versão em inglês, se o
  guia tiver), e itens marcados no `ROADMAP.md`.
- Aceite no app: print da página com pelo menos um jogo em cada estado da tabela 5.2 que der para produzir.

---

## 7. Riscos

| Risco | Probabilidade | Efeito | Mitigação |
|---|---|---|---|
| Licença dos dados mais restrita do que parece | média | ter que tirar o recurso | Não embutir, baixar da fonte, dar crédito, não cobrar. Se preciso, cair no plano B: o binário do Ludusavi baixado pelo jogador, sem tocar nos dados. |
| Manifesto com local errado ou largo demais | média | backup inútil ou enorme, ou restauração no lugar errado | Recusa de raiz larga, limites, prévia, confirmação para Local e restauração pelos `places` do zip. |
| Brigar com Steam Cloud ou Xbox | média | save perdido ou diálogo de conflito da loja | Regra 3.7: a loja sincroniza e o Cerberus só guarda versões. |
| Zip pela metade com jogo aberto ou arquivo travado | baixa | backup corrompido | Não fazer backup com o jogo aberto, abortar a cópia inteira e usar a gravação em `.part` com `persist_noclobber` que já existe. |
| Índice lento ou grande demais | baixa | abertura lenta | Thread em segundo plano, índice compacto, montar 1 vez por dia no máximo. Medir em E1. |
| Mudança de formato do manifesto | baixa | regras ignoradas | Seguir `schema.yaml`, ignorar campo desconhecido e manter o índice anterior se a montagem falhar. |
| Mudar `saves.rs` quebra os recomps | média | regressão no recurso mais usado | E4 só passa com todos os testes atuais verdes e uma fixture de zip v1. |
| Relógios diferentes entre PCs | média | escolha errada de "mais novo" | A decisão é por conteúdo (âncora + impressão), não por relógio. |
| Privacidade | baixa | ids de conta em caminhos e no nome do PC | Tudo fica no PC e na pasta de nuvem do próprio jogador. Nada vai para servidor. Não registrar ids de conta no `trail`. |

---

## 8. O que NÃO fazer

- **Não embutir** `manifest.yaml` nem o índice no repositório, no instalador ou num servidor nosso.
- **Não usar** o CDN do Hydra (`cdn.losbroxas.org`) nem outra cópia de terceiros: só o ramo `data` do repositório
  oficial.
- **Não copiar código do Heroic** (GPL-3) nem do RecompLauncher (GPL). Só ideias. Código do Hydra (MIT) pode ser
  adaptado com crédito no topo do arquivo.
- **Não guardar senhas nem tokens** nos backups, no índice ou no `custom.json`. A conta da Steam e a da Epic entram só
  como **id**, usado para achar a pasta. A pasta de nuvem é do jogador e o Cerberus não faz login em OneDrive, Google
  Drive nem Dropbox.
- **Não mexer** nos arquivos de controle das lojas (`remotecache.vdf`, `containers.index` isolado, `.egstore`) fora de uma
  restauração inteira e confirmada.
- **Não restaurar sozinho** em jogos com nuvem da loja nem em jogos Xbox.
- **Não fazer backup com o jogo aberto** sem confirmação, e nunca enviar zip parcial.
- **Não pedir elevação de administrador** para salvar ou restaurar.
- **Não seguir links simbólicos** nem pastas de junção durante a varredura (evita copiar o disco inteiro).
- **Não ajudar pirataria:** nada de saves "desbloqueados" de terceiros, nada de importar saves de outra conta para pular
  verificação de compra, nada de mexer em DRM. O recurso é só para os saves do próprio jogador.
- **Não apagar** backups do jogador fora do rodízio, nem ao desinstalar o jogo.
- **Não rodar** a varredura de todos os jogos ao abrir o Cerberus. A prévia é por jogo e sob demanda, e a página Saves usa
  o resultado guardado da última varredura.

---

## 9. Pendências marcadas como não verificadas

1. Se a licença CC BY-NC-SA da PCGamingWiki passa para o manifesto (o repositório só declara MIT).
2. A versão exata da licença da PCGamingWiki na fonte primária (a página de copyright deu 403; a Wikipedia diz 3.0).
3. O que a Steam faz quando arquivos em `userdata/<id>/<app>/remote` mudam por fora (diálogo de conflito ou troca
   silenciosa).
4. O que o app Xbox faz com uma pasta `wgs` restaurada (mantém, troca pelo da nuvem ou pergunta).
5. Se `xgamesaveutil` existe no PC do jogador fora do GDK.
6. Onde o GOG Galaxy guarda o id do usuário para `<storeUserId>`.
7. Se o `.item` da Epic no PC traz o modelo de pasta de save da loja.
8. Tamanho do índice montado e tempo de montagem (medir em E1).
9. Comportamento de arquivos "sob demanda" do OneDrive em cada jogo.
10. Se `std::fs` no Windows já lida com caminhos longos sem o prefixo `\\?\` no contexto do Cerberus.

## Fontes

- Ludusavi Manifest: https://github.com/mtkennerly/ludusavi-manifest (README, `LICENSE`, `data/schema.yaml`,
  `data/schema.strict.yaml`)
- Manifesto, ramo `data`: https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/refs/heads/data/data/manifest.yaml
- Ludusavi: https://github.com/mtkennerly/ludusavi (MIT; `docs/help/command-line.md`, `docs/help/roots.md`), release
  v0.31.0
- PCGamingWiki (licença do conteúdo pela Wikipedia): https://en.wikipedia.org/wiki/PCGamingWiki
- Microsoft Learn, XGameSaveFiles: https://learn.microsoft.com/en-us/gaming/gdk/docs/features/common/game-save/game-save-win32
- Microsoft Learn, xgamesaveutil: https://learn.microsoft.com/en-gb/gaming/gdk/_content/gc/tools-pc/commandlinetools/gr-xgamesaveutil
- Hydra (MIT), checkout local: `src/main/services/ludusavi.ts`, `src/main/services/cloud-save/`,
  `native/hydra-native/src/cloud_save/`, `scripts/postinstall.cjs`, `ludusavi/config.yaml`
- Heroic (GPL-3, só ideias), checkout local: `src/backend/cloud_saves/`, `src/backend/save_sync.ts`
- Cerberus: `recomp/saves.rs`, `recomp/cloud.rs`, `recomp/mod.rs` (`pull_from_cloud`), `emulator_saves.rs`, `sessions.rs`,
  `store_time.rs`, `trail.rs`, `ui/src/backups.rs`, `ui/src/pages.rs`
