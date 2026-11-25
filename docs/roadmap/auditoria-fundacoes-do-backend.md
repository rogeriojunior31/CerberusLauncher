# Auditoria das fundações do backend: persistência, confiabilidade e segurança

Revisão de 25/11/2026, só leitura, do branch `ajustes-rapidos` (`src-tauri/src/`, `capabilities/`, `tauri.conf.json`).
Nada foi compilado nem executado. O que não foi confirmado lendo o código está marcado **não verificado**.

Para não repetir o que já está escrito:

- [lojas-e-contas L1](lojas-e-contas.md): `settings.json` gravado por várias tarefas, com o mesmo `settings.json.tmp`, e a
  conta que volta depois de Sair. Aqui só entram **casos novos** (F6) e o problema da leitura que cai no padrão (F1),
  que essa revisão cita de passagem.
- [lojas-e-contas L7](lojas-e-contas.md): janela de login (nonce, PKCE, pop-ups). Não repito.
- [conquistas-e-tempo L10](conquistas-e-tempo.md): concorrência no SQLite e leitura bloqueante em `list_games`. Não repito.
- Onda 8.0 (feito): tokens fora do backup e erros de rede sem a URL (`net::send` usa `without_url`). Conferido em
  `net.rs:94-120`.

Comparação com o Hydra: o Hydra guarda o estado do jogador num banco LevelDB (`src/main/level/level.ts` e `sublevels/`),
com gravação atômica por chave ou por lote, e não em arquivos JSON inteiros reescritos a cada mudança. O Cerberus
não precisa trocar de formato, mas precisa da mesma garantia: nunca regravar por cima de algo que não conseguiu ler.

---

## 1. Inventário

### 1.1 Arquivos de estado e como são gravados

"Leitura que falha" = o que acontece quando o arquivo existe, mas não pôde ser lido ou entendido.

| Arquivo (pasta de dados) | Módulo | Gravação | Trava entre tarefas | Leitura que falha | Tamanho |
| --- | --- | --- | --- | --- | --- |
| `settings.json` (contas, tokens, fontes, pastas) | `settings.rs:122-138` | `.json.tmp` fixo + `rename`, sem `sync_all` | nenhuma (44 chamadas de `save`) | **padrão; o próximo `save` apaga tudo** (F1) | pequeno |
| `library.json` | `library.rs:24-47, 272-284` + `library/recovery.rs` | `.json.tmp` fixo + `rename` | `Mutex` com cache em memória | **bom**: erro de leitura devolve `Err`; JSON cortado é recuperado e o original guardado em `library.original-N.json` | médio |
| `favorites.json` | `favorites.rs:13-29` | `.json.tmp` + `rename` | nenhuma (comandos síncronos, então em série na thread principal) | **vazio → apaga** (F2) | pequeno |
| `collections.json` (coleções e ocultos) | `collections.rs:33-43` | `.json.tmp` + `rename` | idem | **vazio → apaga** (F2) | pequeno |
| `game-notes.json` | `game_notes.rs:31-50` | `.json.tmp` + `rename` | idem | **vazio → apaga** (F2) | pequeno |
| `store-playtime.json` | `store_time.rs:29-44` | `.json.tmp` + `rename` | thread de `store_time::watch` | **vazio → apaga o tempo de todos** (F2) | pequeno |
| `art.json` (arte escolhida) | `art_custom.rs:88-115` | `.json.tmp` + `rename` | `Mutex` do mapa | **vazio na abertura → o próximo `set` apaga** (F2) | pequeno |
| `sessions.jsonl` | `play_log.rs:22-33` | append | `static WRITE: Mutex` | linha ruim é pulada | cresce sem limite (≈100 B por partida; Baixa) |
| `trail.jsonl` | `trail.rs:53-77` | append, corte para 500 linhas via `.jsonl.tmp` + `rename` | `Mutex` | linha ruim é pulada | limitado (bom) |
| `classics.json` (tempo dos clássicos) | `emulation.rs:480-490` | `.json.tmp` + `rename` | não verificado | não verificado | pequeno |
| `cache/…/<loja>-library.json`, `gamepass`, `xbox` | `epic_account.rs:263`, `xbox_account.rs:276`, `gamepass.rs:175`, `xbox.rs:228` | `.json.tmp` fixo + `rename`, erro ignorado | nenhuma | refaz pela rede | médio |
| Caches de conquistas, notícias, Wikipédia, loja | `achievements/{classic,mod,steam,steam_api}.rs`, `steam_news.rs:98`, `wikipedia.rs:115`, `steam_details.rs:48`, `steam_store.rs:81`, `ms_store.rs:44`, `recomp/releases.rs:80`, `recomp/catalog.rs:323` | `let _ = fs::write` direto | nenhuma | refaz pela rede | pequeno |
| Índice de ROMs | `roms.rs:491-498` | `let _ = fs::write` direto | `Mutex` do índice em memória | refaz a varredura | médio |
| Catálogo do site | `recomp/catalog.rs:370` | `.tmp` + `rename` | não verificado | cópia embutida | grande |
| Fontes de catálogo | `recomp/sources.rs:200-207` | `NamedTempFile` + `persist` (nome único, **bom**) | `tokio::Mutex` `Sources.sync` | mantém a cópia anterior | ≤64 MiB |
| Arte em cache | `art_cache.rs:99-114` | `.part` + `rename` | conjunto `busy` | baixa de novo | **sem limite total** (F14) |
| Backups de saves (zip) | `recomp/saves.rs:233-255` | `tempfile .part` + `sync_all` + `persist_noclobber` (**bom**) | `Installing::while_idle` | — | 10 por jogo (`prune`) |
| Restauração de saves | `recomp/saves.rs:370-398` | todos os arquivos preparados antes de trocar o primeiro (**bom**) | — | — | — |
| Atualização de recomp | `recomp/update.rs` (`intent.json` + `sync_all` + `rename`, `recover` ao abrir a biblioteca) | transação (**bom**) | trava por jogo | — | — |
| Configuração dos emuladores (`retroarch.cfg`, `.ini` do PCSX2/DuckStation) | `cheevos.rs:176-209, 409-418`, `emulation.rs:439-467` | **`fs::write` direto no arquivo do jogador** | nenhuma | **lê vazio e grava só as chaves do Cerberus** (F3) | — |
| Pendências do RetroAchievements | `cheevos.rs:398` | `let _ = fs::write` direto | nenhuma | as mudanças no emulador ficam para sempre (F11) | pequeno |
| Mods instalados (`mods.json` por jogo) | `mods.rs:207-219` | `fs::write` direto | nenhuma | **vazio → perde quais arquivos são do mod** (F2) | pequeno |
| `ModsDB.ini`, `cpkredir.ini` (HedgeModManager) | `hedge.rs:110, 148` | `fs::write` direto | nenhuma | não verificado | pequeno |
| `shortcuts.vdf` da Steam | `steam_shortcuts.rs:290-292` | `.cerberus.tmp` + `rename`; tipo desconhecido recusa gravar (**bom**) | exige Steam fechada | — | — |
| Instalador da atualização do Cerberus | `self_update.rs:96-128` | grava direto no nome final | — | confere SHA-256 **se houver** | — |
| Cópia de dados (exportar/importar) | `data_backup.rs` | por arquivo `.restoring` + `rename`, sem desfazer o conjunto | **nenhuma com o cache da biblioteca** (F5) | — | `before-restore-*` sem limite (F14) |

