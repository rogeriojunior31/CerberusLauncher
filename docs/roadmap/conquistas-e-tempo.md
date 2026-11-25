# Conquistas e tempo de jogo das contas: o que falta para ficar completo

Revisão de 25/11/2026 da entrega de 23 a 25/11 (branch `ajustes-rapidos`). Fiz só leitura do código: não compilei,
não rodei testes e não abri o app. Onde está escrito **não verificado**, a afirmação depende de uma API ou de um
comportamento que não testei.

Este documento não repete o que já está no `ROADMAP.md`: Steam sem chave, biblioteca da família, troféus de recomps,
RPCS3 e shadPS4, foto no momento da conquista e separar DLC (6.1), "seu ano" (7.3), som do aviso (7.4). Ele cuida de
deixar **robusto** o que já foi entregue.

## Estado atual

| Peça | Arquivo | O que faz hoje |
| --- | --- | --- |
| Banco local | `src-tauri/src/achievements/store.rs` | SQLite `achievements.db` na pasta de dados (WAL, `busy_timeout` de 5 s). Tabelas `games`, `lists`, `unlocks`, `played` e `stamps`, todas com a chave `(source, account, …)` e o conteúdo em JSON. O progresso só cresce (`merge`) e a data mais antiga vence (`keep_unlocks`; 0 e 1 querem dizer "sem data"). Há 5 min entre sincronizações (`SYNC_EVERY`) e, depois de uma recusa, uma espera de 2 min que dobra até 6 h (`refused`). `highlights` monta Próximas e Mais raras só com listas que têm raridade. `forget` apaga uma conta. |
| Orquestração | `src-tauri/src/achievements/mod.rs` | `hub` junta Steam (cache local e Web API opcional), RA, Xbox e Epic. `sync_xbox` e `sync_epic` passam pelo mutex `SYNCING`. `account_times` aplica o tempo na Biblioteca. `game` monta a página de um jogo. |
| Steam local | `steam.rs`, `live.rs` | Lê `appcache/stats` e `localconfig.vdf`, sem rede. A raridade vem do endpoint público e fica guardada por 7 dias. O aviso ao vivo vigia a data do `UserGameStats_*.bin`. |
| Steam Web API | `steam_api.rs` | `GetOwnedGames` (guardado 5 min), `GetTopAchievementsForGames` (100 jogos por consulta), `GetPlayerAchievements` (25 por atualização, guardado até jogar de novo, com idioma), `GetSchemaForGame` (30 dias, por idioma). |
| RetroAchievements | `retro.rs`, `classic.rs` | Progresso de todos os jogos (500 por página), desbloqueios dos últimos 30 dias, pontos. A lista do jogo só vem quando ele é aberto. |
| Xbox | `xbox.rs` | titlehub (1 consulta), minutos por user stats (100 títulos por consulta), todas as conquistas em páginas de 1000 com contrato 4 (com raridade) na primeira vez, depois até 8 listas por sincronização. 300 ms entre consultas. |
| Epic | `epic.rs` | Tempo de jogo de tudo em 1 consulta. Para cada jogo novo ou jogado de novo, 2 consultas GraphQL (definições e progresso do jogador), no máximo 40 jogos por sincronização. |
| Biblioteca | `src-tauri/src/main.rs` (`list_games`, perto da linha 1639) | Aplica `account_times` e dispara `sync_accounts` em segundo plano, uma de cada vez (`AtomicBool`). Emite `catalog-changed` quando consultou alguma loja. |
| Rede | `src-tauri/src/net.rs` | Põe o site em descanso depois de 429 ou 5xx (de 30 s até 1 h, ou o `Retry-After` em segundos). Esse descanso fica só na memória. |
| UI do hub | `ui/src/achievements.rs` | Primeiro mostra o banco (`online=false`), depois atualiza (`online=true`). Tem botão Atualizar, avisos por fonte (`problems`) e tenta de novo sozinho em 3 min quando a Steam pede pausa. |
| Aba do jogo | `ui/src/game.rs` (linhas 191-192) | Epic pela `namespace`, Xbox por `pfn:<family>`. |
| Big Picture | `ui/src/tv/achievements.rs` | Só `achievements_hub(false)`, ou seja, só lê o banco. Mostra três prateleiras. Só os jogos da Steam abrem a página do jogo. |
| Aviso | `src-tauri/src/unlock_toast.rs`, `ui/src/unlock.rs` | Janela por cima do jogo. Recebe avisos da Steam (`live.rs`) e do RA (log dos emuladores). |

Testes que existem hoje: `store.rs` (2: nada se perde, espera depois de recusa), `xbox.rs` (2: datas, leitura de
títulos e conquistas), `epic.rs` (1: leitura com secretas), `live.rs` (1), `retro.rs` e `steam.rs` (leitura), e um teste
`#[ignore]` que lê a Steam real. **Não há nenhum teste de orquestração** (`sync_xbox`, `sync_epic`, `hub`,
`account_times`), de concorrência nem de UI.

