# Auditoria: clássicos (ROMs) e RetroArch

Revisão de 25/11/2026, feita só lendo o código: nenhum build, nenhum teste rodado e o app não foi aberto.
Base: worktree `CerberusLauncher-ajustes`. Não repete o que já está em
[emulação e saves dos emuladores](emulacao-e-saves-dos-emuladores.md) (backup de saves, Xenia/xemu, standalone,
tabela única de consoles L11) nem em [lojas e contas](lojas-e-contas.md) (tokens em texto puro no `settings.json`, L6).
O que não deu para confirmar está marcado **não verificado**. Regra mantida: o Cerberus nunca baixa BIOS nem
firmware. O código confere isso: `import_bios` só copia de uma pasta do jogador, e `install_system_files` baixa
só os pacotes de suporte do libretro (fontes e arquivos de sistema do Dolphin e do PPSSPP), não BIOS.

Contagem: **1 Crítica, 4 Altas, 13 Médias, 8 Baixas** (26 falhas).

---

## 1. Mapa

### 1.1 O que o jogador faz, passo a passo

| Passo | O que acontece | Onde |
| --- | --- | --- |
| 1. Apontar pasta | Configurações › Biblioteca e ROMs › "Adicionar pasta" abre o diálogo e grava em `settings.rom_dirs`. Depois a pasta é varrida. "Remover" tira a pasta da lista e varre de novo. | `main.rs:349` `add_rom_dir`, `:367` `remove_rom_dir`, `:377` `scan_roms`; UI `settings_page.rs:787-885` |
| 1b. Pastas dos emuladores | Ligado por padrão (`emulator_folders`). Entram as pastas listadas pelos emuladores: `rgui_browser_directory` e as playlists `.lpl` do RetroArch, `GameList` do DuckStation e do PCSX2, `ISOPath*` do Dolphin, `CurrentDirectory` do PPSSPP, Cemu e RPCS3. A raiz do disco e a pasta do usuário ficam de fora (`too_wide`). | `main.rs:317` `classic_folders`, `standalone.rs:306` `game_folders`, `:440` `retroarch_folders`, `roms.rs:303` |
| 2. Reconhecer | `roms::scan` faz uma busca em largura, ignora pastas que começam com `.` e as de `SKIPPED_DIRS`, e para em 300.000 entradas. Em cada arquivo, `identify` decide assim: **N64** pelo cabeçalho (nome, região, código e revisão, nas 3 ordens de bytes); **cartucho** pela extensão; **disco** (`iso chd cso cue gdi cdi rvz wbfs gcm wia xex pkg m3u`) pelo conteúdo (`disc_console::console_of`: Xbox/360 pela partição XDVDFS, GameCube/Wii pelo número mágico, PS1/PS2/PSP pelo `SYSTEM.CNF`/`PSP_GAME`) e, sem resposta, pelo nome da pasta (até 4 níveis acima). A região sai do nome do arquivo ("(USA)", "(E)"...). | `roms.rs:170-187`, `:310-344`; `disc_console.rs:149-211`; `xdvdfs.rs:27-49`; `disc_image.rs` (CHD/CSO) |
| 2b. Vários discos | Uma `.m3u` do jogador junta os discos que ela lista, e eles deixam de aparecer separados. Sem `.m3u`, os arquivos com "(Disc N)", "(Disk N of M)" ou "(CDN)" se juntam quando estão na mesma pasta, com o mesmo nome e a mesma extensão; o disco 1 representa o jogo. | `roms.rs:189-295` |
| 2c. Índice | O resultado vai para `cache/roms.json` e para a memória (`RomIndex`). Ele é carregado ao abrir o app, e uma varredura nova roda em segundo plano **a cada abertura** quando há `rom_dirs`. | `main.rs:3395`, `:3402-3406`, `roms.rs:486-498` |
| 3. Escolher emulador | Configurações › Emulação: um cartão por console com "quem abre" (`set_play_with` → `settings.play_with`), instalar o RetroArch (7z do buildbot *stable*), instalar os núcleos (zip do buildbot *nightly*), importar BIOS (PS1 512 KB, PS2 4 MB) e a lista "Sem emulação". A decisão de verdade fica em `standalone_for`: a escolha do jogador; senão o RetroArch, se houver núcleo para o console; senão o primeiro standalone encontrado. `classic_players` repete essa decisão para a UI. | `main.rs:735-776`, `:947-958`; `emulation.rs:22-38`, `:259-360`; UI `settings_page.rs:1130-1610` |
| 4. Jogar | `play_classic` só aceita arquivos do índice. Usa `known.system`, o console que a varredura leu do arquivo, no lugar do palpite da página. Num standalone, DuckStation e Dolphin recebem a lista de discos; os outros recebem o arquivo. No RetroArch: `connect_retroarch` (conta), `cheevos::prepare` (avisos desligados só nesta sessão), `disc_list` (m3u em `emulators/playlists/` só para PS1 e GC) e `launch` (`retroarch -L <núcleo> --verbose --log-file=… <rom>`). O PS2 é recusado sem BIOS. | `main.rs:589-631`, `:777-795`; `emulation.rs:362-414` |
| 5. Durante o jogo | Uma thread faz `try_wait` a cada 500 ms, credita o tempo a cada 60 s, lê as linhas novas do log e reconhece desbloqueios (`cheevos::parse`). Cada desbloqueio vira aviso do Cerberus (`announce`: busca ícone e descrição no RA com limite de 4 s). | `main.rs:635-685`, `:688-735`; `cheevos.rs:20-97`, `:432-447` |
| 6. Fechar | `cheevos::finish` desfaz as edições da sessão; o tempo restante vai para `classics.json`; `play_log::record` grava em `sessions.jsonl`; vem o backup "Depois de jogar" dos saves do emulador e a linha no histórico. | `main.rs:670-684` |
| 7. Conquistas na página | `classic_achievements`: MD5 da ROM inteira (cartuchos até 64 MB, sem cabeçalho iNES/copiadora, N64 em big-endian), comparado com `API_GetGameList` do console (guardado por 30 dias). Sem resultado, compara pelo título. A resposta fica memorizada por caminho em `ra-matches.json`. | `main.rs:1123-1135`; `achievements/classic.rs:100-185`; `achievements/retro.rs` |
| 8. Conta RA | Contas › RetroAchievements: usuário + chave Web API (`ra_key`) e, se quiser, a senha, trocada pelo token de conexão (`login2`) e guardada como `ra_token`. "Desconectar" tira o token **só do RetroArch**. | `main.rs:1362-1413`; `emulation.rs:416-467`; `retroachievements.rs` |
| 9. Tempo de jogo | `classics.json` guarda `{caminho: {seconds, last}}`, com escrita por arquivo temporário e troca de nome. Aparece na página do clássico, na Biblioteca e no Big Picture. | `emulation.rs:469-490`; `main.rs:158-178` |
| 10. Big Picture | A prateleira "Seus clássicos" (14 itens) e a grade da Biblioteca mostram **só** os clássicos que já abrem; Y joga direto. | `ui/src/tv/pages.rs:134-155`, `:343-370`; `tv/game.rs:97-105` |

