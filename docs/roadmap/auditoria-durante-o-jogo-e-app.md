# Auditoria: durante o jogo e o app em si

Revisão de 25/11/2026, só leitura (nenhum código editado, nada compilado, app não aberto). Área: menu por cima do
jogo, aviso de conquista, fotos e combos do controle, monitores, avisos do Windows, bandeja, abrir com o Windows,
instância única e `--play`, autoatualização e "O que há de novo", primeira configuração, Configurações gerais,
idiomas, janela sem moldura e navegação.

Referências lidas: Hydra (MIT) `src/main/services/achievement-notification-presenter*.ts`, `src/main/index.ts`
(`second-instance`), `src/main/services/screenshots/`; Playnite (MIT) `source/Playnite/App/CmdLineOptions.cs`,
`App/Updater.cs`, `App/PlayniteApplication.cs` (PipeServer), `WindowsNotifyIconManager.cs`,
`Settings/PlayniteSettings.cs` (backup automático). Heroic (GPL) só como ideia, sem leitura de código nesta revisão.
Já cobertos em outros documentos e **não repetidos** aqui: lojas-e-contas L1 (corrida no `settings.json` pelas
contas), trofeus-e-lembrancas (foto da conquista, `WDA_EXCLUDEFROMCAPTURE`, Desktop Duplication), ROADMAP 5.5
(zoom, reduzir movimento, mais idiomas), 6.4 (`cerberus://`, ações ao abrir o jogo, Desktop Duplication), 7.4 (som
do aviso, SDL).

Legenda: **[conferido]** = visto no código citado; **não verificado** = inferido do código ou da documentação da
biblioteca, confirmar no app ou com teste antes de depender disso.

---

## 1. Mapa

### 1.1 Durante o jogo

| O jogador faz | O que acontece | Funções e arquivos | Grava |
|---|---|---|---|
| Abre um jogo pelo Cerberus (recomp, local) | Entra em `Sessions`; se "Sair da frente" estiver ligado, a janela minimiza e volta no fim | `sessions::notify` → `in_game::session` (`in_game.rs:28-38`) | `sessions.jsonl`, `play_log`, histórico (`trail`) |
| Abre um jogo da Steam/Epic/Xbox | A loja abre; `store_time` detecta o processo a cada ciclo e conta o tempo; Steam vigiada para conquistas ao vivo | `store_time.rs:148-190`, `achievements/live.rs` | tempo por loja, `trail` "Conquista" |
| F12 com o jogo em frente | Foto só da área cliente da janela, por `BitBlt` da tela | `screenshots::watch` (laço de 40 ms, `screenshots.rs:182-224`), `take`, `shoot`, `win::grab` | `<dados>\screenshots\<id>\<ms>.png` |
| Ctrl+F12, View+Menu ou segurar View 1 s | Janela "overlay" transparente, sempre no topo, do tamanho do jogo: tempo jogado, relógio, Tirar foto, Abrir no Cerberus, Fechar o jogo | `overlay::toggle` (`overlay.rs:38-89`), UI `ui/src/overlay.rs` | nada |
| View+RB | Foto (como F12) | `Combos::step` (`screenshots.rs:147-176`) | PNG |
| Fechar o jogo pelo menu | `WM_CLOSE` na janela do jogo (como o X) | `overlay_quit_game`, `win::ask_to_close` | nada |
| Conquista cai (Steam ao vivo, RA pelo log dos emuladores) | Fila; janela "unlock" sem foco, ignora o mouse, num dos 4 cantos do monitor do jogo; 6 s por aviso; som gerado na hora | `unlock_toast::show` (`unlock_toast.rs:27-64`), `live.rs:89-113`, `main.rs:690-731`, UI `ui/src/unlock.rs` | `trail` "Conquista", `achievements.db` (pelo sync) |
| Configurações › Durante o jogo | Liga/desliga menu, F12, minimizar, aviso e canto; botão Testar | `set_in_game` (`main.rs:1513`), `in_game::set` | `settings.json` › `in_game` |
| Configurações › Notificações | Escolhe que avisos e se vão para o Windows | `set_notices` (`main.rs:1503`), `notify::tell` (`notify.rs:46-61`) | `settings.json` › `notices` |

### 1.2 O app em si

