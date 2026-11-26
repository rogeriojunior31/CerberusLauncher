# Auditoria: recomps — preparação e personalização

Revisão de 25/11/2026, feita só com leitura do código do Cerberus (branch `ajustes-rapidos`, `a1bc5cb`). Comparei com o
RecompLauncher (GPL, só ideias), o RecompLauncher-SDK (MIT) e o Playnite (MIT). Nada foi compilado ou executado.
Quando uma reprodução não foi confirmada num jogo real, ela está marcada **não verificado**. A lógica citada foi
conferida no código.

Este documento não repete o que já está em [recomps-robustos.md](recomps-robustos.md): versões, `extract`/`contains`/
`destination`, conversão RVZ/WBFS, janela "Resolver problemas" e "Liberar espaço". O foco aqui são **falhas no que já
existe**.

Regras que valem para todas as correções:

- Nunca pirataria.
- Nunca baixar BIOS, chaves ou DLSS sem aprovação do jogador.
- RecompLauncher só como ideia, sem copiar código.

---

## 1. Mapa

### 1.1 Easy Setup e a cópia do jogador

**Achar a cópia.**

- `roms::scan` percorre as pastas de ROMs (`roms.rs:310`).
- `join_discs` junta discos "(Disc N)" e `.m3u` (`roms.rs:247`).
- `match_catalog` casa pelo nome e, no N64, pelo cabeçalho (`roms.rs:459`).
- O índice fica em disco (`save_index`, gravação não atômica).

**Logo após instalar** (`install_recomp`, `main.rs:2208-2222`):

- Se a varredura achou a cópia, `use_game_copy` é chamado sozinho e os erros são engolidos (`if let Ok`).
- Depois `restore_copy` devolve a cópia que a atualização tirou da pasta.

**Escolher a cópia à mão.**

- Na interface, o cartão "Configuração fácil" fica em `ui/src/game.rs:741`.
- No backend, `set_game_copy` (`main.rs:2228`) abre o seletor e chama `use_game_copy` (`recomp/mod.rs:848`).
  - Com `recomplauncher.json`:
    - `manifest::check` confere tipo, extensão, arquivo interno ISO9660/XDVDFS, ids, nome e sha256.
    - `manifest::place` faz a cópia para `into`.
    - `manifest::verified` compara CRC32 com tamanho contra `dumps` e grava `Game.copy_verified`.
  - Catálogo antigo: `copy_fits` e `put_in_folder` (`mod.rs:924`). Conforme o caso, esse passo:
    - copia a pasta;
    - extrai XISO (`xdvdfs::extract`) ou ISO9660 (`iso9660::extract`);
    - converte CHD/CSO em ISO (`disc_image::write_iso`);
    - ou copia o arquivo com `.part` e renomeia (`copy_whole`, `mod.rs:1431`).
- Outras entradas (segundo disco, chaves compartilhadas) passam por `use_input` (`mod.rs:648`). As chaves ficam
  guardadas uma vez só (`manifest::keep_shared_key`).

**O que fica gravado.**

- Na biblioteca: `Game.game_copy`, `Game.inputs`, `Game.copy_verified` e `setting_picks` (inclusive `prepare:<id>`).
- Na pasta do jogo:
  - a cópia ou os arquivos extraídos;
  - `.cerberus-prepared`;
  - `recompdata/`;
  - `.recomplauncher-tmp/`, só durante a preparação.

**Próximo passo.** `next_step` e `next_step_with` (`mod.rs:692-751`) devolvem `NextStep`:

- `Locate`: o programa sumiu;
- `RunInstaller`: falta rodar o instalador;
- `ChooseCopy`: a cópia falta, não confere ou falta outra entrada;
- `Play`.

`ready_to_play` (`main.rs:2523`) usa a mesma resposta antes de abrir o jogo.

**Jogar** (`sessions.rs:~120-260`):

1. `setup_pending` faz rodar o `setup_runner`.
2. `preparation()` faz rodar `prepare_then_start` (`sessions.rs:310`), um processo sem janela com progresso, limite de
   tempo e marcador.
3. `pull_from_cloud`.
4. `launch_plan` monta args, env, `--flag`/`ENV` com a cópia e opções da aba Ajustes (`mod.rs:753`).
5. `prepare_start` grava os `.ini`/`.toml`/`.json` do jogo (`mod.rs:390`).
6. `looks::prepare` coloca o ReShade.
7. `spawn_game`.
8. Quando o jogo fecha, `release` chama `looks::take_back`.

### 1.2 Instalador externo

- `setup_runner::run` (`setup_runner.rs:20`) usa `Command::spawn` (ou `msiexec /i`) e espera numa thread.
  `installer_result` traduz os códigos de saída.
- Em seguida `locate` procura primeiro na pasta da instalação. Depois procura no registro Uninstall, por nome parecido
  (`same_name`).
- Se achar, `use_program` grava o `exe` e `setup_pending=false`. Se não achar, emite o evento `setup-needs-location` e o
  jogador escolhe o programa (`locate_game`).
- Ao iniciar o Cerberus, `recover` aplica a mesma busca, sem perguntar, a todos os jogos `setup_pending`.

### 1.3 Trazer uma instalação existente

`ui/src/adopt.rs` chama `recomp::adopt::scan`, que percorre até 3 níveis e pula as pastas com o marcador do Cerberus.