### 1.2 O que o Cerberus grava (pasta de dados)

| Arquivo | Conteúdo |
| --- | --- |
| `settings.json` | `rom_dirs`, `retroarch`, `emulators`, `play_with`, `emulator_folders`, `ra_user`, `ra_key`, `ra_token` (texto puro; fica fora do backup, ver `data_backup.rs:34`) |
| `cache/roms.json` | Índice da última varredura |
| `classics.json` | Tempo e última vez de cada clássico, **pelo caminho completo** |
| `sessions.jsonl`, `trail.jsonl` | Sessões (`classic:<caminho>`) e histórico |
| `cache/achievements/ra-list-<console>.json`, `ra-matches.json` | Lista de jogos do RA por console e o jogo de cada ROM (por caminho) |
| `emulators/playlists/<jogo>.m3u` | Lista de discos gerada (PS1/GC no RetroArch, DuckStation, Dolphin) |
| `emulators/session-settings.json` | Edições da sessão em andamento, para desfazer depois de uma queda |
| `emulators/retroarch-session.log`, `emulators/ppsspp-session.log` | Logs da sessão (apagados ao abrir o jogo) |

### 1.3 O que é alterado nos emuladores

| Emulador / arquivo | Para sempre | Só na sessão (desfeito no `finish`) |
| --- | --- | --- |
| RetroArch `retroarch.cfg` (ao lado do exe) | `cheevos_enable="true"`, `cheevos_username`, `cheevos_token`, `cheevos_password=""` (`write_token`, em **toda** partida por `connect_retroarch`). Cópia única `retroarch.cfg.cerberus-bak` na primeira vez, que nunca é usada. | `cheevos_visibility_unlock/mastery/account/progress_tracker/lboard_*` = false, `cheevos_visibility_summary=0`, `cheevos_unlock_sound_enable=false`. Pela linha de comando: `--verbose --log-file` |
| DuckStation `settings.ini` | Apaga um `Token` em texto puro que versões antigas do Cerberus deixaram. `SetupWizardIncomplete=false` (só na cópia portátil) | `[Cheevos] Notifications/LeaderboardNotifications/SoundEffects=false`, `[Logging] LogToFile=true` |
| PCSX2 `inis/PCSX2.ini` + `inis/secrets.ini` | `[Achievements] Enabled/Username/LoginTimestamp`; **`Token` em `secrets.ini`**; `[UI] SetupWizardIncomplete=false` (portátil) | `[Achievements] Notifications/LeaderboardNotifications/SoundEffects/UnlockSound=false` |
| Dolphin `Config/RetroAchievements.ini` | `Enabled/Username/ApiToken` | Nada em arquivo: tudo vai por `--config=` (OSD, som, log) |
| PPSSPP `memstick/PSP/SYSTEM/ppsspp.ini` | `AchievementsEnable`, `AchievementsUserName` | `AchievementsSoundEffects=False`, `[Log] AchievementsEnabled/Level`; `--loglevel --log` |

Os standalone só recebem a conta quando estão sem conta ou com a mesma (`may_sign_in`, `cheevos.rs:257`). O
RetroArch não tem essa proteção (F12).

---

## 2. Como os outros fazem

