# Auditoria: recomps — catálogo e ciclo de instalação (25/11/2026)

Escopo: Descobrir, catálogo e fontes, instalar / atualizar / pausar / cancelar / reparar / mover / desinstalar,
Downloads e autoatualização do Cerberus. Leitura do código em `CerberusLauncher-ajustes` (sem compilar, sem abrir o
app). Complementa [recomps-robustos.md](recomps-robustos.md) (versões, campos do SDK, Resolver problemas, liberar
espaço) e os itens 5.3, 6.7 e 7.5 do ROADMAP; o que já está lá não é repetido, só citado.
"Não verificado" = deduzido do código sem reproduzir.

---

## 1. Mapa

### 1.1 Catálogo (o que alimenta o Descobrir)

| Camada | Onde | Em disco |
| --- | --- | --- |
| Catálogo embutido (117 jogos) | `catalog/recomps.json` via `include_str!` (`catalog.rs:284`) | — |
| Catálogo próprio publicado | `refresh_own` baixa `OWN_URL` (raw.githubusercontent, branch `main`) a cada 6 h (`catalog.rs:304`) | `%APPDATA%\app.cerberus.launcher\cache\own-catalog.json` (gravado com `fs::write`, não atômico) |
| Catálogo do recomplauncher.com (~1900) | `refresh` → `from_recomplauncher` → `merge(site, nosso)` (`catalog.rs:348-412`); chave liga/desliga `use_site` | `cache\pc-catalog.json` (via `.json.part` + rename) |
| Fontes do jogador (URL https) | `sources.rs::fetch/store` (formato Cerberus ou RecompLauncher); entram por `set_extras` só se id/repo não existir (`catalog.rs:227`) | `cache\sources\<sha>.json` (tempfile + persist) |
| Release por jogo (quando o catálogo não traz o arquivo exato) | `releases::latest` (GitHub/GitLab/Forgejo), cache 6 h | `cache\releases\<repo>.json` |

Ordem de prioridade: **site > nosso** (o nosso só preenche vazios: saves, handoff, textos em pt-BR) e
**publicado > embutido**. `RecompEntry` é montado em `mod.rs::entries/to_entry` (1092 linhas de catálogo, sem ir à
rede); "Atualização disponível" = `latest.version != installed` (comparação de texto).

### 1.2 Instalar (primeira vez) — `install_recomp` (`main.rs:2175`) → `recomp::install` (`mod.rs:1101`)

1. UI: `discover::start_install` põe `Downloading{0,0}` em `library.progress` (só na memória da UI).
2. Backend reserva o id em `Installing.0` (recusa se já instalando, se externo, se o jogo está aberto).
3. `choose_download` (`mod.rs:1248`): com `tag` + `files[os].sha256` no catálogo baixa direto
   `…/releases/download/<tag>/<nome>` com o checksum **do catálogo**; senão pergunta a release mais nova e escolhe por
   padrão/pontuação (`assets.rs`), checksum = `digest` do GitHub (pode não existir; GitLab nunca tem).
4. `install::download` (`install.rs:64`): `.part` em `data\downloads\<id>\<nome>.<tag10>.part`, Range para retomar,
   4 tentativas, limite de velocidade (`throttle::Pace`), SHA-256 calculado durante o download, rename para o nome final.
5. `prepare_release` (`mod.rs:1199`) em `spawn_blocking`: extrai para `.staging-<id>` ao lado do destino
   (zip/7z/tar.gz/exe), acha o programa (manifesto → `start` do catálogo → `find_program`), confere que está dentro
   do staging, `overlay` (rename/copy) para `recomps\<id>`, grava `.cerberus-install` com o id. Apaga o arquivo baixado.
6. `fetch_repo_manifest` (só Forgejo) e `register` → `library.json` (gen 0).
7. `main.rs`: trilha (`trail.jsonl`), aviso do Windows se fora de foco, Easy Setup (`use_game_copy`).

### 1.3 Atualizar / Reparar