`candidates` separa dois casos:

- **certo**: nome exato de `start`;
- **sugestão**: nome parecido.

`adopt` (`adopt.rs:144`) faz o seguinte:

- grava `external=true` e `install_dir = pasta do .exe`;
- um jogo adicionado à mão com o mesmo exe vira o recomp e mantém o tempo de jogo.

Ao remover um jogo `external`, a pasta não é apagada (`main.rs:2128`).

### 1.4 Ajustes do jogo

- `game_settings::offer_of` lê `settings.config` e `settings.settings` do catálogo ou do manifesto. Os lugares
  permitidos são `{install}`, `{local}`, `{appdata}`, `{config}`, `{documents}` e `{home}`, sem `..`.
- O comando `set_game_setting` (`main.rs:2816`) recusa enquanto o jogo está aberto e chama `choose`, `set_number` ou
  `reset`.
- `ConfigFile` edita só as linhas das chaves e grava com temporário e rename (`game_settings.rs:371`).
- A cada Jogar, `before_start` aplica `new`/`ensure`/`always` e o caminho da cópia (`game_path`).
- O que fica gravado: o arquivo de configuração do jogo (na pasta dele ou em AppData/Documentos) e `setting_picks` na
  biblioteca.

### 1.5 Filtros visuais (ReShade)

- `set_look` (`main.rs:486`) chama `looks::ensure`. Na primeira vez, o Cerberus baixa `ReShade_Setup_<última tag>.exe`
  de reshade.me e tira dele `ReShade64.dll` e `ReShade32.dll`, que ficam em `<dados>/reshade`.
- A cada partida, `looks::prepare` faz o seguinte:
  - grava `ReShade.ini` e `Cerberus.ini` em `<dados>/looks/<jogo>`;
  - escreve `placed.txt` (caminho e sha256);
  - copia a DLL como `dxgi.dll` **na pasta do jogo**;
  - define `RESHADE_BASE_PATH_OVERRIDE`.
- `take_back` só apaga a DLL se o sha256 bater. `take_back_all` roda quando o Cerberus abre.

### 1.6 Mods

`mod_setup` (`main.rs:2265`) decide de onde vêm os mods:

- **Thunderstore**: `mods.rs`.
  - Os arquivos `.nrm`, `.rtz` e `.dll` são achatados em `<LOCALAPPDATA>/<data>/mods` ou em `mods/` quando o jogo é
    portable.
  - O registro fica em `<dados>/mods/<recomp>.json`.
- **GameBanana**: `gamebanana.rs`.
  - Só arquivos marcados como limpos.
  - `.zip`/`.7z` em `downloads/mods`.
  - Os packs `.otr`/`.o2r`/`.nrm` vão para `mods/` ao lado do exe.
- **HedgeModManager**: `hedge.rs`.
  - A pasta com `mod.ini` vai para `mods/`.
  - O Cerberus edita `cpkredir.ini` e `ModsDB.ini`.
  - Guarda `ModsDB.ini.cerberus.bak` na primeira vez.

As ações são instalar ou atualizar (com dependências) e remover. **Não há ligar/desligar.**

### 1.7 DLSS

- `dlss::find` acha `nvngx_dlss*.dll` em até 8 níveis.
- `swap`:
  - aceita só um arquivo com o mesmo nome, versão PE e "NVIDIA" no texto;
  - guarda o original em `<dados>/dlss/<jogo>/...`;
  - copia o novo para `.cerberus-new` e renomeia.
- `restore` devolve o original.
- `list` apaga o original guardado quando o jogo traz de volta o próprio arquivo.
- Nada é baixado.

### 1.8 Pacotes do Windows

- `vc_version` lê o registro (só x64).
- `dx_missing` procura 4 DLLs em `System32`.
- `install_vc`/`install_dx`:
  - baixam de `aka.ms` ou `download.microsoft.com`;
  - conferem o host final e `MZ`;
  - rodam `vc_redist /install /passive /norestart` ou `dxwebsetup /Q` com `Command::status`.

### 1.9 Página do jogo e Gerenciar

Tudo isso aparece em `ui/src/game.rs`:

- Configuração fácil (`EasySetup`, `MoreInputs`, `PrepareTicks`, `PrepareStatus`);
- ações do desenvolvedor (`GameActions`, que esvazia a pasta com clique duplo);
- Ajustes (`ui/src/game_settings.rs`);
- filtros (`ui/src/looks.rs`);
- mods;
- DLSS;
- "Seu PC".

### 1.10 O que o Cerberus altera na pasta do jogo (resumo)

| Item | Onde | Volta ao original? |
| --- | --- | --- |
| Cópia, extração, ISO convertida | `{install}/<folder>` ou `into` | Não; sai com a desinstalação |
| `.cerberus-prepared`, `recompdata/`, `.recomplauncher-tmp/` | `{install}` | Temp sim; marcador não |
| Arquivo de configuração do jogo | `{install}`/AppData/Documentos | **Não, e sem backup** |
| `dxgi.dll` do ReShade | ao lado do exe | Sim, quando o jogo fecha ou o Cerberus abre (se o hash bater) |
| Mods `.nrm/.dll/.otr` | `mods/` (LOCALAPPDATA ou ao lado do exe) | Remover tira só os registrados |
| `cpkredir.ini`, `ModsDB.ini` | AppData ou ao lado do exe | Backup só do `ModsDB.ini` e só se a leitura deu certo |
| DLSS | onde o jogo tem | Sim, com "Voltar à original" |

