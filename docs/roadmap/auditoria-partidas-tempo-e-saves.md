# Auditoria: partidas, tempo de jogo, saves e nuvem

Revisão só de leitura (25/11/2026) do worktree `CerberusLauncher-ajustes`. Não rodei build, testes nem o app. Tudo que
depende do comportamento real do Windows, do OneDrive ou do Tauri e que não confirmei está marcado **não verificado**.
Para não repetir o que já existe: medir melhor, modos por jogo, jogos abertos fora do Cerberus e PC dormindo já estão na
**6.4**; backup automático dos dados do launcher na **6.8/7.1**; âncora e conflito para jogos de PC no plano
[saves-de-jogos-de-pc](saves-de-jogos-de-pc.md) (E9); concorrência do `settings.json` na **8.0/lojas L1**; banco de
conquistas fora do backup na **conquistas L12**. Aqui está o que esses documentos **não** cobrem, ou cobrem como plano
sem dizer que o código de hoje já perde dados.

---

## 1. Mapa

### 1.1 Passo a passo do jogador

| Passo | O que acontece | Onde | O que é gravado |
|---|---|---|---|
| **Jogar (recomp ou jogo adicionado à mão)** | `play` → `sessions::start`. Reserva o id em `Sessions` (`None`), roda o instalador ou a preparação quando é preciso, **puxa saves da nuvem** (recomp), prepara ReShade e ajustes, abre o processo suspenso dentro de um Job Object e retoma. | `main.rs:2484-2524`, `sessions.rs:120-253`, `sessions/windows.rs:42-71` | Nada até o jogo aparecer |
| **Detectar** | Sem "executável acompanhado": a partida começa no próprio clique (`Started(self.started)`) e dura até o job esvaziar (sobrando só ajudantes como crash handler e updater). Com executável acompanhado (`tracking_exe`, ou `FooLauncher.exe` → `Foo.exe` automático): cada execução do programa jogável dentro do job vira uma rodada, medida por `GetProcessTimes`. | `sessions/windows.rs:74-117`, `177-194`, `28-40` | `Sessions[id] = Some(Round)` na memória |
| **Contar** | Uma thread a cada 15 s chama `checkpoint`: grava em `library.json` o que passou de `credited`. Jogos de loja (Steam, Epic, Xbox): `store_time::watch` olha a cada 5 s qualquer processo dentro da pasta de instalação e grava em `store-playtime.json` a cada 60 s. Clássicos: `watch_classic` grava em `classics.json` a cada 60 s. | `main.rs:3378-3382`, `sessions.rs:80-88`, `store_time.rs:122-202`, `main.rs:636-684` | `library.json` (temporário + rename), `store-playtime.json`, `classics.json` |
| **Fechar** | `Closed(duração)` → `record` → `close_session`: `record_session` (total − creditado + duração, `last_played`, `last_session_secs`), linha "Partida" no `trail.jsonl`, **backup "Depois de jogar"** do recomp (com envio para a nuvem), linha no `sessions.jsonl`, avisos. Código de saída → `report_crash` (motivo, log, aviso). | `sessions.rs:229-251`, `480-520`, `441-478`, `crash.rs:12-29` | `library.json`, `trail.jsonl`, `sessions.jsonl`, zip em `saves/<recomp>/` e `.upload` |
| **Backup** | `saves::backup`: junta os arquivos dos modelos do catálogo (ou da pasta do jogo, filtrada), calcula a impressão SHA-256, pula se é igual à do último backup, grava `.part` → fsync → marca `.upload` → `persist_noclobber` → rodízio dos 10 mais novos. | `recomp/saves.rs:210-256`, `343-350` | `saves/<recomp>/<millis>-xxxx.zip`, `.upload` |
| **Nuvem (envio)** | `cloud::retry_game` logo depois do backup e a cada 60 s: copia para `<pasta da nuvem>/Cerberus Saves/<recomp>/` via temporário + `persist_noclobber`, rodízio de 20 na nuvem, apaga `.upload`. Quem leva para a internet é o OneDrive, Drive ou Dropbox. | `recomp/cloud.rs:52-128`, `main.rs:3384-3388` | Zip na pasta sincronizada |
| **Nuvem (trazer)** | Antes de cada Jogar: o zip mais novo da nuvem (por `created`) é comparado com a impressão dos saves atuais. Se for diferente e o backup local mais novo for **mais antigo pelo relógio**, faz a cópia "Antes de trazer da nuvem" (só local) e restaura o da nuvem. | `recomp/mod.rs:214-243` | Saves do jogo, zip local |
| **Restaurar** | Página do jogo ou Saves → `restore_backup`: recusa se `Sessions` tem o jogo, abre o zip escolhido, faz a cópia "Antes de restaurar" (local), restaura (tudo lido antes de trocar) e faz o backup "Restaurado" (vai para a nuvem). | `main.rs:2613-2629`, `recomp/mod.rs:250-261`, `saves.rs:370-398`, `ui/src/backups.rs:93-142` | Saves, 2 zips |
| **Ver** | Página do jogo: `PlayCard` (14 dias, semana, mês, sessões, a mais longa); Histórico (`trail`); Saves (faixa da nuvem, uma linha por recomp, emuladores). | `ui/src/game_overview.rs:44-139`, `ui/src/pages.rs:372-524` | — |
| **Dados do launcher** | Configurações › Exportar/Importar: zip com 9 arquivos, settings sem segredos e `covers/`. Importar copia o atual para `before-restore-<t>`, troca os arquivos e reinicia o app em 1,2 s. | `data_backup.rs:52-161`, `main.rs:1441-1467` | Zip escolhido; `before-restore-*` |
| **Bandeja** | Fechar a janela esconde na bandeja quando há partida (Sessions ou clássico) ou `close_to_tray`; quando a última partida acaba, sai se só estava escondido por ela. "Sair" na bandeja chama `app.exit(0)`. | `background.rs:35-64`, `128-153` | — |
| **PC acordado** | A cada 30 s, se há instalação ativa, `SetThreadExecutionState(ES_SYSTEM_REQUIRED)`. | `awake.rs:12-30` | — |