## Lacunas para ficar completo

A ordem vai do impacto maior para o menor. Esforço: P (horas), M (1 a 2 dias), G (mais que isso).

### L1. Um jogo que falha derruba a sincronização da conta inteira (e volta a falhar sempre) — M

- **Problema.** Em `sync_xbox` (`mod.rs`, linha 391: `xbox::title_list(...).await?`) e em `sync_epic` (linha 456:
  `epic::game_list(...).await?`), basta um jogo dar erro para a função inteira devolver `Err`. Aí `keep_games` não roda
  e a conta entra em `refused`. Na próxima vez, o mesmo jogo continua desatualizado e é o primeiro da fila: no Xbox a
  fila vai do jogado por último para o mais antigo, na Epic do maior tempo para o menor. A espera cresce até 6 h e
  **nenhum outro jogo da conta atualiza**. A primeira leitura do Xbox (`all_lists`, de 14 páginas ou mais) também
  recomeça da página 1 quando uma delas falha.
- **Por que importa e como reproduzir.** Um namespace da Epic sem sandbox de conquistas, um título do Xbox que responde
  404 ou 403 (jogo removido da loja, título de 360) ou um `errors` no GraphQL deixam a conta parada para sempre. Num
  teste, um servidor falso que devolve 500 só para um `titleId` reproduz o problema. Que namespace ≠ sandbox acontece
  é **não verificado**: o Hydra descobre o `achievementSandboxId` de cada jogo pelo servidor dele
  (`hydra/src/main/services/epic/achievement-source.ts`), o que indica que nem sempre os dois coincidem.
- **Como resolver.**
  - Separar "a conta falhou" (sessão, titlehub, 429) de "um jogo falhou" (404, 403, GraphQL sem dados).
  - Quando um jogo falha, gravar uma marca por jogo (`stamps` com `name = "fail:<key>"` e um contador), seguir para o
    próximo e pular esse jogo até a espera dele vencer. Depois de umas 3 falhas, tratar como "sem conquistas" até o
    jogo ser jogado de novo.
  - Gravar `keep_games` do que deu certo mesmo quando a conta falha no meio.
  - No `all_lists`, guardar o `continuationToken` e as listas lidas até ali (`stamps`, `all_lists_cursor`) para
    retomar da página seguinte.
  - Ideia do Hydra (MIT): `completed.set(sandbox, null)` e `pendingObjectIds` com prioridade na volta seguinte.
- **Arquivos.** `src-tauri/src/achievements/mod.rs`, `xbox.rs`, `epic.rs`, `store.rs`.
- **Critério de aceite.** Com um jogo que sempre falha entre 50, uma sincronização grava os outros 49 e a conta fica
  `synced`, não `refused`. Na sincronização seguinte, o jogo que falhou não é o primeiro a ser consultado.
- **Teste sugerido.** Precisa de uma costura: uma URL base injetável (ou um trait `Fetch`) em `xbox::get` e
  `epic::ask`. Com isso, `sync_epic_continues_past_a_broken_game` e `xbox_first_fill_resumes_from_the_cursor`.

### L2. Uma corrida apaga a espera depois de uma recusa — P

- **Problema.** `sync_xbox` e `sync_epic` devolvem `Ok(())` quando, já com o mutex, descobrem que não é hora
  (`if !db.due(scope) { return Ok(()) }`, linhas 357 e 436). Quem chamou não distingue isso de uma sincronização que
  deu certo e chama `db.synced(&scope)` (linhas 501, 510, 607, 621). `synced` zera `failures` e `wait_until`.
- **Como reproduzir.** A Biblioteca chama `sync_accounts`, vê a conta pronta e espera o mutex. Enquanto isso o hub
  sincroniza, leva um 429 e grava `refused`. A Biblioteca pega o mutex, vê que não é hora, devolve `Ok` e grava
  `synced`. Resultado: a espera, que devia dobrar, some, e o carimbo `synced` mente.
- **Como resolver.** Devolver um enum `Synced | Skipped | Failed(e)` e só chamar `synced` ou `refused` dentro da
  própria função, com o mutex na mão. Os chamadores ficam só com a mensagem.
- **Arquivos.** `src-tauri/src/achievements/mod.rs`.
- **Critério de aceite.** Depois de `refused`, uma segunda chamada simultânea não muda `wait_until` nem `failures`.
- **Teste sugerido.** `skipped_sync_keeps_the_wait`, chamando duas vezes com o banco em memória e uma fonte falsa que
  falha.

### L3. A conta é identificada pelo nome de exibição, e a Steam por "" — M