---

## 2. Como os outros fazem

| Tema | RecompLauncher (GPL, só ideia) / SDK (MIT) | Playnite (MIT) | Cerberus hoje |
| --- | --- | --- | --- |
| Verificar a cópia | CRC32 + tamanho contra `dumps`, lido em blocos com progresso e cancelamento. Cache `path,size,mtime,crc` | CRC32 contra DATs, também dentro de arquivos compactados; junta `.cue`/`.m3u` | CRC32 + tamanho só quando o tamanho bate; sem progresso nem cancelamento; `.cue` nunca confere (F4) |
| Colocar a entrada | Temporário `create_new` + `sync_all` + rename. Hard link ≥ 512 MB. **Nunca sobrescreve arquivo não gerenciado.** Confere espaço | — | Arquivo: `.part` + rename. Pasta e extração: direto no destino (F3); sobrescreve; espaço só para arquivo |
| Extrair XISO/ISO | Saneia nomes (`..`, `/`, `:`, controle), confere espaço, depois confere `default.xex`. Ponto fraco: grava no lugar e retoma pelo tamanho | — | Saneia por componentes; sem `:`/nomes reservados (F17); sem espaço nem staging (F3) |
| Pasta temporária do setup | Marcador `.made-by-recomplauncher`, limpeza por `Drop` e na próxima execução, 1 GB livre | — | 1 GB livre; apaga `.recomplauncher-tmp` sem marcador; processo órfão se o Cerberus fecha (F12) |
| Instalador | ShellExecute com UAC (1223 = recusou); espera cancelável; "fechou cedo demais" (< 10 s sem tocar o exe); espera os filhos (`wait_for_rest`); recusa pasta de outro jogo | `InstallController`/`InvokeOnInstallationCancelled`; estados que bloqueiam ações concorrentes | `Command::spawn` (sem UAC, F5); registro por nome parecido aceito sem confirmar (F6) |
| Adotar | Desce um nível com subpasta única; recusa manifesto de outro repo; recusa pasta que contém ou está dentro de outro jogo | — | Sem essas recusas (F18) |
| Ajustes `.ini` | Linha a linha + reativa `;key=`; `.rltmp` + rename; **também sem backup** | — | Igual, mas leitura que falha vira arquivo vazio e sobrescreve (F1) |
| Mods | **Recusa conflito** de arquivo de outro mod; HMM com rollback da pasta; remover mantém arquivos compartilhados | — | Sobrescreve e apaga o que for do mesmo nome (F11); atualização apaga antes de ter a nova (F10) |
| ReShade | Recusa `dxgi.dll` ou `ReShade.ini` alheios; `placed.txt` com SHA-256; 20 tentativas para tirar; camada Vulkan por env | — | Mesma ideia; tenta 1 vez; pode pôr ao lado do exe errado (F7); download sem hash fixo (F8) |
| DLSS | Manifesto com URL `github.com/NVIDIA`, tamanho + SHA-256 + Authenticode, `.part` | — | Só troca local; backup sem `.part` (F9) |
| VC++/DirectX | Pede elevação só em 5/740; renomeia DLL velha do jogo para `.old` | — | `Command::status` sem elevação (F5); sem Authenticode (F8); só x64 (F19) |
| Ações de jogo | — | Pre/Post/Exit scripts, `CancelStartup`, variáveis `{InstallDir}`/`{ImagePath}`, TrackingMode | Ações do manifesto (abrir/esvaziar pasta); scripts estão na Onda 6.4/7.4 |

Fontes:

- RL `src-tauri/src/inputs.rs:471-487`, `extract.rs:25,174`, `setup_temp.rs`, `installer.rs:76-378`, `own_setup.rs:351,765`,
  `adopt.rs:701-799`, `profile.rs:1318-1447`, `gamebanana.rs:362-594`, `looks.rs:597-788`, `dlss.rs:125`, `dllcheck.rs:110`,
  `lib.rs:2581,4013-4063`.
- Playnite `source/PlayniteSDK/Plugins/Actions.cs:91-220`, `GamesEditor.cs:319-380,875,945,1276`,
  `Emulators/Scanner.cs:863-1100`.

---

## 3. Falhas

### F1 — Ajustes do jogo apagam o arquivo de configuração quando a leitura falha · **Crítica**

**Onde:**

- `src-tauri/src/game_settings.rs:260-268` (`ConfigFile::open`: `read_to_string(...).unwrap_or_default()` e JSON
  `from_str(...).ok()...unwrap_or_default()`);
- gravação em `:371-383`;
- caminhos de chamada:
  - `before_start` (`:400-436`, roda **a cada Jogar** via `recomp/mod.rs:390` e `sessions.rs:183`);
  - `choose` (`:508`), `set_number` (`:526`) e `reset` (`:544`).

**Reprodução (lógica conferida; jogo real não verificado):** qualquer um destes casos dispara a falha.

- O arquivo de configuração está em UTF-16 (comum em jogos Windows) ou em Latin-1 com acento: `read_to_string` falha.
- O arquivo está aberto com bloqueio por outro programa: a leitura falha.
- O JSON tem BOM, comentários ou vírgula sobrando: `serde_json` recusa.

