# Auditoria: Big Picture Mode (modo TV com controle)

Revisão de 25/11/2026, só leitura (sem build, sem abrir o app). Código lido: todo `ui/src/tv/` (mod, input, focus,
pages, game, sheets, achievements, sound), a ligação em `ui/src/main.rs`, `ui/src/settings_page.rs` (seção Big
Picture e Controles), `src-tauri/src/monitors.rs` e `to_chosen_monitor` em `src-tauri/src/main.rs`, e `ui/styles.css`
(regras `.tv*`). Comportamento do Leptos conferido no fonte do `reactive_graph` 0.2.16 usado pelo projeto
(`with_value`/`get_value`/`get` em sinal descartado chamam `unwrap_signal!` e **dão panic**; `set`/`update_value` não).

Já registrado em outros documentos e **não repetido aqui como falha nova**: jogos não instalados, selo de origem,
página da loja e conquistas só da Steam na TV ([lojas L15](lojas-e-contas.md)); hub da TV só lê o banco, sem
Próximas/Raras e sem `problems` ([conquistas L9](conquistas-e-tempo.md)); filtros salvos, situação, nota e coleções
pela folha X ([metadados E10](metadados-filtros-e-estatisticas.md)); itens do 6.6 do ROADMAP (menu de energia,
trocar confirmar/cancelar, música, bateria, diagnóstico do controle).

"Não verificado" = deduzido do código, sem confirmação no app.

---

## 1. Mapa: o que o jogador faz com o controle

Entrada em `ui/src/tv/input.rs`: Gamepad API a 30 Hz (intervalo de 33 ms), só o **primeiro** controle conectado.
A, B, X, Y, LB, RB, Menu (toque = Configurações, segurar 0,9 s = sair), direcional e analógico esquerdo (repete
após 400 ms, depois a cada 120 ms; o analógico só vale depois de visto em repouso). Teclado: setas, Enter/Espaço = A,
Esc = B, Backspace = B (ou apagar no teclado na tela), PageUp/PageDown = LB/RB, Q/E = LB/RB, X/Y, F10 = Menu,
F11 = sair. Não usados: View/Select (8), LT/RT, analógico direito, L3/R3, botão Guide/PS, touchpad.

Global (`mod.rs:244`): LB/RB trocam de aba (6 abas, circular); B volta na pilha, ou vai ao Início, ou no Início
abre "Sair do Big Picture Mode?"; Menu vai a Configurações; segurar Menu sai. Uma folha aberta recebe todos os botões.
Barra de dicas embaixo com os ícones do controle (Xbox, PlayStation ou Nintendo, detectados pelo nome do driver).
Relógio (atualiza a cada 15 s), "Jogando X · N min", avisos (toasts) do app em tamanho de TV.

