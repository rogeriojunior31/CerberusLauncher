# Emulação e saves dos emuladores: o que falta para ficar completo

Revisão de 25/11/2026 das entregas de 24 e 25/11 (backup dos saves dos emuladores, Xenia e xemu, clássicos com
selo de console, fotos dos consoles). Só leitura: nada foi compilado nem aberto. Complementa a **6.5** do
`ROADMAP.md` sem repetir o que já está lá (CRC/No-Intro, buscas por pasta, escolher disco, instalar e atualizar
emuladores de forma geral, troféus do RPCS3/shadPS4, cartões de PS1/PS2 por save). Quando um item abaixo
aprofunda um da 6.5, está dito.

Licenças: **Hydra MIT** e **Playnite MIT** podem ser adaptados com crédito; RecompLauncher e Heroic (GPL) só
ideias. Nunca baixar BIOS, firmware, chaves (`prod.keys`, `keys.bin`) nem jogos.

Legenda: esforço **P** (até um dia), **M** (alguns dias), **G** (uma semana ou mais). "Não verificado" = não
confirmei no código do emulador nem na documentação oficial.

---

## Estado atual

**Backup dos saves dos emuladores** (`src-tauri/src/emulator_saves.rs`)

- Cobre **RetroArch, DuckStation, PCSX2, Dolphin e PPSSPP**. Para cada um, `places()` devolve uma raiz e as pastas
  copiadas inteiras:
  - RetroArch: `savefile_directory` e `savestate_directory` do `retroarch.cfg` (os estados só entram se estiverem
    ao lado da pasta de saves);
  - DuckStation: `memcards`, `savestates`;
  - PCSX2: `memcards`, `sstates`;
  - Dolphin: `GC`, `StateSaves`;
  - PPSSPP: `SAVEDATA`, `PPSSPP_STATE`.
- O modo portátil é reconhecido por `portable.txt`/`portable.ini` (DuckStation, PCSX2), `portable.txt` → `User`
  (Dolphin) e `memstick` ao lado do exe (PPSSPP). Fora disso: Documentos (inclusive dentro do OneDrive) ou AppData.
- Usa o mesmo motor dos recomps (`recomp/saves.rs`): zip com manifesto, impressão digital SHA-256 para não repetir
  cópia igual, **10 cópias** por emulador (`KEEP`), arquivos acima de 64 MB ignorados (`MAX_FILE`), extensões e
  pastas que nunca são save (`NOT_SAVES`, `NOT_SAVE_DIRS`).
- Backup automático "Depois de jogar" quando um clássico fecha (`main.rs`, perto da linha 679) e backup manual
  (`backup_emulator_saves`).
- Restaurar (`restore_emulator_backup`) primeiro faz um backup "Antes de restaurar" e grava tudo por arquivos
  temporários antes de trocar o primeiro save. Recusa se `background::classic_running()` (só conta clássicos abertos
  **pelo Cerberus**).
- Restaurar **mescla**: arquivos criados depois do backup continuam lá (não são apagados).
- Ficam em `<dados>/backups/emulator-<id>`. **Sem nuvem** (passa `cloud: None`).
- UI: `ui/src/pages.rs` (`EmulatorSaves`) mostra um cartão por emulador encontrado, com o componente
  `ui/src/backups.rs` (lista, "Fazer backup agora", "Abrir pasta", restaurar com segundo clique).
- Testes: 2 (DuckStation portátil ida e volta; pastas do RetroArch).

**Emuladores standalone** (`src-tauri/src/standalone.rs`)

- DuckStation, PCSX2, RPCS3, PPSSPP, Dolphin, Cemu, **Xenia Canary** (`xenia-canary/xenia-canary-releases`) e
  **xemu** (`xemu-project/xemu`), cada um da release oficial. Instalação gerenciada em `<dados>/emulators/<id>`, ou
  encontrado em Arquivos de Programas, ou escolhido pelo jogador ("Já tenho").
- `install()` cria `portable.txt` para DuckStation, PCSX2 e Dolphin.
- BIOS: só PS1/PS2 (por tamanho) e firmware do PS3 pelo próprio RPCS3 (`--installfw`). Xbox (xemu) não tem
  verificação no Cerberus: o próprio xemu pede os arquivos ao abrir.
- RetroAchievements levado para RetroArch, DuckStation, PCSX2, Dolphin e PPSSPP (`cheevos.rs`). Xenia, xemu, Cemu e
  RPCS3 não têm.
- Não existe **atualizar** emulador standalone: o botão "Instalar" só aparece quando ele não está instalado
  (`settings_page.rs`, perto da linha 1300).

**Clássicos** (`src-tauri/src/roms.rs`, `ui/src/classics.rs`)

- Cartuchos por extensão, discos por conteúdo (`disc_console.rs`, `xdvdfs.rs` para Xbox/360) e pelo nome da pasta.
- Selo na capa com emulador e sigla (`console_short`).