Em qualquer desses casos o arquivo abre "vazio". Basta mudar um ajuste, ou um manifesto com `always`/`game_path`, e
o arquivo inteiro do jogo é trocado por um com só as chaves do Cerberus. Configurações, controles e às vezes o
progresso de quem guarda tudo no mesmo `.json` se perdem.

**Correção:**

- `open` deve devolver `Result`.
  - Se o arquivo existe mas não é UTF-8 legível, recusar com uma mensagem clara (ou ler UTF-16 com BOM e gravar no
    mesmo formato).
  - JSON inválido também deve recusar.
- Antes da primeira alteração de cada arquivo, guardar uma cópia `<arquivo>.cerberus.bak`, ou em `<dados>/settings-backup/<jogo>/`,
  e oferecer "Voltar ao arquivo original" em Ajustes. É a lacuna que o RL também tem.
- Em `before_start`, um erro deve ir para o histórico e **não** gravar.

**Teste de aceite:**

- `tests::a_file_it_cannot_read_is_never_overwritten` com arquivos UTF-16LE com BOM, JSON com BOM, JSON com
  comentário e arquivo bloqueado (Windows).
  - `before_start`, `choose` e `set_number` devolvem erro.
  - Os bytes ficam idênticos.
- `reset_restores_the_original_backup`.

### F2 — HedgeModManager: `cpkredir.ini` e `ModsDB.ini` sobrescritos sem cópia quando a leitura falha · **Alta**

**Onde:** `hedge.rs:98-112` (`mods_db`: `read_to_string(...).unwrap_or_default()` e depois `fs::write`, sem backup) e
`:133-149` (`change_db`: o backup só é feito se `before.is_some()`; a gravação é `fs::write` direto, não atômica).

**Reprodução (não verificado com o HMM real):**

- Um `ModsDB.ini` gravado pelo HMM em UTF-16, ou travado pelo HMM aberto: `before = None`, nenhum `.bak` é feito, e o
  arquivo é refeito só com o mod do Cerberus. A lista de mods do jogador some.
- `cpkredir.ini` ilegível vira um arquivo com só `[CPKREDIR]`.
- Se a luz cair durante `fs::write`, o arquivo fica pela metade.

**Correção:**

- Mesmo padrão da F1: ler com `Result`, recusar quando não conseguir ler.
- Backup de **ambos** antes da primeira mudança.
- Gravar com temporário e rename.

**Teste:** `hedge::an_unreadable_db_is_left_alone` e `cpkredir_gets_a_backup_too`.

### F3 — Pasta copiada ou disco extraído pela metade conta como pronto · **Alta**

**Onde:**

- `recomp/manifest.rs:888-899` (`place` de pasta chama `copy_tree` direto em `target`);
- `manifest.rs:613-634` (`missing_for` considera a entrada presente se `target.exists()`);
- `recomp/mod.rs:924-947` (`put_in_folder`: `copy_tree`, `xdvdfs::extract` e `iso9660::extract` direto em `dir`);
- `mod.rs:742` (para `Folder` sem `file_name` basta `game_copy` existir);
- `iso9660.rs:41-49` e `xdvdfs.rs:100-113`: `File::create` no destino final, sem conferir espaço.

**Reprodução:**

- Escolher a pasta de um jogo de PS3 (dezenas de GB) ou um XISO. Fechar o Cerberus no meio, ou deixar o disco
  encher. A pasta `into` já existe, então `next_step` devolve `Play` e o jogo abre com arquivos faltando.
- Trocar uma pasta já escolhida por outra que falha no meio mistura arquivos novos e velhos, e `game_copy` continua
  o antigo.

**Correção:**

- Extrair ou copiar para `<dir>.cerberus-part-<n>` ao lado.
- Conferir o espaço antes (`disk::require` com o tamanho total; no XDVDFS e no ISO9660 dá para somar os tamanhos
  percorrendo a árvore).
- Confirmar o arquivo exigido (`check.file`/`xdvdfs`/`iso9660`).
- Só então renomear para o lugar e gravar `game_copy`. Restos `.cerberus-part-*` são varridos quando o Cerberus abre.
- Opcional: marcador `.cerberus-copy` com origem, tamanho e mtime, para `missing_for` confiar só em pasta completa.

**Teste:** `an_interrupted_folder_copy_is_not_ready`. Simular uma falha de E/S no N-ésimo arquivo (injetar um `Write`
que falha) e conferir que:

- `next_step` continua `ChooseCopy`;
- não sobra nada no destino;
- a cópia anterior continua intacta.

### F4 — `.cue`/`.gdi`/`.m3u` aceitos, mas só a folha é copiada · **Alta**

**Onde:**

- `manifest.rs:810` (aceita `.cue/.gdi/.m3u` onde se pede `bin`/`iso`);
- `place` (`:888-899`) e `copy_whole` (`mod.rs:961`) copiam um arquivo só;
- `placed_at` com `name` renomeia o `.cue`;
- `verified` (`:645-667`) calcula o CRC32 do próprio `.cue`, por isso nunca confere com o Redump;
- `check` com `sha256` (`:858`) compara o hash do `.cue`.

**Reprodução (não verificado com um recomp real que peça `into`):**

- Um input com `into` e formato `bin` recebe `Jogo.cue`. Na pasta do jogo aparece só o `.cue`, e as faixas `.bin`
  ficam para trás.