| Tela | O que dá para fazer | Observações |
| --- | --- | --- |
| **Início** (`pages.rs:165`) | Destaque do jogo em foco; fileiras: Jogados recentemente (12), Favoritos, Sua biblioteca (mais 14), Seus clássicos, Você já tem o jogo, Populares, Atualizados recentemente (14 cada). A abre a página, Y joga, X opções. | Clássico: A e Y jogam direto (não há página do clássico na TV); X não faz nada. |
| **Biblioteca** (`pages.rs:343`) | Chips Todos / Favoritos / Nativos / PC / Clássicos e Ordem (Recentes, A a Z, Mais jogados); grade de 6 colunas; A abre, Y joga, X opções. | Sem busca, sem pular por letra, sem filtro de plataforma ou loja; só instalados. |
| **Descobrir** (`pages.rs:490`) | Chips Buscar (teclado na tela), Você já tem o jogo, Gênero (circula), Ordem; grade que cresce de 36 em 36; A abre a página do recomp, Y busca, X opções. | Busca sem acento e só ASCII. |
| **Conquistas** (`achievements.rs:147`) | Resumo (conquistas, jogos, platinados) e três fileiras: Perto dos 100% (≥ 40%), Platinados, Conquistas recentes (14 cada). A abre a página (Steam na biblioteca) ou a lista do jogo. | Lista do jogo (`GameList`): desbloqueadas primeiro, depois as mais comuns; A revela uma secreta. |
| **Downloads** (`pages.rs:651`) | Downloads de recomps em andamento e atualizações prontas; A abre a página (em andamento) ou começa a atualização. | Só recomps; pausar/cancelar só na página do jogo. |
| **Configurações** (`pages.rs:747`) | Abrir o Cerberus no Big Picture, Sons de navegação, Sair do Big Picture, Fechar o Cerberus; mostra o tipo de controle. | Só 4 itens. |
| **Página do jogo** (`game.rs:62`) | Botão principal que segue o estado (Jogar, Voltar ao jogo = minimizar, Instalar, Baixando % · cancelar, Pausar, Continuar), Jogar o original no emulador, Atualizar, Fazer backup dos saves (recomp), Opções; descrição (catálogo ou loja Steam); capturas da Steam com visualizador (esquerda/direita, A ou B fecha); fatos; conquistas da Steam (x/y e 5 ícones). | Jogos Epic/Xbox/locais: sem descrição, sem capturas, sem conquistas (L15). |
| **Folha de opções (X)** (`sheets.rs:60`) | Jogar/Instalar, Página do jogo, Favoritar, Fazer backup dos saves (recomp), Abrir a pasta do jogo, Desinstalar/Remover (recomp e local) com confirmação. | Sem situação, nota, coleção, ocultar, propriedades. |
| **Teclado na tela** (`sheets.rs:218`) | Grade A–Z e 0–9 (6 colunas), Espaço, Apagar, Limpar, Pronto; Y espaço, X apaga, B fecha; o teclado físico digita. Filtra ao vivo. | Só usado no Descobrir. |
| **Entrar e sair** | Segurar Menu 0,9 s ou F11 fora da TV entra (`mod.rs:414`); Menu segurado, F11, Configurações › Sair ou a folha de saída saem. Entra no monitor escolhido e depois em tela cheia (`mod.rs:232`); sair sempre desfaz a tela cheia (`main.rs:469`). | Ver F2 e F17. |

---

## 2. Paridade com o modo normal

| Recurso | Modo normal | Big Picture | Situação |
| --- | --- | --- | --- |
| Jogos Epic/Xbox/Game Pass/Steam não instalados | Filtro Instalação, apagados, instalam pela loja | Tirados em `library_games` (`pages.rs:131`) | Falta (L15) |
| Selo de origem na capa | Sim (`SourceMark`) | Não | Falta (L15) |
| Busca na Biblioteca | Sim (`/`, Ctrl+F) | **Não** | Falta (F4) |
| Filtro por plataforma/loja | Sim ("Plataforma") | Só Nativos / PC / Clássicos | Falta |
| Situação, nota, coleções, ocultos | Sim | Não | Falta (E10, 6.6) |
| Ordens | Nome, recentes, tempo, nota, conquistas | Recentes, A a Z, Mais jogados | Parcial |
| Jogar algo aleatório | Sim | Não | Falta |
| Nome da aba de recomps | "Nativos" e "Recomps" (ROADMAP) | "Nativos" | Conferir texto |
| Hub de conquistas: filtros, Por plataforma, Próximas, Mais raras, Atualizar | Sim | 3 fileiras de 14, só o banco | Parcial (conquistas L9; F6) |
| Conquistas Xbox/Epic/RA | No hub e na aba do jogo | Só pela lista do jogo (`GameList`); página do jogo só Steam | Parcial (L15) |
| Lista de conquistas do jogo: filtro, ordem, hardcore, subconjuntos | Sim | Ordem fixa, sem filtro, sem selo hardcore | Parcial |
| Página do jogo: Visão geral, Original, Saves, Mods, Fotos, Conquistas, Ajustes, Gerenciar (`game.rs:21`) | 8 abas | Ações, texto, capturas da Steam, fatos | Parcial: faltam restaurar saves, mods, fotos do jogador, ajustes por jogo, gerenciar, cartão IGDB, Jogos parecidos, "Seu tempo de jogo", anotações |
| Página do clássico (`Route::Classic`) | Sim | Não; A só joga | Falta |
| Saves dos emuladores / página Saves | Sim (`Route::Saves`) | Não | Falta |
| Contas (Steam, Epic, Xbox, RA) | Configurações › Contas | Não | Falta (aceitável por ora; ao menos "entre de novo" visível) |
| Downloads | Página própria | Só recomps | Parcial (não verificado o que a página normal mostra além disso) |
| Notícias | Sim | Não | Falta |
| Configurações: monitor, idioma, menos animações, tamanho do texto, controles conectados | Sim | Não | Falta |
| Diagnóstico do controle | "Controles conectados" (`settings_page.rs:598`) | Só o tipo | Falta (6.6) |