**Imagens dos consoles** (`ui/emulation/*.png`, `console_art` em `ui/src/settings_page.rs`)

- 20 imagens: PS1, PS2, PS3, PSP, GameCube e Wii do Hydra; NES, SNES, N64, GB, GBC, GBA, Mega Drive, Master System,
  Game Gear, DS, 3DS, Xbox e Xbox 360 com fotos de domínio público do Evan Amos; Wii U ainda desenho.

---

## Lacunas para ficar completo

### L1. Remover ou reinstalar um emulador gerenciado apaga os saves dele — **crítico**

- **Problema**: `remove_standalone` (`main.rs:877`) faz `remove_dir_all` na pasta gerenciada, e
  `standalone::install` (`standalone.rs:541`) apaga o destino antes de pôr a versão nova. Nas cópias que o próprio
  Cerberus instala, os saves ficam **dentro dessa pasta**: DuckStation, PCSX2 e Dolphin (o Cerberus cria
  `portable.txt`), PPSSPP (`memstick`), RPCS3 (`dev_hdd0`, sempre ao lado do exe), Xenia Canary (portátil por padrão
  no Windows, `content/`, confirmado no `xenia_main.cc`) e as BIOS que o jogador importou. O botão "Remover" só pede
  um segundo clique, sem avisar que os saves vão junto. Hoje a reinstalação só acontece se o exe sumiu (antivírus,
  pasta mexida), mas qualquer "Atualizar" futuro vai cair aqui.
- **Por que importa**: é perda de progresso silenciosa, e o backup só protege 4 dos 8 emuladores (RPCS3, Cemu,
  Xenia e xemu não têm backup nenhum).
- **Como resolver**: generalizar o `swap_keeping_player_files` do RetroArch (`emulation.rs:628`, que já faz troca
  atômica e volta atrás se der erro) para os standalone, com uma lista por emulador do que fica: `portable.txt`,
  `memcards`, `sstates`/`savestates`, `bios`, `inis`, `settings.ini`, `User` (Dolphin), `memstick` (PPSSPP),
  `dev_hdd0`, `dev_flash`, `config` (RPCS3), `mlc01`, `settings.xml`, `portable` (Cemu), `content`,
  `xenia-canary.config.toml` (Xenia), `xemu.toml` (xemu). Em "Remover": backup antes (quando coberto) e uma escolha
  clara, "Remover e manter saves e configurações" (padrão, move para `<dados>/emulators/.<id>-kept`) ou "Apagar tudo".
- **Arquivos**: `src-tauri/src/standalone.rs`, `src-tauri/src/emulation.rs` (extrair a troca para uma função comum),
  `src-tauri/src/main.rs` (`remove_standalone`, `install_standalone`), `ui/src/settings_page.rs` (texto do botão).
- **Critério de aceite**: com um memory card em `<dados>/emulators/duckstation/memcards`, reinstalar o DuckStation
  mantém o arquivo com o mesmo hash; "Remover" mostra o aviso e, no padrão, a pasta de saves continua existindo; uma
  falha no meio da troca deixa a instalação anterior como estava.
- **Teste sugerido**: `a_reinstall_keeps_each_emulators_player_files` (tempdir, um diretório falso por emulador,
  `swap` com pasta nova sem esses arquivos) e `a_failed_swap_puts_the_old_copy_back`.
- **Esforço**: M.

### L2. O backup do Dolphin leva a BIOS do GameCube e deixa de fora os saves do Wii

- **Problema**: a pasta `User/GC` inteira entra no backup. É nela que o jogador põe `GC/<região>/IPL.bin` e
  `GC/dsp_rom.bin`/`dsp_coef.bin` (dumps do console). Os saves de Wii (`User/Wii/title/00010000/<id>/data` e
  `00010004`) não entram. Os estados de Wii passam fácil de 64 MB e são ignorados sem aviso.
- **Por que importa**: BIOS dentro do backup é ruim hoje e proibitiva quando houver nuvem (L7). E quem joga Wii
  acha que tem backup, mas não tem.