- Com `name: "game.cue"`, as referências `FILE "Jogo (Track 1).bin"` quebram.
- O selo "Confere com o disco verificado" nunca aparece para jogos de CD.

**Correção:**

- Ler a folha: `FILE "..."` no `.cue`, linhas no `.gdi`, discos no `.m3u` (reaproveitar `roms::listed_discs`).
- Copiar ou fazer link de todas as faixas juntas e manter os nomes.
- Se `name` for pedido, reescrever o `.cue` copiado.
- Verificar faixa por faixa contra `dumps`. O SDK ainda não define hash de várias partes, então dá para aceitar
  "todas as faixas batem com alguma entrada de dumps".
- Para `sha256`, recusar uma folha com uma mensagem clara em vez de "conteúdo diferente".

**Teste:** `a_cue_sheet_brings_its_tracks` (2 `.bin` + `.cue`, `into` com e sem `name`) e
`a_cue_whose_tracks_match_dumps_is_verified`.

### F5 — Instalador que pede administrador não abre · **Alta**

**Onde:** `setup_runner.rs:23-33` (`Command::new(installer).spawn()`) e `runtimes.rs:96` (`Command::new(&installer).status()`).

**Reprodução (não verificado por instalador):**

- Um setup NSIS com `RequestExecutionLevel admin` (manifesto `requireAdministrator`) faz o `CreateProcess` falhar
  com o erro 740, e o jogador lê "Não foi possível abrir o instalador: ... (os error 740)".
- O mesmo é provável no `dxwebsetup.exe`.
- O comentário em `runtimes.rs:74` ("Windows asks the player for permission") não vale para `CreateProcess`.

**Correção:**

- Em 740, repetir com `ShellExecuteExW` e verbo `runas`, `SEE_MASK_NOCLOSEPROCESS`, e esperar o handle.
- Tratar 1223 como "você recusou a permissão".
- O RL faz isso. O `sessions.rs:281` também traduz 740, mas só para o jogo.

**Teste:**

- Unidade: `installer_result` e o mapeamento de 740/1223.
- Manual: um instalador NSIS admin de teste e o `dxwebsetup` num Windows sem D3DX.

### F6 — Depois do instalador, um programa de outro jogo pode ser adotado sem perguntar · **Média**

**Onde:** `setup_runner.rs:120-185` (`locate`/`installed_location`/`same_name`, aceitando mais de metade das palavras)
e `:54-63` (`recover` aplica a busca quando o Cerberus abre, para todos os `setup_pending`).

**Reprodução:** um recomp "Sonic Unleashed Recompiled" com o jogo original instalado pela Steam.

- O registro Uninstall tem "Sonic Unleashed", que bate com 2 de 3 palavras.
- `InstallLocation` aponta para a pasta da Steam, e `find_game_program` escolhe o exe original.
- `use_program` grava esse exe e `setup_pending=false`. Jogar abre outro jogo (lógica conferida; caso real não
  verificado).
- Um setup que só abre outro processo e sai (bootstrapper) é tratado como sucesso imediato.

**Correção:**

- Uma correspondência pelo registro vira **sugestão** que o jogador confirma: é o mesmo modelo "certo/sugestão" do
  `adopt`.
- Recusar pastas que já pertencem a outro jogo da biblioteca ou à Steam/Epic.
- Heurística de "fechou cedo demais" (< 10 s e nenhum exe novo) e esperar os processos filhos, como faz o RL.

**Teste:** `a_registry_match_is_only_a_suggestion` e `a_folder_of_another_library_game_is_refused`.

### F7 — Filtro avaliado num programa e aplicado em outro; DLL que não saiu continua ativa · **Média**

**Onde:**

- `main.rs:455-476`: `look_status` usa `tracking_exe` quando existe;
- `sessions.rs:205`: `looks::prepare` usa `exe`, isto é, o launcher do jogo;
- `looks/mod.rs:268-279`: `take_back` tenta uma vez só.

**Reprodução:**

- ReStreet (`FifaStreetLauncher.exe` abre `fifastreet.exe`): a aba diz que o filtro funciona (a rota é avaliada em
  `fifastreet.exe`), mas a partida tenta pôr o ReShade ao lado do launcher. Se o launcher não citar `d3d11.dll`,
  aparece a mensagem "Esse jogo não aceita filtros". Se estiverem em pastas diferentes, o filtro simplesmente não
  aparece (não verificado no ReStreet).
- Se a DLL ainda estiver presa quando o jogo fecha, `take_back` desiste. O jogador desliga o filtro e a próxima
  partida ainda abre com ReShade. A interface diz "sem filtro".

**Correção:**

- Usar o mesmo programa nos dois lugares (`drawing_program`) e pôr `dxgi.dll` ao lado dele.
- `take_back` com novas tentativas por alguns segundos (como `throttle::rename_patiently`).
- Antes de cada partida **sem** filtro, chamar `take_back(game)`.

**Teste:**

- `the_look_goes_beside_the_drawing_program`.
- `a_lingering_dll_is_removed_before_a_start_without_look`: simular a falha de remoção uma vez.

### F8 — Executáveis de terceiros baixados sem conferir hash ou assinatura · **Média**

**Onde:** `looks/mod.rs:155-197` (a versão é a "primeira tag" da API do GitHub, só se confere `MZ`, e as duas DLLs são
gravadas sem `.part`) e `runtimes.rs:75-94` (só host e `MZ`).