---

## 3. Como os outros fazem

**Playnite Fullscreen** (MIT, `Playnite.FullscreenApp`, `Playnite/Settings/FullscreenSettings.cs`):
- **Para de ler o controle quando a janela não está ativa**: `GameController.StandardProcessingEnabled = IsActive`
  (`FullscreenApplication.cs:166-168`); também pausa a música (`MuteInBackground`).
- `GuideButtonFocus` e `GuideButtonFocusOnlyWhenNoGameRunning`: o botão Guide traz o Playnite para a frente.
- `SwapConfirmCancelButtons` (`FullscreenApplication.cs:378`), `ButtonPrompts` (escolher os ícones), `DisabledGameControllers`.
- Busca por janela de texto (`OpenSearchCommand` → `TextInputWindow`), filtros e predefinições de filtro
  (`SelectedFilterPreset`), `DarkenUninstalledGamesGrid`, linhas/colunas configuráveis, `ListNavigationSpeed`,
  `AsyncImageLoading`, `SmoothScrolling`, grade virtualizada (`Controls/FullscreenTilePanel.cs`).
- Menu do jogo (`GameMenuViewModel.cs:61-71`): jogar, instalar, desinstalar, favorito, ocultar, HDR, remover,
  ações do jogo, editar campos (`SetFields`).
- Menu principal: desligar, reiniciar, suspender, hibernar, bloquear, sair da conta, minimizar, jogo aleatório,
  atualizar biblioteca, ir para o modo desktop. Monitor, tamanho da fonte, esconder o cursor.

**Hydra Big Picture** (MIT, `hydra/src/big-picture/src`):
- `isWindowInputActive()` = `document.hasFocus() && visibilityState === "visible"` antes de agir
  (`components/providers/navigation-input.provider.tsx:145`).
- `services/gamepad.service.ts`: vários controles, troca o **controle ativo** pelo que foi usado por último
  (`activeGamepadIndex`), `gamepadconnected`/`disconnected`, laço por `requestAnimationFrame`, vibração.
- Teclado virtual QWERTY com números, Shift, camada de símbolos, cursor ← → e Enter
  (`components/common/virtual-keyboard/index.tsx:93`); posição flutuante que não cobre o campo.
- Biblioteca com filtros e coleções, grupos de foco por região (`navigation-layer`, `grid-focus-group`), página do
  jogo com conquistas, saves na nuvem, capturas do jogador, HowLongToBeat, ajustes; diagnóstico do controle.

**RecompLauncher** (GPL, só ideias, `ui/tv.js`):
- O controle chega pelo núcleo (eventos `pad`), não pela Gamepad API da página.
- Abas seguem a ordem e os ocultos do menu lateral; aba Em breve; capturas por jogo; saves por jogo; mods.
- Grade que cresce perto do fim (`growGrid`, também na Biblioteca); Cover Flow; nomes que nunca cortam.
- Configurações na TV: conta, monitor, tamanho (3 escalas), visualização, tema, página inicial, fundo, idioma,
  ícones dos botões, trocar botões, armazenamento, saves, sair.

---

## 4. Falhas

Contagem: **Crítica 0 · Alta 4 · Média 6 · Baixa 9.**

### F1 — Alta — Timer do fundo lê sinais descartados e derruba o WASM
- **Onde:** `ui/src/tv/mod.rs:163-175` (`show_backdrop`): o `set_timeout` de 220 ms chama
  `pending.with_value(...)` e `backdrop.with_untracked(...)`. Os dois pertencem ao `TvMode`. Se a TV fecha antes
  dos 220 ms, `with_value` em `StoredValue` descartado dá **panic** (`reactive_graph` 0.2.16, `traits.rs:730`).
  O timer nunca é cancelado.