### 1.2 Comandos síncronos com I/O (rodam na thread principal no Windows)

168 comandos em `main.rs`, 5 em `overlay.rs`, 2 em `unlock_toast.rs`. 75 são `async fn`; 14 usam
`#[tauri::command(async)]` (`look_status`, `emulation_status`, `play_classic`, `classic_players`, `set_play_with`,
`remove_game`, `play`, `list_backups`, `preparation_pending`, `prepare_game`, `in_steam`, `emulator_backups`,
`backup_emulator_saves`, `restore_emulator_backup`). Os outros ~86 são síncronos. Os com custo real:

| Comando (linha em `main.rs`) | O que faz na thread principal | Custo |
| --- | --- | --- |
| **`settings_view`**, chamado por `get_settings`, `finish_setup`, `set_start_page`, `set_autostart`, `set_start_hidden`, `set_language`, `set_monitor`, `set_close_to_tray`, `set_game_pass`, `rerun_setup`, `ra_disconnect`, `set_download_limit`, `set_notices`, `set_in_game`, `set_start_in_tv`, `reset_recomps_dir`, `clear_art_cache`, `xbox_logout`, `epic_logout` (115-180) | lê `settings.json`; `disk::available` duas vezes; `read_dir` + `metadata` de **toda a arte em cache** (milhares de arquivos); `cloud::pending_count` percorre os backups **e a pasta da nuvem** (OneDrive/Drive, que pode baixar placeholders); registro do autostart; `owned_games` cruza ~1.900 recomps com as ROMs; `emulation::played` lê `classics.json` | **Alto**: cada toque num interruptor de Configurações faz tudo isso |
| `clear_art_cache` (1572) | apaga a pasta de arte inteira, depois `settings_view` | Alto |
| `create_shortcut` (2939) | se não está na biblioteca, `steam::installed_games` + `epic::installed_games` + `xbox::installed_games` (registro, manifests, pacotes AppX) e cria `.lnk` | Alto |
| `xbox_logout` (1902), `epic_logout` (1919) | abre o SQLite `achievements.db` e apaga as conquistas, grava settings, `settings_view` | Médio |
| `art_save` (449) | decodifica até 25 MB em base64 e grava a imagem | Médio |
| `remove_from_steam` (3104) | lê e regrava `shortcuts.vdf` de cada conta, apaga arte | Médio |
| `game_settings` (2804), `set_game_setting` (2815) | lê e grava arquivos de configuração do jogo | Médio |
| `recomp_suggestions` (2685), `link_to_recomp` (2694) | procura recomps parecidos na pasta (não verificado o quanto percorre) | Médio |
| `prepare_options`, `set_prepare_option`, `game_actions`, `game_inputs`, `open_prepare_license` (2717-2776) | leem o manifest do jogo no disco | Baixo/Médio |
| `run_game_action` (2759), `open_standalone` (893) | `Command::spawn` | Baixo (spawn é rápido; ShellExecute às vezes não) |
| `ra_disconnect` (1401) | reescreve `retroarch.cfg` | Baixo |
| `rename_game` (2050), `set_launch_args` (2449), `set_favorite`, `set_hidden`, `set_game_note`, coleções (2852-2911) | regrava o JSON inteiro | Baixo (cresce com a biblioteca) |
| `steam_local_account` (1037), `pc_status` (3136) | `loginusers.vdf`; registro (VC++ e DirectX) | Baixo |
| `game_sessions` (1196), `recent_activity`, `game_trail` (3162-3170) | leem `sessions.jsonl`/`trail.jsonl` inteiros | Baixo |
| `list_screenshots` (3173), `open_*` (abrir pasta ou URL) | `read_dir`, ShellExecute | Baixo |
| `list_monitors`, `to_chosen_monitor`, `overlay_*`, `unlock_next` | janela e monitores | Baixo (nenhum cria janela: as janelas são criadas só em `async fn` ou em threads, como `unlock_toast.rs:25` avisa) |

Nenhum comando síncrono faz rede. As janelas de login são `async fn` de propósito (`main.rs:1700-1702`).

### 1.3 `unwrap`/`expect`/`panic` em código de produção (fora de `#[cfg(test)]`)