- **Problema.**
  - `Scope.account` é `xbox_user` (gamertag) e `epic_user` (nome de exibição, ou "Conta Epic" quando a Epic não manda
    nome) (`main.rs`, linhas 1758 e 1873). A Steam usa `Scope::new(Steam, "")` (`mod.rs`, linhas 559 e 776).
  - Trocar a gamertag, que o Xbox permite, cria um escopo novo. O hub "perde" os desbloqueios guardados (que ficam
    órfãos), e como `all_lists_v4` não existe no escopo novo, a conta inteira é baixada de novo.
  - Duas contas Epic sem nome caem no mesmo escopo.
  - Duas contas Steam no mesmo PC (família) se misturam para sempre no escopo "": `steam::account` escolhe a de
    arquivo mais recente.
  - Entrar com outra conta sem sair antes não chama `forget`, e a conta antiga fica no banco.
  - `EPIC_SESSION` (`mod.rs`, linha 407) guarda `(access, account)` por 30 min e não é zerada no logout nem no login.
    Quem sai da Epic e entra com outra conta nesse intervalo grava os dados da conta antiga no escopo da nova.
- **Como resolver.**
  - Usar ids estáveis: o `xuid` do Xbox (já vem na `Session`), o `account_id` da Epic (já vem do token) e o `account.id`
    da Steam (pasta `userdata/<id>`). Ao entrar, gravar `xbox_id` e `epic_id` nas configurações.
  - Migrar o que já existe: na abertura, um `UPDATE` de `account` para o id quando o nome guardado for o da conta atual.
  - Zerar `EPIC_SESSION` e `xbox::SESSION` no login e no logout, com uma função `achievements::signed_out(source)`.
  - No login de uma conta diferente da anterior, chamar `forget` da antiga.
- **Arquivos.** `store.rs`, `mod.rs`, `main.rs` (login e logout de Xbox e Epic), `steam.rs`, `src-tauri/src/settings.rs`.
- **Critério de aceite.** Trocar a gamertag não muda o número de conquistas no hub e não dispara uma releitura total.
  Duas contas Steam do mesmo PC aparecem separadas. Sair e entrar com outra conta Epic em menos de 30 min mostra só a
  conta nova.
- **Teste sugerido.** `scope_survives_a_name_change` (migração em memória), `epic_session_is_forgotten_on_logout`.

### L4. Sessão vencida aparece como "loja ocupada" — P/M

- **Problema.** Um refresh token revogado ou vencido, um 401 ou um 403 viram a mesma coisa que um 429: `refused`, com
  espera de até 6 h, e a mensagem genérica "O Xbox Live não entregou as conquistas" ou "A Epic não entregou…". O
  jogador não fica sabendo que precisa entrar de novo. A validade do refresh token da Epic é **não verificada**: o
  Legendary sugere algo em torno de horas, e se for isso, quem fica um dia sem abrir o Cerberus cai aqui sempre. Perfil
  privado do Xbox, com conquistas escondidas nas configurações de privacidade, também cai como erro genérico
  (**não verificado**: qual status o Xbox devolve nesse caso).
- **Como resolver.** Classificar os erros (`NeedsLogin`, `Private`, `Busy`, `Offline`, `Broken`). `NeedsLogin` não faz
  backoff: grava `needs_login` em `stamps` e o hub mostra "Entre de novo" com um botão para Configurações. `Private`
  mostra como mudar a privacidade, como a Steam já faz (`steam_api::NOT_PUBLIC`). Ideia do Hydra:
  `fetchEpicWithRetry` (`hydra/src/main/services/epic/source-retry.ts`) renova o token uma vez num 401 antes de
  desistir e marca `requiresReconnect`.
- **Arquivos.** `xbox.rs` (`get`), `epic.rs` (`ask`, `playtimes`), `mod.rs` (`epic_session`), `src-tauri/src/xbox_account.rs`,
  `src-tauri/src/epic_account.rs`, `shared/src/lib.rs`, `ui/src/achievements.rs`.
- **Critério de aceite.** Com o refresh token apagado à mão nas configurações (ou trocado por um inválido), o hub mostra
  "Entre de novo na Epic" na primeira tentativa, e nenhuma espera de 6 h é gravada.
- **Teste sugerido.** Classificar respostas fixas (401, 403 com corpo de privacidade, 429, 500) em
  `xbox::classify` e `epic::classify`.

### L5. Data desconhecida aparece como 31/12/1969 — P

- **Problema.** Xbox e Epic gravam `unlocked_at = Some(1)` quando a conquista está desbloqueada mas a data não veio
  (`xbox.rs`, linha 162; `epic.rs`, linha 86). A UI só trata o 0:
  - `ui/src/achievements.rs`, linha 786: `if t == 0 { "Desbloqueada" } else { format::date_time(t) }`;
  - `ui/src/game_overview.rs`, linhas 382-383, e `ui/src/tv/achievements.rs`, linha 120, usam `format::ago(t)`, que
    para 1 cai em `date_time(1)`.
  Num fuso de -3 h, isso mostra "31/12/1969".