- **Reprodução:** no Início, mover o foco e apertar F11 (ou clicar em Sair) em menos de 220 ms; ou segurar F11 (F2),
  que entra e sai a cada repetição da tecla e chama `show_backdrop` na montagem do Início. Esperado com panic:
  a interface para de responder (não verificado no app).
- **Correção:** `try_with_value`/`try_set` no callback, ou guardar o `TimeoutHandle` num `StoredValue` e cancelar
  em `on_cleanup` do `TvMode`. Regra geral: em timer, rAF e depois de `await`, só `try_*`.
- **Aceite:** script de teste que monta `TvMode`, chama `show_backdrop` e desmonta antes de 220 ms, sem panic
  no console. Busca no código (`rg "set_timeout|request_animation_frame"` em `ui/src/tv`) sem `get`/`with_value` não protegido.

### F2 — Alta — Entrar segurando Menu cai em Configurações, e segurar mais faz entra-e-sai
- **Onde:** `ui/src/tv/mod.rs:414-441` (`watch_for_tv` entra aos 900 ms com Menu ainda pressionado) e
  `ui/src/tv/input.rs:151,171-187` (o `PadState` novo começa com `menu_since = None`, então o Menu já pressionado
  conta como um toque novo). Soltando antes de mais 900 ms dispara `Btn::Menu` → `tv.tab(Settings)`
  (`mod.rs:294`). Segurando mais 900 ms dispara `MenuHold` → sai (`mod.rs:258`), e `watch_for_tv` volta a contar
  com o botão ainda pressionado → entra de novo. Com F11: o `keydown` repetido do teclado não é filtrado
  (`mod.rs:446` e `input.rs:239` não olham `e.repeat()`), então F11 segurado alterna a ~30 Hz.
- **Reprodução:** no modo normal, segurar Menu ~1 s e soltar: abre em Configurações, não no Início. Segurar ~2 s:
  entra, sai e entra de novo. Segurar F11: a tela pisca entre os modos.
- **Correção:** como já se faz com o analógico, o Menu (e A/B) só vale **depois de visto solto** ao montar
  (`PadState { pressed: snapshot_inicial, menu_seen_released: false }`). Em `watch_for_tv`, depois de sair, exigir
  soltar antes de recontar. Ignorar `KeyboardEvent.repeat` para F11 nos dois lugares.
- **Aceite:** teste unitário da máquina de estados extraída (`PadState::advance(snapshot, t) -> Vec<Btn>`):
  Menu pressionado desde o primeiro quadro e solto aos 300 ms não gera nada; pressionado de novo gera `Menu`.
  No app, com o controle Xbox: segurar Menu 1 s abre no Início; segurar 3 s entra uma vez só.

### F3 — Alta (não verificado) — O controle continua comandando o Cerberus com o jogo aberto
- **Onde:** `ui/src/tv/input.rs:153-222` lê o controle sem checar `document.hasFocus()`/visibilidade, nem se há jogo
  rodando; `mod.rs:416` (`watch_for_tv`) idem. O Hydra checa os dois (`navigation-input.provider.tsx:145`) e o
  Playnite desliga a leitura quando a janela não está ativa (`FullscreenApplication.cs:168`).
- **Reprodução (se o WebView2 entregar dados do controle com a janela sem foco):** abrir um jogo pela TV; dentro do
  jogo, tocar Start/Menu para pausar → o Cerberus atrás pula para Configurações; segurar Start 1 s → sai da TV e
  desfaz a tela cheia; apertar A/Y → age no item em foco (pode tentar abrir outro jogo). No modo normal, segurar
  Start 1 s dentro de um jogo põe o Cerberus em Big Picture e tela cheia.
- **Correção:** pular o quadro quando `!document.has_focus()` ou `visibility_state != Visible`; descartar o estado
  (como no repouso do analógico) ao voltar o foco; opcionalmente ignorar tudo enquanto `library.running` não estiver
  vazio, salvo Guide/Menu para trazer de volta (ideia do `GuideButtonFocus` do Playnite).