### 1.2 Arquivos e quem escreve

| Arquivo | Escritor(es) | Forma | Limite |
|---|---|---|---|
| `library.json` | `LocalLibrary::commit` (cache na memória com mutex) | temporário + rename, **sem fsync**; leitura com `recovery` | — |
| `store-playtime.json` | só `store_time::watch` (lê, muda, grava) | temporário + rename, sem fsync; **leitura com erro vira vazio** | — |
| `classics.json` | `emulation::add_played` (uma thread por clássico) | igual ao de cima | — |
| `sessions.jsonl` | `play_log::record` (mutex estático) | append, erros ignorados | **sem limite** |
| `trail.jsonl` | `Trail::append` | append; corta para 500 ao passar de 600 | 500 linhas para **todos** os jogos |
| `saves/<recomp>/*.zip` | `saves::backup`, `pull_from_cloud` (cópia da nuvem) | `.part` + fsync + `persist_noclobber` | 10 por jogo, **todos os motivos juntos** |
| `<nuvem>/Cerberus Saves/<recomp>/*.zip` | `cloud::send` (de todos os PCs) | temporário + fsync + `persist_noclobber` | 20 por jogo, por `created` |

### 1.3 Testes que existem

`sessions.rs` (checkpoint sobrevive a reinício e a falha de gravação; falha de gravação encerra a sessão; durações;
recusas do Windows; ajudantes; crash depois de fechar a janela; launcher que entrega ao jogo; rodadas com o menu do
launcher aberto), `store_time.rs` (3), `play_log.rs` (2), `trail.rs` (4), `saves.rs` (12), `cloud.rs` (1),
`recomp/mod.rs` (fluxos de nuvem entre dois PCs, perto das linhas 1660-1801), `data_backup.rs` (3, um `#[ignore]`).
**Não há teste** para: arquivo de tempo corrompido, relógio errado, conflito entre dois PCs, rodízio com cópias de
segurança, reinstalar, importar com jogo aberto, Cerberus fechado no meio, linha JSONL sem `\n`, UI.

---

## 2. Como os outros fazem

| Tema | Hydra (MIT) | Playnite (MIT) | Heroic (GPL, só ideia) | RecompLauncher (GPL, só ideia) | Cerberus hoje |
|---|---|---|---|---|---|
| Detectar o jogo | Varre processos a cada intervalo e casa com o executável do jogo e com `trackingExecutablePaths` extras (`process-watcher.ts:372-447`); conta qualquer jogo aberto, até fora do Hydra | Modos `Default`, `Process` (árvore), `Directory`, `OriginalProcess` e `ProcessName`, com atraso inicial e frequência (`GameAction.cs:15-42`, `GenericGameController.cs:531-656`) | O próprio launcher abre e espera o processo | Por processo do recomp | Job Object (árvore desde o clique) e executável acompanhado; lojas pela pasta; locais e recomps abertos por fora não contam |
| Tempo | Deltas por tick com `performance.now()`, gravação parcial a cada tick e sincronização a cada 3 min; no fechamento, delta desde o último tick (`game-running-state.ts:14-23`, `process-watcher.ts:613-712`) | `Stopwatch` da sessão inteira (`GenericGameController.cs:380-430`) | — | Tempo no manifesto do backup (`played`) | Checkpoint de 15 s (recomp), 60 s (loja e clássico) |
| PC dormindo | `powerMonitor` suspend/resume só para a sessão gerenciada da Epic (`index.ts:215-216`); o resto **não verificado** | Nada encontrado (**não verificado**) | — | — | Nada |
| Cloud: quem é mais novo | **Âncora**: hash da base remota guardado no PC; estados `local-ahead`, `remote-ahead` e `conflict`; antes de abrir só restaura, depois de fechar só envia, e o envio só acontece se o remoto não mudou desde a base (`sync-game/policy.ts`, `launch-guard.ts`) | — | Download antes de abrir (`--skip-upload`), upload depois (`--skip-download`); em conflito, **pergunta** "Usar os da nuvem / os deste PC / Cancelar" (`gog/games.ts:97-124`, `launcher.ts:159-185`) | `synced_hash` por jogo, estado `conflict` guardado até o jogador escolher, nome do PC no backup, `after_play` que refaz o backup na próxima abertura se o launcher morreu (`saves.rs:695-770`) | **Relógio** (`created`) do zip mais novo contra o do backup local mais novo |
| Rodízio | Rótulos de backup (automático ou manual) | — | — | 10 por jogo, nome `when = max(agora, mais_novo + 1)`: o relógio voltando não reordena (`saves.rs:869-904`) | 10 por jogo, todos os motivos juntos, ordem pelo relógio de quem fez |
| Restauração | Confere hash e tamanho de cada arquivo baixado antes de trocar (`restore-replacements.ts`) | — | — | Restaura por pasta, com checagem de colisão | Lê tudo antes, troca um por um sem desfazer |