Mesmo comando. Diferenças: destino = `install_dir` atual; `check_install_folder` exige o marcador (ou pasta legada
em `default_installs`); `backup_saves("Antes de atualizar")`; `update::Update::begin` cria
`.cerberus-update-<sha(id)>\intent.json` + `.lock`; extrai em `…\next`, `preserve` traz (hard link ou cópia) tudo o
que a release nova **não** tem, `publish` renomeia `alvo → previous` e `next → alvo`; `register` grava gen+1;
`finish` limpa ou faz rollback. Na abertura, `LocalLibrary::access` chama `update::recover` para cada recomp.
"Reparar instalação" (`ui/src/game.rs:1376`) é só `start_install` de novo.

### 1.4 Pausar / Continuar / Cancelar

`pause_install`/`cancel_install` (`main.rs:2542/2583`) mexem no `Halt` (atômicos). Pausa devolve `PAUSED`, o
`Halt` vai para `Installing.1` e o `.part` fica. Continuar = instalar de novo (reaproveita o `.part` pelo nome).
Cancelar apaga o `.part`. Só durante o download; "Preparando arquivos…" não tem botão. Nada disso é gravado:
fechar o app perde o estado de pausa (o `.part` fica e é apagado após 14 dias por `sweep_stale_downloads`).

### 1.5 Downloads (`ui/src/pages.rs:50`)

Lista `library.progress` (sem fila: cada Instalar roda em paralelo), "Pausar tudo"/"Continuar tudo",
"Atualizações prontas" (com "Atualizar todos" sequencial no front), "Não terminou" (memória da sessão) e "Recentes"
(da trilha).

### 1.6 Mover (`main.rs:2554` → `relocate::move_install`)

Mesmo volume: `rename`. Outro volume: `disk::require`, cópia para `<destino>\.moving-<nome>`, rename, **apaga a
pasta antiga**, depois grava a biblioteca (exe, tracking_exe, game_copy rebaseados).

### 1.7 Desinstalar (`main.rs:2105 remove_game`)

`while_idle` → `backup_saves("Antes de desinstalar")?` → `delete_files` (marcador, renomeia para
`.cerberus-removing-<id>` e apaga; o resto vai em `sweep_removed` na próxima abertura) → `library.remove`.
Backups ficam em `data\saves\<id>`.

### 1.8 Autoatualização do Cerberus (`self_update.rs`)

20 s após abrir e a cada 6 h: `releases/latest` do repo; asset `*_x64-setup.exe` só de
`https://github.com/<repo>/releases/download/`; baixa para `data\downloads\Cerberus-Launcher_<v>_x64-setup.exe`,
confere tamanho e `digest` (se houver), PowerShell espera o PID, roda `/S` e reabre. Recusa só se há **jogo** aberto.

---

## 2. Comparação

| Tema | Cerberus | RecompLauncher (GPL, ideias) | Heroic (GPL, ideias) | Hydra (MIT; só downloads/fila) |
| --- | --- | --- | --- | --- |
| Fila | Sem fila: downloads em paralelo, estado só na UI | Pausa/retomada por id com `.part` guardado (`install.rs` `pause/kept/resumed/discard`) | Fila persistida (`downloadManager` store: `queue`, `finished`), um por vez, ordem, "desligar/suspender ao terminar" (`downloadqueue.ts:100-168`) | Fila e ordem persistidas em LevelDB (`queueOrder`, `pausedOrder`), ativação serializada (`download-activation.ts`) |
| Rede caiu | 4 tentativas e erro | — | Pausa sozinho offline e retoma online (`downloadqueue.ts:40`) | Retoma com estado de range persistido (`range-download-state.ts`) |
| Disco cheio no meio | Erro cru do SO; `.part` fica | — | — | Para o download, marca erro, tira da fila, avisa e segue a fila (`download-manager.ts:928`) |
| Sair com download ativo | Fecha sem perguntar | não verificado | Pergunta "operações pendentes" (`utils.ts:253 handleExit`) | não verificado |
| Arquivos que a versão nova tirou | Ficam para sempre (`preserve`) | Remove os que a release velha tinha e a nova não, se não mudaram desde a instalação (`install.rs:765 dropped`) | — | — |
| Links simbólicos no pacote | tar/zip/7z das bibliotecas | Valida cadeia de links dentro da pasta (`validate_links`) | — | — |
| Atualização do launcher | Instala e reabre; sem volta | Registro de falha no primeiro início e volta à versão anterior (`update_rollback_tests.rs`) | — | — |
| Checksum | Catálogo ou `digest` do GitHub; sem nenhum, instala calado | Equivalente (não verificado) | — | Verificação pós-download (`download-verifying`) |