| Tema | Cerberus | Playnite (MIT) | Hydra (MIT) |
| --- | --- | --- | --- |
| Onde procurar | Pastas soltas + pastas "conhecidas" dos emuladores; a varredura é inteira e única para todos os consoles | **Scanner salvo por pasta** (`GameScannerConfig`): emulador + perfil + plataforma, com exclusões de arquivo e de pasta (`Scanner.cs:150-214`, `:401-520`), refeito sob demanda | Uma pasta por sistema; o resultado marca `wrongPlatform` em vez de mudar o console calado (`scan-rom-folder.ts:17`, `:587`) |
| Arquivos só na nuvem | Abre e lê os arquivos (dispara o download) | **`ExcludeOnlineFiles`** + `UseSimplifiedOnlineFileScan`: checa `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS/OFFLINE` e pula o arquivo ou só o CRC (`Scanner.cs:803-814`, `:1300`; `Native/Winnt.cs:25-29`) | Não verificado |
| Cancelar e progresso | Não tem; o limite é de 300 mil entradas | `CancellationToken` em todo o laço (`Scanner.cs:65`, `:574`, `:789`) | `signal.cancelled`, `onProgress` e limite de 50 mil arquivos com aviso (`scan-rom-folder.ts:11`, `:36`, `:481`) |
| Identificar | Cabeçalho N64, extensão, conteúdo do disco, nome da pasta | **CRC contra DATs** (No-Intro/Redump), também dentro de zip/7z; serial; nome (`Scanner.cs:158`, `:1106`, `:1171`); região pela base (`Regions.yaml`) | Leitura dos primeiros 16 MB do disco (`sniff-disc-platform.ts`), CRC32 da ROM também dentro de arquivo compactado (`retroarch/rom-hash.ts`, `rom-archive.ts`) |
| Compactados | Não lê `.zip`/`.7z` | `zip 7z rar…` na maioria dos perfis do RetroArch (`Emulation/Emulators/RetroArch/emulator.yaml`) | Sim (`rom-archive.ts`) |
| Vários discos | `.m3u` do jogador ou "(Disc N)" na mesma pasta | `.cue`/`.m3u` juntam os arquivos (`Scanner.cs:869-875`); `DiscName` para nomear cada ROM do jogo (`:1466-1620`); o jogador escolhe a ROM ao jogar | `.m3u` vira o arquivo principal do grupo (`scan-rom-folder.ts:280-281`) |
| Perfis do emulador | Uma tabela fixa `CORES` (14 consoles, 10 núcleos) | Um `emulator.yaml` por emulador, com `StartupArguments`, `ImageExtensions` e `ProfileFiles` por núcleo | `known-binaries.ts`, `emulator-install-sources.ts` |
| Mexer na config do emulador | Lista de edições num único `session-settings.json`, apagado mesmo se desfazer falhar | Não mexe | Backups numa **lista** (`levelKeys.*SouvenirConfigBackups`); quem não volta **fica guardado** para tentar de novo (`emulator-souvenir-config.ts:63-110`) |
| Conta RA no emulador | RetroArch: escreve sempre e liga `cheevos_enable` | Não tem | Só quando o emulador está sem conta ou é a mesma sem token (`shouldWriteEmulatorAccount`); **não mexe em `cheevos_enable`** (teste `emulator-credential-config.test.ts:54`) |

O que vale copiar (as duas referências são MIT): a checagem de arquivo só na nuvem do Playnite, o cancelamento com
progresso e limite visível do Hydra, os backups que ficam até voltar de verdade (Hydra) e a regra de conta do Hydra
no RetroArch.

---

## 3. Falhas

### F1. O tempo de jogo de todos os clássicos pode ser zerado — **Crítica**

- **Onde.** `emulation.rs:476-490` (`played` devolve um mapa vazio em qualquer erro de leitura ou de JSON; `add_played`
  regrava só com a entrada nova). Quem chama: `main.rs:651` (a cada minuto) e `:673` (ao fechar).
- **Como acontece.** (a) Um `classics.json` inválido (editado à mão, backup ruim, setor ruim): a próxima partida
  grava `{"<só este jogo>": …}` e todo o resto some. (b) Dois clássicos abertos ao mesmo tempo (RetroArch + DuckStation):
  as duas threads leem, modificam e gravam **o mesmo** `classics.json.tmp` sem trava. Uma troca de nome pode levar o
  arquivo do outro pela metade, e a leitura seguinte cai em (a).
- **Reprodução.** Escreva `{` em `classics.json` e jogue um clássico por 61 s: o arquivo fica com uma entrada só.
- **Correção.** Ao falhar a leitura de um arquivo que existe, **recusar gravar** (devolver erro e guardar o crédito
  na memória para tentar depois) e copiar o arquivo ruim para `classics.json.bad-<data>`. Um `static Mutex` em volta
  de ler-modificar-gravar (como `play_log::WRITE`) e um nome de temporário único.
- **Aceite.** Unitário: um JSON inválido não é sobrescrito e `add_played` devolve `Err`. Teste com 2 threads × 100
  `add_played`: a soma bate e o JSON continua válido.

### F2. Trocar de conta do RetroAchievements mantém o token da conta antiga — **Alta**

- **Onde.** `main.rs:1376-1385`. `chosen.ra_user` recebe o usuário novo **antes** do teste
  `else if chosen.ra_user.as_deref() != Some(profile.user.as_str())`, que por isso nunca é verdadeiro.