O que vale trazer: **âncora por hash e guarda de abertura** (Hydra, MIT, pode adaptar com crédito); **pergunta clara
no conflito** (ideia do Heroic); **`after_play` persistido** e **nome monotônico** (ideias do RecompLauncher);
**modos de acompanhamento** (Playnite, já na 6.4).

---

## 3. Falhas

Gravidade: **Crítica** = perde dados ou segurança; **Alta** = quebra o uso; **Média**; **Baixa**.

### F1 — Crítica — Um `store-playtime.json` ou `classics.json` ilegível zera o tempo de todos os jogos de loja e clássicos
- **Onde:** `store_time.rs:29-31` (`load` → `unwrap_or_default`) e `33-44` (`add` grava o mapa inteiro); igual em
  `emulation.rs:476-490`. Sem fsync antes do rename (`store_time.rs:42-43`, `emulation.rs:488-489`).
- **Reprodução:** Com tempo guardado, escreva `{` no `store-playtime.json` (ou desligue o PC na gravação: no NTFS, sem
  fsync, o arquivo pode voltar vazio ou com zeros — **não verificado** em hardware). Abra um jogo da Steam por 2 min. O
  arquivo passa a ter só esse jogo, e o tempo de todos os outros some (Epic e Xbox voltam pela conta; jogos sem conta, não).
  O mesmo vale se o arquivo estiver travado pelo antivírus no momento da leitura.
- **Correção:** `load` devolve `Result` e diferencia "não existe" de "ilegível". Ilegível: mover para
  `store-playtime.json.corrupt-<t>`, tentar o `.bak` e **não gravar** por cima sem avisar. `sync_all` antes do rename e
  `.bak` da versão anterior (como `library::recovery`). Uma função `atomic_json::write` única para os três arquivos.
- **Teste de aceite:** arquivo com JSON quebrado + `add` → o arquivo original fica preservado ao lado, o tempo dos
  outros jogos continua e aparece um aviso; arquivo travado → `add` falha sem perder nada.

### F2 — Crítica — A nuvem escolhe o save "mais novo" pelo relógio: dois PCs se sobrescrevem em silêncio
- **Onde:** `recomp/mod.rs:227-231` (`local_is_newer` compara `created` e nome); `saves.rs:314-318`, `288-296`.
- **Reprodução A (relógio):** PC A adiantado 1 dia. B joga e envia (`created` = hoje). A abre o jogo: o backup local de
  A ("ontem à noite", mas datado de amanhã) parece mais novo, e o progresso de B é ignorado. A joga e envia (datado de
  amanhã). B abre: traz o de A por cima do seu e deixa o próprio progresso só em "Antes de trazer da nuvem", **local**.
- **Reprodução B (sem conexão):** A e B jogam com a sincronização pausada. Quando volta, quem tem o relógio mais à frente
  ganha, e o outro perde o progresso sem nenhum aviso de conflito.
- **Reprodução C:** em A, "Fazer backup agora" sem jogar, depois de B ter jogado: o backup manual tem `created` maior, A
  ignora a nuvem e o próximo envio de A sobrepõe a linha de B.
- **Efeito:** a cópia de segurança existe, mas some no rodízio depois de 10 partidas (F3). Na prática, o progresso se perde.
- **Correção:** âncora por jogo em arquivo **local** (`saves/<recomp>/sync.json`: impressão da última versão
  sincronizada, nome, PC). Regra: local = âncora e nuvem ≠ âncora → trazer; local ≠ âncora e nuvem = âncora → enviar;
  os dois ≠ âncora → **conflito**, sem trocar nada, com um cartão "Este PC (data, tempo jogado) × Outro PC (nome,
  data)" e a escolha. Guardar o nome do PC no manifesto. Antes de jogar, só trazer; depois de jogar, só enviar se a nuvem
  não mudou desde a abertura (guarda do Hydra). Isso é a E9 do plano de jogos de PC, mas precisa valer **já** para os recomps.
- **Teste de aceite:** teste com dois `Folders` e relógios falsos (A +1 dia): nenhum save de B é trocado sem estado
  `conflict`; nos três cenários acima, o resultado é "conflito", nunca uma troca silenciosa.

### F3 — Crítica — O rodízio apaga o backup errado: o recém-feito (relógio atrasado) e as cópias de segurança
- **Onde:** `saves.rs:24` (`KEEP = 10`), `343-350` (`prune` pela ordem de `list`), `288-296` (ordem por `created`).
- **Reprodução A:** com 10 backups, atrase o relógio do Windows (bateria do CMOS, dual boot com o relógio em UTC). Jogue e
  feche: o backup novo tem o menor `created`, `prune` apaga **ele mesmo** e a marca `.upload`, e nada chega à nuvem. O
  histórico diz "N arquivos copiados".