Todos foram lidos. Nenhum quebra com dados de fora:

| Local | Por que é seguro |
| --- | --- |
| `main.rs:3649` `expect("failed to start…")` | início do app; sem janela não há o que fazer |
| `settings.rs:117`, `recomp/catalog.rs:286` | JSON fixo ou embutido, coberto por teste |
| `collections.rs:98`, `steam_shortcuts.rs:190,194`, `hedge.rs:34` | o item acabou de ser inserido |
| `disc_image.rs:169-180` | fatias de tamanho fixo (`try_into` de 8 ou 4 bytes) |
| `recomp/saves.rs:239` `file_name().unwrap()` | caminho de `tempfile`, sempre tem nome |
| `recomp/update.rs:78` `parent().unwrap()` | `work` é sempre `<pasta>/<nome>` (não verificado para destino na raiz de um disco) |
| `library/recovery.rs:71` `unreachable!()` | depois de um laço `1u64..` |
| Índices `[0]` em `crash.rs`, `emulation.rs:375`, `steam_shortcuts.rs:66`, `disc_console.rs`, `iso9660.rs` | tamanho conferido antes |

`Mutex` envenenado: todas as travas usam `lock().unwrap_or_else(|p| p.into_inner())`, exceto
`take_play_request` (`main.rs:2934`), que usa `.ok()` e só perde o pedido. Não há `lock().unwrap()`.
`block_on` em produção: só `achievements/live.rs:57`, chamado da thread própria de `store_time::watch`
(`std::thread::spawn`), não de dentro do runtime. Os demais estão em testes.

### 1.4 Rede

- Um único `reqwest::Client` com `timeout` de 10 s (`main.rs:3408-3411`). As chamadas longas sobrescrevem o valor
  por pedido (download de recomp 6 h, mods 30 min, núcleos 5 min).
- `net::send` (espera depois de falha, sem URL no erro) é usado em 18 módulos. **26 chamadas não passam por ele**:
  `epic_account.rs` (3), `xbox_account.rs` (5), `emulation.rs` (4), `art_custom.rs` (2), `gamebanana.rs` (2),
  `looks/mod.rs` (2), `mods.rs` (2), `recomp/install.rs` (2), `main.rs:3040`, `recomp/mod.rs`, `runtimes.rs`,
  `self_update.rs`, `standalone.rs`. A maioria é download do próprio jogador ou login (correto ficar de fora da
  espera), mas os erros delas usam `e.to_string()`, que inclui a URL. Nenhuma dessas URLs leva segredo hoje
  (conferido: tokens vão em cabeçalho ou corpo), então é só um risco para código futuro.
- Limites de tamanho: fontes 64 MiB em streaming (bom); manifest do Forgejo 1 MiB **depois** de ler tudo;
  arte `MAX_BYTES` **depois** de ler tudo; mods só pelo `Content-Length` (F8).
- https: exigido em fontes (`recomp/sources.rs:172`), mods (`mods.rs:267`), arte para a Steam (`main.rs:3039`),
  links abertos no navegador (`open_wiki`, `open_news`). Os redirecionamentos seguem a política padrão do reqwest
  (até 10, **inclusive de https para http**; F8).

### 1.5 Segurança: o que já está certo

- Capacidades: só `main`, `overlay` e `unlock` (`capabilities/default.json`), com `core:default` e cinco
  permissões de janela. As janelas de login (`epic-login`, `xbox-login`) não têm capability nem `remote.urls`.
- `assetProtocol.scope` vazio no `tauri.conf.json`. Em tempo de execução são liberadas apenas `covers/`,
  `cache/art/`, as capturas, o cache de capas da Steam e os logos dos jogos Xbox, arquivo por arquivo
  (`main.rs:3436-3462`).
- A UI não usa `inner_html` (procurado em `ui/src`), então texto de loja ou de catálogo não vira HTML.
- Comandos que recebem caminho do frontend conferem a origem: `open_rom_folder` e `play_classic` (só ROMs do
  índice), `open_screenshot` (`screenshots::is_ours` canonicaliza e exige `.png` dentro da pasta), `dlss_restore` e
  `dlss_swap` (`dlss.rs:96-102`: relativo, sem `..` e na lista de DLSS do jogo), backups (`saves::open_backup`:
  só nome simples `.zip`), cópia de dados (lista fixa de nomes e `plain_name` nas capas).
- Extração: zip com `enclosed_name` (`recomp/install.rs:325`, `emulation.rs:347`, `mods.rs:246`) e conferência de
  espaço; tar via `Archive::unpack`; 7z confia na biblioteca (F17). Restauração de save recusa entradas com `..`
  (`recomp/saves.rs:401-412`).
- Programas do manifest de terceiros: `manifest::inside` restringe ao diretório do jogo (`recomp/mod.rs:482, 508`).
  O PowerShell da atualização escapa aspas simples (`self_update.rs:147`).

---

## 2. Falhas

Gravidade: **Crítica** = perde dados do jogador ou abre brecha de segurança; **Alta** = quebra o uso;
**Média**; **Baixa**.

### F1. `settings.json` ilegível vira padrão e a próxima gravação apaga contas, fontes e pastas — Crítica

- **Onde:** `settings.rs:122-130` (`load`), consagrado pelo teste `a_broken_file_means_defaults` (`settings.rs:190`).
- **Problema:** `load` trata dois casos como "sem arquivo":
  1. Erro de leitura que não é "não existe" (arquivo preso pelo antivírus ou pelo OneDrive, permissão, disco
     ocupado): devolve `Settings::default()` com `setup_done = false`. O app abre no assistente de primeira vez.
  2. JSON inválido (arquivo zerado depois de falta de luz, já que não há `sync_all` antes do `rename`; versão nova
     com um campo de tipo diferente depois de voltar de versão): devolve o padrão com `setup_done = true`.
  Nos dois casos, **qualquer** `settings::save` seguinte (44 lugares, inclusive tarefas de fundo como
  `achievements/mod.rs:20, 421` e a renovação de tokens) grava o padrão por cima. Somem as contas Epic, Xbox,
  Steam e RA, as fontes de catálogo, as pastas de ROMs e de recomps, a pasta da nuvem e as escolhas de emulador.
  Não fica cópia.