**Risco:**

- Uma tag nova ou um mirror comprometido coloca uma DLL arbitrária **dentro da pasta do jogo**.
- Sem Authenticode, um proxy corporativo que intercepte TLS passa.
- Se a gravação de `ReShade32.dll` for interrompida, fica um arquivo truncado. `ready()` diz pronto e o jogo de 32
  bits trava.

**Correção:**

- Fixar a versão do ReShade e o sha256 do setup no código (atualizado por PR).
- Conferir Authenticode (`WinVerifyTrust`, assinante "crosire" ou "Microsoft Corporation") nos três executáveis.
- Gravar com `.part` e rename e conferir o tamanho.
- A primeira vez já avisa que baixa o ReShade (`ui/src/looks.rs:42`). Incluir o nome e a licença do ReShade no aviso.

**Teste:** `a_setup_with_another_hash_is_refused` e `a_cut_dll_is_not_ready` (truncar no meio).

### F9 — Backup do DLSS interrompido vira o "original" · **Média**

**Onde:** `dlss.rs:131-135` (`fs::copy(&here, &backup)` direto) e `:147-160` (`restore` copia o backup e o apaga).

**Reprodução:** fechar o Cerberus ou encher o disco durante a primeira troca.

- O backup truncado fica com `is_file()` verdadeiro, então a próxima troca não refaz a cópia.
- "Voltar à original" coloca a DLL quebrada no jogo e apaga o backup. O jogo para de abrir com DLSS.

**Correção:**

- Copiar para `backup.part`, conferir o tamanho e a versão PE, e renomear.
- `restore` confere o tamanho e a versão antes de usar e só apaga o backup depois que o arquivo no lugar confere.

**Teste:** `a_partial_backup_is_never_restored`.

### F10 — Atualizar um mod apaga o antigo antes de ter o novo; registro frágil · **Média**

**Onde:**

- `main.rs:2383-2391` (Thunderstore: `remove_files(old)` e depois `unpack`);
- `hedge.rs:227` (`remove_folders(old)` antes de mover o novo);
- `mods.rs:209-219` (`load_installed` engole erro e devolve vazio; `save_installed` é `fs::write` não atômico).

**Reprodução:**

- Um zip novo corrompido ou o disco cheio fazem o mod antigo sumir. O registro não é salvo, então a interface
  continua mostrando a versão antiga como "instalada".
- Se `<recomp>.json` for truncado, todos os mods ficam órfãos. "Remover" não tira nada e "Instalar" duplica.

**Correção:**

- Desempacotar para staging.
- Remover os arquivos velhos só depois que os novos estão no lugar.
- Registro com temporário e rename.
- Registro ilegível é guardado à parte (`.broken`) e avisado, nunca tratado como vazio.

**Teste:** `a_failed_update_keeps_the_old_mod` e `a_broken_record_is_kept_and_reported`.

### F11 — Mods sobrescrevem e removem arquivos que não são deles · **Média**

**Onde:** `mods.rs:254-256` (rename sobre um arquivo existente de mesmo nome), `mods.rs:282-289` (remove pelo nome) e
`gamebanana.rs:306` (rename sobre um existente).

**Reprodução:**

- O jogador tem `cool.nrm` posto à mão, ou dois mods trazem a mesma `.dll`.
- Instalar troca o arquivo sem avisar. Remover um dos mods apaga o arquivo do outro, ou o do jogador.

**Correção:**