| O jogador faz | O que acontece | Funções e arquivos | Grava |
|---|---|---|---|
| Abre o Cerberus de novo (ou um atalho) | Plugin de instância única mostra a janela existente e, com `--play <id>`, emite `play-request` | `main.rs:3317-3323`, `shortcut::play_request`, UI `main.rs:418-436` | nada |
| Atalho na área de trabalho | `.lnk` com `--play "<id>"` | `create_shortcut` (`main.rs:2940`), `shortcut.rs` | `.lnk` na área de trabalho, `trail` |
| Fecha a janela | Sem bandeja e sem jogo: fecha. Com jogo: esconde e fecha sozinho quando o último jogo termina. Com bandeja: esconde | `background::on_close`, `game_ended` (`background.rs:44-64`) | `sessions::checkpoint` no `RunEvent::Exit` |
| Bandeja | Clique abre; menu com 5 recentes, Abrir, Sair | `background::tray`, `set_recent` (`background.rs:80-153`), UI `shell.rs:194-205` | nada |
| Abrir com o Windows / começar escondido | Plugin autostart com `--autostart`; com `start_hidden`, a janela é escondida no setup | `set_autostart`, `set_start_hidden` (`main.rs:1012-1029`), `main.rs:3369-3373` | registro do Windows (plugin), `settings.json` |
| Monitor | Move a janela para o monitor escolhido ao abrir e antes do Big Picture | `monitors::place`, `set_monitor`, `to_chosen_monitor` | `settings.json` › `monitor` |
| Atualização do Cerberus | 20 s após abrir e a cada 6 h pergunta ao GitHub; aviso no menu; "Atualizar agora" baixa, confere tamanho e SHA-256 (quando o GitHub dá), roda o NSIS com `/S` por PowerShell depois que o Cerberus sai, e reabre | `self_update.rs`, `update_install` (`main.rs:1155`), UI `updates.rs` | `<dados>\downloads\Cerberus-Launcher_<v>_x64-setup.exe` |
| Depois de atualizar | "O que há de novo" uma vez, com as notas da release como texto (sem HTML) | `whats_new`, `whats_new_seen` (`main.rs:1170-1192`), `notes_view` | `settings.json` › `seen_version` |
| Novidades | Atualizações de recomps, problemas não vistos, notícias da Steam, histórico por dia | `ui/src/news.rs`, `recent_activity` (40 últimas) | `localStorage` `cerberus-news-seen` |
| Primeira configuração | 6 passos: idioma, mesa/TV, pastas de ROMs, pasta dos recomps, nuvem, pronto | `ui/src/first_run.rs`, `finish_setup` (`main.rs:503`) | `settings.json` (`setup_done`, `start_in_tv`, pastas) |
| Idioma | Grava, aplica no backend e na bandeja, recarrega a janela | `set_language` (`main.rs:1277`), `language::apply`, `ui/src/i18n.rs` | `settings.json` › `language`, `localStorage` `cerberus-language` |
| Navegação | Janela sem moldura com botões próprios; Ctrl+1…7, Ctrl+,, Ctrl+K/F, `/`, Alt+←, `?` | `ui/src/shell.rs` | nada |

Janelas do processo: `main` (config), `overlay` e `unlock` (criadas sob demanda e só escondidas depois, nunca
fechadas). As três têm a mesma capacidade (`capabilities/default.json`) e `csp: null`.

---

## 2. Comparação

| Tema | Cerberus | Hydra (MIT) | Playnite (MIT) | Heroic (GPL, ideia) |
|---|---|---|---|---|
| Aviso de conquista | Janela sem foco, 4 cantos, fila simples, sem vigia | `AchievementNotificationPresenter`: máquina de estados com tempo-limite para "host pronto" (7 s), "conteúdo pronto" (5 s) e "terminou" (8,5 s); se a janela recebe foco, fecha ou o renderer cai, recria e cai para notificação do sistema; `backgroundThrottling: false`; `setAlwaysOnTop("screen-saver")`; aviso combinado "N conquistas em M jogos"; `setContentProtection` quando é foto | não tem | não tem |
| Tela cheia exclusiva | Aviso e menu não aparecem; foto sai preta e vira erro | Mesmo limite no aviso; fotos por Desktop Duplication (`desktop-duplication-frame.ts`) | Não desenha sobre o jogo | — |
| Segunda instância | Mostra a janela e emite `play-request`; se a UI ainda não ouve, perde | `second-instance` trata deep link; links "de fundo" (rodar jogo) **não** trazem a janela; cria a janela se não existe | `PipeServer` com mensagens; opções `--start`, `--uridata`, `--startfullscreen`, `--shutdown`, `--safestartup`, `--resetsettings` | deep link `heroic://launch` |
| Autoatualização | SHA-256 do próprio GitHub, opcional; sem assinatura | electron-updater (assinatura conferida pelo updater, não verificado nesta leitura) | Manifesto assinado com **Ed25519** e chave pública embutida, mais SHA-256 do arquivo (`Updater.cs:84-141`) | — |
| Configurações | Um JSON; arquivo quebrado vira padrão e é sobrescrito | — | Backup automático com rodízio (`AutoBackupFrequency`, `RotatingBackups`) | — |
| Sair com coisa em andamento | Sai sem perguntar | `before-quit` espera salvar tempo de jogo e fechar integrações | — | ideia: avisar download ativo |