- **Aceite:** primeiro confirmar com o Xbox 360 conectado se o WebView2 entrega os dados sem foco (anotar no
  documento). Depois da correção: com um jogo em primeiro plano, A/B/Menu não mudam nada no Cerberus; ao voltar,
  o primeiro A não dispara sozinho.

### F4 — Alta — Biblioteca de ~950 jogos sem busca nem salto
- **Onde:** `ui/src/tv/pages.rs:343-479`: só chips e ordem. O teclado na tela existe (`sheets.rs:218`) mas o
  `tv.search` só filtra o Descobrir (`pages.rs:518`). Com 6 colunas são ~160 linhas; a 120 ms por repetição,
  ~20 s segurando para baixo, e LT/RT não fazem nada.
- **Reprodução:** com uma biblioteca grande, achar um jogo que começa com "W" em ordem A a Z.
- **Correção:** chip "Buscar" na Biblioteca usando o mesmo teclado (busca local separada do Descobrir); LT/RT
  saltam uma página ou a próxima letra em A a Z (letra grande no canto ao saltar); opcional: Y longo para jogo aleatório.
- **Aceite:** com 1.000 jogos sintéticos, chegar a qualquer jogo em menos de 10 s pelo controle; busca sem acento
  ("pokemon" acha "Pokémon").

### F5 — Média (não medido) — A grade da Biblioteca desenha todos os jogos e refaz tudo a cada mudança
- **Onde:** `ui/src/tv/pages.rs:448-462` desenha as ~950 capas de uma vez (o Descobrir cresce aos poucos,
  `pages.rs:515`). Cada capa tem 3 fechos reativos (`tile`, `pages.rs:84-127`): `class:on` lê `focus.row` e
  `focus.col` (todas reexecutam a cada movimento), `bar` lê `library.progress` (todas reexecutam a cada evento de
  progresso de download), `badge` lê `library.running`. Qualquer mudança em `library.games` refaz a grade inteira
  (sem `<For>` com chave), recarregando as imagens. `.tv-grid` não tem `content-visibility` (o desktop tem,
  `styles.css:272`). A ordem A a Z chama `Item::name()` → `game_names` (busca linear no catálogo) em cada comparação
  (`pages.rs:378`).
- **Correção:** a mesma grade que cresce do Descobrir (ou janela visível), `<For>` com chave pelo id, um
  `Memo<(usize, usize)>` com o item em foco e `class:on` por `Memo` derivado, `content-visibility: auto` em
  `.tv-tile`, nome calculado uma vez antes de ordenar.
- **Aceite:** com 1.500 jogos sintéticos, abrir a Biblioteca em menos de 300 ms e cada passo do foco abaixo de
  16 ms (perfil do WebView2); um download em andamento não reexecuta os fechos das outras capas.

### F6 — Média — Conquistas: a maioria dos jogos não é alcançável e o vazio engana
- **Onde:** `ui/src/tv/achievements.rs:154-170`: só três fileiras de 14. Um jogo com menos de 40% fora dos 14 mais
  recentes não aparece em lugar nenhum. Se nenhum jogo cai nas fileiras, mostra "Nenhuma conquista ainda. Jogue um
  jogo da Steam ou conecte o RetroAchievements..." (`:223`) mesmo com o resumo contando jogos e sem citar Xbox/Epic.
- **Correção:** quarta fileira ou grade "Todos os jogos" (ordem pela última conquista, depois nome) e mensagem de
  vazio que dependa de `hub.games.is_empty()` e cite as quatro fontes. (Próximas, Raras e Atualizar ficam com a L9.)
- **Aceite:** com 40 jogos no hub, todos alcançáveis pelo controle; com jogos só abaixo de 40% e sem data, a tela
  não diz "nenhuma conquista".

### F7 — Média — Só o primeiro controle conectado é lido
- **Onde:** `ui/src/tv/input.rs:113-141` retorna no primeiro `connected`. Um controle virtual parado (Steam Input,
  DS4Windows/ViGEm, receptor sem fio ligado) no índice 0 bloqueia o controle real; um DualSense com DS4Windows
  aparece duas vezes. O tipo (ícones) também vem só do primeiro. O Hydra troca para o controle usado por último.