- **Como resolver.** Uma função só, `format::unlocked_when(t)`, que devolve "Desbloqueada" para `t <= 1`, usada nos
  três lugares. A ordem "Recentes" já põe esses por último. Vale também conferir conquistas de 360 que vêm sem data.
- **Arquivos.** `ui/src/format.rs`, `ui/src/achievements.rs`, `ui/src/game_overview.rs`, `ui/src/tv/achievements.rs`.
- **Critério de aceite.** Nenhuma tela mostra um ano anterior a 1971.
- **Teste sugerido.** Um teste de unidade da função nova (`t = 0`, `1`, um valor real).

### L6. Conquistas revogadas ou zeradas nunca saem do hub — M

- **Problema.** `merge` faz `unlocked = max(new, old)` e `keep_unlocks` nunca apaga nada. Se o jogador zera as
  estatísticas da Steam ou o desenvolvedor revoga uma conquista (acontece no Xbox e na Steam), o hub continua contando,
  e a página do jogo da Steam, que lê ao vivo, mostra outro número. Um jogo pode aparecer em Platinados com 30/31 na
  própria página.
- **Como resolver.**
  - Manter o histórico (a promessa "nunca perde" vale para o que foi visto), mas separar "visto um dia" de "estado
    atual da loja".
  - Quando a fonte manda a lista **completa e com autoridade** (lista inteira do Xbox, Epic, Steam local ou Web API),
    gravar o progresso atual dela no `games` em vez do `max`, e marcar em `unlocks` uma coluna `revoked_at` para o que
    sumiu.
  - O `max` fica só para leituras parciais (titlehub sem total, `tops` sem datas).
  - O hub conta o estado atual e, opcionalmente, mostra "N revogadas".
- **Arquivos.** `store.rs` (`merge`, `keep_list`, coluna nova, o que pede a L11), `mod.rs`.
- **Critério de aceite.** Zerar uma conquista de teste com uma ferramenta oficial do desenvolvedor, ou simular uma
  lista nova sem um id, faz o hub e a página mostrarem o mesmo número. O desbloqueio antigo continua consultável como
  revogado.
- **Teste sugerido.** `authoritative_list_lowers_progress_but_keeps_history`.

### L7. As listas guardadas não sabem em que idioma estão — P/M

- **Problema.** `lists.data` e `unlocks.data` guardam nome e descrição no idioma da leitura: Xbox pelo
  `Accept-Language` do mercado (`xbox.rs`, linha 42), Epic `pt-BR` ou `en-US` (`epic.rs`, linha 68), Steam pela
  preferência do momento. Se o jogador troca o idioma do Cerberus, as listas do Xbox e da Epic, Próximas, Raras e
  Últimas continuam no idioma antigo até o jogo ser jogado de novo, e as telas ficam misturadas. A Steam Web API já
  resolve isso no cache dela (`MineFile.lang`, `schema-<app>-<lang>.json`), e o Hydra guarda `language` junto da
  memória de conquistas (`hydra/src/main/services/achievements/reconcile-achievement-memory.ts`).
- **Como resolver.** Uma coluna `lang` em `lists`. Uma lista com idioma diferente do atual conta como desatualizada:
  na Epic, só a consulta de definições, porque o progresso não depende de idioma; no Xbox, a lista do jogo quando ele
  é aberto, além de entrar na fila de `XBOX_LISTS_PER_SYNC`. `keep_unlocks` atualiza o texto, mas nunca a data.
- **Arquivos.** `store.rs`, `mod.rs`, `epic.rs`, `xbox.rs`.
- **Critério de aceite.** Trocar para inglês e abrir a aba Conquistas de um jogo Xbox mostra os textos em inglês. Nas
  sincronizações seguintes, o hub converge sem baixar a conta inteira.
- **Teste sugerido.** `list_in_other_language_is_stale`.

### L8. Steam e RA quase nunca alimentam Próximas e Mais raras; a página da Epic só lê o banco — M

- **Problema.**
  - `highlights` só usa listas guardadas que têm raridade. A Steam só grava a lista no banco quando o jogo é aberto
    (`mod.rs`, linha 776), e o hub nunca grava as listas que `steam_part` já leu do disco. O RA também só grava ao abrir
    o jogo. Na prática, Próximas e Raras são quase só Xbox e Epic, e a Steam, que é a maior fonte, fica de fora.
  - A aba Conquistas de um jogo Epic (`game()`, linhas 788-795) só lê o banco. Se o jogo ainda não entrou nas 40 por
    sincronização, aparece "Esse jogo não está nas conquistas da Epic." O Xbox, nessa situação, busca na hora.