O que vale trazer: o vigia com queda para notificação do Hydra, o "não trazer a janela" para `--play`, o manifesto
assinado do Playnite, e o backup de configurações com rodízio (já pedido na 7.1 para a biblioteca).

---

## 3. Falhas

Contagem: **Crítica 2 · Alta 3 · Média 9 · Baixa 11** (25).

### F1. `settings.json` quebrado vira padrão e é sobrescrito sem cópia — Crítica

- **Onde.** `src-tauri/src/settings.rs:122-138`.
- **Problema.** `load` cai em `Settings { setup_done: true, ..default }` quando o JSON não lê. O próximo `save`
  (qualquer um dos 44 pontos, inclusive `whats_new` na abertura, `main.rs:1180`) grava os padrões por cima. Somem
  pastas de ROMs, pasta dos recomps, nuvem, fontes, chaves e contas, sem aviso. `fs::write` não chama `sync_all`
  antes do `rename`: depois de uma queda de energia o arquivo pode ficar vazio (não verificado no NTFS). A corrida no
  `.tmp` (lojas-e-contas L1) também leva a esse estado.
- **Reproduzir.** Fechar o Cerberus, escrever `{` no começo do `settings.json`, abrir, trocar o tema de canto do
  aviso. O arquivo agora tem só padrões.