- **Correção:** ler todos e eleger o ativo pelo último que teve botão ou eixo acima do limite; ícones seguem o ativo.
- **Aceite:** teste unitário da escolha do ativo com dois instantâneos; no app, com dois controles, qualquer um navega.

### F8 — Média — Remover/desinstalar lê sinal da TV depois do `await`
- **Onde:** `ui/src/tv/sheets.rs:129-140`: depois de `api::remove_game(...).await` chama
  `tv.view.get_untracked()` (`:136`). Desinstalar um recomp grande leva segundos; se o jogador sair da TV nesse meio
  tempo, `get_untracked` em sinal descartado dá panic.
- **Correção:** `tv.view.try_get_untracked()` e sair cedo se for `None`.
- **Aceite:** teste que descarta o dono antes do futuro terminar, sem panic.

### F9 — Média — Teclado na tela: espaço, acentos, mouse e busca sem acento
- **Onde:** `ui/src/tv/input.rs:231` (Espaço do teclado físico vira A, então digita a tecla em foco em vez de espaço
  enquanto o teclado está aberto); `sheets.rs:211` (só A–Z e 0–9: sem "é", "ç", ":", "-", "'"); `pages.rs:523`
  (a busca só faz `to_lowercase`, então "pokemon" não acha "Pokémon"); `sheets.rs:275` (as teclas não têm `on:click`,
  diferente do resto da TV, que aceita mouse).
- **Correção:** com o teclado aberto, Espaço físico = `Type(' ')`; dobrar acentos nos dois lados da comparação
  (função comum com a busca do desktop); teclas com `on:click`; linha de símbolos `- : ' &` ou camada como no Hydra.
- **Aceite:** teste unitário da busca ("pokemon" ↔ "Pokémon", "zelda totk"); no app, Espaço físico digita espaço.

### F10 — Média — Quase nada da TV tem teste
- **Onde:** os únicos testes da TV são `pad_name` e `kind_of` (`input.rs:263-279`). Sem teste: `Focus::step` e
  `clamp` (`focus.rs:50-85`, lógica pura), `grid_rows` (`pages.rs:336`), a máquina de estados do controle (presa num
  fecho em `input.rs:161`), a tradução de teclas em `dispatch` (`mod.rs:244`), as ações da página do jogo
  (`game.rs:85-127`), a escolha das fileiras (Início, Conquistas).
- **Correção:** extrair `PadState::advance` e o mapeamento de teclas como funções puras; testar `Focus` com
  `Owner` de teste; `actions` como função de `(Option<Game>, Option<RecompEntry>, downloading, running)`.
- **Aceite:** `cargo test -p ui` cobre: repetição 400/120 ms, deriva do analógico, Menu toque × segurar, Menu já
  pressionado na montagem (F2), foco pulando fileira vazia, grade com última linha curta.

### F11 — Baixa — Textos sem tradução
- `pages.rs:800` "◀ Ligado ▶" / "◀ Desligado ▶" sem `tr!`; `pages.rs:262` `format!("{} downloads")` sem `tr!`.
- **Aceite:** com o app em inglês, Configurações da TV e o destaque do Início sem português.

### F12 — Baixa — LB/RB na página do jogo contam a partir do Início
- `mod.rs:275-278`: em `View::Game`/`View::Achievements` a aba atual vale 0, então RB vai sempre à Biblioteca,
  mesmo vindo do Descobrir. Correção: guardar a aba de origem (o fundo da pilha). Aceite: do Descobrir → jogo → RB
  vai para Conquistas.

### F13 — Baixa — "Fechar o Cerberus" fecha sem confirmar
- `pages.rs:785`: A fecha na hora (a folha de saída tem a mesma opção, mas é uma escolha dentro de uma pergunta).
  Downloads em andamento são perdidos ou pausados (não verificado). Correção: abrir `Sheet::Leave` ou confirmar.

### F14 — Baixa — Dicas incompletas
- Descobrir não mostra X Opções (`pages.rs:586`); a lista de conquistas do jogo apaga as dicas e não mostra
  B Voltar (`achievements.rs:62`); Downloads e Configurações não mostram B; "Indisponível" fica em foco e A não diz
  nada (`game.rs:206`).