- **Reprodução B:** "Antes de trazer da nuvem", "Antes de restaurar" e "Antes de desinstalar" disputam as mesmas 10 vagas.
  Dez partidas depois, a única cópia do progresso trocado pela nuvem (F2) ou por uma restauração errada desaparece.
- **Reprodução C:** relógio adiantado uma vez (ano 2030): esse zip nunca sai do rodízio e conta para sempre como "o mais
  novo" no teste de impressão (`saves.rs:221`) e na decisão da nuvem.
- **Correção:** nome e ordem monotônicos (`max(agora, mais_novo + 1)`, como o RecompLauncher) e ordem pelo nome, não
  por `created`. Cotas separadas: automáticos (10), manuais, e cópias de segurança (as 3 últimas de cada motivo, por pelo
  menos 30 dias). Nunca apagar o backup que acabou de ser criado.
- **Teste de aceite:** com 10 backups e relógio em 2000, um backup novo sobrevive ao `prune`; 12 "Depois de jogar"
  depois de um "Antes de trazer da nuvem" ainda deixam a cópia de segurança.

### F4 — Crítica — Desinstalar e reinstalar um recomp zera o tempo de jogo
- **Onde:** `main.rs:2126-2131` (`library.remove`), `recomp/mod.rs:1346-1360` (jogo novo com `playtime_secs: 0`,
  `last_played: None`).
- **Reprodução:** jogue um recomp 10 h, desinstale (os saves ficam) e instale de novo. O total volta a "Nunca jogado"; o
  `sessions.jsonl` continua com as sessões antigas, então "Sessões" e "Mais longa" mostram números maiores que o Total.
- **Correção:** guardar o tempo à parte, por id estável (`recomp:<id>`), num arquivo de tempo que sobrevive à remoção
  (ou um "túmulo" com tempo, nota e favoritos no `library.json`), e devolver ao reinstalar. Na falta disso, reconstruir
  pela soma do `sessions.jsonl`.
- **Teste de aceite:** `remove_game` + instalação de novo → `playtime_secs` e `last_played` iguais aos de antes.

### F5 — Alta — Importar dados com o app aberto: o cache velho pode regravar o `library.json` por cima do restaurado
- **Onde:** `main.rs:1453-1467` (troca os arquivos e só reinicia 1,2 s depois); `library.rs:23-47` (cache na memória);
  `main.rs:3378-3382` (checkpoint a cada 15 s), `3651` (`checkpoint` no `RunEvent::Exit`); `store_time.rs:33-44`.
- **Reprodução:** com um jogo aberto pelo Cerberus, importe uma cópia. O checkpoint (dentro de 1,2 s) ou o do `Exit` no
  reinício chama `record_session` → `commit` grava o cache **antigo** no `library.json` e desfaz a biblioteca restaurada.
  Que o `restart()` passa pelo `RunEvent::Exit` é **não verificado**; o checkpoint de 15 s basta. Também: o reinício
  derruba o acompanhamento do jogo aberto, e o `store_time` soma o minuto corrente no arquivo restaurado.
- **Correção:** recusar a importação com partida aberta (Sessions, clássico ou loja), como o `update_install`; antes de
  trocar, travar as escritas (sinalizar `LocalLibrary` para invalidar o cache e não gravar), ou gravar a restauração como
  "pendente" e aplicar no início do próximo processo, antes de qualquer `manage`.
- **Teste de aceite:** `LocalLibrary` com cache + `data_backup::restore` + `record_session` → o arquivo continua o
  restaurado (ou a importação é recusada com jogo aberto).

### F6 — Alta — Cerberus fechado no meio da partida (Sair, falha, desligar o Windows): sem última vez, sem backup, e o próximo Jogar pode restaurar saves debaixo do jogo aberto
- **Onde:** `sessions/windows.rs:48-49` (job sem kill: o jogo continua sem dono); `background.rs:136` ("Sair" sem
  confirmação); `main.rs:3651`; `sessions.rs:158-172` (pull antes de abrir); `sessions.rs:68-73` (`reserve` só conhece a memória).
- **Reprodução:** abra um recomp, clique em Sair na bandeja e abra o Cerberus de novo. O tempo fica (até 15 s perdidos),
  mas `last_played` e `last_session_secs` não, o `sessions.jsonl` não ganha a linha, não há "Backup depois de jogar" nem
  envio. Clique em Jogar: não há sessão na memória, então **outra instância** abre, e, se a nuvem tiver um zip "mais
  novo", `pull_from_cloud` troca os saves do jogo que ainda está aberto (que depois grava os da memória por cima).
- **Correção:** (1) "Sair" e fechar com partida aberta pedem confirmação. (2) Gravar `sessions-open.json` com id, pid e
  hora de início; no próximo início, se o pid ainda vive e é o mesmo programa (hora de criação do processo), **adotar** a
  partida (abrir o processo, esperar e fazer o backup no fim); se não vive, fechar a sessão com o último checkpoint, gravar
  `last_played` e a linha do log, e fazer o backup pendente (o `after_play` do RecompLauncher). (3) Antes do pull e de
  restaurar, procurar o executável do jogo entre os processos (`store_time::running_programs`).
- **Teste de aceite:** sessão gravada como aberta + processo morto → no início vira sessão fechada com backup; processo
  vivo → `play` responde "já está aberto" e o pull não roda.