- **Como resolver.**
  - **Steam:** em `hub`, gravar com `keep_list` as listas completas que `steam::read_game` já devolve (custo zero de
    rede). Preencher a raridade aos poucos: até N jogos em andamento por atualização, chamando `steam::rarity`
    (1 consulta por jogo, guardada 7 dias).
  - **RA:** depois de `retro::games`, ler `API_GetGameInfoAndUserProgress` dos até 5 jogos jogados por último cuja lista
    está velha (pelo `last_played`).
  - **Epic:** em `game()`, buscar na hora com `epic::game_list` quando não há lista ou ela está velha, como o Xbox faz.
- **Arquivos.** `mod.rs` (`hub`, `game`), `steam.rs`, `retro.rs`.
- **Critério de aceite.** Com só a Steam conectada e um jogo em andamento, depois de uma atualização o hub mostra
  Próximas e Mais raras. Um jogo Epic nunca sincronizado abre a aba com a lista.
- **Teste sugerido.** `hub_keeps_steam_lists_from_disk` (Steam falsa em pasta temporária, como os testes de `steam.rs`).

### L9. Estados de UI por fonte: quando sincronizou, quando tenta de novo, forçar — M

- **Problema.**
  - `AchievementsHub` só tem `problems: Vec<String>` e `retry_soon`. Não diz quando cada fonte sincronizou pela última
    vez, se está esperando depois de uma recusa (e até quando) ou se precisa de login.
  - O botão **Atualizar** não força nada: `hub` respeita `due()` (5 min) e a espera. Clicar logo depois de uma
    atualização não consulta ninguém e não avisa o jogador.
  - Se a Biblioteca está fazendo a primeira sincronização do Xbox, que dura minutos, o hub fica em "Atualizando" preso
    no mutex, sem explicar.
  - Não existe aviso de "Epic não conectada", que existe para Xbox e RA.
  - A Big Picture nunca atualiza (`online=false`), não mostra `problems` nem Próximas ou Raras, e a mensagem de vazio
    só fala de Steam e RA. `library_id` só abre a página dos jogos da Steam, embora jogos Xbox e Epic da Biblioteca
    também tenham página.
- **Como resolver.**
  - Acrescentar `sources: Vec<SourceStatus { source, synced_at, wait_until, needs_login, syncing, error }>` no tipo
    compartilhado, preenchido a partir de `stamps`.
  - No hub, uma linha discreta por cartão de plataforma: "Atualizado há 3 min", "Tentando de novo às 14:20", "Entre de
    novo".
  - `achievements_hub(force: true)` ignora `SYNC_EVERY`, mas respeita `wait_until` com um piso de 60 s, e mostra "a loja
    pediu para esperar até…" quando for o caso.
  - Usar `try_lock` no hub e, quando a Biblioteca já está sincronizando, mostrar "Sincronizando a conta Xbox…" e
    escutar `catalog-changed` para recarregar.
  - Na TV, mostrar a última sincronização e os problemas, abrir a página dos jogos Xbox e Epic da Biblioteca
    (`epic:<app_name>`, Xbox pela `pfn`) e corrigir a mensagem de vazio.
- **Arquivos.** `shared/src/lib.rs`, `mod.rs`, `src-tauri/src/main.rs` (`achievements_hub`), `ui/src/achievements.rs`,
  `ui/src/tv/achievements.rs`, `ui/src/api.rs`.
- **Critério de aceite.** Cada cartão de plataforma mostra quando sincronizou. Atualizar 1 min depois da última vez
  consulta de novo, ou explica por que não. Um 429 aparece com o horário da próxima tentativa.
- **Teste sugerido.** `source_status_reads_stamps` no `store.rs`. Na UI, conferir com captura de tela, como pede a
  memória do projeto.

### L10. Concorrência no SQLite: escrita que some em silêncio — P/M

- **Problema.**
  - Cada chamada abre sua própria `Connection` (hub, `sync_accounts`, `game`, `account_times`).
  - O mutex `SYNCING` cobre só Xbox e Epic. As escritas da Steam e do RA no hub e as de `game()` correm em paralelo.
  - As transações de `keep_games`, `keep_unlocks` e `keep_played` são DEFERRED e leem antes de escrever. No modo WAL,
    passar de leitura para escrita com outra conexão escrevendo pode dar `SQLITE_BUSY` na hora, sem passar pelo
    `busy_timeout`. Todos os erros são engolidos com `let _ =`, então um lote de desbloqueios pode simplesmente não
    entrar (**não verificado** na prática).
  - `account_times` faz leitura bloqueante do SQLite dentro do comando assíncrono `list_games`.