- **Reprodução.** Ligue a conta A com senha. Ligue a conta B sem senha. `ra_token` continua o de A. Na próxima partida,
  `connect_retroarch` grava `cheevos_username=B` + `cheevos_token=<A>` (o login falha), e no PCSX2/Dolphin vai
  `Username=B` com o token de A. A tela continua dizendo "Conecta sozinho ao abrir um jogo" (`settings_page.rs:1576-1581`).
- **Correção.** Guardar o usuário anterior antes de sobrescrever e apagar `ra_token` quando ele mudar (sem diferenciar
  maiúsculas). Tirar das configurações dos emuladores o token antigo (ver F3).
- **Aceite.** Teste do comando (ou de uma função pura extraída): A com token → B sem senha ⇒ `ra_token == None`.

### F3. "Esquecer a conta neste PC" deixa o token no PCSX2 e no Dolphin — **Alta**

- **Onde.** `main.rs:1400-1413` só chama `emulation::forget_token` (RetroArch). O token vai para sempre para
  `inis/secrets.ini` do PCSX2 (`cheevos.rs:337`) e `Config/RetroAchievements.ini` do Dolphin (`cheevos.rs:358`), e o
  nome para o PPSSPP (`:380`).
- **Reprodução.** Ligue a conta com senha, jogue um jogo de PS2 no PCSX2 e clique em Desconectar: `secrets.ini` ainda
  tem `Token = …`, e o próximo a usar o PC desbloqueia conquistas na sua conta.
- **Correção.** Um `cheevos::forget(data, token)` que percorre os emuladores encontrados (`emulators_here`) e tira
  `Token`/`ApiToken`/`Username` **só quando o token é o do Cerberus** (mesma regra de `forget_token`). Dizer na tela
  quais emuladores foram desconectados.
- **Aceite.** Unitário com cópias portáteis do PCSX2 e do Dolphin: depois de `forget`, nenhum arquivo contém o token;
  um token diferente (do próprio jogador) continua lá.

### F4. A varredura baixa arquivos que estão só na nuvem (OneDrive) — **Alta**

- **Onde.** `roms.rs:338` → `identify` abre os arquivos: `n64_header` lê 64 bytes (`:131-134`), `listed_discs` lê a
  `.m3u`, e `disc_console::console_of` lê `.iso/.chd/.cso/.cue` em até 4 lugares do Xbox (`xdvdfs.rs:27-40`, até a
  posição 0x1830_0000 + 64 KB), o setor 0 e o setor 16. Nada no código olha `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`
  ou `OFFLINE` (grep sem resultado em `src-tauri/src`). A varredura roda a cada abertura do app (`main.rs:3402-3406`).
- **Reprodução.** Coloque ROMs/ISOs em `OneDrive\Jogos` como "só online" e adicione a pasta: o OneDrive começa a
  baixar. **Não verificado** se o OneDrive baixa o arquivo inteiro por causa de uma leitura parcial; o Playnite trata
  como se baixasse.
- **Correção.** Ler os atributos da entrada (`std::os::windows::fs::MetadataExt::file_attributes`, já disponível na
  busca do diretório sem abrir o arquivo). Arquivo só na nuvem entra pelo nome, sem ler o conteúdo (como o
  `UseSimplifiedOnlineFileScan`), com o console pela pasta e um selo "na nuvem". Opção para pular de vez.
- **Aceite.** Unitário com uma função pura `should_read(attrs)`. Manual: uma pasta com arquivo só online termina a
  varredura sem que o ícone do OneDrive mude para "baixando".

### F5. ROMs em `.zip`/`.7z` não aparecem — **Alta**

- **Onde.** `roms.rs:102-122` e `:103` não têm `zip` nem `7z`; `identify` devolve `None`.
- **Reprodução.** Uma pasta com sets No-Intro zipados (o formato mais comum para cartuchos) mostra "Nenhum clássico
  encontrado" (`classics.rs:432`), mesmo com o RetroArch abrindo zip direto.
- **Correção.** Para `.zip` (biblioteca `zip`, já usada): ler o diretório central sem extrair e usar a extensão da
  primeira entrada que for ROM para decidir o console. O MD5 do RA pode ser feito lendo essa entrada (até 64 MB).
  `.7z` pelo nome só, com o console pela pasta. A Onda 6.5 já prevê o CRC dentro do zip.
- **Aceite.** Unitário: `Mario (USA).zip` com `Mario (USA).sfc` dentro é reconhecido como `snes`; um zip sem ROM é
  ignorado.

### F6. Um disco ou compartilhamento fora do ar no momento da varredura apaga todos os clássicos da lista — **Média**

- **Onde.** `roms.rs:320-322` (`read_dir` que falha é pulado em silêncio) e `main.rs:339-344` (o resultado vazio é
  salvo e publicado; só um pânico mantém a lista anterior).
- **Reprodução.** ROMs num HD externo. Abra o Cerberus sem o HD: a Biblioteca fica sem clássicos e
  `cache/roms.json` vira `[]`. Ligar o HD não traz nada de volta até a próxima varredura.
- **Correção.** `scan` devolve também as **raízes inacessíveis**. Para cada uma, manter as entradas antigas do índice
  marcadas "pasta indisponível" (jogar avisa em vez de dizer "não está nas suas pastas"). Mostrar isso em
  Biblioteca e ROMs.