### F7 — Média — Tempo com o PC dormindo conta como jogado (já na 6.4; aqui o detalhe técnico)
- **Onde:** `sessions.rs:83` e `sessions/windows.rs:114` (`Instant`, QueryPerformanceCounter), `windows.rs:177-194`
  (`GetProcessTimes`, hora de parede), `store_time.rs:161`, `main.rs:651`.
- **Reprodução:** jogo aberto, PC em suspensão por 8 h e de volta. Se o QPC avança na suspensão (comportamento
  documentado do Windows 10 e 11 segundo a issue do Rust sobre `Instant`; **não verificado** neste PC), as 8 h entram no
  total; `GetProcessTimes` conta com certeza, porque é hora de parede.
- **Correção:** escutar `WM_POWERBROADCAST` (`PBT_APMSUSPEND` / `PBT_APMRESUMEAUTOMATIC`) ou comparar o salto entre
  ticks (relógio de parede contra `QueryUnbiasedInterruptTime`, que não conta a suspensão) e descontar. No modo
  executável acompanhado, trocar `GetProcessTimes` pelo relógio "sem suspensão".
- **Teste de aceite:** função pura `credit(ticks)` com um salto de 8 h entre ticks de 15 s → crédito ≤ 15 s; teste
  manual com `rundll32 powrprof.dll,SetSuspendState`.

### F8 — Média — "Última sessão" dos jogos de loja mostra só o último minuto
- **Onde:** `store_time.rs:38` (`last_session = seconds` de cada `add`), `165` (pedaços de 60 s), `189` (o resto).
- **Reprodução:** jogo da Steam por 1 h com o Cerberus aberto. `last_session` fica com o resto depois do último
  crédito (de 0 a 60 s), não 3600. Onde a UI mostra `last_session_secs` (cartões e página), o número está errado.
- **Correção:** `add(seconds, session_total: Option<u64>)`: o checkpoint não mexe em `last_session`; o fechamento grava a
  duração total.
- **Teste de aceite:** `add(60, None)` ×59 + `add(10, Some(3550))` → `last_session == 3550`.

### F9 — Média — Gráfico, "Sessões" e "Mais longa" erram quando o Cerberus reinicia no meio; o aviso culpa "fora do Cerberus"
- **Onde:** `store_time.rs:179-190` e `main.rs:672-675` (a linha do log só nasce no fechamento e com `started` deste
  processo); `play_log.rs:22-33`; `ui/src/game_overview.rs:115-123`.
- **Reprodução:** Steam aberta, Cerberus reinicia (atualização ou Sair) depois de 50 min, jogo fecha 10 min depois. Total
  +60 min, log +10 min. Com Cerberus fechado no fim (F6), o log não ganha nada e o cartão diz "Jogou nesses dias, mas
  fora do Cerberus", o que é falso.
- **Correção:** o log nasce no checkpoint (uma linha "aberta" atualizada no lugar, ou um `sessions-open.json` fechado no
  início seguinte, como na F6); o texto do cartão diz "partidas sem registro detalhado".
- **Teste de aceite:** sessão interrompida e retomada → `of_game` devolve uma sessão de 60 min (ou duas que somam 60).

### F10 — Média — JSONL: uma linha cortada sem `\n` engole a próxima; erros ignorados; `sessions.jsonl` sem limite
- **Onde:** `play_log.rs:29-32` (`let _`, `if let Ok`, `unwrap_or_default` grava linha vazia), `trail.rs:59-60`,
  `trail.rs:32` e `play_log.rs:42-43` (`map_while(Result::ok)` para tudo no primeiro erro de E/S ou UTF-8 inválido).
- **Reprodução:** termine o arquivo com `{"game_id":"a","ended":1` sem quebra de linha. A próxima sessão fica na mesma
  linha, as duas são descartadas na leitura e a sessão nova some. (O teste `a_damaged_line_does_not_hide_the_rest`
  coloca um `\n` depois do pedaço, por isso não pega o caso.) Um byte não-UTF-8 no meio faz `lines()` devolver `Err`, e
  `map_while` **para** a leitura: tudo depois some da tela.
- **Correção:** antes do append, se o arquivo não termina em `\n`, escrever um. Ler com `filter_map(Result::ok)`, ou bytes
  com `from_slice`. Devolver o erro do `record` para o histórico. Compactar o `sessions.jsonl` por ano ou manter um
  índice por jogo; hoje cada página de jogo lê o arquivo inteiro.
- **Teste de aceite:** pedaço sem `\n` + `record` → `of_game` devolve a sessão nova; linha não-UTF-8 no meio → as linhas
  seguintes continuam visíveis.

### F11 — Média — Gravações de JSON sem `sync_all` antes do rename
- **Onde:** `library.rs:281-283`, `store_time.rs:42-43`, `emulation.rs:488-489`, `data_backup.rs:135-137`,
  `trail.rs:72-73`, `play_log.rs:90`.
- **Reprodução:** falta de energia logo depois de um checkpoint. Sem fsync, o rename pode chegar ao disco antes dos dados
  (**não verificado** no NTFS; documentado como possível). O `library.json` tem recuperação; os outros caem na F1.
- **Correção:** um módulo `atomic_file::write(path, bytes)`: temporário no mesmo diretório, `sync_all`, `.bak` da versão
  anterior, rename (`MoveFileExW` com `MOVEFILE_WRITE_THROUGH`). Usar em todos.