- Antes de colocar, recusar um nome que já existe e não está registrado para este mod, como faz o RL ("já existe
  `x.nrm` de outro mod").
- Ao remover, manter um arquivo que outro mod registrado também lista.
- Considerar ligar/desligar (renomear para `.disabled`) em vez de só remover.

**Teste:** `a_file_of_another_mod_is_neither_replaced_nor_removed`.

### F12 — Preparação: processo órfão, segunda preparação em paralelo e marcador engolido · **Média**

**Onde:**

- `sessions.rs:310-417`;
- `:384` (`child.kill()` mata só o processo direto);
- `:400` (`let _ = fs::write(&step.marker, ...)`);
- sem Job Object;
- `.recomplauncher-tmp` apagada sem marcador próprio (`:318-320`).

**Reprodução:**

- Fechar o Cerberus durante "Preparando o jogo" deixa o processo rodando. Ao reabrir e apertar Jogar, como não há
  marcador, uma **segunda** preparação começa sobre a mesma pasta, e a primeira apaga e recria o temp da outra.
- Se o marcador não puder ser gravado (pasta somente leitura, antivírus), a preparação roda de novo a cada Jogar e
  ninguém fica sabendo.

**Correção:**

- Pôr a preparação num Job Object com `KILL_ON_JOB_CLOSE`, para que morra junto com o Cerberus.
- Gravar `.recomplauncher-tmp/.made-by-cerberus` e só apagar a pasta se o marcador existir.
- Erro ao gravar o marcador vai para o histórico e para um toast.
- Ao reabrir, detectar o temp de uma preparação interrompida e mostrar "A preparação foi interrompida; Jogar refaz".
  Isso cobre a 5.3, "notar uma instalação interrompida".

**Teste:** `a_killed_launcher_takes_the_preparation_with_it` (Windows, processo de teste que dorme) e
`a_marker_write_failure_is_reported`.

### F13 — Valor de INI com `#` ou `;` é cortado e corrompe a linha · **Média**

**Onde:** `game_settings.rs:231-232` (`rest.find([';', '#'])` em INI) e `:336` (a troca mantém `line[end..]`).

**Reprodução:** a cópia está em `D:\Jogos #2\zelda.iso`, ou já existe a linha `iso=C:\a#b.iso`.

- A leitura devolve `D:\Jogos `.
- A gravação produz `iso=<novo>#b.iso`.
- O jogo recebe um caminho inválido a cada partida.

**Correção:**

- Em INI, tratar `;` e `#` como comentário só no início da linha ou depois de um espaço (` ;`).
- Valor com esses caracteres vai entre aspas quando o formato aceita, ou o Cerberus recusa com uma mensagem.

**Teste:** `a_path_with_a_hash_survives_the_ini`.

### F14 — Cópias e conversões grandes sem espaço conferido, progresso ou cancelamento · **Média**

**Onde:**

- `main.rs:2254-2257` (`spawn_blocking(use_game_copy)` sem eventos);
- `mod.rs:924-962` (só `manifest::place` de arquivo chama `disk::require`);
- `disc_image.rs:243-257` (`write_iso` sem `require`);
- `manifest.rs:645-667` (CRC lido inteiro, sem progresso).

**Reprodução:** escolher um ISO de 8 GB. O botão fica "ocupado" por minutos sem nenhuma informação, e não dá para
cancelar. Com o disco cheio, o erro aparece só no fim e cai na F3.

**Correção:**

- Usar o mesmo canal de progresso da preparação (`prepare-progress`) ou da fila de downloads.
- Cancelamento com `AtomicBool`.
- `require(tamanho + margem)` antes.
- Considerar hard link para arquivos grandes que o jogo só lê, como o RL (já previsto em 7.5).

**Teste:** `copy_reports_progress_and_can_be_cancelled` e `not_enough_room_fails_before_writing`.

### F15 — CHD de CD de PS1 vira ISO de 2048 bytes: setores Mode 2 Form 2 se perdem · **Média**

**Onde:** `disc_image.rs:82-84` (só a faixa 1, recortando 2048 bytes por setor) e `:243-257` (`write_iso`).

**Reprodução (não verificado com um recomp real):** CHD de PS1 com vídeos STR ou áudio XA (Form 2, 2324 bytes).

- A ISO gerada descarta esses dados e as faixas de áudio.
- Um recomp que leia a ISO pode tocar vídeo corrompido.

**Correção:**

- Para CD, só oferecer a conversão quando o recomp pede ISO de dados e o disco não tem Form 2. Senão, gerar
  BIN/CUE 2352 (já previsto em 5.3 como "CHD para BIN/CUE") ou recusar com explicação.

**Teste:** `a_cd_chd_with_form2_sectors_is_not_flattened`.

### F16 — Mensagem diz que a cópia não serve quando falta outra entrada · **Média**

**Onde:**

- `mod.rs:700-716`: `ChooseCopy` também quando `missing_for` acusa um segundo disco ou uma chave;
- `main.rs:2527` e `mod.rs:677-687`: `copy_problem` cai em "Ele pede: ...";
- `ui/src/game.rs:750,778`: o aviso "A cópia escolhida não serve mais".

**Reprodução:** jogo de dois discos, com o disco 1 escolhido e o disco 2 não. A página e o Jogar dizem que a cópia
"não serve" ou "mudou de lugar". O problema real é o disco 2.

**Correção:** um `NextStep::ChooseInput { id, label }`, ou `ChooseCopy` com o motivo, para que a interface aponte a
entrada que falta.

**Teste:** `a_missing_second_disc_is_named` (backend) e uma checagem da string na interface.

### F17 — Nomes vindos do disco não barram nomes reservados do Windows nem `:` · **Baixa**

**Onde:** `iso9660.rs:82` e `xdvdfs.rs:146` (só conferem os componentes).

**Reprodução:** uma imagem forjada com `CON`, `NUL`, `COM1` ou `a:b` (fluxo alternativo NTFS). A gravação vai para o
dispositivo ou cria um ADS. Não sai da pasta, mas falha de um jeito confuso.

Também ficam sem tratamento:

- arquivos ISO9660 com vários extents (> 4 GB);
- nomes que só diferem em maiúsculas, que se sobrescrevem.

**Correção:** recusar nomes reservados, `:`, pontos ou espaços no fim e duplicatas sem diferença de caixa, como o
`safe` do RL.

**Teste:** `reserved_and_stream_names_are_refused`.

### F18 — Adotar não confere de quem é a pasta · **Baixa**

**Onde:** `recomp/adopt.rs:144-190`.

**Reprodução:**

- Escolher um exe que está dentro da instalação gerenciada de outro recomp. O resultado é um `external` aninhado, e
  desinstalar o outro recomp apaga esse.
- Um exe em `bin/` faz `install_dir` virar `bin`, e `{install}`, saves e manifesto procuram no lugar errado (não
  verificado em um recomp real).

**Correção:**

- Recusar uma pasta que contém, ou está dentro de, outra pasta da biblioteca ou das instalações do Cerberus.
- Subir até a pasta que tem `recomplauncher.json`.
- Recusar um manifesto de outro repositório.

**Teste:** `adopting_inside_another_install_is_refused`.

### F19 — Pacotes do Windows conferidos só em 64 bits · **Baixa**

**Onde:** `runtimes.rs:32-54` (VC só x64; DirectX só `System32`).

**Reprodução:** um jogo de 32 bits num PC sem as DLLs x86 em `SysWOW64`. "Seu PC" mostra tudo em ordem.

**Correção:**

- Conferir também `VC\Runtimes\x86` e `SysWOW64`.
- Oferecer `vc_redist.x86.exe` quando o exe do jogo é de 32 bits (dá para reaproveitar `looks::is_64bit`).

**Teste:** `x86_parts_are_checked_on_64bit_windows`.

### F20 — Chave compartilhada apagada antes de gravar a nova · **Baixa**

**Onde:** `manifest.rs:97-110` (`remove_dir_all` e depois `write`).

**Reprodução:** uma falha de gravação (antivírus, disco) depois do `remove_dir_all` deixa todos os jogos sem a chave.
O arquivo original do jogador continua onde estava.

**Correção:** gravar em `<kind>.new` e trocar com rename.

**Teste:** `a_failed_key_write_keeps_the_old_key`.

### F21 — Erros engolidos ao devolver cópias e preparar a pasta de dados · **Baixa**

**Onde:**

- `mod.rs:836` (`.ok()`) e `:843` (`let _ = restore_copy`), que rodam quando o Cerberus abre;
- `mod.rs:397` (`let _ = create_dir_all(recompdata)`);
- `main.rs:2213` (`if let Ok(...) = use_game_copy` depois de instalar).

**Reprodução:** a cópia original mudou de lugar ou o disco está cheio. Ninguém fica sabendo, e o jogo pede a cópia de
novo sem explicar.

**Correção:** `trail::note` com a causa em todos esses pontos, mais um toast no Easy Setup logo após instalar.

**Teste:** `restore_copy_failures_reach_the_history`.

### F22 — Download do Thunderstore sem limite quando não há `Content-Length` · **Baixa**

**Onde:** `mods.rs:266-279`. O limite de 512 MB só vale com o cabeçalho, e `bytes()` guarda tudo em memória.

**Correção:** streaming com contagem, como em `gamebanana::download`.

**Teste:** um servidor de teste sem `Content-Length` com 600 MB devolve erro.

### Contagem

| Gravidade | Quantas | Ids |
| --- | --- | --- |
| Crítica | 1 | F1 |
| Alta | 4 | F2, F3, F4, F5 |
| Média | 11 | F6–F16 |
| Baixa | 6 | F17–F22 |

---

## 4. Lacunas (não são bugs, mas faltam para ficar completo)

- **Testes:**
  - nenhum teste de interrupção para cópia ou extração (F3), DLSS (F9), mods (F10), preparação (F12) ou ajustes com
    arquivo ilegível (F1);
  - `setup_runner` só testa códigos e nomes;
  - nada cobre `prepare_then_start` nem `take_back` com DLL presa.
- **Desfazer ajustes:** "Voltar ao original" para os arquivos de configuração (F1) e um histórico do que o Cerberus
  mudou na pasta do jogo (a tabela 1.10 como tela).
- **Mods:** ligar/desligar sem apagar, conflitos visíveis, ordem de carga e "o mod X quebrou o jogo" (já em 6.7).
- **Desinstalar um jogo instalado por instalador externo:** a pasta fora de `install_dir` e o registro ficam. Falta
  rodar o `UninstallString` ou `unins*.exe`, como o RL (`installer.rs:378`).
- **DLL velha na pasta do jogo:** renomear para `.old` em vez de só avisar (o RL faz; está registrado em 6.7
  "Resolver problemas").
- **Verificação com cache** `(path, size, mtime, crc)`, para o selo "verificado" não exigir uma nova leitura depois de
  reinstalar.
- **ReShade em Vulkan e OpenGL** (camada por env, como o RL). Hoje é só uma mensagem.
- **Ações antes e depois de jogar** (Playnite: Pre/Post/Exit com `CancelStartup`). Já está em 6.4/7.4. As ações do
  manifesto hoje só abrem ou esvaziam pastas.
- **Não verificado:**
  - se `sevenz_rust2::decompress_file` recusa mesmo caminhos `..` e links (o comentário em `install.rs:347` afirma);
  - se `RESHADE_BASE_PATH_OVERRIDE` funciona com acento no perfil do usuário;
  - o comportamento do OneDrive (arquivos "só online") em `copy_whole` e no CRC.

## 5. Ordem sugerida

1. **F1 e F2** (perdem dados do jogador; regra da Onda 8.0): leitura com `Result`, backup antes da primeira mudança,
   gravação atômica no Hedge. — P
2. **F3 + F14** juntos: staging, espaço conferido, progresso e cancelamento para a cópia e a extração. — M
3. **F9, F10 e F20** (o mesmo padrão `.part`/rename, pequenos). — P
4. **F5 e F6** (instalador com UAC; localizar com confirmação, que fecha o item 3 da tabela de prioridade). — M
5. **F4 e F15** (CD: folha com faixas e conversão honesta). — M
6. **F7, F11, F12 e F16** (UI que mente e efeitos colaterais). — P/M
7. **F8** (ReShade e runtimes com hash fixo e Authenticode). — P
8. **F13, F17, F18, F19, F21 e F22**. — P