- **Como resolver.**
  - `transaction_with_behavior(TransactionBehavior::Immediate)` nas escritas.
  - Registrar os erros (`eprintln!` ou `trail`) em vez de descartá-los.
  - Opcional: uma conexão única de escrita num `Mutex` gerenciado pelo Tauri, com as leituras à parte.
  - `account_times` dentro de `spawn_blocking`, como já é feito com `steam_playtimes`.
- **Arquivos.** `store.rs`, `mod.rs`, `main.rs`.
- **Critério de aceite.** Duas tarefas escrevendo 1000 desbloqueios cada, ao mesmo tempo, em conexões separadas sobre o
  mesmo arquivo, terminam com 2000 linhas.
- **Teste sugerido.** `parallel_writers_lose_nothing` com `tempfile` e duas threads.

### L11. Esquema sem versão — P

- **Problema.** Só existe `CREATE TABLE IF NOT EXISTS`. Não há `PRAGMA user_version` nem migrações. A versão
  improvisada é o carimbo `all_lists_v4`. L3, L6 e L7 precisam de colunas novas, e uma coluna nova num banco antigo
  quebra os `INSERT`.
- **Como resolver.** `user_version` com uma lista ordenada de migrações aplicadas numa transação ao abrir, e um teste
  que abre um banco "v1" de exemplo. Rodar as migrações uma vez por processo (um `OnceLock` por caminho), em vez de
  repetir o `execute_batch` a cada `Store::open`.
- **Arquivos.** `store.rs`.
- **Critério de aceite.** Um banco criado pela versão de 25/11 abre na versão nova sem perder linhas, e o
  `user_version` sobe.
- **Teste sugerido.** `opens_a_v1_database`.

### L12. O banco fica fora do backup e acumula órfãos — P/M

- **Problema.**
  - `data_backup.rs` exporta `library.json`, `sessions.jsonl`, `store-playtime.json` e outros, mas **não**
    `achievements.db`. A promessa "um desbloqueio visto nunca se perde" não sobrevive a uma troca de PC, ao contrário
    do Hydra, que guarda no servidor dele.
  - Escopos órfãos (L3), contas trocadas sem logout e os arquivos `cache/achievements/mine-*.json`,
    `schema-*-<lang>.json` e `steam-rarity-*.json` só crescem.
  - Estimativa (**não verificada**): para 250 jogos do Xbox com cerca de 40 conquistas e uns 400 bytes de JSON cada,
    `lists` fica com uns 4 MB e `unlocks` com mais 1 ou 2 MB. É aceitável, mas um ícone repetido em
    `icon`/`icon_locked` dobra o texto.
- **Como resolver.**
  - Incluir uma cópia consistente no backup com `VACUUM INTO 'achievements-backup.db'`, porque copiar o arquivo com o
    WAL aberto não é seguro, e restaurar fechando as conexões antes.
  - Limpeza em Configurações ("Apagar conquistas de contas desconectadas").
  - `PRAGMA wal_checkpoint(TRUNCATE)` ao sair.
- **Arquivos.** `src-tauri/src/data_backup.rs`, `store.rs`, Configurações na UI.
- **Critério de aceite.** Exportar e restaurar num PC limpo mantém o hub igual, offline.
- **Teste sugerido.** Estender `a_copy_leaves_keys_out_and_a_restore_keeps_this_pcs` com um banco de conquistas.

### L13. Custo de consultas maior que o necessário — M

- **Problema.**
  - **Epic.** Toda vez que um jogo é jogado de novo, pergunta as **definições** de novo (elas quase nunca mudam) e o
    progresso. A primeira sincronização pergunta também todos os jogos **nunca jogados** (`secs = 0`): uma conta com
    300 a 500 jogos gratuitos dá 600 a 1000 consultas, em 8 a 13 rodadas de 40 jogos, uma a cada 5 min ou mais.
  - **Xbox.** `minutes` ignora erros e 429 (o `break` cala) e não respeita a pausa do `net.rs` como erro.
  - **Steam.** `fill_recent_icons` pode pedir até 40 schemas na primeira atualização. `fill_names` pergunta de novo, a
    cada atualização, os apps que a loja não reconhece, porque não guarda a falha.
- **Como resolver.**
  - Epic: guardar as definições por `(namespace, lang)` por 30 dias e, quando o jogo é jogado de novo, só fazer a
    consulta `PLAYER`. Jogos com `secs = 0` só quando abertos, ou bem no fim da fila.
  - Xbox: tratar o 429 de `minutes` como recusa.
  - Steam: guardar "sem nome" com data em `steam-names.json`.
  - Medir: um contador de consultas por fonte em cada sincronização, registrado no `trail`.
- **Arquivos.** `epic.rs`, `mod.rs`, `xbox.rs`, `steam.rs`.
- **Critério de aceite.** Numa segunda sincronização sem nada jogado, a Epic faz 1 consulta e o Xbox faz
  1 + ⌈PCs/100⌉. A primeira sincronização da Epic consulta só os jogos com tempo de jogo.