- **Teste de aceite:** teste de unidade que injeta falha entre a escrita e o rename → o arquivo anterior fica intacto e é
  lido; revisão: nenhum `fs::write` direto em arquivo de dados.

### F12 — Média — Cada Jogar abre os 20 zips da nuvem: com o OneDrive "sob demanda", baixa tudo antes de abrir o jogo
- **Onde:** `recomp/mod.rs:218` → `saves.rs:314-318` → `list` (`288-296`) → `read_manifest` em todos os zips.
- **Reprodução:** pasta da nuvem no OneDrive com "Liberar espaço" ligado e 20 zips de 30 MB. Jogar fica esperando o
  OneDrive baixar 600 MB (o comportamento exato da hidratação é **não verificado**). Sem internet, a abertura falha →
  aviso "Não foi possível conferir" e o jogo abre com os saves locais; o backup no fim sobe com `created` maior e, pela F2,
  vira o "mais novo" mesmo sem ter os saves do outro PC.
- **Correção:** índice pequeno ao lado (`index.json` com nome, impressão, PC e data, gravado por quem envia) ou impressão
  no **nome** do arquivo; abrir só o zip escolhido. Ler o atributo `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` e avisar em vez
  de travar. Limite de tempo para conferir a nuvem, e lembrar "não conferido" para pedir confirmação antes do envio seguinte.
- **Teste de aceite:** pasta com 20 zips → `pull_from_cloud` abre no máximo 1 zip (contador num `Read` falso); pasta
  inacessível → o estado fica "não conferido" e o envio seguinte vira conflito, não sobrescrita.

### F13 — Média — Restaurar troca um arquivo de cada vez e não desfaz se um falhar
- **Onde:** `saves.rs:394-396`.
- **Reprodução:** backup com 3 arquivos; deixe o 2º aberto em outro programa (sem compartilhamento de exclusão). O 1º é
  trocado, o 2º falha e o 3º não é tocado: os saves ficam misturados. A cópia "Antes de restaurar" existe, mas a mensagem
  é só "Não foi possível restaurar X".
- **Correção:** antes de trocar, renomear cada destino existente para `.cerberus-old`; se algum `persist` falhar, voltar
  os já trocados; no fim, apagar os `.old`. Repetir 3× com 2 s em `ERROR_SHARING_VIOLATION` (como o plano de jogos de PC).
- **Teste de aceite:** um destino travado → os 3 arquivos ficam como estavam e o erro diz que nada mudou.

### F14 — Média — Backup manual e restauração com o jogo aberto por fora do Cerberus
- **Onde:** `main.rs:2600-2608` (`backup_now` não confere se o jogo está aberto); `main.rs:2621-2623` (`restore_backup` só
  olha `Sessions`).
- **Reprodução:** abra o recomp pelo `.exe` (ou depois da F6) e restaure um backup: o jogo grava o que tem na memória por
  cima. Backup manual no meio de uma gravação → zip com o save pela metade (a impressão é calculada antes da cópia, então
  o manifesto pode nem bater com o conteúdo, `saves.rs:220` × `249`).
- **Correção:** procurar processos com a imagem dentro de `install_dir` (já existe `running_programs`) e recusar
  restaurar; no backup manual, pedir confirmação. Calcular a impressão a partir dos bytes gravados no zip.
- **Teste de aceite:** processo de teste rodando da pasta do jogo → `restore_backup` devolve "Feche o jogo".

### F15 — Média — A UI diz "Na nuvem" e "Tudo enviado" sem saber
- **Onde:** `ui/src/pages.rs:504` (toda linha vira "Na nuvem" só porque há pasta configurada), `395-404`;
  `recomp/cloud.rs:82` e `113-119` (conta só as marcas do destino atual).
- **Reprodução:** (a) Troque a pasta da nuvem com envios pendentes: as marcas antigas ficam órfãs, o contador vai a 0 e a
  faixa diz "Tudo enviado"; os backups feitos antes da troca nunca vão para a pasta nova. (b) O último backup de um jogo é
  "Antes de restaurar" (só local): a linha diz "Na nuvem · última há 1 min". (c) "Tudo enviado" quer dizer "copiado para a
  pasta do OneDrive", não "na internet".
- **Correção:** estado por jogo vindo do backend (`synced` / `waiting` / `conflict` / `problem` / `local-only`, como o
  `sync_state` do RecompLauncher); ao trocar de pasta, reapontar as marcas ou oferecer "Enviar os N backups existentes";
  texto "Copiado para a pasta sincronizada".
- **Teste de aceite:** marca para outro destino → `pending_count` > 0 ou a marca é reapontada; linha cujo último backup
  é local mostra "Só neste PC".

### F16 — Baixa — `restore_backup` engole o erro do backup "Restaurado"
- **Onde:** `recomp/mod.rs:259` (`let _ = backup_saves(...)`), `205-207` (`let _ = cloud::retry_game`).
- **Reprodução:** pasta da nuvem cheia na hora de restaurar: a restauração diz "ok", o outro PC continua com o save antigo
  como o mais novo e, pela F2, pode trazê-lo de volta.
- **Correção:** devolver um aviso junto do resultado ("Restaurado neste PC; não foi para a nuvem: …") e registrar no histórico.
- **Teste de aceite:** destino que não aceita gravação → o toast e o histórico mostram o aviso.