- **Reproduzir:** com o app fechado, troque `settings.json` por `{"rom_dirs": 5}`. Abra, mude o idioma. O arquivo
  passa a ter só os padrões. Variante: abra `settings.json` com `FileShare.None` (PowerShell
  `[IO.File]::Open(..., 'Open', 'Read', 'None')`) e abra o Cerberus: aparece o assistente.
- **Correção:** `load` passa a devolver `Result<Settings, LoadError>`. `NotFound` → padrão de jogador novo.
  Erro de leitura → tentar de novo 3 vezes com 200 ms e, se continuar, falhar sem gravar nada. JSON inválido →
  copiar o arquivo para `settings.corrupt-<n>.json` (como `library/recovery.rs:57-71`), tentar `settings.json.bak`
  (ver padrão P1) e só então usar o padrão, avisando o jogador. Enquanto o arquivo estiver ilegível, `save` recusa.
  Trocar o teste para exigir que o original continue lá.
- **Aceite:** com um `settings.json` inválido, o app avisa "Configurações recuperadas da cópia de <data>" ou
  "Configurações danificadas, guardadas em …", e o arquivo original não é sobrescrito. Arquivo preso não mostra o
  assistente.

### F2. Favoritos, coleções, notas, tempo das lojas, arte escolhida e mods: leitura que falha apaga tudo — Crítica

- **Onde:** `favorites.rs:13-15`, `collections.rs:33-35`, `game_notes.rs:31-33`, `store_time.rs:29-31`,
  `art_custom.rs:88-91` (lê uma vez na abertura), `mods.rs:207-211`.
- **Problema:** o mesmo padrão `read_to_string().ok().and_then(from_str).unwrap_or_default()`, seguido de
  "modificar e gravar o conjunto inteiro". Um arquivo ilegível ou inválido vira vazio, e a próxima ação do jogador
  (marcar um favorito, dar uma nota, 5 s de jogo da Steam em `store_time::add`) grava esse vazio por cima.
  O `store-playtime.json` é o pior caso: quem grava é uma thread em segundo plano, sem nenhuma ação do jogador, e o
  tempo de jogo da Steam e da Epic não existe em outro lugar. Em `art.json`, o mapa vazio fica em memória e o próximo
  `art_save` apaga a arte de todos os jogos. Em `mods.rs`, a lista é gravada sem arquivo temporário: uma queda no
  meio da gravação deixa o JSON cortado, e "Remover mod" passa a não saber quais arquivos apagar.
  O documento [metadados](metadados-filtros-e-estatisticas.md) recomenda esse formato "como padrão a seguir para tudo
  o que o jogador edita". A troca atômica está certa; a leitura não.
- **Reproduzir:** troque `game-notes.json` por `{"x":` e dê uma estrela a um jogo: as outras notas somem. Para o
  tempo: deixe `store-playtime.json` inválido e abra um jogo da Steam por 10 s.
- **Correção:** usar o `store::Json<T>` do padrão P1 em todos. Leitura com erro → `Err`, e a gravação é recusada
  até o jogador decidir. JSON inválido → original guardado em `<nome>.corrupt-<n>.json`, depois a cópia `.bak`.