- **Corrigir.** Ao falhar a leitura, renomear para `settings.broken-<data>.json`, tentar `settings.json.bak` (cópia
  do último bom feita a cada `save`), avisar no app ("Suas configurações estavam danificadas; voltamos à cópia de
  ..."). `sync_all` no temporário antes do `rename`. Junta com o `settings::update` com trava da L1.
- **Aceite.** Um arquivo quebrado nunca é sobrescrito; a cópia boa volta sozinha; o jogador é avisado.
- **Teste.** Unitário: arquivo inválido + `.bak` válido → `load` devolve o `.bak` e deixa o quebrado à parte; sem
  `.bak` → padrão, e o quebrado continua no disco depois de um `save`.

### F2. Autoatualização roda o instalador sem assinatura e, se o GitHub não der o `digest`, sem hash — Crítica

- **Onde.** `self_update.rs:54` (`sha256` opcional), `:120-121` (`size == 0` e `sha256 == None` passam), `:143-160`
  (`/S` silencioso).
- **Problema.** A única conferência é contra dados do **mesmo** GitHub. Sem `digest` na resposta (releases antigas,
  mudança da API) nada é conferido. Não há Authenticode (`WinVerifyTrust`) nem manifesto assinado. Quem conseguir
  publicar uma release (token vazado, conta comprometida) instala código em silêncio em todos os PCs. Além disso,
  o ROADMAP 5.2 marca como feito "`version.json` com URL, SHA-256 e tamanho, versão mínima obrigatória, arquivo ruim
  marcado para não ser oferecido de novo e conferência da versão exata dentro do instalador": nada disso existe no
  código (`grep` por `min_version`, `version.json`, `WinVerifyTrust` sem resultado) [conferido].
- **Reproduzir.** Teste com uma release fictícia sem `digest` e `size: 0`: `from_release` aceita e `fetch_checked`
  devolve `Ok` para qualquer arquivo.
- **Corrigir.** (1) Recusar quando `sha256` ou `size` faltam. (2) Manifesto `latest.json` assinado com Ed25519
  (chave pública embutida no binário), como o Playnite; o `tauri-plugin-updater` faz isso pronto (minisign). (3)
  Quando o instalador for assinado (5.2), conferir o Authenticode e o editor antes de rodar. (4) Conferir a versão
  do `.exe` baixado (recurso de versão do PE). (5) Corrigir o ROADMAP 5.2 para "parcial".
- **Aceite.** Arquivo sem assinatura válida nunca roda; release sem hash não é oferecida.
- **Teste.** Unitários de `from_release` sem `digest` → `None`; de verificação com assinatura trocada → erro.

### F3. Fechar a janela depois de usar o menu ou o aviso deixa o processo vivo e sem janela — Alta

- **Onde.** `overlay.rs:68-79` e `unlock_toast.rs:39-52` criam janelas que só são escondidas; `background.rs:44-53`
  deixa a `main` fechar; `main.rs:3640-3651` não trata `ExitRequested` nem a destruição da `main`.
- **Problema.** No Tauri, o app só sai quando a última janela fecha; `overlay` e `unlock` escondidas continuam
  abertas (não verificado no app; é o comportamento documentado do Tauri 2). A `main` é destruída, o ícone da
  bandeja fica, e `background::show` e o plugin de instância única procuram `main` e não acham nada: abrir o
  Cerberus de novo não faz nada até matar o processo.
- **Reproduzir.** Bandeja desligada. Configurações › Durante o jogo › Testar (cria `unlock`). Fechar a janela. O
  processo segue no Gerenciador de Tarefas; clicar no atalho não abre nada.
- **Corrigir.** Ao destruir a `main` (sem bandeja), chamar `app.exit(0)`; ou, no `CloseRequested` da `main`,
  fechar `overlay` e `unlock`. Em `background::show`, se a `main` não existir, recriá-la.
- **Aceite.** Fechar a janela sempre encerra o processo quando a bandeja está desligada e não há jogo.
- **Teste.** Integração Windows (como o `handoff` de `self_update`): abrir, criar `unlock`, fechar `main`, o
  processo termina em até 2 s.

### F4. Fechar, "Sair" da bandeja, fim de jogo e atualização matam downloads, instalações e o backup do fim da partida — Alta

- **Onde.** `background.rs:35-37` (`playing` conta só `Sessions` e clássicos), `:56-64` (`game_ended` chama
  `app.exit(0)`), `:136` (`"quit" => app.exit(0)`), `main.rs:1161` (`update_install` olha só `Sessions`).
- **Problema.**
  1. Recomp baixando, jogo aberto, janela fechada (escondida pelo jogo): quando o jogo termina, o app sai e o
     download/instalação morre.
  2. Bandeja › Sair com um jogo aberto: sai na hora, sem o backup de saves do fim da sessão nem a releitura de
     conquistas; o tempo fica só até o último `checkpoint`.
  3. Janela fechada (sem bandeja) com só um download: fecha sem perguntar.
  4. Jogo da Steam aberto pelo Cerberus não está em `Sessions` (`main.rs:2490-2491`): fechar a janela sai e perde o
     aviso ao vivo e o tempo daquela partida; a atualização também não espera RetroArch nem jogos da loja.
- **Corrigir.** Um `busy(app)` único: sessões, clássicos, jogos de loja do `store_time`, `recomp::Installing`, fila
  de downloads e `setup_runner`. `game_ended` e `on_close` esperam ou escondem; "Sair" e "Atualizar" perguntam
  ("Um download está em andamento. Sair mesmo assim?").
- **Aceite.** Nenhuma saída automática com algo em andamento; saída manual sempre pergunta.
- **Teste.** Unitário de `busy` com cada fonte; teste de `game_ended` com `Installing` não vazio → não sai.

### F5. Panic na interface ao tirar foto depois de sair da página do jogo — Alta

- **Onde.** `ui/src/game.rs:1562-1566` (`Photos`) com `ui/src/api.rs:37-49` (`listen` sem `unlisten`, closure
  esquecida).
- **Problema.** Cada vez que a seção Fotos monta, registra um ouvinte de `screenshot-taken` que nunca sai. Depois
  de trocar de página, o `StoredValue` `id` está descartado e `id.get_value()` entra em pânico
  (`reactive_graph 0.2.16`, `traits.rs:766-767`: `try_get_value().unwrap_or_else(unwrap_signal!)`) [conferido na
  biblioteca]. Em WASM o pânico derruba a instância: a interface para de responder.
- **Reproduzir.** Abrir a página de um jogo com a seção Fotos, ir para a Biblioteca, abrir o jogo e apertar F12.
- **Corrigir.** `on_event` devolver um guarda que chama `unlisten` em `on_cleanup`; no handler, `try_get_value()`.
  Revisar também `settings_page.rs:1179` (`on_emu_progress` acumula um ouvinte por visita; `set` não entra em
  pânico, mas vaza).
- **Aceite.** N visitas à página não deixam N ouvintes; F12 depois de sair da página não derruba a UI.
- **Teste.** `wasm-bindgen-test`: montar e desmontar `Photos`, emitir o evento, sem pânico.

### F6. Segunda instância perde o `--play` quando a janela ainda não ouve — Média

- **Onde.** `main.rs:3317-3323` emite `play-request` sem guardar; `PlayRequest` só recebe o pedido da primeira
  instância (`main.rs:3354`); UI registra o ouvinte em `ui/src/main.rs:425`.
- **Problema.** Dois cliques rápidos no atalho com o Cerberus fechado: a segunda instância chega antes do WASM
  carregar e o evento some. Também `background::show` traz a janela para a frente justo quando o jogo vai abrir (o
  Hydra não mostra a janela para links de "rodar").
- **Corrigir.** No callback, gravar em `PlayRequest` e então emitir; a UI chama `take_play_request` também ao
  receber o evento (ou só ele). Não chamar `show` quando houver `--play` (opcional "Mostrar o Cerberus ao abrir por
  atalho").
- **Aceite.** O jogo do atalho sempre abre, uma vez.
- **Teste.** Unitário do estado: pedido guardado antes da UI, `take` devolve uma vez.

### F7. Aviso de conquista e mensagens somem em tela cheia exclusiva e com o "Não perturbe" do Windows — Média

- **Onde.** `unlock_toast.rs:27-64`, `notify.rs:46-61`, `overlay.rs:44-47`.
- **Problema.** O aviso é mostrado (e o som toca) mesmo quando nada pode aparecer sobre o jogo; o jogador ouve o
  som e não sabe o quê. O Windows, com a regra automática "ao jogar", engole a notificação e `show()` devolve `Ok`,
  então a queda para o aviso interno nunca acontece. "Não foi possível tirar a foto" e "Não foi possível abrir o
  menu" também somem.
- **Corrigir.** `SHQueryUserNotificationState`: com `QUNS_RUNNING_D3D_FULL_SCREEN` ou `QUNS_BUSY`, guardar os
  avisos e mostrar um resumo quando o jogo sair da frente ("3 conquistas em Hades"), como o combinado do Hydra; o
  mesmo vale para as respostas de F12/Ctrl+F12. Guardar também numa lista "durante a partida" nas Novidades.
- **Aceite.** Nenhuma conquista ou resposta é dada só por som.
- **Teste.** Unitário da decisão (estado → mostrar agora / guardar), com o estado injetado.

### F8. O aviso não tem vigia: janela que falha ou não carrega prende a fila — Média

- **Onde.** `unlock_toast.rs:27-64`, `ui/src/unlock.rs:30-58`.
- **Problema.** Se o WebView do aviso trava, recarrega ou nunca chama `unlock_next`, a fila cresce e a janela fica
  visível (transparente, sempre no topo) para sempre. Só a falha ao **criar** a janela cai para a notificação do
  Windows. Há ainda uma corrida: `unlock_next` devolve `None` (o backend esconde a janela), um `show` chega antes de
  a UI tratar a resposta; o evento é ignorado porque `busy` ainda é `true`, e o aviso fica parado até a próxima
  conquista, com a janela visível.
- **Corrigir.** Vigia no backend (como o Hydra): se a UI não pedir o próximo em 8 s, recriar a janela e, na segunda
  falha, usar `notify::tell`. Na UI, ao receber `None`, pedir mais uma vez se um evento chegou no meio.
- **Aceite.** Um aviso nunca fica parado mais de 8 s; a janela nunca fica visível sem aviso.
- **Teste.** Unitário de uma fila com relógio injetado (presenter testável, como `achievement-notification-presenter.test.ts`).

### F9. Menu por cima do jogo fica preso e o controle também comanda o jogo — Média

- **Onde.** `overlay.rs:60-89`, `screenshots.rs:147-176`, `ui/src/overlay.rs:98-125`.
- **Problema.**
  1. Sem tratamento de perda de foco: Alt+Tab com o menu aberto deixa uma janela sempre no topo, do tamanho do
     jogo, por cima de tudo, até Ctrl+F12 ou Esc.
  2. `SetForegroundWindow` chamado por um processo em segundo plano pode ser recusado pelo Windows (o atalho é lido
     por `GetAsyncKeyState`, não por `RegisterHotKey`, que daria o direito): o menu aparece sem teclado (não
     verificado; depende do `set_focus` do tao).
  3. XInput não é exclusivo: com o menu aberto, A/B/direcionais chegam também ao jogo.
  4. "Segurar View 1 s" abre o menu em jogos que usam Select segurado (mapa, placar, clássicos); a única chave
     desliga teclado e controle juntos (`InGame.menu`).
- **Corrigir.** Esconder o menu ao perder o foco (`WindowEvent::Focused(false)` da `overlay`) e devolver o foco ao
  jogo; usar `RegisterHotKey` para Ctrl+F12 (dá o direito de primeiro plano); opção separada "Atalhos do controle"
  e "Segurar View" desligado por padrão; avisar no menu que o jogo pode receber os botões (ou pausar o jogo,
  `WM_ACTIVATE` já faz isso na maioria).
- **Aceite.** Alt+Tab esconde o menu; segurar View não abre nada com a opção desligada.
- **Teste.** Unitário de `Combos` com a opção desligada; manual com um jogo em janela.

### F10. A janela escondida do menu continua lendo o controle a 30 Hz — Média (não verificado)

- **Onde.** `ui/src/overlay.rs:101-125` chama `tv::input::listen` uma vez; o componente nunca desmonta;
  `tv/input.rs:153-213` usa `setInterval` de 33 ms.
- **Problema.** Gasta CPU durante toda a partida. Se o WebView2 entregar o estado do controle a página escondida
  (não verificado), B no jogo chama `overlay_close` (puxa o foco para o jogo) e A clica no botão que ficou focado:
  se for "Tirar foto", `overlay_screenshot` **mostra o menu de novo** (`overlay.rs:136-139`).
- **Corrigir.** Parar o `listen` ao esconder (evento `overlay-closed`) e reabrir em `overlay-opened`; ou destruir a
  janela ao fechar e recriar (custa ~200 ms).
- **Aceite.** Com o menu escondido, nenhum timer de entrada roda na janela `overlay`.
- **Teste.** Manual com o DevTools da janela `overlay`; unitário do liga/desliga do `listen`.

### F11. ROADMAP 6.1 diz que o aviso tem 6 posições, versão secreta, agrupamento e volume — Média

- **Onde.** `ROADMAP.md` 6.1 (item marcado [x]); código: `shared/src/lib.rs:639-645` (`Normal`, `Rare`,
  `Complete`), `InGame` com 4 cantos (`main.rs:1515`), nenhuma fila combinada, volume fixo (`unlock.rs:242`).
- **Problema.** Pela regra da Onda 8, o item não poderia estar [x]. O mesmo vale para o 5.2 (ver F2).
- **Corrigir.** Desmarcar ou listar o que falta; implementar o combinado junto com a F7.
- **Aceite.** ROADMAP reflete o código.
- **Teste.** —

### F12. Diálogos de atualização e "O que há de novo" sem Esc, foco e rótulo — Média

- **Onde.** `ui/src/updates.rs:97-123`, `:143-157`; `ui/src/first_run.rs:315`; `shell.rs:30-39` só trata a lista
  de atalhos e a busca.
- **Problema.** `role="dialog"` sem `aria-labelledby`, sem foco inicial, sem prender o Tab e sem Esc. O leitor de
  tela entra no meio da página por trás. O botão "Maximizar" diz sempre "Maximizar" (`shell.rs:326`). A lista de
  atalhos (`?`) não fala de F12, Ctrl+F12 e dos combos do controle.
- **Corrigir.** Um componente `Dialog` único (rótulo, foco no botão principal, Esc fecha quando não está ocupado,
  devolve o foco), usado aqui e nos outros; rótulo "Restaurar" quando maximizada; incluir os atalhos de jogo na
  lista.
- **Aceite.** Esc fecha os dois diálogos; NVDA anuncia o título ao abrir.
- **Teste.** `wasm-bindgen-test` do `Dialog` (foco e Esc).

### F13. Enter na primeira configuração faz duas coisas — Média

- **Onde.** `ui/src/first_run.rs:316`.
- **Problema.** O `keydown` do contêiner chama `next()` em qualquer Enter, e o Enter num botão focado também clica
  nele: Enter em "Remover" remove a pasta e avança; Enter em "Voltar" volta e avança (não sai do lugar); Enter em
  "Pular configuração" chama `finish` duas vezes (não verificado se o `disabled` chega antes do segundo).
- **Corrigir.** Só avançar quando o alvo do evento não for um botão, link ou campo.
- **Aceite.** Enter num botão faz só o que o botão diz.
- **Teste.** `wasm-bindgen-test` com foco em "Voltar".

### F14. `settings.json` gravado em mais lugares do que a L1 lista — Média

- **Onde.** `whats_new` (`main.rs:1172-1180`, roda na abertura junto com as sincronizações), `finish_setup`,
  `rerun_setup`, `set_in_game`, `set_notices`, `set_language`, `set_monitor`, `set_close_to_tray`,
  `set_start_hidden`, `use_recomps_dir`, `set_site_catalog` (este com `await` de rede **depois** do `save`, ok) —
  todos `load` → mexe → `save` sem trava, alguns no thread principal e outros no tokio.
- **Problema.** Além do que a L1 descreve: na primeira abertura, `whats_new` grava `seen_version` com a cópia lida
  antes de `finish_setup`/pastas da primeira configuração terminarem em outra tarefa; trocar o canto do aviso
  enquanto uma sincronização Epic grava desfaz o canto. Os estados em memória (`in_game::set`, `notify::set`)
  ficam diferentes do arquivo quando o `save` perde a corrida.
- **Corrigir.** O `settings::update` da L1 em **todos** os 44 pontos (lista no `grep settings::save`), com teste
  que falha se aparecer um `save` novo fora dele.
- **Aceite.** Nenhum `settings::save` direto fora de `settings.rs`.
- **Teste.** O de 8 threads da L1, mais um teste de fonte que procura `settings::save(` fora do módulo.

### F15. "Sair da frente" volta a janela com outro jogo ainda aberto — Baixa

- **Onde.** `in_game.rs:28-38` (um só `AtomicBool`), chamado por `sessions.rs:557` e `store_time.rs:152,181`.
- **Problema.** Dois jogos (um recomp e um da Steam): o primeiro que fecha desminimiza o Cerberus por cima do outro.
- **Corrigir.** Só voltar quando `Sessions` e `store_time` estiverem vazios.
- **Teste.** Unitário com contador.

### F16. Foto: tudo que está por cima entra, e quadro escuro vira "tela cheia exclusiva" — Baixa

- **Onde.** `screenshots.rs:275-327`, `:113-119`.
- **Problema.** `BitBlt` da tela com `CAPTUREBLT` pega o próprio aviso de conquista, o Discord e notificações por
  cima. Uma tela de carregamento toda preta dá "O jogo está ... em tela cheia exclusiva". O PNG é codificado no
  laço de 40 ms (um 4K trava o laço por centenas de ms; não medido). Ctrl+F12 rápido (< 40 ms) pode passar
  despercebido.
- **Corrigir.** `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` nas janelas `unlock` e `overlay` (já previsto no
  plano de lembranças, vale para o F12 também); mensagem neutra para quadro preto; codificar em outra thread.
- **Teste.** Unitário da mensagem; manual com o aviso de teste na tela.

### F17. Pedido de idioma não chega às janelas já criadas — Baixa

- **Onde.** `ui/src/i18n.rs:55-60` recarrega só a janela atual; `overlay` e `unlock` ficam no idioma antigo até
  reiniciar. Backend usa `GetUserDefaultUILanguage` (`language.rs:24-30`) e a UI usa `navigator.language`
  (`i18n.rs:19-22`): no "Automático" podem divergir.
- **Corrigir.** Emitir `language-changed` e recarregar as três janelas; no automático, a UI pede o idioma ao
  backend.

### F18. Textos sem tradução fora do `tr!` — Baixa

- **Onde.** `ui/src/first_run.rs:235` (`format!("{} livres neste disco")`), `src-tauri/src/monitors.rs:25`
  (`" (principal)"`), `sessions.rs:540-541` ("1 arquivo copiado", "{} arquivos copiados", vão para o histórico e
  aparecem nas Novidades em inglês), `settings_page.rs` `row("Monitor", ...)`.
- **Problema.** O teste `every_text_has_its_english` só vê o que está em `tr!`/`tk!`.
- **Corrigir.** Passar por `tr!`; acrescentar ao teste uma busca de literais com acento dentro de `format!`,
  `push_str` e `view!` fora de `tr!` (lista de exceções explícita).

### F19. Começar escondido mostra a janela por um instante — Baixa (não verificado)

- **Onde.** `tauri.conf.json` (janela `main` sem `"visible": false`), `main.rs:3369-3373` esconde só no `setup`.
- **Corrigir.** `visible: false` na configuração e `show()` no fim do setup quando não for começar escondido.

### F20. Falha da bandeja impede o app de abrir — Baixa (não verificado)

- **Onde.** `main.rs:3356` (`background::tray(app.handle())?`). Com "Abrir com o Windows", o Cerberus pode subir
  antes do Explorer estar pronto.
- **Corrigir.** Registrar o erro e seguir sem bandeja; tentar de novo em 5 s.

### F21. Versão de teste instalada nunca recebe atualização — Baixa

- **Onde.** `self_update.rs:28-39`: `version("0.12.0-beta.1")` é `None`, então `is_newer(x, current)` é sempre
  falso e "O que há de novo" nunca aparece.
- **Corrigir.** Comparar pré-releases (semver) e oferecer a estável seguinte.
- **Teste.** Unitário `is_newer("0.12.0", "0.12.0-beta.1")`.

### F22. Atualização: PowerShell bloqueado deixa o app fechado sem reabrir — Baixa (não verificado)

- **Onde.** `self_update.rs:143-168`. Com política que bloqueia PowerShell (AppLocker, modo restrito), o `spawn`
  funciona mas o script não roda, e o Cerberus já saiu.
- **Corrigir.** Antes de sair, o script grava um arquivo-sinal; se não aparecer em 3 s, não sair e mostrar o erro.
  Ou usar um pequeno executável próprio de troca (como o Playnite).

### F23. "Fechar o jogo" sem retorno — Baixa

- **Onde.** `overlay.rs:144-152`. `WM_CLOSE` ignorado pelo jogo (ou bloqueado pelo UIPI se o jogo roda como
  administrador, não verificado) e o menu já sumiu: nada acontece.
- **Corrigir.** Depois de 5 s com o jogo vivo, reabrir o menu com "O jogo não fechou. Forçar?".

### F24. Monitor escolhido não aparece como escolhido depois de trocar — Baixa

- **Onde.** `ui/src/settings_page.rs:478-508`: `MonitorRow` guarda o `chosen` da primeira leitura e não atualiza
  `view_state`; também não dá retorno.
- **Corrigir.** Usar o `apply` como os outros controles.

### F25. Todas as janelas com todas as permissões e sem CSP — Baixa (endurecimento)

- **Onde.** `capabilities/default.json` (`main`, `overlay`, `unlock`), `tauri.conf.json` `csp: null`.
- **Problema.** Hoje nada põe HTML de fora na página (notas e notícias são texto), mas qualquer XSS futuro alcança
  `update_install`, contas e arquivos, inclusive a partir da janela do aviso.
- **Corrigir.** CSP restrita (`default-src 'self'; img-src 'self' asset: https:`), e capacidades separadas para
  `overlay`/`unlock` só com os comandos que usam (Tauri 2 permite permissões por comando com `build.rs`).

---

## 4. Lacunas (o que falta para a área ficar completa, sem ser defeito)

- **Jogos da loja sem menu nem F12**: `game_in_front` só olha `Sessions`; jogos da Steam/Epic/Xbox abertos pelo
  Cerberus (que só estão no `store_time`) não têm menu, foto nem combos.
- **Linha de comando**: só `--play` e `--autostart`. Faltam `--bigpicture`, `--shutdown`, `--safe` (sem
  filtros/mods, padrão de tema) e `--reset-settings` como no Playnite; deep link já está em 6.4.
- **Bandeja**: sem "Big Picture", "Pausar downloads", nem indicação de download/jogo em andamento no ícone ou na
  dica.
- **Atualização**: sem canal (estável/teste), sem "pular esta versão", sem verificação manual visível na primeira
  tela (só em Configurações), sem registro do que falhou.
- **"O que há de novo"** busca as notas no GitHub na hora: sem internet, não aparece (e volta a tentar a cada
  abertura); embutir as notas no binário resolve.
- **Primeira configuração**: sem indicação de passo para leitor de tela ("Passo 3 de 6"; os pontos são
  `aria-hidden`), e sem o passo de contas (Steam, Epic, Xbox, RA).
- **Testes que faltam**: `overlay::toggle`/`close`/`game_closed`; fila do aviso; `background::on_close` e
  `game_ended`; `in_game::session` com dois jogos; instância única com `--play`; `notify::tell` (ramo do Windows);
  qualquer teste de componente da UI (`wasm-bindgen-test` não aparece nesta área).

---

## 5. Ordem sugerida

1. **F1 + F14** (junto com a L1 de lojas-e-contas): uma trava e uma cópia boa para o `settings.json`. — M
2. **F2**: recusar sem hash agora (P), manifesto assinado depois (M); corrigir o ROADMAP 5.2.
3. **F3 + F4**: um `busy(app)` e uma saída só, que fecha as janelas auxiliares e pergunta. — M
4. **F5**: `on_event` com `unlisten` e `try_*` nos handlers (P), e varrer os outros `on_*`.
5. **F7 + F8 + F11**: presenter do aviso com vigia, tela cheia e combinado; atualizar o ROADMAP 6.1. — M
6. **F9 + F10**: menu que some ao perder foco, entrada parada quando escondido, opção do controle. — P/M
7. **F6, F12, F13**: `--play` guardado, `Dialog` acessível, Enter da primeira configuração. — P cada
8. Baixas (F15–F25) conforme passar pelos arquivos.