### F17 — Média — Importar dados: a cópia "antes" pode falhar em silêncio, a troca não é atômica e o tempo deste PC some
- **Onde:** `data_backup.rs:105-111` (`let _ = fs::copy`), `114-139` (para no primeiro erro, com metade trocada), `103`
  (`before-restore-*` nunca é limpo).
- **Reprodução:** disco cheio: as cópias "antes" falham sem aviso, a troca segue e para no meio. Importar uma cópia
  antiga substitui `store-playtime.json`, `classics.json` e `sessions.jsonl` inteiros: as horas jogadas desde a
  exportação somem do total (ficam só na pasta `before-restore`, que a UI não menciona — **não verificado** na tela).
- **Correção:** falhar se a cópia "antes" falhar; montar tudo num diretório temporário e trocar no fim; **mesclar** os
  arquivos de tempo (máximo por jogo, ou soma das sessões sem repetir por `(game_id, ended)`); guardar só as 3 últimas
  pastas `before-restore`; dizer onde ficou a cópia.
- **Teste de aceite:** cópia antiga com menos tempo → o tempo de cada jogo fica no máximo dos dois; cópia "antes" que
  falha → nenhum arquivo é trocado.

### F18 — Baixa — Contagem por pasta das lojas: em dobro com um jogo adicionado à mão, e "jogando" com ajudantes
- **Onde:** `store_time.rs:73-84`, `94-99` (no máximo 4096 pids, cortado sem aviso).
- **Reprodução:** adicionar à mão o `.exe` de um jogo da Steam e abri-lo pelo Cerberus: conta em `local:…` e em
  `steam:…`. Um anticheat ou launcher que fica rodando da pasta do jogo mantém a partida aberta.
- **Correção:** ignorar os processos já acompanhados por `Sessions` (pids do job) e aplicar `is_helper` na lista de lojas;
  pedir mais pids quando `used == len`.
- **Teste de aceite:** `running()` com um pid do job e um crash handler → nenhum jogo de loja conta.

### F19 — Média — Fechar a janela com um jogo de loja aberto fecha o Cerberus e para de contar
- **Onde:** `background.rs:35-37` (`playing` só olha `Sessions` e os clássicos), `44-53`; `close_to_tray` é falso por
  padrão (`settings.rs:76-77`).
- **Reprodução:** abra um jogo da Epic pela loja, feche a janela do Cerberus: o app sai e a partida não termina de ser
  contada (o texto do módulo promete o contrário). Steam, Epic e Xbox recuperam pela conta, quando ela está ligada; jogos
  sem conta não.
- **Correção:** `store_time` publica o conjunto `playing` num estado compartilhado, e `playing()` o inclui.
- **Teste de aceite:** com um jogo de loja em `playing`, `on_close` esconde em vez de sair.

### F20 — Baixa — O checkpoint grava no disco com o mutex de `Sessions` travado
- **Onde:** `sessions.rs:80-88`, `90-95`.
- **Reprodução:** disco lento ou antivírus segurando o `library.json`: `running_games`, o menu por cima do jogo e
  `is_running` esperam a gravação.
- **Correção:** copiar `(id, started, credited)` para fora do lock, gravar e depois atualizar `credited` (conferindo que a
  rodada é a mesma).
- **Teste de aceite:** `LocalLibrary` falsa que demora 2 s → `is_running` responde em menos de 50 ms.

### F21 — Baixa — Gráfico: sessão inteira no dia em que terminou; "Esta semana" = 7 dias corridos
- **Onde:** `ui/src/game_overview.rs:59-73`, `96`.
- **Reprodução:** jogar das 23 h às 3 h → 4 h no dia seguinte; "Esta semana" numa segunda-feira inclui a semana passada.
- **Correção:** dividir a sessão por meia-noite (com `ended - seconds`); chamar a linha de "Últimos 7 dias", ou usar a
  semana do calendário.
- **Teste de aceite:** sessão 23 h–3 h → 1 h num dia e 3 h no outro.

### F22 — Baixa — O PC pode dormir durante a preparação do jogo e o envio para a nuvem
- **Onde:** `awake.rs:16` (só `Installing::any_active`); `sessions.rs:310-418` (a preparação, que pode levar minutos,
  não entra em `Installing` — **não verificado** se outro caminho a registra).
- **Correção:** um contador de "trabalho que segura o sono" usado pela instalação, pela preparação e pelo envio de backups grandes.
- **Teste de aceite:** durante `prepare_then_start`, `awake` faz o pedido.

### F23 — Baixa — Restos que nunca saem: `.part`, `.cerberus-*.tmp` e zips ilegíveis
- **Onde:** `saves.rs:236-252` (o `.part` some no drop, não numa falha do processo), `386`, `293` (zip sem manifesto
  legível fica fora de `list` e nunca entra no rodízio), `cloud.rs:29` (temporários na pasta da nuvem).
- **Correção:** varrer `.part` e `.tmp` com mais de 1 dia em `saves/` e na pasta da nuvem no início; listar os zips
  ilegíveis como "danificados" na UI.
- **Teste de aceite:** um `.part` velho some no início; um zip quebrado aparece marcado.