- **Aceite:** um teste por arquivo ("arquivo inválido + uma mudança = original preservado e mudança recusada ou
  aplicada sobre a cópia `.bak`"). Teste manual com `store-playtime.json` preso: o tempo é acumulado em memória e
  gravado quando o arquivo voltar.

### F3. Configuração do emulador lida como vazia e regravada só com as chaves do Cerberus — Crítica

- **Onde:** `cheevos.rs:177` (`apply`), `cheevos.rs:197` (`keep`), `emulation.rs:441` (`write_token`). Também
  `cheevos.rs:409-418` (`undo_all`) e `emulation.rs:459-467` (`forget_token`), com gravação direta.
- **Problema:** `fs::read_to_string(file).unwrap_or_default()` trata qualquer erro como arquivo vazio. Se
  `retroarch.cfg`, `PCSX2.ini` ou `settings.ini` do DuckStation não puderem ser lidos (arquivo preso pelo emulador
  ou pelo antivírus, ou texto que não é UTF-8, que dá `InvalidData`), o Cerberus grava um arquivo **só com as
  chaves de conquistas**. Todos os controles, caminhos de BIOS e opções de vídeo do jogador somem. O
  `.cerberus-bak` do RetroArch (`emulation.rs:448-451`) só é feito quando o texto lido não está vazio, ou seja,
  justamente não nesse caso. E é feito uma vez só. Todas as gravações são `fs::write` direto: uma queda no meio
  também corrompe o arquivo.
- **Reproduzir:** acrescente um byte `0xFF` a `retroarch.cfg` (ou mantenha-o aberto com `FileShare.None`) e
  conecte o RetroAchievements no RetroArch pelo Cerberus. O arquivo fica com 4 linhas.
- **Correção:** ler com `fs::read` e distinguir `NotFound` (vazio) de erro (abortar com mensagem). Para texto que
  não é UTF-8, editar como bytes ou recusar. Gravar via P1 (temporário no mesmo diretório + `sync_all` + `rename`).
  Fazer um `.cerberus-bak-<data>` antes da primeira edição de cada sessão, com no máximo 3 cópias.
- **Aceite:** teste com arquivo não UTF-8 e com arquivo preso: o original fica idêntico byte a byte e o jogador vê
  "Não foi possível ler as configurações do RetroArch". Teste de queda: matar o processo entre gravar o temporário e
  o `rename` deixa o original intacto.

### F4. Reinstalar um emulador interrompido no meio deixa os saves em `.<nome>-old`, e a reinstalação seguinte os apaga — Crítica (janela estreita)

- **Onde:** `emulation.rs:653-692` (`swap_keeping`), principalmente a linha 658 `let _ = fs::remove_dir_all(&old)`.
- **Problema:** a troca põe a instalação antiga em `.<nome>-old`, move os arquivos do jogador (saves, memory cards,
  BIOS, configuração) para a nova e só no fim apaga a antiga. Se o Cerberus fechar ou o PC desligar entre a linha
  659 e o fim, a pasta do emulador some ou fica pela metade, e parte dos saves fica só em `.<nome>-old`. Nada
  recupera isso na abertura. Na próxima reinstalação, o `target` não existe, então a nova cópia vai direto
  (linha 654) e `.old` continua lá. Na reinstalação **seguinte**, a linha 658 apaga `.old` sem olhar o conteúdo,
  junto com os saves que só existiam ali.
- **Reproduzir:** em teste, injete uma falha de processo (ou um `panic` controlado) depois do `rename(target, old)`;
  reinstale duas vezes. Os arquivos de `memcards/` que estavam em `old` somem.
- **Correção:** na abertura do app (ou antes de qualquer troca), procurar `.<nome>-old` ao lado de cada emulador
  gerenciado. Se o alvo não existir, renomear `.old` de volta. Se existir, mover os `keep` que ainda estão em `.old`
  e depois apagá-lo. Nunca `remove_dir_all(old)` sem antes reconciliar. Gravar um `swap.json` com o estado da troca,
  como `recomp/update.rs` faz com `intent.json`.
- **Aceite:** teste de unidade que simula a interrupção em cada passo (antes, durante e depois de mover os `keep`) e
  confere que, depois de "abrir de novo" mais duas reinstalações, os saves estão na pasta do emulador.

### F5. Importar uma cópia de dados pode ser desfeito pelo cache da biblioteca antes do reinício — Alta

- **Onde:** `main.rs:1454-1468` (`import_data`: restaura e reinicia depois de 1,2 s), `data_backup.rs:96-141`,
  `library.rs:13-47` (cache `Mutex<Option<Vec<Game>>>`), `main.rs:3378-3381` (checkpoint a cada 15 s) e `3651`
  (checkpoint em `RunEvent::Exit`), `store_time.rs:122-199` (thread que grava `store-playtime.json`).
- **Problema:** a restauração grava `library.json`, `store-playtime.json`, `trail.jsonl` etc. por baixo dos
  módulos que guardam o estado em memória. Com um jogo aberto, `sessions::checkpoint` chama `library.update`, que
  grava o **cache antigo** inteiro por cima do `library.json` restaurado; o mesmo vale para a saída do app no
  `restart()`. A thread do tempo das lojas faz `load → somar → gravar` e pode cair no meio da troca. Além disso:
  não há bloqueio com instalação, backup ou jogo em andamento; se a gravação falhar no quinto arquivo, os quatro
  primeiros ficam trocados sem volta (a pasta `before-restore-*` existe, mas o app não a usa sozinho); cada entrada
  do zip é lida inteira na memória sem limite (`read_to_end`).
  O efeito do cache é **parcialmente verificado**: confirmei que `checkpoint` grava pela biblioteca, mas **não
  verifiquei** se ele grava quando não há sessão aberta.
- **Reproduzir:** abra um jogo pelo Cerberus, importe uma cópia com outra biblioteca e espere o reinício. A
  biblioteca volta a ser a de antes.
- **Correção:** recusar a importação com jogo, instalação ou backup em andamento. Restaurar para
  `restore-staging/`, validar tudo, e só então trocar os arquivos com o app parado: gravar um marcador
  `restore-pending.json`, reiniciar e aplicar a troca no começo do `setup`, antes de qualquer módulo ler o disco.
  Limitar cada entrada (por exemplo 256 MiB) pelo `size()` do zip.
- **Aceite:** com um jogo aberto, a importação é recusada com uma mensagem. Sem jogo, depois do reinício
  `library.json` é igual ao do zip. Um zip com uma entrada de 2 GB é recusado sem estourar a memória.

### F6. Mais leituras e gravações de `settings.json` atravessando um `await`, além das que L1 já lista — Alta

- **Onde (casos novos):** `link_retroarch_achievements` (`main.rs:973-987`: `load` **antes** de
  `connect_token(...).await`, `save` depois); `add_source` (`main.rs:234-247`) e `steam_connect`
  (`main.rs:1050-1068`): `load` depois da rede, mas sem trava contra as renovações de token; `use_recomps_dir`,
  `set_cloud_dir`. Também as gravações em `.json.tmp` com nome fixo nos caches das contas (`epic_account.rs:263`,
  `xbox_account.rs:276`, `gamepass.rs:175`, `xbox.rs:228`), que podem rodar ao mesmo tempo pela Biblioteca e pelo
  hub, e por isso uma pode renomear o temporário pela metade da outra.
- **Problema:** é a mesma classe de L1 (mudança perdida, conta que volta). O caso do RetroArch apaga, por exemplo,
  um `epic_refresh` renovado durante a digitação da senha.
- **Correção:** a de L1 (`settings::update` com trava e releitura) e, nos caches, o temporário com nome único de P1.
  Uma regra de revisão: nenhum `settings::load` cujo resultado seja gravado depois de um `.await`.
- **Aceite:** um teste que roda 50 `update` concorrentes em campos diferentes e confere que todos ficaram; um
  `grep` no CI que falha se houver `settings::save(` fora de `settings.rs`.

### F7. Comandos síncronos pesados travam a janela — Alta

- **Onde:** tabela 1.2; o principal é `settings_view` (`main.rs:115-180`) com `recomp/cloud.rs:113-119`.
- **Problema:** no Windows, comando sem `async` roda na thread principal, a mesma que desenha a janela e entrega os
  eventos. Cada interruptor de Configurações percorre a pasta de arte, os backups e **a pasta da nuvem**. Com
  OneDrive "arquivos sob demanda" ou uma unidade de rede lenta, isso leva segundos, e a janela inteira fica
  "Não respondendo", inclusive a barra de título. `create_shortcut` e `clear_art_cache` têm o mesmo efeito.
  **Não medido**: o tempo real com 5.000 capas e OneDrive.
- **Correção:** marcar com `#[tauri::command(async)]` todo comando que toca disco, registro, SQLite ou processos
  (é uma troca de uma linha, porque `State`/`AppHandle` já funcionam assim, como em `look_status`). Em
  `settings_view`, guardar o tamanho da arte e as pendências da nuvem em memória, atualizando em segundo plano.
  Deixar síncronos só os que leem memória (`running_games`, `update_found`, `take_play_request`, `overlay_*`).
- **Aceite:** com uma pasta de nuvem apontando para um compartilhamento de rede lento (ou um `sleep` de teste em
  `pending_count`), alternar "Fechar para a bandeja" não congela a janela; a animação do botão continua.

### F8. Respostas sem limite e redirecionamento para http — Média

- **Onde:** `mods.rs:266-279` (o limite `MAX_MOD_BYTES` só vale se houver `Content-Length`; resposta em pedaços
  passa e fica toda na memória); lista de pacotes do Thunderstore (`mods.rs:185-200`, a lista inteira de uma
  comunidade em memória e em disco); `art_cache.rs:104` e `recomp/mod.rs:1308` (leem tudo e só depois comparam o
  tamanho); `art_custom.rs:252`, `emulation.rs` (núcleos em memória); redirecionamentos com a política padrão.
- **Problema:** um servidor (ou um espelho de catálogo de terceiros) pode responder sem fim e esgotar a memória.
  Um redirecionamento de https para http entrega o download (inclusive executáveis de recomps e núcleos) sem TLS.
- **Correção:** uma função `net::read_limited(reply, max)` que lê em streaming e para no limite, usada em todos os
  `.bytes()`/`.text()`/`.json()`. No `Client`, `redirect::Policy::custom` que recusa sair de https.
- **Aceite:** teste com um servidor local que responde 200 MB em chunked para `mods::download`: erro "grande
  demais" com memória estável. Teste com redirecionamento 302 para `http://`: recusado.

### F9. Lugares de saves vindos do manifest aceitam `..` depois da variável — Média

- **Onde:** `recomp/saves.rs:73-83` (`resolve`), usado no backup e na restauração (`target_for`, 401-412).
- **Problema:** o caminho **dentro** do zip é conferido (`Component::Normal`), mas o modelo do lugar não. Um
  `recomplauncher.json` de terceiro com `"{home}/../.."` ou `"{documents}/../../Windows"` faz o backup copiar
  árvores enormes (o disco enche) e a restauração gravar fora da pasta esperada. `allowed_places` só compara o texto
  com o manifest do jogo, que é a origem do problema. Risco limitado porque quem publica um recomp já roda código no
  PC, mas o backup roda antes de o jogador abrir o jogo.
- **Correção:** em `manifest` e em `resolve`, recusar um resto com `ParentDir`, `RootDir` ou `Prefix`, e recusar
  `{home}` ou `{documents}` sem subpasta.
- **Aceite:** teste com `"{home}/../x"` e `"{home}"`: `resolve` devolve `None` e o manifest é recusado com aviso.

### F10. Fechar o app no meio de operações longas não avisa nem limpa — Média

- **Onde:** `background.rs:44-53` (fechar a janela só segura o app se houver jogo ou bandeja), `background.rs:136`
  (Sair da bandeja → `app.exit(0)`), `main.rs:3641-3652` (só `RunEvent::Exit`, sem `ExitRequested`).
- **Comportamento em cada operação:**

  | Operação | Se o app fecha no meio | Situação |
  | --- | --- | --- |
  | Download de recomp | `.part` com continuação (`recomp/install.rs:221-223`) | bom |
  | Extração e instalação | `staging` apagado na próxima tentativa; atualização com `intent.json` e `recover` | bom; arquivos parciais de uma **primeira** instalação ficam no destino (não verificado se a biblioteca já tem o jogo) |
  | Backup de saves | `tempfile .part` não é apagado quando o processo morre; `prune` só conta `.zip` | arquivos `.part` órfãos acumulam na pasta de backups e na da nuvem |
  | Envio para a nuvem | marcador `.upload` + `persist_noclobber` | bom |
  | Restauração de saves | tudo preparado antes de trocar; a troca de N arquivos não é atômica entre si | metade dos saves trocada se cair no laço final (`saves.rs:394-396`) |
  | Troca de emulador | F4 | ruim |
  | Sessão RetroAchievements | `cheevos-pending` desfaz na próxima abertura (`cheevos.rs:423`) | bom, se a gravação do pendente não falhou (F11) |
  | Atualização do Cerberus | instalador gravado direto no nome final | um instalador cortado fica em `downloads/` até a próxima versão |
  | Importar dados | F5 | ruim |

- **Correção:** tratar `RunEvent::ExitRequested` e o Sair da bandeja: se houver instalação, backup, restauração ou
  troca em andamento, perguntar ("Uma instalação está em andamento. Sair mesmo assim?") e, se sim, cancelar pelo
  `Halt`/`cancel_install` e esperar até 5 s. Na abertura, varrer `*.part`/`.cerberus-*.tmp` com mais de 1 h nas
  pastas de backups e de downloads. Na restauração, registrar a lista trocada para retomar.
- **Aceite:** Sair durante um download mostra a pergunta; cancelar mantém o download. Matar o processo durante um
  backup e abrir de novo não deixa `.part` depois de 1 h.

### F11. Gravações diretas, sem temporário, em arquivos que importam — Média

- **Onde:** `cheevos.rs:398` (pendências: se essa gravação falhar ou ficar cortada, as mudanças da sessão no
  emulador nunca são desfeitas), `hedge.rs:110, 148` (`cpkredir.ini`, `ModsDB.ini` do jogador),
  `looks/mod.rs:212-229` (arquivos do ReShade na pasta do jogo), `emulation.rs:454, 466`, `mods.rs:218`.
  Também o padrão atual `fs::write(tmp) + rename` em settings, biblioteca e demais **não chama `sync_all`** antes do
  `rename`. No NTFS, uma falta de luz logo depois pode deixar o arquivo novo com tamanho zero. A biblioteca se
  recupera (`recovery.rs`); os outros caem em F1/F2.
- **Correção:** P1 em todos (temporário único, `sync_all`, `rename`, `.bak`). `let _ =` na gravação das pendências
  vira erro: sem pendência gravada, a sessão não altera o emulador.
- **Aceite:** teste de unidade de P1 que confere `sync_all` (por injeção) e que o `.bak` é a versão anterior.

### F12. Caches gravados com `let _ = fs::write` direto — Baixa

- **Onde:** `achievements/classic.rs:122, 183`, `achievements/mod.rs:129, 581`, `achievements/steam.rs:401`,
  `achievements/steam_api.rs:209, 257, 303, 326, 370`, `steam_news.rs:98`, `wikipedia.rs:115`, `steam_details.rs:48`,
  `steam_store.rs:81`, `ms_store.rs:44`, `recomp/releases.rs:80`, `recomp/catalog.rs:323`, `roms.rs:496`,
  `gamebanana.rs:174`, `mods.rs:119, 195`.
- **Problema:** gravação cortada → cache inválido → refeito pela rede. Só custa consultas, e consultas contam para
  limites de uso (ver o orçamento de [conquistas](conquistas-e-tempo.md)). **Não verificado:** se algum desses
  "known" serve de base para avisar desbloqueios novos (perdê-lo repetiria avisos).
- **Correção:** P1 sem `.bak`, para uniformizar.

### F13. Overlay e aviso de conquista com todas as permissões da janela principal; CSP desligada — Baixa

- **Onde:** `capabilities/default.json` (`"windows": ["main", "overlay", "unlock"]`), `build.rs` (sem
  `AppManifest`, então todos os comandos do app são liberados para toda janela com capability),
  `tauri.conf.json` (`"csp": null`, `withGlobalTauri: true`).
- **Problema:** as três janelas carregam o mesmo `index.html` local, então hoje não há brecha. Mas qualquer XSS
  futuro (um `inner_html` com a descrição de uma loja) teria acesso a ~175 comandos, inclusive `import_data`,
  `restore_*` e `run_game_action`. Sem CSP, não há segunda barreira.
- **Correção:** capability `overlay` só com `overlay_*` e `unlock_next` (via
  `tauri_build::Attributes::new().app_manifest(AppManifest::new().commands(&[...]))` e permissões por comando).
  Uma CSP mínima: `default-src 'self'; img-src 'self' asset: http://asset.localhost https: data:; connect-src ipc: http://ipc.localhost`
  (**não verificado** com o Trunk/Leptos, que pode precisar de `'wasm-unsafe-eval'`). Um teste que lê
  `capabilities/*.json` e falha com `"*"` ou `remote` (já proposto no plano da Steam).
- **Aceite:** no overlay, `invoke('import_data')` é recusado; a janela principal funciona com a CSP ligada.

### F14. Pastas que crescem sem limite — Baixa

- **Onde:** `before-restore-<hora>` com cópia de todas as capas a cada importação (`data_backup.rs:103-112`); arte
  em cache sem teto (`art_cache.rs`; só "Limpar"); `sessions.jsonl` (pequeno); `library.original-N.json`;
  `.cerberus-bak` (um só, bom).
- **Correção:** manter as 3 últimas `before-restore-*`; teto de 1 GB na arte, apagando as mais antigas por data de
  acesso.

### F15. Atualização do Cerberus: hash opcional e instalador gravado no nome final — Baixa

- **Onde:** `self_update.rs:96-128`.
- **Problema:** `sha256.is_none_or(...)` aceita a release sem hash, conferindo só o tamanho (e tamanho 0 aceita
  qualquer um). O arquivo é gravado direto com o nome que depois é executado em silêncio (`/S`), e a gravação usa
  `std::fs` dentro de `async`. Como vem por https do próprio repositório, o risco é baixo.
- **Correção:** exigir o `digest` da API do GitHub (ou um `SHA256SUMS` na release); gravar em `.part` e renomear
  depois da conferência; `tokio::fs` ou `spawn_blocking`.

### F16. I/O bloqueante dentro de `async fn` — Baixa

- **Onde:** muitos `async fn` chamam `settings::load/save`, `library.update` e `fs::*` direto (por exemplo
  `add_source`, `link_retroarch_achievements`, `steam_connect`, `self_update::fetch_checked`).
- **Problema:** prende uma thread do Tokio pelo tempo do disco. Com arquivos pequenos, é aceitável. Fica pior com
  a biblioteca grande (`library.json` inteiro regravado a cada 15 s durante o jogo). [Conquistas L10](conquistas-e-tempo.md)
  já cobre o SQLite.
- **Correção:** P1 já resolve com `spawn_blocking` interno; o resto, só quando medir.

### F17. Extração de 7z e tar depende de garantias da biblioteca que não foram testadas aqui — Baixa (não verificado)

- **Onde:** `recomp/install.rs:348-359`; também usado por `gamebanana.rs:290` e `hedge.rs:206`.
- **Problema:** o comentário diz que `sevenz_rust2::decompress_file` recusa `..`, raiz e unidade. **Não
  verificado** na versão em `Cargo.lock`. `tar::Archive::unpack` recusa `..`, mas cria links simbólicos (no
  Windows exige privilégio ou modo de desenvolvedor); as versões recentes do crate `tar` impedem gravar através de
  um link que sai do destino (**não verificado** na versão usada). 7z não tem limite de memória por entrada.
- **Correção:** testes com arquivos maliciosos: 7z com `..\x`, `C:\x` e nome absoluto; tar com link simbólico para
  `..` seguido de um arquivo dentro do link; tar com hardlink. Se algum passar, extrair entrada por entrada,
  conferindo `enclosed`, como no zip.
- **Aceite:** os três testes falham com "caminho inseguro" e nada é escrito fora do `staging`.

---

## 3. Padrões a adotar no projeto

### P1. Um módulo `store` para todo arquivo de estado

```rust
// src-tauri/src/store.rs
pub struct Json<T> { path: PathBuf, lock: Mutex<()>, _t: PhantomData<T> }

pub enum Read<T> { Missing, Ok(T), Recovered { value: T, from: PathBuf, kept: PathBuf } }

impl<T: Serialize + DeserializeOwned + Default> Json<T> {
    /// NotFound → Missing. Erro de E/S → Err (com 3 tentativas de 200 ms: antivírus/OneDrive).
    /// JSON inválido → guarda `<nome>.corrupt-<n>.json`, tenta `<nome>.bak`; sem .bak → Err.
    pub fn read(&self) -> Result<Read<T>, StoreError>;
    /// Trava, relê do disco, aplica `f`, grava. Nunca grava se a leitura deu Err.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, StoreError>;
}

/// Temporário com nome único no mesmo diretório (tempfile::NamedTempFile::new_in),
/// write_all + sync_all, cópia do atual para `.bak`, persist (rename atômico).
pub fn write_atomic(path: &Path, bytes: &[u8], keep_bak: bool) -> io::Result<()>;
```

- Uma instância por arquivo em `app.manage`, ou `static` com `OnceLock`, para que a trava valha entre comandos,
  tarefas e threads.
- `settings.rs` passa a ser `Json<Settings>` com `update`; favoritos, coleções, notas, tempo das lojas, `art.json`,
  mods, `classics.json` e as pendências do RetroAchievements também. A biblioteca mantém o próprio cache e a
  recuperação, mas usa `write_atomic`.
- Para arquivos de **terceiros** (configuração de emulador, `ModsDB.ini`, `shortcuts.vdf`): `edit_foreign(path, f)`,
  que lê bytes, distingue `NotFound` de erro, faz `.cerberus-bak-<data>` (até 3 cópias) e grava com `write_atomic`.
- Teste de propriedade: N threads fazendo `update` em chaves diferentes, nenhuma mudança perdida; arquivo inválido
  nunca é sobrescrito.

### P2. Regras de comando

- Todo `#[tauri::command]` que toca disco, registro, SQLite, processos ou rede é `async fn` ou
  `#[tauri::command(async)]`. Síncronos só para ler memória. Um teste (ou `grep` no CI) lista os síncronos e compara
  com uma lista permitida.
- Nenhum valor lido antes de um `.await` é gravado depois dele; use `Json::update` depois da rede.
- Janela só se cria fora da thread principal (já documentado; manter).

### P3. Rede

- Tudo passa por `net::send`, exceto login e downloads grandes, que usam `net::send_unthrottled` (mesmo erro sem
  URL, sem espera).
- `net::read_limited(reply, max)` para ler corpos; `max` obrigatório.
- `Client` com `redirect::Policy` que recusa sair de https e `connect_timeout(10 s)`.

### P4. Operações longas

- Um registro central `Busy` (instalação, backup, restauração, troca de emulador, importação) que `ExitRequested`,
  a bandeja e `import_data` consultam.
- Toda operação em várias etapas deixa um diário (`intent.json`) e tem um `recover()` chamado no `setup`, como
  `recomp/update.rs`.
- Varredura de órfãos (`*.part`, `.cerberus-*.tmp`, `.*-old`) no `setup`.

---

## 4. Ordem sugerida

| # | Item | Tamanho | Por quê agora |
| --- | --- | --- | --- |
| 1 | **P1** `store::Json` + `write_atomic` com testes | P/M | base para F1, F2, F6, F11 e L1 de lojas |
| 2 | **F1** settings e **F2** dados do jogador migrados para P1 (inclui o L1 de [lojas](lojas-e-contas.md)) | M | perda total de dados com um arquivo ruim; `store-playtime.json` grava sozinho |
| 3 | **F3** configuração de emulador com `edit_foreign` | P | apaga a configuração do RetroArch e dos emuladores |
| 4 | **F4** recuperar `.<nome>-old` e diário da troca | P | saves apagados, janela curta |
| 5 | **F5** importação aplicada no reinício | P/M | restauração desfeita sem aviso |
| 6 | **F7** comandos `async` + cache em `settings_view` | P | janela congelando; a mudança é mecânica |
| 7 | **F6** casos novos de settings (cai junto com o item 2) | P | — |
| 8 | **F10** + P4 (perguntar ao sair, varrer órfãos) | M | — |
| 9 | **F8** `read_limited` e política de redirecionamento | P | — |
| 10 | **F9** templates sem `..`; **F17** testes de 7z e tar | P | segurança com manifest de terceiro |
| 11 | **F11**, **F12** (restante para P1) | P | — |
| 12 | **F13** capability do overlay e CSP; **F14**, **F15**, **F16** | P | defesa em profundidade e limpeza |

Contagem: 4 Críticas (F1–F4), 3 Altas (F5–F7), 4 Médias (F8–F11), 6 Baixas (F12–F17).