- **Como resolver**: lista de "nunca copiar" por emulador (`IPL.bin`, `dsp_*.bin`, `font_*.bin`, `*.nvm`/`*.mec`
  do PCSX2, `keys.bin`, `otp.bin`, `seeprom.bin`, `dev_flash`) aplicada antes de `NOT_SAVES`. Para o Wii, usar o
  formato de padrão de `saves.rs` (`split_pattern`) e copiar só `Wii/title/00010000` e `Wii/title/00010004` (a
  NAND do sistema fica fora). Quando um arquivo passar de `MAX_FILE`, contar e mostrar ("2 estados grandes demais
  ficaram de fora").
- **Arquivos**: `src-tauri/src/emulator_saves.rs`, `src-tauri/src/recomp/saves.rs` (parâmetro de exclusão), UI
  `ui/src/backups.rs`.
- **Critério de aceite**: um backup do Dolphin com `GC/USA/IPL.bin` e um save em `Wii/title/00010000/52534245/data`
  contém o save do Wii e não contém o `IPL.bin`.
- **Teste sugerido**: `dolphin_backup_skips_the_console_files_and_takes_wii_saves`.
- **Esforço**: P.

### L3. Restaurar com o emulador aberto

- **Problema**: a trava só olha clássicos abertos pelo Cerberus. Não pega o emulador aberto por "Abrir emulador"
  (`open_standalone`, `main.rs:895`), pelo atalho do Windows nem pelo próprio jogador. Com o emulador aberto, ele
  regrava o memory card que tem na memória ao fechar (DuckStation e PCSX2 fazem isso com cartões compartilhados) e
  desfaz a restauração. Não verificado em cada emulador, mas é o comportamento esperado de quem mantém o cartão aberto.
- **Como resolver**: antes de restaurar (e antes de L1), procurar processo cujo caminho seja o exe localizado
  (`sysinfo`, ou a mesma checagem usada pelas sessões) e recusar com "Feche o DuckStation antes de restaurar".
  Na UI, desabilitar "Restaurar" enquanto `classic_running` estiver ativo e dizer por quê.
- **Arquivos**: `src-tauri/src/main.rs` (`restore_emulator_backup`), `src-tauri/src/background.rs`,
  `ui/src/backups.rs`.
- **Critério de aceite**: com o DuckStation aberto fora do Cerberus, restaurar devolve o erro e nenhum arquivo muda
  (mtime igual).
- **Teste sugerido**: função pura `running_from(exe, processos)` testada com uma lista falsa de processos.
- **Esforço**: P.

### L4. RPCS3, Cemu e Xenia sem backup (e o card some sem explicar)

- **Problema**: `places()` devolve `None` para eles; `emulator_backups` usa `filter_map` e o emulador simplesmente
  não aparece em Saves. O jogador não sabe se está coberto.
- **Como resolver** (pastas na tabela abaixo):
  - RPCS3: `dev_hdd0/home/00000001/savedata` e `savestates` (estados em `<exe>/savestates`: não verificado). Nunca
    `dev_flash`. O Hydra (MIT) tem leitura por título em `cloud-save/rpcs3-save-layout.ts` e
    `rpcs3-config-root.ts`, que também resolve uma `dev_hdd0` mudada no `config/vfs.yml`.
  - Cemu: `mlc01/usr/save`, com o `mlc_path` do `settings.xml` (portátil com pasta `portable` ao lado do exe; senão
    `%APPDATA%\Cemu`: não verificado).
  - Xenia Canary: `content/<perfil>/<TitleID>/00000001` (saves). Ficar fora de `content/0000000000000000/<TitleID>/
    000B0000` (atualizações) e `00000002` (DLC). Raiz: pasta do exe por padrão no Windows (confirmado), ou a que
    `storage_root`/`content_root` dizem no `xenia-canary.config.toml`.
  - No cartão de cada emulador sem suporte, uma linha: "O Cerberus ainda não copia os saves do X".
- **Arquivos**: `src-tauri/src/emulator_saves.rs`, `src-tauri/src/main.rs` (`emulator_backups` devolve também os
  não cobertos), `shared/src/lib.rs` (`EmulatorBackups { supported: bool }`), `ui/src/pages.rs`.
- **Critério de aceite**: em uma instalação portátil de cada um, um backup contém os saves e não contém
  atualizações, DLC ou firmware.
- **Teste sugerido**: um teste de `places()` por emulador com tempdir, mais
  `xenia_backup_leaves_title_updates_and_dlc_out`.
- **Esforço**: M.

### L5. xemu: arquivos do console, HD e saves

- **Problema**: o save do Xbox fica **dentro da imagem do HD** (partição E, segundo o FAQ do xemu), um `.qcow2` de
  centenas de MB a GB, que não passa pelo limite de 64 MB. O Cerberus também não diz ao jogador o que falta (MCPX boot
  ROM, flash ROM, imagem do HD; a documentação oficial diz que a BIOS de varejo sem modificação não roda jogos).
- **Como resolver**:
  1. Ler `xemu.toml` (portátil se estiver ao lado do exe, confirmado no FAQ; senão `%APPDATA%\xemu\xemu`: não
     verificado) e mostrar na página do console o estado de `bootrom_path`, `flashrom_path`, `hdd_path` e
     `eeprom_path` (nomes das chaves em `[sys.files]`: não verificado). Botões "Escolher arquivo" que só gravam o
     caminho no `xemu.toml`, como o `import_bios` faz para PS1/PS2. Nunca baixar.
  2. A imagem de HD pré-montada do xemu-dashboard (dashboard aberto, sem arquivos da Microsoft) pode ser oferecida
     por link para a página oficial, não baixada pelo Cerberus, até a licença estar confirmada (não verificado).
  3. Backup do HD como item separado: só manual, com aviso do tamanho, fora do rodízio de 10, fora da nuvem.
     Exportar saves individuais exigiria ler FATX: deixar para depois.
- **Arquivos**: `src-tauri/src/standalone.rs` (`Needs::XboxFiles`), `src-tauri/src/emulator_saves.rs`,
  `ui/src/settings_page.rs`.
- **Critério de aceite**: com `xemu.toml` sem `bootrom_path`, a página do Xbox diz "Falta o MCPX boot ROM do seu
  console" e, depois de escolher o arquivo, o `xemu.toml` tem o caminho e o resto do arquivo continua igual.
- **Teste sugerido**: `xemu_files_are_read_and_written_without_touching_other_keys` (toml de exemplo).
- **Esforço**: M (sem backup do HD) / G (com saves individuais via FATX).

### L6. Três lugares decidem onde fica a pasta do emulador, e discordam

- **Problema**: `emulator_saves::user_dir`, `cheevos::prepare` e `standalone::config_files`/`bios_dir` repetem a
  mesma lógica de formas diferentes:
  - **Dolphin**: `emulator_saves` prefere Documentos e depois AppData; `cheevos` prefere AppData. O código do
    Dolphin (`UICommon::SetUserDirectory`) usa, nesta ordem: `portable.txt`, os valores `LocalUserConfig` e
    `UserConfigPath` no registro, `Documentos\Dolphin Emulator` se existir, e por último AppData. Ou seja,
    `cheevos.rs` erra quando as duas pastas existem, e nenhum dos três lê o registro.
  - **DuckStation**: `emulator_saves` considera `%LOCALAPPDATA%\DuckStation`; `cheevos` e `bios_dir` só
    Documentos. Qual versão passou a usar LOCALAPPDATA: não verificado.
  - Nenhum lê as pastas que o jogador mudou: `[Folders] MemoryCards` do PCSX2 (o Hydra lê, em
    `emulators/ps2-memcard-dirs.ts`), `[MemoryCards] Directory` do DuckStation (não verificado), `installed.txt`
    do PPSSPP, `mlc_path` do Cemu.
- **Como resolver**: um módulo `emulator_paths.rs` com `user_dir(id, exe)`, `memcards_dir`, `states_dir` e
  `config_file`, usado pelos três. Cada regra com o link do código do emulador no comentário.
- **Arquivos**: novo `src-tauri/src/emulator_paths.rs`; `emulator_saves.rs`, `cheevos.rs`, `standalone.rs`.
- **Critério de aceite**: com `Documentos\Dolphin Emulator` e `%APPDATA%\Dolphin Emulator` existindo, backup,
  conquistas e pastas de jogos usam a mesma (Documentos). Com `MemoryCards=D:\Cartoes` no PCSX2.ini, o backup
  copia `D:\Cartoes`.
- **Teste sugerido**: tabela de casos por emulador (portátil, Documentos, AppData, pasta mudada no ini).
- **Esforço**: M.

### L7. Rodízio por quantidade, sem limite de espaço, e sem nuvem

- **Problema**: 10 cópias da pasta inteira. Com estados do PCSX2 (dezenas de MB cada) ou do RetroArch, isso pode
  chegar a GB sem aviso. A impressão digital lê todos os arquivos inteiros depois de cada partida. E os recomps têm
  nuvem por pasta, mas os emuladores não (`cloud: None`).
- **Como resolver**:
  1. Separar "saves" (memory cards, `.srm`, SAVEDATA) de "estados": estados opcionais (desligados por padrão para
     PS2/GC/Wii/PS3) e com rodízio próprio menor (3).
  2. Limite de espaço por emulador (ex.: 1 GB) que apaga os mais antigos primeiro, e mostrar o total ocupado no
     cartão ("10 backups · 340 MB").
  3. Pular a releitura quando tamanho e mtime de todos os arquivos forem iguais aos do último manifesto.
  4. Nuvem: reaproveitar `recomp::cloud::queue` com a pasta da nuvem do jogador, **depois** de L2 (nunca subir
     BIOS). Só saves, sem estados. Como o pacote é a pasta inteira, dois PCs jogando jogos diferentes vão conflitar:
     resolver com "mais recente vence + cópia do outro guardada" até L8 existir.
- **Arquivos**: `src-tauri/src/recomp/saves.rs` (`prune` por bytes), `src-tauri/src/emulator_saves.rs`,
  `src-tauri/src/settings.rs`, `ui/src/backups.rs`, `ui/src/pages.rs`.
- **Critério de aceite**: com limite de 100 MB e backups de 30 MB, nunca há mais de 3; o cartão mostra o total; com
  a nuvem ligada, nenhum arquivo da lista "nunca copiar" chega à pasta da nuvem.
- **Teste sugerido**: `prune_keeps_under_the_byte_limit`, `unchanged_files_skip_the_hash`,
  `cloud_copy_never_has_console_files`.
- **Esforço**: M.

### L8. Saves por jogo, não só a pasta inteira (aprofunda "Cartões de memória de PS1 e PS2" da 6.5)

- **Problema**: restaurar devolve **todos** os jogos daquele emulador ao estado do backup. Quem quer voltar o save
  de um jogo perde o progresso dos outros. A mescla também deixa estados novos que não batem com o save restaurado.
- **Como resolver**, do mais barato ao mais caro:
  - RetroArch: `.srm`/`.state*` com o nome do arquivo da ROM. É só filtrar pelo nome.
  - PPSSPP: `SAVEDATA/<DISC_ID>*`; o Hydra (MIT) lê o ID do disco (`emulation-file-saves.ts`, `PSP_DISC_ID_RE`).
  - Dolphin GC (`.gci` por pasta) e Wii (`title/00010000/<hex do game id>`): o Hydra já mapeia (`DOLPHIN_GAME_ID_RE`).
  - DuckStation com cartão por jogo (`<serial>_1.mcd`): por nome. Cartão compartilhado e PCSX2 (`Mcd001.ps2`)
    exigem ler o cartão: o Hydra (MIT) tem leitor e escritor (`emulators/ps1-memory-card`, `ps2-memory-card`, com ECC).
  - RPCS3: `savedata/<TitleID>*` (Hydra `rpcs3-save-layout.ts`).
  - Na UI do clássico (`classics.rs`), uma aba "Saves" com os backups só daquele jogo e "Restaurar só este jogo".
- **Arquivos**: `src-tauri/src/emulator_saves.rs` (novo `saves_of_game`), `src-tauri/src/roms.rs` (serial/ID do
  disco), `ui/src/classics.rs`, `ui/src/backups.rs`.
- **Critério de aceite**: com dois jogos de RetroArch salvos, restaurar só um muda apenas os arquivos dele.
- **Teste sugerido**: `restoring_one_game_leaves_the_others` (RetroArch e PPSSPP primeiro).
- **Esforço**: M para RetroArch/PPSSPP/Dolphin/RPCS3, G para os cartões de PS1/PS2.

### L9. Atualizar Xenia, xemu e os outros standalone (aprofunda "instalar e atualizar" da 6.5)

- **Problema**: não há "Atualizar" para emulador standalone. O Xenia Canary muda quase todo dia e a compatibilidade
  depende disso. Nada guarda a versão instalada.
- **Como resolver**: gravar `version.json` (tag e nome do arquivo) ao instalar, comparar com `latest_asset` em
  segundo plano (com a mesma espera econômica da sincronização), mostrar "Atualizar" e usar a troca de L1. O Hydra
  tem `emulator-version-probe.ts` e `get-emulator-version.ts` para ler a versão de cópias que não são dele. Para o
  Xenia, oferecer "Fixar esta versão" (um build novo pode quebrar um jogo).
- **Arquivos**: `src-tauri/src/standalone.rs`, `src-tauri/src/main.rs`, `shared/src/lib.rs`
  (`StandaloneView { version, latest }`), `ui/src/settings_page.rs`.
- **Critério de aceite**: com `version.json` antigo, o cartão mostra "Atualizar"; depois de atualizar, os saves e a
  configuração continuam (L1) e a versão nova aparece.
- **Teste sugerido**: `an_older_installed_tag_offers_an_update` (release JSON de exemplo) e o
  `#[ignore]` de rede que já existe estendido para conferir os nomes de arquivo de Xenia e xemu.
- **Esforço**: M (depende de L1).

### L10. Xenia e xemu: formatos, configuração, controles e conquistas

- **Formatos**: o Playnite (`Emulation/Emulators/Xenia/emulator.yaml`) aceita para o 360 `iso`, `xex`, `zar` e
  pastas GOD/STFS (sem extensão); o Cerberus só `iso`/`xex` (`roms.rs`, `DISC`). Para o xemu, o Playnite aceita
  `xiso`. Adicionar `zar` e `xiso` e reconhecer a pasta GOD (`<TitleID>/00007000/<arquivo sem extensão>`), abrindo
  pelo arquivo de cabeçalho. Esforço: M.
- **Configuração**: atalho "Abrir configuração" para `xenia-canary.config.toml` e para o menu do xemu, e no mínimo
  escolher o perfil/idioma do Xenia (`user_language`: não verificado). Esforço: P.
- **Controles**: Xenia usa XInput (DualSense e controle de Switch só via Steam Input/DS4Windows: não verificado);
  xemu usa SDL e associa os controles na tela dele. Dizer isso na página do console. Esforço: P.
- **Conquistas**: RetroAchievements não existe para Xbox/360. O Xenia Canary tem conquistas próprias, gravadas no
  perfil (GPD) e escritas no log (formato não verificado). Dá para ler como o `cheevos.rs` lê os outros e guardar no
  `achievements.db` como plataforma "Xbox 360 (Xenia)". Esforço: M, depois de confirmar o log.
- **Ícones**: `emulator_icon` não tem Cemu, Xenia e xemu (aparece a inicial). Esforço: P; conferir a licença dos
  logotipos antes (projetos GPL/BSD; logotipo é marca, não código).

### L11. Uma tabela só de consoles (nomes, siglas, imagens)

- **Problema**: existem quatro tabelas que não batem:
  - `emulation::CORES` (nomes);
  - `roms::console_name`: sem PlayStation, Xbox, GameCube e Wii, então a mensagem de `recomp/mod.rs:910` pode
    mostrar "ps2";
  - `classics::console_name`: Sega CD, PC Engine, 3DO, CD-i e PS4 caem em "Console";
  - `settings_page::console_name`, mais duas listas `DISC_CONSOLES` diferentes (`roms.rs` com 16, `settings_page.rs`
    e `main.rs:800` com 9).
  Nas siglas (`console_short`), uns são palavra ("Wii U", "Xbox") e outros caixa alta crua: "SWITCH", "SATURN",
  "SEGACD", "PCE", e "360" sozinho. DS, 3DS, 32X e Switch são reconhecidos e têm imagem (DS/3DS), mas não aparecem na
  página Emulação (`CART_CONSOLES` não tem esses quatro), então o jogador não vê por que "não abre".
- **Como resolver**: `shared/src/lib.rs` com `pub struct ConsoleInfo { id, name, short, disc: bool, art }` e uma
  `CONSOLES` só, usada pelo backend e pela UI. Siglas fixas: PS1, PS2, PS3, PS4, PSP, GC, Wii, Wii U, NES, SNES, N64,
  GB, GBC, GBA, DS, 3DS, Switch, MD, 32X, SMS, GG, Sega CD, Saturn, DC, PCE, 3DO, CD-i, Xbox, X360. Consoles sem
  emulador também com cartão em Emulação ("Ainda sem emulador no Cerberus").
- **Arquivos**: `shared/src/lib.rs`, `src-tauri/src/roms.rs`, `src-tauri/src/emulation.rs`, `src-tauri/src/main.rs`,
  `ui/src/classics.rs`, `ui/src/settings_page.rs`.
- **Critério de aceite**: um teste percorre todo id que `roms.rs`/`disc_console.rs` pode devolver e exige nome, sigla
  de até 6 caracteres e (opcional) imagem.
- **Teste sugerido**: `every_console_the_scan_returns_has_a_name_and_a_short_mark`.
- **Esforço**: M.

### L12. Imagens que faltam e a licença das que existem

- Faltam: Wii U (ainda desenho), 32X, Dreamcast, Saturn, Switch, Sega CD, PC Engine, 3DO, CD-i e PS4. Candidatas na
  tabela abaixo.
- `ui/emulation/LICENSE.md` se contradiz: diz que NES, SNES, N64, GB, GBC, GBA, Mega Drive, Master System e Game Gear
  são "desenho próprio" e, mais abaixo, que são fotos do Evan Amos. Tirar o parágrafo velho.
- As imagens do Hydra (PS1, PS2, PS3, PSP, GC, Wii) vêm sob MIT do repositório, mas a origem delas não é dita. Para
  um conjunto uniforme e com licença por arquivo, trocar também por fotos do Evan Amos (nomes não verificados).
- Registrar no `LICENSE.md`, por arquivo: link, autor, licença e data da conferência. Evitar CC BY-SA quando existir
  alternativa em domínio público. Se usar CC BY-SA, a atribuição tem de aparecer no app (tela Sobre) e a imagem
  modificada sai sob CC BY-SA.
- **Critério de aceite**: `console_art` devolve imagem para todo console da L11; cada arquivo tem linha no
  `LICENSE.md` com licença. Esforço: P.

### L13. Estados da UI e testes que faltam

- `EmulatorSaves` (`pages.rs:461`): `unwrap_or_default()` esconde erro e carregamento; sem nenhum emulador, a seção
  some sem explicar. Precisa de "Carregando...", erro com "Tentar de novo" e "Nenhum emulador encontrado: instale em
  Configurações › Emulação".
- O texto do cartão diz "Memory cards e estados salvos de {pasta}" mesmo quando os estados não entram (RetroArch com
  `states` em outro lugar). Mostrar as pastas de verdade (`folders`).
- O toast "Nada novo para copiar" fala em "o jogo" no cartão de um emulador.
- Mostrar no cartão o tamanho total e a data do último backup automático (o resultado já vai para `trail`).
- Um backup feito numa raiz (portátil) e restaurado em outra (instalado) volta sem aviso: guardar a raiz no manifesto
  e avisar quando for diferente.
- Testes que faltam em `emulator_saves.rs`: PCSX2 instalado e portátil, Dolphin (portátil, Documentos e AppData),
  PPSSPP com e sem `memstick`, RetroArch com estados fora da pasta irmã e com `savefile_directory` absoluto,
  emulador sem suporte devolvendo erro, restauração que mescla (o arquivo criado depois continua),
  `open_backup` com nome de outro emulador. Em `standalone.rs`: reinstalar mantém saves (L1), `remove_standalone`
  com e sem pasta escolhida.
- Esforço: P (UI) + P (testes).

---

## Emulador → onde ficam os saves → coberto hoje?

| Emulador | Onde ficam (Windows) | Coberto hoje? |
|---|---|---|
| RetroArch | `savefile_directory` (`saves/`, `.srm` por jogo) e `savestate_directory` (`states/`) do `retroarch.cfg`; `default` = ao lado do exe (o build do Windows; não verificado) | **Sim**. Estados só se estiverem ao lado de `saves` |
| DuckStation | `memcards/`, `savestates/` na pasta do exe (`portable.txt`) ou `Documentos\DuckStation`; `%LOCALAPPDATA%\DuckStation` em versões novas (não verificado); pasta de cartões mudável no ini (não verificado) | **Sim** (sem pasta mudada) |
| PCSX2 | `memcards/`, `sstates/` na pasta do exe (`portable.ini`/`portable.txt`) ou `Documentos\PCSX2`; `[Folders] MemoryCards` no `PCSX2.ini` (Hydra lê) | **Sim** (sem pasta mudada) |
| Dolphin (GC) | `User/GC/<região>/Card A` (`.raw` ou pastas GCI) e `User/StateSaves`; raiz: `portable.txt` → registro → `Documentos\Dolphin Emulator` → `%APPDATA%\Dolphin Emulator` (confirmado no código) | **Parcial**: copia também `IPL.bin`/DSP (BIOS); não lê o registro |
| Dolphin (Wii) | `User/Wii/title/00010000/<id>/data` (e `00010004`) | **Não** |
| PPSSPP | `memstick/PSP/SAVEDATA` e `PPSSPP_STATE` ao lado do exe, ou `Documentos\PPSSPP\PSP`; `installed.txt` pode apontar outra pasta (não verificado) | **Sim** |
| RPCS3 | `<exe>/dev_hdd0/home/00000001/savedata` (e `trophy`); estados em `<exe>/savestates` (não verificado); `dev_hdd0` mudável em `config/vfs.yml` | **Não** |
| Cemu | `mlc01/usr/save/<id alto>/<id baixo>/user`; `mlc_path` no `settings.xml`; portátil com pasta `portable` ou `%APPDATA%\Cemu` (não verificado) | **Não** |
| Xenia Canary | `<storage_root>/content/<perfil>/<TitleID>/00000001`; `storage_root` = pasta do exe por padrão no Windows (confirmado no `xenia_main.cc`); `content_root` mudável no toml | **Não** |
| xemu | Dentro da imagem do HD (partição E, FAQ do xemu), caminho em `xemu.toml` (portátil ao lado do exe, confirmado; senão `%APPDATA%\xemu\xemu`, não verificado) | **Não** (arquivo grande demais para o motor atual) |
| shadPS4 (não integrado) | `user/savedata` ao lado do exe (não verificado; Hydra `shadps4-save-layout.ts`) | n/a |

## Console → imagem → fonte livre candidata

Licenças conferidas na API do Wikimedia Commons (`extmetadata`, `LicenseShortName`) em 25/11/2026. Conferir de
novo na página do arquivo antes de baixar.

| Console | Imagem hoje | Candidata | Licença |
|---|---|---|---|
| Wii U | desenho próprio | [Nintendo-Wii-U-Console-FL.jpg](https://commons.wikimedia.org/wiki/File:Nintendo-Wii-U-Console-FL.jpg) (Evan Amos) | Domínio público |
| 32X | nenhuma | [Sega-Genesis-Model2-32X.jpg](https://commons.wikimedia.org/wiki/File:Sega-Genesis-Model2-32X.jpg) (Evan Amos) | Domínio público |
| Dreamcast | nenhuma | [Sega-Dreamcast-Console-FL.jpg](https://commons.wikimedia.org/wiki/File:Sega-Dreamcast-Console-FL.jpg) (Evan Amos). Evitar `Dreamcast-Console-Set.png`, que é CC BY-SA 3.0 | Domínio público |
| Saturn | nenhuma | [Sega-Saturn-Console-Set-Mk1.png](https://commons.wikimedia.org/wiki/File:Sega-Saturn-Console-Set-Mk1.png) (Evan Amos) | Domínio público |
| Switch | nenhuma | [Nintendo-Switch-wJoyCons-BlRd-Standing-FL.jpg](https://commons.wikimedia.org/wiki/File:Nintendo-Switch-wJoyCons-BlRd-Standing-FL.jpg) (Evan Amos) | Domínio público |
| Sega CD | nenhuma | [Sega-CD-Model1-Set.jpg](https://commons.wikimedia.org/wiki/File:Sega-CD-Model1-Set.jpg) (Evan Amos) | Domínio público |
| PC Engine / TurboGrafx-16 | nenhuma | [TurboGrafx16-Console-Set.png](https://commons.wikimedia.org/wiki/File:TurboGrafx16-Console-Set.png) (Evan Amos; modelo americano) | Domínio público |
| PS4 | nenhuma | [PS4-Console-wDS4.png](https://commons.wikimedia.org/wiki/File:PS4-Console-wDS4.png) (Evan Amos) | Domínio público |
| 3DO | nenhuma | [3DO-FZ1-Console-Set.png](https://commons.wikimedia.org/wiki/File:3DO-FZ1-Console-Set.png) (Evan Amos). Procurar alternativa em domínio público | CC BY-SA 3.0 |
| CD-i | nenhuma | não encontrada (não verificado) | — |
| Xbox 360 | foto | [Xbox-360-S-and-E.png](https://commons.wikimedia.org/wiki/File:Xbox-360-S-and-E.png) (já em uso) | Domínio público (confirmado) |
| PS1, PS2, PS3, PSP, GC, Wii | Hydra (MIT, origem não dita) | fotos do Evan Amos nas categorias de cada console no Commons (nomes não verificados) | a conferir por arquivo |
| NES, SNES, N64, GB, GBC, GBA, MD, SMS, GG, DS, 3DS, Xbox | foto do Evan Amos | já em uso (ver `LICENSE.md`) | Domínio público |

---

## Riscos

- **Perda de saves** ao remover ou reinstalar emulador gerenciado (L1). É o maior risco e já existe hoje.
- **BIOS e arquivos do console em backups** (Dolphin `IPL.bin`/DSP hoje; RPCS3 `dev_flash`, Wii NAND, `keys.bin`,
  HD do xemu com dashboard da Microsoft se forem cobertos sem cuidado). Eles nunca podem ir para a nuvem nem ser
  distribuídos (L2, L7).
- **Restaurar com o emulador aberto** desfaz a restauração ou corrompe o cartão (L3).
- **Caminhos divergentes** entre backup, conquistas e BIOS fazem o Cerberus mexer numa pasta e o emulador usar outra
  (L6).
- **Disco cheio** com estados grandes e 10 cópias (L7).
- **Mudanças dos emuladores**: nomes de arquivo nas releases (Xenia muda com frequência), pastas padrão (DuckStation)
  e formato de log de conquistas. O teste `#[ignore]` de rede e um link para o código no comentário de cada regra
  ajudam a perceber cedo.
- **xemu**: a documentação oficial pede BIOS modificada para rodar jogos. O Cerberus só pode apontar arquivos que o
  jogador já tem, sem dizer onde conseguir nem baixar.
- **Switch** (reconhecido em `roms.rs`): qualquer emulador pede `prod.keys` e firmware. Nunca baixar; se entrar, mesmo
  modelo do firmware do PS3 (arquivo do próprio jogador).
- **Licenças de imagem**: CC BY-SA exige atribuição visível e compartilhamento pela mesma licença da imagem
  modificada. Logotipos de emuladores são marcas.

## Ordem sugerida

1. **L1** Não apagar saves ao remover ou reinstalar (M): risco atual de perda.
2. **L2** Tirar BIOS do backup do Dolphin e incluir os saves do Wii (P).
3. **L3** Restaurar só com o emulador fechado de verdade (P).
4. **L13** Estados da UI e os testes que faltam em `emulator_saves.rs` (P), junto com as três acima.
5. **L6** Um módulo só para os caminhos dos emuladores (M): base para L4, L7 e L8.
6. **L4** Backup de RPCS3, Cemu e Xenia, e o cartão "ainda não copia" (M).
7. **L7** Rodízio por espaço, estados opcionais, nuvem só com saves (M).
8. **L9** Atualizar emuladores standalone, começando pelo Xenia (M).
9. **L11** e **L12** Uma tabela de consoles, siglas e imagens que faltam (M + P).
10. **L5** xemu: arquivos do console no `xemu.toml` e backup manual do HD (M).
11. **L10** Formatos do 360 (`zar`, GOD), configuração, controles e conquistas do Xenia (M).
12. **L8** Saves por jogo: RetroArch e PPSSPP primeiro, cartões de PS1/PS2 por último (M a G).