### F24 — Baixa — Textos da restauração que prometem demais
- **Onde:** `ui/src/backups.rs:115-118` ("Os saves de antes ficaram num backup novo", mesmo quando a cópia de segurança
  deu `Ok(None)` porque os saves já estavam num backup), `96-105` (o botão fica armado para sempre), `78` ("Os 10 mais
  recentes ficam guardados" — vale para todos os motivos, F3).
- **Correção:** o backend devolve se criou cópia ou qual backup já tinha os saves; desarmar o botão em 5 s ou ao perder o foco.
- **Teste de aceite:** restaurar sem mudança → o texto diz "Os saves atuais já estavam no backup de {data}".

### F25 — Baixa — O histórico guarda 500 linhas para todos os jogos juntos
- **Onde:** `trail.rs:16-17`; as conquistas dos emuladores escrevem uma linha cada (`main.rs:660`).
- **Reprodução:** uma noite de RetroAchievements apaga o histórico (inclusive o "Backup depois de jogar" com erro) dos
  outros jogos.
- **Correção:** limite por jogo (por exemplo 100) ou por tipo, ou conquistas fora do `trail`.
- **Teste de aceite:** 600 linhas de "Conquista" no jogo A não apagam as 5 linhas do jogo B.

### F26 — Média — Sem executável acompanhado: o tempo conta desde o launcher, e um launcher que entrega o jogo para fora do job encerra a partida na hora
- **Onde:** `sessions/windows.rs:78-83` e `114`; `windows.rs:49` (job sem `BREAKAWAY`, mas processos abertos pelo Explorer,
  por `steam://`, por COM ou como app UWP não entram).
- **Reprodução:** um jogo adicionado à mão cujo `.exe` é um launcher que chama `steam://run/…` e sai: a partida dura 2 s,
  o "Backup depois de jogar" roda antes do jogo começar e o tempo real se perde (a Steam conta, mas só se o id casar).
  Jogo com menu do launcher aberto 10 min antes de jogar: 10 min a mais.
- **Correção:** a 6.4 já prevê os modos; aqui, dois detalhes: se o job esvazia em menos de 30 s, acompanhar por pasta (como
  as lojas) durante 2 min antes de fechar a partida, e não fazer backup de partida de menos de 30 s.
- **Teste de aceite:** launcher falso que abre um filho fora do job (`cmd /c start` via `explorer.exe`) → a partida
  acompanha o filho pela pasta.

---

## 4. Lacunas

- **Nenhum aviso de conflito existe hoje.** Não há estado `conflict`, nome do PC nem escolha (F2). É o maior buraco em
  relação a Hydra, Heroic e RecompLauncher.
- **Reinstalar não oferece os saves de volta** quando não há nuvem: a pasta `{install}` some com o jogo e os backups ficam,
  mas não vi oferta de restaurar na instalação (**não verificado** na UI).
- **Backups do recomp fora do zip dos dados** (`saves/` não entra no `data_backup`); levar o Cerberus para outro PC sem
  pasta de nuvem perde os backups. Decidir se entra (com limite de tamanho) ou se a tela avisa.
- **Pasta da nuvem sem validação:** nada impede escolher a própria pasta de dados do Cerberus, um disco removível ou uma
  pasta que nenhum sincronizador vigia.
- **Sem ver os backups de outro PC** antes de trazê-los: a lista mostra só a pasta local.
- **Desligar o Windows com jogo aberto:** se o `RunEvent::Exit` chega no `WM_ENDSESSION` é **não verificado**; o
  checkpoint de 15 s cobre o tempo, não a última vez, o log nem o backup (F6).
- **Testes que faltam:** F1 (arquivo ilegível), F2 (dois PCs e relógio falso), F3 (rodízio com relógio atrasado e cópias
  de segurança), F4 (reinstalar), F5 (importar com cache), F6 (sessão aberta no reinício), F10 (linha sem `\n`), F13
  (restauração que falha no meio), e nenhum teste de UI dos estados da página Saves.
- **Uma costura de relógio** (`Clock` com `now()` e `monotonic()`) em `saves`, `store_time` e `sessions` deixaria F2, F3,
  F7 e F8 testáveis sem esperar o tempo passar.

---

## 5. Ordem sugerida

1. **F1 + F11** (P): `atomic_json` com fsync, `.bak` e "ilegível ≠ vazio" para `store-playtime`, `classics` e `library`.
   Pequeno, e tira a perda total.
2. **F3** (P): nome monotônico, ordem pelo nome e cotas por motivo. Precisa vir antes da F2, porque a cópia de segurança
   é a rede da F2.
3. **F4** (P): o tempo sobrevive a desinstalar.
4. **F5** e **F17** (P/M): importar recusa com partida aberta, troca tudo de uma vez e mescla o tempo.
5. **F2 + F12 + F15 + F16** (M): âncora local, conflito com escolha, índice da nuvem e estado por jogo na página Saves.
   Fazer junto com a E9 do plano de jogos de PC, para ter um só mecanismo.
6. **F6 + F9** (M): sessão aberta persistida, adoção do processo no início, confirmação em Sair, log no checkpoint.
7. **F13, F14, F19** (P cada): restauração que desfaz, checagem de processo por pasta, bandeja com jogos de loja.
8. **F7, F8, F26** (M, junto da 6.4): suspensão, `last_session` e entrega para fora do job.
9. **F10, F18, F20-F25** (P): limpezas.