---

## 3. Falhas

Gravidade: **Crítica** = perde dados/segurança; **Alta** = quebra o uso; **Média**; **Baixa**.

### F1 — Instalação nova adota uma pasta alheia com o mesmo nome e depois a apaga inteira (Crítica)
- **Onde:** `mod.rs:1149-1151` (destino `installs/<id>` sem checagem), `mod.rs:1206-1224` (overlay + grava marcador),
  `mod.rs:1382-1397` (`delete_files` confia no marcador).
- **Reproduzir:** pasta de instalação = `D:\Jogos`; já existe `D:\Jogos\zelda64recomp` (instalado à mão, com mods e
  pacotes de textura). Instalar Zelda 64 pelo Cerberus: os arquivos são sobrepostos e o marcador gravado. Desinstalar:
  a pasta inteira vai embora; só os saves dos locais conhecidos têm backup.
- **Correção:** antes de extrair, se o destino existe e não tem o nosso marcador com o mesmo id (nem é resto de
  instalação interrompida nossa — ver F11), recusar com "Já existe uma pasta com esse nome; use 'Já tenho instalado'
  ou escolha outra pasta", ou instalar em `<id>-2`.
- **Aceite / teste:** teste de `prepare_release` com destino pré-existente sem marcador → erro e nenhum arquivo
  alterado; com marcador do mesmo id → segue.

### F2 — "Reparar" e "Atualizar" sobrescrevem configurações e mods que a release também traz; a UI promete o contrário (Crítica)
- **Onde:** `update.rs:239-245` (`preserve`: se o arquivo existe na release nova, o do jogador é descartado);
  texto em `ui/src/game.rs:1385-1390` ("saves, configurações e mods ficam").
- **Reproduzir:** recomp que publica `config.ini`/`settings.json` padrão; o jogador muda; Reparar → volta ao padrão.
  Mesmo com DLL trocada pela troca de DLSS do Cerberus ou um mod do GameBanana que substitui um arquivo da release.
  (Complementa "Mods e DLSS trocados" em [recomps-robustos.md §1](recomps-robustos.md), que só registra o caso
  dos arquivos que a release não tem.)
- **Correção:** guardar, na instalação, a lista e o hash dos arquivos da release (`.cerberus-files.json`). No
  `preserve`, arquivo cujo hash atual ≠ hash instalado = mudado pelo jogador: manter o do jogador (ou guardar como
  `.cerberus-old`) e anotar na trilha; mods/DLSS do Cerberus registram o que trocaram. Ajustar o texto da UI.
- **Aceite / teste:** release v1 com `config.ini=A`; jogador grava `B`; update para v2 com `config.ini=A2` → fica `B`
  e a trilha cita o arquivo; arquivo não mexido → vira `A2`.

### F3 — Catálogo de terceiros manda no repositório e no checksum dos jogos do catálogo próprio (Crítica, segurança)
- **Onde:** `catalog.rs:378-412` (`merge`: entrada do site vence; a nossa só preenche vazios), `catalog.rs:260`
  (`OWN_URL` em `main` sem assinatura), `mod.rs:1253-1259` (checksum vem do mesmo catálogo que dá a URL).
- **Reproduzir (não verificado ao vivo):** se o `pc-catalog.json` do recomplauncher.com trouxer, para um id do nosso
  catálogo, outro `repo` e outro `files.sha256`, o Cerberus baixa e executa o binário desse repo com checksum "válido".
  O SHA-256 só protege o transporte, não a origem.
- **Correção:** para ids/repos do catálogo próprio, `repo`, `host`, `tag`, `files` e `start` vêm só do nosso; o site
  só completa texto e arte. Assinar `recomps.json` (minisign/ed25519 com chave embutida) e recusar cópia sem assinatura.
  Mostrar na página do jogo de onde veio a entrada (Cerberus, site, fonte X).