### F15 — Baixa — Mouse pela metade
- `.tv { cursor: none }` (`styles.css:1078`): o ponteiro só aparece sobre o que é clicável; `tile` não tem
  `on:mouseenter` (`pages.rs:118`), então passar o mouse não move o foco (os chips movem); o teclado não aceita clique (F9).

### F16 — Baixa (não verificado) — Sons podem não tocar
- `sound.rs:57` cria o `AudioContext` no primeiro som, que vem do controle (não é gesto do usuário para a política
  de autoplay) e nunca chama `resume()`. Se o WebView2 seguir a política do Chrome, fica suspenso. Correção:
  `resume()` quando `state() == Suspended`. Aceite: entrar pela TV direto na abertura e ouvir o primeiro movimento.

### F17 — Baixa (não verificado) — Monitor
- `mod.rs:232-241` move a janela para o monitor escolhido ao entrar, mas sair não a devolve para onde estava;
  `monitors::place` (`monitors.rs:41`) não faz nada se o monitor escolhido estiver desligado (a TV abre no monitor
  atual, sem aviso). Não há escolha de monitor dentro da TV (o RecompLauncher e o Playnite têm). Conferir com dois monitores.

### F18 — Baixa — Releitura das ROMs a cada visita
- `pages.rs:171` e `:354`: `library.roms.refetch()` ao montar o Início e a Biblioteca, ou seja, a cada LB/RB que passa
  por eles (chama `get_settings`, que calcula `owned_games`). Correção: só ao entrar na TV ou no evento `catalog-changed`.

### F19 — Baixa (não verificado) — Texto longo da página do jogo fora do alcance
- `game.rs:305-315`: descrição, "Como começar" e loja ficam abaixo dos botões. Sem capturas (jogos não Steam) não há
  linha para baixo, e o `scroll_to` só rola até o item em foco. Com um texto grande, o fim fica escondido
  (`.tv-main` rola, mas o controle não chega). Correção: fazer do bloco de texto uma linha focável que rola, ou
  limitar o texto com "Mais" na folha. Conferir no app com um recomp de descrição longa.

---

## 5. Lacunas (o que não é defeito, mas falta)

- Busca e filtro por plataforma/loja na Biblioteca (F4); "Jogar algo aleatório".
- Página do clássico na TV (saves, conquistas RA, emulador escolhido) e página Saves.
- Página do jogo: restaurar backup, fotos do jogador, mods, ajustes por jogo, Jogos parecidos, tempo de jogo.
- Configurações da TV: monitor, idioma, menos animações, tamanho, controles conectados, trocar confirmar/cancelar,
  escolher os ícones (6.6), "voltar ao Cerberus" pelo botão Guide/PS.
- Contas: ao menos o aviso "entre de novo" visível na TV (depende da L3 de lojas).
- LT/RT para pular página, View/Select para opções, vibração ao confirmar (Hydra), música (6.6).
- Escurecer não instalados quando a L15 trouxer esses jogos (ideia do Playnite).
- Teste manual com controle físico ainda pendente pelo ROADMAP (Xbox 360); faltam também DualSense/DualShock 4 e
  Switch Pro (troca de A/B em `input.rs:124`).

## 6. Ordem sugerida

1. **F1 e F8** (panic): trocar por `try_*` e cancelar o timer. P.
2. **F2** (entrar/sair pelo Menu e F11), com a máquina de estados extraída e testada (início da F10). P.
3. **F3**: medir no WebView2 se há entrada sem foco; aplicar a guarda de foco de qualquer forma. P.
4. **F4 + F9**: busca na Biblioteca com o teclado melhorado e busca sem acento. M.
5. **F5**: grade que cresce, `<For>` com chave, foco por `Memo`; medir com 1.500 jogos. M.
6. **F6, F7**. P/M.
7. **F10** completo (testes de foco, ações e fileiras). M.
8. Baixas F11–F19 num lote só, conferido no app com controle e em captura de tela.
9. Depois, a paridade da tabela 2 junto com L15 e E10.