- **Teste sugerido.** Com a costura da L1, contar as requisições do servidor falso.

### L14. O aviso ao vivo e a sincronização ao fechar o jogo não cobrem Xbox e Epic — M/G

- **Problema.** `live.rs` só vigia a Steam e `classic.rs` só o RA. Jogando um jogo do Xbox ou da Epic, nenhum aviso do
  Cerberus aparece, e o hub só fica sabendo na próxima sincronização da conta (5 min ou mais, e só se o hub ou a
  Biblioteca forem abertos). `Live::start` também chama `block_on` para buscar a raridade; se isso for chamado de dentro
  do runtime assíncrono, pode travar ou entrar em pânico (**não verificado**: depende de onde `store_time.rs`, linha
  155, roda).
- **Como resolver.**
  - **Primeiro passo (P).** Quando um jogo Xbox ou Epic acompanhado por `store_time.rs` fecha, sincronizar **só esse
    jogo**: 1 `title_list` no Xbox, 1 consulta `PLAYER` na Epic. Emitir `achievements-changed`. É barato e cobre 90%
    do valor.
  - **Segundo passo (M).** Durante a partida, consultar a cada 2 a 3 min só o jogo aberto (Xbox: `title_list` com
    `orderBy=UnlockTime&maxItems=5`, **não verificado**; Epic: `PLAYER`) e avisar o que for novo, respeitando a espera.
  - **Alternativa no Xbox (G, não verificada).** O canal em tempo real do Xbox Live (RTA) para progresso de
    conquistas.
  - Trocar o `block_on` por uma tarefa que preenche a raridade depois.
- **Arquivos.** `src-tauri/src/store_time.rs`, `live.rs`, `mod.rs`, `xbox.rs`, `epic.rs`.
- **Critério de aceite.** Ao fechar um jogo Xbox com uma conquista nova, ela aparece em Últimas em menos de 1 min, sem
  abrir o hub.
- **Teste sugerido.** `news` já existe em `live.rs`: reaproveitar para comparar a lista antes e depois.

### L15. Casos de borda de DLC, 360 e totais — P (complementa "Separar DLC" da 6.1)

- **Problema.**
  - Quando sai um DLC com conquistas, o total cresce e o jogo **sai de Platinados** sem explicação.
  - A Epic tem `achievementSetId` (o Hydra lê esse campo em `achievement-protocol.ts`), que o Cerberus ignora. Se
    dois conjuntos tiverem o mesmo `achievementName`, colidem (**não verificado**).
  - Títulos de Xbox 360 aparecem no titlehub com total, mas o contrato 4 de `achievements` pode devolver a lista vazia,
    e a página do jogo mostra 0 conquistas com "X/Y" no cartão (**não verificado**).
  - Na Epic, vários `Owned` com a mesma `namespace` geram o mesmo `key`, e o título do último sobrescreve o do
    primeiro em `keep_games`.
- **Como resolver.**
  - Guardar `award_at` do primeiro 100% e mostrar "Platinado antes do DLC". O id da conquista da Epic passa a ser
    `setId:name`.
  - Para o 360, testar a resposta com uma conta real e, se vier vazia, usar o contrato antigo
    (**não verificado**) ou mostrar "lista indisponível para jogos de Xbox 360".
  - Na Epic, deduplicar por namespace preferindo o jogo base.
- **Arquivos.** `store.rs` (`merge`), `epic.rs`, `xbox.rs`, `mod.rs`, `ui/src/achievements.rs`.
- **Critério de aceite.** Um jogo que tinha 100% e ganhou 5 conquistas de DLC mostra "Platinado (antes do DLC)" e
  50/55.
- **Teste sugerido.** `merge_keeps_first_platinum_when_total_grows`.

### L16. Testes que faltam (resumo) — M

O que mais protege:

1. Uma costura de rede (URL base injetável em `xbox::get`, `epic::ask` e `retro::get`) e um servidor falso local
   (`tokio` com `hyper`, ou `wiremock` se for aceitável como dependência só de testes).
2. Orquestração: L1, L2, L13 (contagem de consultas), a primeira leitura completa do Xbox e as seguintes com 8 listas,
   e a Epic com 40 jogos por rodada.
3. Banco: L6, L7, L10, L11, `forget` (apaga as 5 tabelas e nada de outra conta) e `highlights` (um por jogo, ignora as
   secretas).
4. `account_times`: o maior tempo vence, e uma `pfn` em maiúsculas casa com a guardada em minúsculas.
5. UI: L5 e a TV mostrando Xbox e Epic.

## Orçamento de consultas por fonte

Números lidos do código. "N" é a quantidade de jogos. As pausas são as do próprio código. Os limites oficiais das
lojas não são públicos (**não verificado**).