- **Aceite / teste:** teste de `merge` com site trazendo `repo` diferente para um id nosso → fica o nosso; catálogo
  publicado com assinatura inválida → ignorado, segue o anterior.

### F4 — Recomp que saiu do catálogo não pode ser desinstalado nem ter backup (Alta)
- **Onde:** `main.rs:2126` (`backup_saves(...)?` antes de apagar) → `mod.rs:199` (`entry_at` → `find` → "Esse recomp
  não está no catálogo.").
- **Reproduzir:** instalar um recomp que só existe no recomplauncher.com; desligar "Usar o catálogo do site" (ou
  remover a fonte que o trouxe, ou o site renomear sem `replaced`). Remover → erro; o jogo fica preso na biblioteca.
  O backup depois de jogar falha do mesmo jeito.
- **Correção:** guardar na instalação uma cópia da entrada (`.cerberus-entry.json` ao lado do marcador) e usá-la quando
  `find` falhar; em último caso, desinstalar avisando "sem backup de saves (locais desconhecidos)" com confirmação.
- **Aceite / teste:** instalar com entrada de fonte, limpar `set_extras`, `remove_game` → apaga a pasta e remove da
  biblioteca; backup usa a entrada guardada.

### F5 — Reparar falha justamente quando a pasta sumiu (Alta)
- **Onde:** `mod.rs:1153-1155` (`check_install_folder` exige marcador), `update.rs:232` (`preserve` exige que o alvo
  exista).
- **Reproduzir:** apagar a pasta do jogo (ou o antivírus pôr em quarentena / o jogador copiar sem o marcador). A página
  mostra "Localizar programa"; "Reparar instalação" responde "O Cerberus não reconheceu a pasta de instalação".
  A saída é remover e instalar de novo (perdendo vínculo de cópia etc.).
- **Correção:** se a pasta não existe, tratar como instalação nova no mesmo caminho (mantendo o registro e gen+1);
  se existe sem marcador mas a biblioteca aponta para ela, pedir confirmação antes de adotá-la.
- **Aceite / teste:** biblioteca com recomp cuja pasta foi apagada → `install` recria a pasta e mantém id, tempo de
  jogo e cópia escolhida.

### F6 — Mover entre discos apaga a pasta antiga antes de gravar a biblioteca (Alta)
- **Onde:** `relocate.rs:40-65`. A ordem é cópia → rename → `let _ = remove_dir_all(&from)` → `library.update`.
- **Reproduzir:** mover para outro disco com `library.json` bloqueado (ou o app fechado logo depois da cópia): a
  biblioteca aponta para a pasta antiga, já apagada (às vezes pela metade, porque o `remove_dir_all` é ignorado), e a
  pasta nova não está registrada. Mesmo no mesmo disco, falha em `library.update` deixa o registro errado sem volta.
- **Correção:** gravar a biblioteca logo depois do rename; só então apagar a antiga (em segundo plano, renomeando para
  `.cerberus-removing-*` para o `sweep_removed` terminar). Se a gravação falhar, desfazer o rename/apagar a cópia.
  Varrer `.moving-*` órfãos. Rebasear também `inputs` (não verificado se contêm caminhos internos).
- **Aceite / teste:** teste com `library.json.tmp` bloqueado (como em `update.rs:396`) → jogo continua jogável no
  lugar antigo e sem sobra no destino.

### F7 — Fechar o Cerberus (janela, bandeja ou autoatualização) mata downloads e instalações sem perguntar (Alta)
- **Onde:** `background.rs:44-50` (fecha se não há jogo nem bandeja), `background.rs:136` (Sair),
  `main.rs:1156-1165` (`update_install` só checa jogos, não `Installing::any_active`) → `self_update.rs:166 app.exit(0)`.
- **Reproduzir:** instalar um recomp grande e clicar "Atualizar agora" do Cerberus, ou fechar a janela: o processo
  sai no meio. O `.part` sobrevive, mas a UI esquece; se estava extraindo uma instalação nova, sobra pasta parcial
  sem marcador (F11).
- **Correção:** como o Heroic (`handleExit`): com `any_active()`, perguntar "Há downloads em andamento. Pausar e
  sair?" e pausar tudo antes de sair; `update_install` recusa ou pausa antes. Cobrir também emuladores/núcleos.
- **Aceite / teste:** teste do comando `update_install` com uma instalação reservada → erro claro; manual: fechar
  durante download mostra a pergunta.

### F8 — "Atualizações prontas" ignora jogos instalados com o id antigo (Média)
- **Onde:** `ui/src/pages.rs:182` e `ui/src/tiles.rs:112` comparam `e.id == recomp_id`; o backend usa `entry.is()`
  com aliases (`mod.rs:1023`). `RecompEntry` não leva os aliases.
- **Reproduzir:** jogo instalado como `2ship2harkinian` antes de o site renomear: a página do jogo mostra a versão
  nova, Downloads e o selo da capa não.
- **Correção:** levar `aliases` (ou o `recomp_id` instalado) em `RecompEntry` e usar a mesma regra do backend.
- **Aceite / teste:** teste de UI/`shared` com alias → aparece em Atualizações prontas.

### F9 — Atualização "fantasma" ou rebaixamento por comparar versão como texto (Média)
- **Onde:** `pages.rs:183`, `tiles.rs:113`, `tv/pages.rs:665` (`!=`); `mod.rs:1026-1033` (`latest` = `tag` do
  catálogo) × `mod.rs:1261-1275` (sem arquivo direto, versão instalada = release mais nova do servidor);
  `releases.rs:76-82` (rede falha → cache velho).
- **Reproduzir:** entrada sem `files` com `tag` atrasada no site: instala `v1.3` do GitHub, catálogo diz `v1.2` →
  "Atualização disponível v1.3 → v1.2" para sempre; clicar reinstala a mesma. Com arquivo direto e catálogo atrasado,
  "Atualizar" pode baixar uma versão **mais velha** que a instalada.
- **Correção:** comparar com semver tolerante (como `self_update::version`) e só oferecer se maior; se igual ou menor,
  não mostrar. Guardar no registro a origem da versão.
- **Aceite / teste:** tabela de casos (`v1.2` < `1.3.0`, tags não numéricas → só se diferente e catálogo mais novo).

### F10 — Checksum do catálogo velho: download completo descartado em loop com mensagem errada (Média)
- **Onde:** `install.rs:102-108`, `mod.rs:1258`.
- **Reproduzir:** o autor reenvia o arquivo com o mesmo nome; `files.sha256` do catálogo não bate. Cada tentativa
  baixa tudo, apaga e diz "não bate com o checksum do GitHub… Tente de novo".
- **Correção:** na divergência, consultar o `digest` da API para o mesmo asset; se o GitHub confirma o arquivo novo,
  dizer "O catálogo está desatualizado para este jogo" e oferecer atualizar o catálogo; nunca instalar sem um dos
  dois checksums batendo. Manter o arquivo em quarentena para não baixar de novo.
- **Aceite / teste:** servidor local com asset ≠ sha do catálogo → mensagem nova, um só download.

### F11 — Instalação nova interrompida deixa pasta parcial que não é notada (Média)
- **Onde:** `mod.rs:1210-1224` (marcador só depois do overlay; `register` depois); nenhuma varredura de
  `.staging-*` (o `remove_dir_all(staging)` só roda no próximo `unpack_over` do mesmo id).
- **Reproduzir:** fechar o app durante "Preparando arquivos…" na primeira instalação: `recomps\<id>` parcial sem
  marcador e sem registro; `.staging-<id>` com GB. Complementa "Notar uma instalação interrompida" (5.3).
- **Correção:** gravar um `intent` também na instalação nova (como `update.rs`), extrair numa pasta temporária ao lado
  e renomear de uma vez para o destino; varrer `.staging-*` e intents na abertura.
- **Aceite / teste:** simular parada entre overlay e register → na reabertura a pasta parcial some (ou é retomada) e
  o espaço volta.

### F12 — Arquivos que a versão nova removeu ficam para sempre (Média)
- **Onde:** `update.rs:229-254` (`preserve` copia tudo o que não está na release nova).
- **Reproduzir:** v1 traz `old_runtime.dll`, v2 não; depois do update a DLL continua e pode ser carregada no lugar da
  nova (o RecompLauncher documenta um caso real em `install.rs:760`). Complementa 6.7 "tirar do caminho DLLs antigas".
- **Correção:** com a lista de arquivos de F2, não levar para `next` os arquivos da release antiga que a nova não
  tem e que não mudaram desde a instalação (eles ficam só em `previous`, que já é apagado).
- **Aceite / teste:** v1{a,b} → v2{a}: `b` some; `b` editado pelo jogador fica.

### F13 — Sem fila de verdade e estado de download só na memória da UI (Média)
- **Onde:** `ui/src/discover.rs:14-63` (`library.progress`), `mod.rs:138` (`Installing.1` em memória),
  `pages.rs:84` ("Continuar tudo" dispara todos em paralelo).
- **Reproduzir:** pausar dois downloads, reiniciar o PC: Downloads diz "Nada baixando agora"; os `.part` só voltam se o
  jogador lembrar de clicar Instalar; "Não terminou" também some. Três instalações ao mesmo tempo dividem banda e disco.
  O ROADMAP 5.3 marca "Fila única de downloads" como feita.
- **Correção:** fila no backend persistida (`downloads.json`: id, estado, bytes, erro, ordem), um ativo por vez
  (configurável), reordenar, retomar pausados na abertura, como Heroic/Hydra. Evento único de estado para a UI.
- **Aceite / teste:** teste do módulo de fila: enfileirar 3, pausar 1, reabrir → mesma ordem e estados.

### F14 — Disco cheio e rede caída: sem pausa automática nem mensagem clara (Média)
- **Onde:** `install.rs:233` (`write_all` → `Stop::Fatal(e.to_string())`, texto cru do SO), `install.rs:94-97`
  (4 tentativas em ~20 s e desiste).
- **Reproduzir:** encher o disco durante o download → "Espaço insuficiente no disco. (os error 112)"; tirar o cabo
  por 1 minuto → falha e vai para "Não terminou".
- **Correção:** mapear `StorageFull`/112 para "O disco X encheu; o download foi pausado e continua quando houver
  espaço" e virar `Paused`; queda de rede → pausa e retoma sozinho (Heroic `onConnectivityChange`).
- **Aceite / teste:** teste com `Halt` + servidor local que corta a conexão 5 vezes → termina; erro 112 simulado →
  estado `Paused`.

### F15 — Autoatualização instala sem checksum quando o GitHub não dá `digest`, e sem volta (Média)
- **Onde:** `self_update.rs:120-121` (`size == 0` ou `sha256 == None` aceitam), sem checagem de assinatura.
  ROADMAP 5.2 marca como feitos "version.json", "versão mínima", "arquivo ruim não oferecido de novo" e "conferência
  da versão exata dentro do instalador": **não existem no código** (busca por `version.json`/`min` sem resultado).
- **Correção:** recusar sem `digest`; conferir a versão do `.exe` baixado (recurso de versão) e, quando houver, a
  assinatura Authenticode; gravar "versão ruim" se o app não confirmar o primeiro início (ideia do RecompLauncher
  `update_rollback_tests.rs`). Corrigir o ROADMAP.
- **Aceite / teste:** `fetch_checked` com `sha256: None` → erro; teste do registro de versão ruim.

### F16 — Fontes do jogador no formato RecompLauncher podem baixar de qualquer servidor (Média, segurança)
- **Onde:** `sources.rs:132-135` aceita `apps` inteiro via `from_recomplauncher`, que aceita `host: forgejo` com
  qualquer domínio (`forge.rs:31`). O cabeçalho do módulo promete "um projeto no GitHub".
- **Reproduzir (não verificado ao vivo):** fonte com `{"apps":[{"id":"x","repo":"qualquer.site/a/b","host":"forgejo",…}]}`
  → instalação baixa de `https://qualquer.site/...`.
- **Correção:** para fontes do jogador, só GitHub (ou lista de servidores conhecidos), mesmo no formato RecompLauncher;
  mostrar "Fonte: <nome>" e o servidor na página antes de instalar.
- **Aceite / teste:** teste de `parse` com host forgejo numa fonte → entrada descartada.

### F17 — Download vai sempre para o disco do sistema (Baixa)
- **Onde:** `mod.rs:93` (`downloads = data_dir/downloads`), `mod.rs:1171`.
- **Reproduzir:** C: quase cheio, instalações em D:: "Espaço insuficiente em C:\Users\…\downloads". Ocupa os dois
  discos ao mesmo tempo.
- **Correção:** baixar em `<pasta de instalação>\.cerberus-downloads` (mesmo volume do destino, rename instantâneo).
- **Aceite / teste:** `Folders` com installs em outro volume → `.part` no volume do destino.

### F18 — Mensagens falam "GitHub" para GitLab/Forgejo e citam a pasta interna (Baixa)
- **Onde:** `install.rs:107,198,213`; `throttle.rs:95` cita `…\.cerberus-update-<hash>\previous`.
- **Correção:** usar `forge.name()`; citar a pasta do jogo e o processo provável.

### F19 — Aviso de atualização interrompida pode sumir (Baixa)
- **Onde:** `library.rs:31,41` — `OnceLock::set` duas vezes; o segundo aviso (recuperação pendente) é descartado se a
  biblioteca também foi recuperada.
- **Correção:** acumular avisos num `Vec`.

### F20 — Desinstalar repetido após falha diz "nada foi apagado" e continua falhando (Baixa)
- **Onde:** `mod.rs:1391` (`let _ = remove_dir_all(&aside)`): se `.cerberus-removing-<id>` sobrou preso, o rename
  seguinte falha até reiniciar. `sweep_removed` (`mod.rs:1444`) só olha a pasta atual e a padrão: sobras em pastas de
  instalação antigas nunca são limpas.
- **Correção:** nome com sufixo único (`-<timestamp>`); varrer também os pais das `install_dir` da biblioteca.

### F21 — Catálogo novo com o mesmo número de jogos não atualiza a tela (Baixa)
- **Onde:** `main.rs:2158-2161` só emite `catalog-changed` se `len()` mudou.
- **Reproduzir:** o site publica versões novas sem jogos novos → "Atualizações prontas" só aparece depois de navegar
  ou reiniciar.
- **Correção:** comparar um hash do texto ou emitir sempre que `refresh` substituir a lista.

### F22 — Progresso não anda quando o servidor não informa o tamanho (Baixa)
- **Onde:** `mod.rs:1161-1167` (percent sempre 0 com `total == 0` → um evento só).
- **Correção:** emitir também a cada N MB ou 500 ms.

### F23 — Lacunas de teste (Baixa)
- 7z com `../` (a biblioteca recusa: conferido em `sevenz-rust2-0.25.0/src/util/decompress.rs:153 safe_join`, mas
  não há teste no Cerberus); tar.gz com link simbólico para fora (não verificado); `relocate` com falha de gravação
  (F6); `remove_game` com recomp fora do catálogo (F4); `merge` com repo divergente (F3); fila/pausa após reinício.

---

## 4. Lacunas em relação aos outros launchers

- Fila persistente, um download por vez, reordenar, "desligar/suspender ao terminar" [Heroic, Hydra] — F13, ROADMAP 7.2.
- Pausar sozinho sem internet e retomar [Heroic]; parar a fila com disco cheio e seguir para o próximo [Hydra] — F14.
- Perguntar antes de sair com operações pendentes [Heroic] — F7.
- Lista de arquivos por release para tirar os que a versão nova removeu [RecompLauncher `dropped`] — F12, F2.
- Volta automática da autoatualização que falhou no primeiro início [RecompLauncher] — F15.
- Histórico de downloads concluídos/falhos persistido [Heroic `finished`] — hoje só a trilha mostra "Instalação".

## 5. Ordem sugerida

1. **F1, F2, F3** (dados e segurança) — F2 e F12 compartilham a lista de arquivos por release; fazer juntos.
2. **F4, F5, F6, F7** (quebram o uso ou deixam o jogo inacessível).
3. **F11 + F13 + F14** (instalação nova transacional, fila persistida, pausa automática) — um bloco só.
4. **F8, F9, F10, F15, F16**.
5. Baixas (F17–F23) quando mexer nos arquivos.

Contagem: Crítica 3 · Alta 4 · Média 9 · Baixa 7.