- **Aceite.** Unitário: índice com 3 ROMs em `X:\` mais varredura com `X:\` ausente ⇒ as 3 continuam, marcadas.

### F7. Varredura completa, sem cache, sem progresso e com espera bloqueante no executor — **Média**

- **Onde.** A cada abertura e a cada mudança de emulador ou pasta (`main.rs:362, 372, 378, 851, 871, 887, 968, 3405`),
  `scan` lê de novo o conteúdo de todo `.iso/.chd/.cso/.cue`. O CHD é aberto e descomprimido duas vezes
  (`disc_console.rs:161-166`: `xbox_kind` e depois `Cooked`). `classic_folders` (`main.rs:317-333`) roda **no
  executor async** e chama `is_dir()` e `canonicalize` em caminhos que vêm das configurações (`standalone.rs:310`,
  `:320`; `roms.rs:306`). Num caminho de rede fora do ar, isso trava uma thread do runtime por dezenas de segundos
  (**não verificado** o tempo exato no Windows).
- **Correção.** Cache por `(caminho, tamanho, data de modificação)` → `RomFile` em `roms.json`: só se lê o conteúdo
  de arquivo novo ou mudado. `classic_folders` dentro do `spawn_blocking`. Eventos de progresso (`roms-progress`) e
  cancelamento como no Hydra.
- **Aceite.** Teste: a segunda varredura de uma pasta com 50 discos falsos não abre nenhum (contador injetado).
  Manual: 2.000 discos em HD, a segunda abertura do app termina em menos de 2 s.

### F8. Pastas de "último lugar navegado" dos emuladores ampliam a varredura — **Média**

- **Onde.** `standalone.rs:447` (`rgui_browser_directory` do RetroArch) e `:297` (`CurrentDirectory` do PPSSPP).
  `too_wide` (`roms.rs:303-307`) só barra a raiz do disco e a pasta do usuário.
- **Reprodução.** No PPSSPP, navegue até `Downloads` ou `OneDrive\Documentos`. Na próxima abertura do Cerberus a pasta
  inteira é varrida (e cai no F4 e no F9).
- **Correção.** Usar só pastas que o jogador **cadastrou** no emulador (`GameList`, `ISOPath*`, playlists). O "último
  navegado" entra só como sugestão ("Adicionar esta pasta?"). Estender `too_wide` para as pastas conhecidas do
  Windows (Downloads, Documentos, Área de Trabalho, raiz do OneDrive).
- **Aceite.** Unitário: `CurrentDirectory = C:/Users/x/Downloads/` não entra em `classic_folders`.

### F9. `README.md` vira jogo de Mega Drive; ISOs de PC e Linux viram clássicos — **Média**

- **Onde.** `roms.rs:114` (`"md"` → `genesis`) e `:103`/`:183` (qualquer `.iso`/`.pkg` entra, mesmo com console vazio).
- **Reprodução.** Uma pasta com `README.md` e `ubuntu.iso`: aparecem "README" (Mega Drive, com Jogar, que abre o
  `genesis_plus_gx` com um texto) e "ubuntu" ("O arquivo não diz de qual console é").
- **Correção.** Para `.md`/`.bin`/`.gen`, conferir a assinatura `SEGA` em 0x100 (ou 0x200 no `.smd`) e recusar
  arquivos de texto (primeiros bytes ASCII imprimíveis). Disco sem console definido pelo conteúdo nem pela pasta fica
  fora da Biblioteca e entra numa lista "Não reconhecidos" em Biblioteca e ROMs.
- **Aceite.** Unitário: `README.md` com texto ⇒ `None`; um `.md` com `SEGA` em 0x100 ⇒ `genesis`; um ISO9660 sem
  `SYSTEM.CNF` fora de uma pasta de console ⇒ não listado.

### F10. Os avisos desligados podem não voltar (emulador ainda aberto ou arquivo travado) — **Média**

- **Onde.** `cheevos.rs:422-428`: `restore_pending` desfaz e **apaga o arquivo pendente sempre**, mesmo quando
  `undo_all` pulou o arquivo (`:413` `continue`) ou não conseguiu gravar (`:417` `let _ = fs::write`). Ele é chamado
  na abertura do app (`main.rs:3342`) sem saber se o emulador da sessão antiga ainda está aberto.
- **Reprodução.** Jogue no RetroArch pelo Cerberus e feche o Cerberus à força (ou deixe ele reiniciar). Abra o
  Cerberus com o RetroArch ainda aberto: a restauração regrava o `retroarch.cfg`. Ao sair, o RetroArch grava a
  configuração que tem na memória (`config_save_on_exit`, ligado por padrão, **não verificado** nesta instalação) com
  `cheevos_visibility_*="false"`, e o arquivo pendente já foi apagado: os avisos do RetroArch ficam desligados para
  sempre. O mesmo vale para `LogToFile=true` do DuckStation (o log cresce sem fim). **Não verificado** se a
  autoatualização espera o fim do jogo: `classic_running` só é consultado em `main.rs:3252`.
- **Correção.** Guardar no arquivo pendente o PID e o exe do emulador. Na abertura, se o processo ainda existir, adiar
  a restauração e vigiar esse PID. Apagar o arquivo pendente só depois de conferir que cada edição foi desfeita (ou
  que o jogador mudou o valor). Manter as que falharam, como o Hydra faz (`retainedBackups`).
- **Aceite.** Unitário: um `undo_all` que não consegue gravar (arquivo só leitura) mantém o arquivo pendente. Com um
  PID vivo, `restore_pending` não mexe em nada.

### F11. Dois clássicos ao mesmo tempo desfazem a sessão um do outro — **Média**

- **Onde.** `cheevos.rs:283-284` (`prepare` chama `restore_pending` primeiro), `:394-399` (um único
  `session-settings.json` sobrescrito) e `:404-407` (`finish` apaga o arquivo da outra sessão). O log
  `retroarch-session.log` é único e apagado a cada partida (`main.rs:615`, `emulation.rs:407`).
- **Reprodução.** Abra um jogo de PS1 no DuckStation e depois um de SNES no RetroArch: os avisos do DuckStation voltam
  no meio do jogo. Ao fechar o primeiro, as edições do RetroArch deixam de estar guardadas para o caso de uma queda.
  Duas janelas do RetroArch perdem o log uma da outra.
- **Correção.** Um arquivo pendente por sessão (`session-<id>.json`), `restore_pending` só para os órfãos (ver F10),
  um log por sessão. Ou bloquear um segundo clássico enquanto `classic_running()` for verdadeiro, com uma mensagem
  clara.
- **Aceite.** Unitário: `prepare(A)`, `prepare(B)`, `finish(A)` ⇒ as edições de B continuam aplicadas e guardadas.

### F12. O RetroArch recebe a conta à força em toda partida — **Média**

- **Onde.** `emulation.rs:439-455` e `main.rs:989-993`. Não há `may_sign_in`, e `cheevos_enable` volta para `"true"`
  para sempre.
- **Reprodução.** No RetroArch, entre com outra conta ou desligue as conquistas (para jogar um hack): na próxima
  partida pelo Cerberus, a conta e o `cheevos_enable` são trocados sem aviso.
- **Correção.** A mesma regra dos standalone e do Hydra: escrever só quando `cheevos_username` está vazio ou é o mesmo
  usuário, e não mexer em `cheevos_enable` (ou ligá-lo só na sessão, como edição desfeita no `finish`). Tirar o
  `.cerberus-bak`, que nunca é usado, ou oferecer "Restaurar configurações originais".
- **Aceite.** Unitário: um cfg com `cheevos_username="Outro"` não muda; `cheevos_enable="false"` continua `"false"`
  depois de `finish`.

### F13. Conquistas pelo título mostram o jogo errado, e "não achou" fica guardado para sempre — **Média**

- **Onde.** `achievements/classic.rs:166-180`: discos nunca têm hash, e um cartucho com hash diferente cai no título.
  `:159-163` e `:181-183` memorizam `Some` **e `None`** por caminho, sem prazo.
- **Reprodução.** Um "Final Fantasy VII (Japan)" ou um hack "Super Mario World (Hack)" mostra a lista e o progresso
  do jogo americano ou original, mas o emulador não concede nada (o hash não está ligado). E uma ROM sem resultado num
  dia continua sem resultado depois que o RA adiciona o hash.
- **Correção.** Separar "por hash" (confiável) de "por título" (provável, com um selo "pode não ser a sua versão" na
  página). Memorizar com o tamanho e a data do arquivo, e guardar `None` por no máximo 30 dias, como a lista. Hash dos
  discos (PS1/PS2/PSP/GC) pelo algoritmo do rcheevos fica para o 6.5.
- **Aceite.** Unitário: um `pick` por título devolve o tipo `Title`, e a UI mostra o selo; um `None` mais antigo que
  30 dias é refeito.

### F14. Jogos de PS2 e Wii em vários discos só abrem o disco 1, e a tela promete a troca — **Média**

- **Onde.** `emulation.rs:364` (`DISC_LISTS` só tem `ps1` e `gc`), `:374-376` (os outros recebem `discs[0]`);
  `main.rs:604-607` (o PCSX2 standalone recebe o arquivo). `join_discs` esconde os discos 2 em diante
  (`roms.rs:286-289`). A UI diz "O {} abre o disco 1; quando o jogo pedir outro, troque pelo menu dele"
  (`classics.rs:687`, `:870`).
- **Reprodução.** Xenosaga III (2 discos) no RetroArch/LRPS2: não há como abrir o disco 2 pelo Cerberus, e no
  RetroArch o Controle de disco só tem o disco 1.
- **Correção.** "Escolher o disco ao jogar" (Onda 6.5) para os consoles sem `.m3u`. Para o RetroArch com PS2,
  **não verificado** se o LRPS2 aceita `.m3u`; se aceitar, incluir em `DISC_LISTS`. Texto honesto: "Para trocar de
  disco, escolha o disco 2 aqui".
- **Aceite.** Unitário de `disc_list` por console; teste da UI: com 2 discos de PS2 aparece a escolha do disco.

### F15. A `.m3u` gerada muda o nome dos saves do PS1 — **Média**

- **Onde.** `emulation.rs:379-386`: a lista recebe o nome do jogo sem "(Disc 1)". O `pcsx_rearmed` nomeia o `.srm`
  pelo arquivo aberto.
- **Reprodução.** Um jogador que já jogava pelo `FF7 (USA) (Disc 1).cue` direto no RetroArch tem
  `FF7 (USA) (Disc 1).srm`. Pelo Cerberus, abre `FF7 (USA).m3u` e cria um cartão vazio: o save "sumiu" (o arquivo
  continua lá). **Não verificado** o comportamento do DuckStation (cartão por serial ou por título).
- **Correção.** Antes de criar a lista, procurar na pasta de saves um `<disco 1>.srm` sem `<nome>.srm` e avisar ou
  oferecer copiar (sem apagar nada).
- **Aceite.** Unitário: com `saves/X (Disc 1).srm` presente, `disc_list` devolve o aviso.

### F16. Tempo, histórico e jogo do RA ficam presos ao caminho completo — **Média**

- **Onde.** `emulation.rs:480` (chave = caminho), `achievements/classic.rs:160`, os ids `classic:<caminho>`
  (`main.rs:620`, `:637`).
- **Reprodução.** Mova `D:\ROMS` para `E:\ROMS` (ou o HD externo troca de letra): o tempo e as sessões somem da tela.
  Os dados antigos ficam órfãos nos arquivos.
- **Correção.** Chave estável: o hash da ROM ou do primeiro disco, ou (tamanho + nome) para discos grandes. Na
  varredura, migrar a entrada antiga quando um caminho some e outro igual aparece.
- **Aceite.** Unitário: depois de "mover" o arquivo e varrer de novo, `played` mostra o mesmo total.

### F17. Núcleos e arquivos de sistema ignoram as pastas configuradas no RetroArch — **Média**

- **Onde.** `emulation.rs:167-169` (`core_file` sempre em `<exe>/cores`, sem ler `libretro_directory`) e `:342`
  (`install_system_files` sempre em `<exe>/system`), enquanto a BIOS segue `system_directory` (`:46-62`).
- **Reprodução.** Um RetroArch com `system_directory = "D:\BIOS"`: o pacote do Dolphin vai para `<exe>/system`, o
  núcleo do GameCube não encontra os arquivos dele e o cartão diz "Pronto".
- **Correção.** Usar `retroarch_dir(exe, "libretro_directory", "cores")` e `system_dir(exe)` nos dois lugares.
- **Aceite.** Unitário igual a `the_bios_is_looked_for_where_retroarch_is_set_to_read_it` para os núcleos e os
  arquivos de sistema.

### F18. Uma varredura antiga pode sobrescrever a nova — **Média**

- **Onde.** `main.rs:336-345` não tem trava nem número de geração. A varredura da abertura (`:3402-3406`) disputa com
  a de `add_rom_dir`.
- **Reprodução.** Logo depois de abrir o app (enquanto a varredura demorada roda), adicione uma pasta: a varredura da
  pasta nova termina primeiro, a da abertura termina depois e publica a lista sem a pasta nova.
- **Correção.** Um `tokio::sync::Mutex` em volta de `rescan_roms`, ou um contador de geração: só publica quem começou
  por último.
- **Aceite.** Teste com duas varreduras simuladas fora de ordem: fica a mais recente.

### F19. Região pelo nome é frágil — **Baixa**

- **Onde.** `roms.rs:156-168`, `:144-149`. "(Japan, USA)" vira Japan; "(World)", "(Brazil)", "(Korea)", "(Asia)"
  ficam vazios; o N64 com região `B` (Brasil) não é reconhecido.
- **Correção.** Ler a lista de códigos dentro dos primeiros parênteses (No-Intro), separada por vírgula, numa tabela
  como o `Regions.yaml` do Playnite.
- **Aceite.** Unitário com 8 nomes reais.

### F20. Discos do mesmo jogo em subpastas ou com extensões diferentes não se juntam — **Baixa**

- **Onde.** `roms.rs:262-268` (a chave inclui a pasta e a extensão) e `:200-209` (só olha a primeira ocorrência de
  "(disc": "Jogo (Disc Edition) (Disc 2)" não é lido).
- **Correção.** Juntar também quando estão em pastas irmãs com o mesmo nome base, e continuar a busca depois de uma
  tag inválida.
- **Aceite.** Unitário com `FF7 (Disc 1)\FF7 (Disc 1).cue` e `FF7 (Disc 2)\FF7 (Disc 2).cue`.

### F21. BIOS reconhecida só pelo tamanho — **Baixa**

- **Onde.** `emulation.rs:79-83`, `:100-107`. Qualquer `.bin` de 512 KB (uma ROM de Mega Drive, por exemplo) é
  importado como BIOS de PS1, e a tela diz "BIOS do PS1 pronta".
- **Correção.** Conferir o MD5 contra uma lista pública de BIOS conhecidas (sem baixar nada), e na dúvida mostrar
  "não reconhecida".
- **Aceite.** Unitário: 512 KB de zeros ⇒ não reconhecida.

### F22. Erros engolidos e limites calados — **Baixa**

- **Onde.** `roms.rs:491-498` (`save_index`), `cheevos.rs:192`, `:208`, `:417` (gravações ignoradas, mas `apply` diz
  que as edições foram feitas), `achievements/classic.rs:121-122`, `:182-183`, `roms.rs:325-328` (o teto de 300 mil
  para sem aviso), `main.rs:339` (uma varredura que falha não é avisada e a tela diz "Pastas verificadas de novo.",
  `settings_page.rs:811`).
- **Correção.** `scan` devolve um resumo (lidos, parou no teto, pastas inacessíveis). A UI mostra esse resumo, e as
  edições cuja gravação falhou não entram no arquivo pendente.
- **Aceite.** Unitário: com o teto em 10, o resumo diz `truncated = true`.

### F23. RetroArch, núcleos e arquivos de sistema baixados sem conferência — **Baixa**

- **Onde.** `emulation.rs:262-267` (`sha256: None`) e `:302-323` (núcleo *nightly*, só confere `MZ`; `fs::write`
  direto sobre a DLL).
- **Correção.** Gravar a DLL num temporário e trocar de nome. Conferir o tamanho/ETag do `.index-extended` do
  buildbot. Considerar os núcleos *stable* da mesma versão do RetroArch.
- **Aceite.** Unitário: um download cortado não substitui a DLL que já estava lá.

### F24. O tempo conta o RetroArch parado no menu — **Baixa**

- **Onde.** `main.rs:640-667` mede o processo inteiro. Ao "Fechar conteúdo", o RetroArch volta ao menu e continua
  contando. **Não verificado** se o `Instant` avança com o PC suspenso.
- **Correção.** Ler do log `--verbose` o fim do conteúdo ("Unloading core" ou equivalente; **não verificado** o texto
  exato), ou passar `--no-menu`/`quit_on_close_content` só na sessão.
- **Aceite.** Unitário do parser com um log real.

### F25. O Big Picture esconde clássicos sem emulador pronto — **Baixa**

- **Onde.** `ui/src/tv/pages.rs:136-149` (`ready_player` filtra). Um clássico sem núcleo some sem dica, enquanto o
  desktop mostra "Falta o núcleo".
- **Correção.** Mostrar o clássico com o selo e, ao apertar A, a página com o que falta.
- **Aceite.** Teste da UI: um clássico com `problem` aparece com o selo.

### F26. As conquistas do clássico usam o console da página, não o lido do arquivo — **Baixa**

- **Onde.** `main.rs:1123-1131` repassa o `console` vindo da UI, enquanto `play_classic` (`:598`) prefere
  `known.system`. Os mapas de console da UI (`classics.rs:16-82`) e do backend (`disc_console.rs:173-196`) também são
  diferentes (`segacd`, `pce`, `ps4` somem na UI). O lado das tabelas já está no L11 da revisão de emulação.
- **Correção.** Usar `known.system` quando não estiver vazio, igual ao `play_classic`.
- **Aceite.** Unitário do comando com um índice onde `system = "ps1"` e a página manda `"ps2"`.

---

## 4. Lacunas (não são falhas, mas faltam para ficar completo)

- **Reconhecer pelo CRC/serial** com No-Intro/Redump (Onda 6.5). Hoje o nome exibido é o nome do arquivo e não
  existe ano.
- **Mais consoles no RetroArch**: há extensões reconhecidas sem núcleo (`ds`, `3ds`, `32x`, `segacd`, `saturn`,
  `dc`, `pce`). Faltam extensões comuns: `.bin` (Mega Drive/Atari), `.pce`, `.a26`, `.lnx`, `.ws`, `.ngp`. Um perfil
  por núcleo, em dados, como o `emulator.yaml` do Playnite.
- **Exclusões por pasta e por arquivo** e "ignorar arquivos só na nuvem" (Playnite).
- **Escolher o núcleo por console** (hoje é fixo; ex.: `mesen` em vez de `fceumm`, `swanstation`/`beetle_psx`).
- **Hash dos discos** para o RA (rcheevos: PS1 pelo executável de boot, PS2, PSP, GC/Wii); saber hardcore ou
  softcore pelo log.
- **Atualizar o RetroArch e os núcleos** (hoje "Instalar" só aparece quando falta), com a versão instalada à mostra.
- **Restaurar o `retroarch.cfg` original** a partir do `.cerberus-bak`, que hoje é criado e nunca usado.
- **Tokens no Cofre de credenciais**: `ra_key` e `ra_token` em texto puro no `settings.json`. Já está em
  [lojas L6](lojas-e-contas.md); a chave Web API vai na URL porque a API do RA pede assim, e os erros de rede já não
  carregam a URL (8.0).
- **Testes que faltam**: `scan` com arquivo só na nuvem (atributos simulados) e com raiz inacessível; `add_played`
  concorrente e com JSON inválido; `ra_connect` trocando de conta; `ra_disconnect` limpando os standalone;
  `restore_pending` com falha de gravação e com o processo vivo; `prepare` duas vezes; `identify` de `README.md`;
  `game_for` com `None` vencido; `disc_list` por console; `classic_folders` sem bloquear o executor.

---

## 5. Ordem sugerida

1. **F1** (perde tempo de jogo) e **F2 + F3** (conta e token): pequenos, P cada.
2. **F10 + F11** (configurações dos emuladores que não voltam): um arquivo pendente por sessão, com o PID. — M
3. **F6 + F18 + F22** (índice confiável: raízes inacessíveis, geração, resumo da varredura). — M
4. **F4 + F7 + F8** (varredura que não baixa da nuvem, com cache, progresso e cancelamento, sem "último navegado"). — M
5. **F9 + F5** (o que entra e o que fica de fora: `.md`, ISOs de PC, zip). — M
6. **F12 + F13** (conquistas honestas: conta do RetroArch, selo "por título", memória com prazo). — P/M
7. **F14 + F15 + F16** (vários discos e chave estável de tempo e saves). — M
8. **F17, F19 a F21, F23 a F26** (acabamento). — P cada