| Fonte | Primeira vez | Seguintes (nada jogado) | Seguintes (k jogos jogados) | Ritmo e espera | Abrir um jogo |
| --- | --- | --- | --- | --- | --- |
| Steam local | 0 (disco) | 0 | 0 | Só lê de novo se os arquivos mudaram (`LAST`) | 1 de raridade (7 dias) |
| Steam Web API | 1 owned + ⌈N/100⌉ tops + até 25 `GetPlayerAchievements` + até 40 schemas (ícones de Últimas) + nomes de 6 em 6 | 1 owned (a cada 5 min ou mais) + 0 tops + 0 mine | 1 + ⌈k/100⌉ + mín(k, 25) | 1 por vez, 350 ms entre consultas; para no 1º "não" | 1 owned + 1 mine + 1 schema + 1 raridade |
| RetroAchievements | ⌈N/500⌉ + 1 recentes + 1 perfil | o mesmo (≈3), a cada 5 min ou mais | o mesmo | Espera do banco (2 min a 6 h) | 1 |
| Xbox | até 3 de login (sessão de 1 h) + 1 titlehub + ⌈PCs/100⌉ minutos + ⌈conquistas/1000⌉ páginas (≈14 para 200 jogos) | 1 titlehub + ⌈PCs/100⌉ | o mesmo + mín(k, 8) listas | 300 ms; a cada 5 min ou mais; espera de 2 min a 6 h; `net.rs` respeita o `Retry-After` | 0 se a lista é nova, senão 1 |
| Epic | 1 token (30 min) + 1 tempo de jogo + até 40 × 2 GraphQL por rodada (tudo em ⌈N/40⌉ rodadas) | 1 tempo de jogo (+1 token) | 1 + mín(k, 40) × 2 | 250 ms; a cada 5 min ou mais; espera de 2 min a 6 h | 0 (só o banco; ver L8) |
| Biblioteca (`list_games`) | dispara `sync_accounts` (Xbox e Epic acima) | só se `due()`; o `catalog-changed` que ela emite não entra em laço, porque `due()` passa a ser falso | — | Um de cada vez (`AtomicBool`) | — |

Meta depois de L1, L13 e L14: a Epic na segunda rodada com 1 consulta; Epic e Xbox ao fechar um jogo com 1 consulta;
nenhuma releitura total por causa de troca de nome (L3).

## Riscos

- **APIs não oficiais.** titlehub, `achievements.xboxlive.com` com contrato 4, `userstats` e a GraphQL da loja da Epic
  com o `User-Agent` do launcher não são documentados para terceiros. Podem mudar de contrato, exigir outro cabeçalho
  ou bloquear clientes. Mitigação: o banco local continua mostrando o que já foi visto (é o ponto forte desta
  entrega); erros classificados (L4); um teste `#[ignore]` com conta real, como já existe para a Steam, para rodar à mão
  antes de cada release.
- **Limites desconhecidos.** Nenhuma das lojas publica limites (**não verificado**). Com espera gravada, `Retry-After`
  e pausas fixas, o risco é baixo, mas L2 enfraquece a espera hoje.
- **Tokens.** Os refresh tokens de Xbox e Epic ficam em `settings.json`. Não são senhas, mas são credenciais de longa
  duração. `data_backup.rs` já deixa as chaves de fora; manter assim também ao incluir o banco (L12). Nunca guardar
  senhas.
- **Cliente público da Epic.** É o mesmo client id do Legendary, do Heroic e do Hydra. Se a Epic o restringir, login e
  conquistas param juntos.
- **Steam Web API.** A chave do jogador pode ser bloqueada por excesso. O código já para no primeiro "não".
- **Licenças.** Hydra MIT pode ser adaptado com crédito (ideias citadas: `source-retry.ts`, `achievement-source.ts`,
  `achievement-protocol.ts`, `reconcile-achievement-memory.ts`). RecompLauncher e Heroic (GPL), só ideias. Nada de
  arquivos de emuladores de Steam de jogos piratas.

## Ordem sugerida

1. **L2** (P) e **L5** (P): correções pequenas que hoje enganam o jogador ou a espera.
2. **L1** (M), com a costura de rede da **L16**: tira o maior risco de a conta ficar parada para sempre.
3. **L4** (P/M): "Entre de novo" em vez de 6 h de espera.
4. **L11** (P), depois **L3** (M): versão do esquema antes de mexer na identidade das contas.
5. **L10** (P/M) e **L12** (P/M): escrita confiável e backup, para a promessa "nunca perde" valer de verdade.
6. **L9** (M): estados por fonte, Atualizar que força, Big Picture.
7. **L8** (M) e **L13** (M): Steam e RA em Próximas e Raras, Epic na hora, menos consultas.
8. **L14** (P o primeiro passo, depois M/G), **L6** (M), **L7** (P/M) e **L15** (P).
