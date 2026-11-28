# Roadmap

Comparado ao RecompLauncher (referência de comportamento), ao Hydra, ao Playnite e ao Heroic. Cada onda só
fecha com teste real, não só compilação.

## Mapa rápido (atualizado em 25/11/2026)

Onde estamos e o que vem a seguir, para não se perder. O detalhe de cada item está nas ondas abaixo; a Onda 7 é o
levantamento de 25/11 (um agente por launcher: Heroic, Hydra, Playnite e RecompLauncher).

**Feito agora (branch `ajustes-rapidos`, 23 a 25/11, sem PR ainda):**

- Lojas: jogos do app Xbox e do PC Game Pass (com arte oficial e instalação pelo app Xbox), login da Epic (inclusive
  com Xbox/Google/Apple) e da conta Microsoft, biblioteca inteira das duas contas com os não instalados, filtro por
  plataforma e não instalados em Todos; Game Pass não duplica jogo comprado em outra loja.
- Conquistas: banco local `achievements.db` (SQLite) que nunca perde um desbloqueio, sincronização econômica com espera
  gravada depois de recusa; Xbox completo (249 jogos na conta real) e Epic; cartão por plataforma, Platinados e Perto
  dos 100% com Ver todos, Próximas conquistas e Suas mais raras; aba Conquistas nos jogos Epic e Xbox.
- Tempo de jogo das contas Xbox (horas e última vez) e Epic na Biblioteca, sincronizado em segundo plano.
- Página do jogo com dados da loja também para Epic e Xbox (Steam quando vende lá, senão Microsoft Store).
- Capas com selo de origem (Steam, Epic, Xbox/Game Pass, Recomp, emulador e console) e fotos reais dos consoles
  (Hydra + domínio público do Evan Amos).
- Backup dos saves dos emuladores, Xenia e xemu para os clássicos de Xbox, varredura de bugs do backend.

**Próximos, nesta ordem:**

0. **Corrigir as falhas da auditoria completa** (Onda 9, 178 falhas, 17 críticas), lote A primeiro: nenhum arquivo do
   jogador apagado por uma leitura que falhou. Junto, os itens 8.0 que restam.
   **Deixar completo o que já foi entregue** (Onda 8): primeiro o que perde dados ou mexe em contas — saves dos
   emuladores apagados ao remover/reinstalar, tokens indo no backup, conta que volta depois de Sair, uma falha
   derrubando a sincronização inteira. Revisões: [lojas e contas](docs/roadmap/lojas-e-contas.md),
   [conquistas e tempo](docs/roadmap/conquistas-e-tempo.md), [emulação](docs/roadmap/emulacao-e-saves-dos-emuladores.md).
1. **Saves de qualquer jogo de PC** (Steam, Epic, Xbox, GOG, adicionados) pelos locais conhecidos, reaproveitando o
   backup e a nuvem dos recomps — apontado por Hydra, Heroic e Playnite. (7.1, [plano](docs/roadmap/saves-de-jogos-de-pc.md))
2. **GOG**: jogos instalados pelo GOG Galaxy, depois login e biblioteca, conquistas e tempo de jogo como Epic e Xbox.
   Depois Amazon, Ubisoft, EA e Battle.net. (7.2, [plano](docs/roadmap/gog-e-outras-lojas.md))
3. **Steam sem chave** (token da sessão da loja) e **biblioteca da família**. (6.1, [plano](docs/roadmap/steam-sem-chave-e-gerenciar-lojas.md))
4. **Gerenciar jogos das lojas sem sair do Cerberus**: desinstalar, verificar e mover pela própria loja; argumentos de
   inicialização da Steam. (7.2, mesmo [plano](docs/roadmap/steam-sem-chave-e-gerenciar-lojas.md))
5. **Ações antes e depois de jogar** e variáveis de ambiente por jogo. (6.4, 7.4)
6. **Metadados guardados na biblioteca** (gênero, desenvolvedora, ano, série...) que destravam filtros salvos,
   agrupamento e edição de vários jogos. (7.3, 6.3, [plano](docs/roadmap/metadados-filtros-e-estatisticas.md))
7. **Estatísticas e "seu ano"** só com dados locais. (6.3, 7.3, mesmo plano)
8. **Troféus do RPCS3 e do shadPS4** e **foto no momento da conquista**. (5.4, 6.1, [plano](docs/roadmap/trofeus-e-lembrancas.md))
9. **Recomps**: escolher, fixar e voltar uma versão; campos do SDK ainda ignorados; RVZ e cia.; janela "Resolver
   problemas" com o log do jogo. (7.5, [plano](docs/roadmap/recomps-robustos.md))
10. Pequenos de alto retorno: aviso de sem internet, central de notificações, som e volume do aviso de conquista,
    HowLongToBeat e anti-cheat na página do jogo. (7.4, 7.6)

## Referência Hydra e próximas entregas

Comparação com o checkout local do Hydra em `6b7ad7bf2`.

| Área | Comportamento observado no Hydra | Entrega no Cerberus |
| --- | --- | --- |
| Fontes | O cliente cadastra a URL via HydraApi e persiste a fonte. As publicações têm título, tamanho e data. [Cadastro](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/main/events/download-sources/add-download-source.ts) e [tipos](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/types/index.ts) | Implementado. Configurações importa os metadados por HTTPS, guarda a última cópia válida e permite pesquisar por fonte. Links HTTP(S) diretos entram no Descobrir e instalam quando o jogador pedir; magnet e torrent ficam só para consulta. |
| Sessões | O observador compara caminhos de executáveis e grava incrementos de tempo durante a execução. [Observador](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/main/services/process-watcher.ts) | O Cerberus grava o tempo a cada 15 segundos e ao sair normalmente. No Windows, Gerenciar permite escolher o executável acompanhado. O teste usa processos reais em pastas distintas e inclui um segundo executável de mesmo nome. Falta comparar a próxima partida real do ReStreet com o relógio. |
| Disco e downloads | O Hydra compara os bytes restantes com o espaço disponível no volume de destino e considera dados reaproveitáveis. [Verificação de espaço](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/main/services/download/disk-space.ts) | Configurações mostra o espaço dos dois destinos. O download verifica os bytes restantes quando o tamanho é conhecido. ZIP, 7z e executáveis verificam espaço antes da extração ou cópia. O teste retomou um ZIP em C: e instalou em D:. A previsão do espaço descompactado para tar.gz segue pendente. |
| Novidades por jogo | O Hydra cruza a identidade do jogo com mudanças das fontes e mantém a referência anterior se uma consulta falhar. [Verificador de fontes](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/main/services/download-sources-checker.ts) | Pendente. Vincular publicação e jogo com confirmação quando o título for ambíguo. Só então mostrar novidades na página do jogo. |

## Prioridade atual — confiabilidade dos recomps

A próxima entrega é conseguir atualizar um recomp, interromper o processo e voltar a jogar
sem perder o registro, a versão anterior ou os saves. O trabalho de catálogo da onda 4
continua útil. A expansão no estilo Hydra (arte, lojas e descoberta) vem depois deste ciclo.

A comparação abaixo usa o Cerberus em `ba59805` e o checkout local do RecompLauncher em
`3f26f5c`. Os links apontam para o comportamento observado naquela revisão. As mudanças no
Cerberus são implementadas em Rust sobre os nossos formatos e comandos Tauri.

| Ordem | O que existe no RecompLauncher | Lacuna observada no Cerberus e entrega verificável |
| --- | --- | --- |
| 1. Biblioteca | A leitura recupera registros válidos individualmente e guarda o arquivo com problemas. [library.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/library.rs) | Em `src-tauri/src/library.rs`, a gravação já usa arquivo temporário e renomeação. Falhas ainda exigem dois cuidados distintos, manter memória e disco coerentes e recuperar dados ilegíveis na abertura. Ver a sequência abaixo. |
| 2. Atualizações | A instalação prepara arquivos em staging. Na troca de um bundle de macOS, guarda a versão anterior até confirmar a substituição. [install.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/install.rs) | `recomp/install.rs` sobrepõe arquivos no destino e `install_now` procura o executável depois. Primeiro validar a release no staging. Depois tornar a troca e o registro recuperáveis. Interromper uma atualização e verificar executável, versão, configuração e saves depois de reabrir. |
| 3. Instalação externa | Há operações distintas para executar o instalador, parar a espera, conferir uma pasta e registrar o programa encontrado. [installer.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/installer.rs) | `setup_runner.rs` descarta o resultado de `child.wait()` e tenta localizar o jogo mesmo depois de um cancelamento. Modelar os estados de espera, falha, localização e conclusão. Testar cancelamento com uma instalação antiga presente, fechamento do launcher no meio e seleção manual do executável. |
| 4. Preparação por jogo | O perfil descreve como iniciar o jogo e aceita entradas nomeadas, inclusive discos, DLC e arquivos opcionais. [boot.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/boot.rs) e [inputs.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/inputs.rs) | `Game.game_copy` guarda um único caminho. Evoluir `CopyHandoff` e `launch_plan` para que a mesma validação alimente a tela e o processo de lançamento. Abrir uma biblioteca antiga e testar um jogo de um disco, outro de dois discos e a ausência de um arquivo opcional. |
| 5. Instalações existentes | A adoção associa uma pasta existente ao projeto do catálogo e distingue correspondências confirmadas de sugestões. [adopt.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/adopt.rs) | Hoje adicionar um executável cria um jogo local. Permitir vincular a pasta a um recomp e registrar quem gerencia seus arquivos. No teste, importar a mesma pasta duas vezes deve produzir um único registro, e remover o vínculo deve preservar a pasta externa. |
| 6. Diagnóstico | Um histórico local com tamanho limitado registra eventos e permite consultar as últimas operações de um jogo. [trail.rs](https://github.com/elripalda/RecompLauncher/blob/3f26f5c/src-tauri/src/trail.rs) | `sessions::finish` descarta erros ao gravar tempo de jogo e ao copiar saves. Registrar etapa, jogo e resultado localmente e permitir consultar a última falha. Forçar um destino de backup sem escrita e verificar que a sessão termina e a causa continua disponível depois de reiniciar. |

### Como distribuir as responsabilidades

- O gerador em `tools/catalog-builder` cuida de releases e curadoria. O módulo
  `recomp/catalog.rs` deve converter tanto o catálogo próprio quanto o externo para o mesmo
  perfil de preparação. Uma entrada antiga de um único disco é o primeiro caso de compatibilidade.
- O backend em `src-tauri` deve decidir se o próximo passo é obter a cópia, abrir o instalador,
  localizar o executável ou jogar. Expor esse resultado em `shared` permite que Início,
  Biblioteca, Tela Grande e a página do jogo mostrem a mesma ação. Hoje `ui/src/main.rs` escolhe
  o botão com `setup_pending`, enquanto `setup_runner.rs` e `recomp/mod.rs` decidem o que executar.
- O workflow `Testes do launcher` roda os testes Rust e compila a interface WASM no Windows em
  pull requests. O catálogo tem seu próprio workflow. Um cenário de referência deve
  instalar uma release de teste, simular falha na atualização, reiniciar e ler os dados da versão anterior.
  O teste manual com mídia própria cobre a interação final com o jogo.

### Primeira entrega e próximos testes

- [x] Publicar alterações de `LocalLibrary` em memória somente depois da gravação em disco.
      A regressão em `library.rs` força uma falha real no arquivo temporário ao adicionar,
      renomear e remover jogos. Depois libera o destino, repete as operações e reabre a biblioteca.
- [x] Recuperar registros válidos na abertura, inclusive os completos antes de um fim de
      arquivo truncado. A tela Biblioteca mostra o caminho da cópia original numerada. Um arquivo
      ilegível ou sem jogos recuperáveis permanece intacto e a leitura pode ser repetida. Testes
      cobrem primeira execução, bloqueio de leitura no Windows, registros incompatíveis, truncamento,
      falha ao criar a cópia, falha ao gravar a recuperação e reabertura.
- [x] Localizar o executável ou instalador entre os arquivos da nova release em staging e só
      então sobrepor a instalação. Testes com dois pacotes locais confirmam que uma release sem
      programa preserva o executável anterior, os dados e os saves. O teste de instalador confirma que o
      caminho registrado aponta para o programa do novo pacote.
- [x] Recuperar atualizações interrompidas de recomps já instalados. O launcher prepara o novo
      pacote ao lado da instalação e guarda a pasta anterior até gravar a biblioteca. Ao reabrir,
      o registro em disco determina qual pasta usar, inclusive ao reinstalar a mesma versão.
      Testes com releases locais simulam interrupções na preparação, entre as renomeações e antes
      e depois do registro. Também forçam uma falha de gravação e conferem executável, configuração
      e save após a recuperação e a nova tentativa.
- [x] Abrir FIFA Street pelo ReStreet. O jogador confirmou o funcionamento no Windows.
      A biblioteca gravou o caminho de `FifaStreetLauncher.exe`, a conclusão da instalação
      e 3 segundos de tempo de jogo.
- [x] Corrigir o encerramento prematuro da sessão no Windows. No ReStreet, o Cerberus
      acompanha `fifastreet.exe` aberto por `FifaStreetLauncher.exe` e grava cada partida ao
      fechar o jogo, inclusive quando o menu fica aberto. Testes com processos reais no Windows
      cobrem a saída antecipada do launcher e duas partidas no mesmo menu.
- [x] Distinguir o cancelamento ou falha de um instalador externo antes de procurar o jogo instalado.
      O teste cobre os códigos de saída, inclusive cancelamento do MSI e sucesso que pede reinício.
- [x] Instalador externo depois de reiniciar: ao abrir, e antes de rodar o instalador de novo, o Cerberus procura
      o jogo já instalado (ao lado do instalador ou no registro do Windows) e o deixa pronto. Testado com
      uma pasta montada; ainda sem um instalador real interrompido.
- [ ] Comparar a próxima partida real do ReStreet com a duração registrada após a correção.
- [x] Restaurar o mais antigo dos dez backups mesmo quando a cópia de segurança provoca rotação.
      Se a leitura de um slot falha por corrupção do ZIP, os slots atuais são preservados. Ambos os casos têm testes em pastas temporárias.
- [x] Guardar um histórico local por jogo (`trail.jsonl`, até 600 registros): partidas, backups,
      saves da nuvem, instalação, instalador externo e restauração. A aba Gerenciar mostra a
      última falha com a causa e a lista completa. Testes cobrem reabertura, linha danificada,
      limite de tamanho, destino sem escrita e uma sessão cuja gravação de tempo falha. Conferido
      no app com um backup manual real do ReStreet.
- [x] Decidir no backend o próximo passo de cada jogo (`NextStep`): jogar, abrir o instalador,
      escolher a cópia do jogo (com um clique quando as pastas de ROMs já têm) ou localizar o
      programa que sumiu. Início, Biblioteca, Tela Grande e a página do jogo mostram o mesmo botão,
      e Jogar recusa o que o botão não ofereceria. Testes cobrem cópia por argumento, por pasta,
      arquivo que sumiu, instalador pendente e programa ausente.
- [x] Aba **Ajustes** na página do jogo: as opções do próprio jogo (tela cheia, resolução,
      FPS...) descritas no catálogo, gravadas direto no `.ini`, `.toml` ou `.json` dele mexendo só
      na linha da opção. 261 jogos do catálogo têm ajustes. Conferido no SpaghettiKart: 60 fps
      gravado sem mudar nenhuma das outras 189 chaves, e mantido pelo jogo depois de abrir e fechar.
- [x] Configuração guiada na primeira abertura: como joga (mesa ou TV), pastas de ROMs com os
      jogos que já têm recomp, pasta de instalação com espaço livre e saves na nuvem. Quem escolhe
      TV abre direto na Big Picture Mode (também em Configurações). Instalações existentes não veem a tela.
- [x] Trazer recomps instalados sem o Cerberus: "Trazer seus recomps" no Descobrir procura numa
      pasta pelo nome do programa que o catálogo dá (607 recomps o informam) e "Já tenho instalado"
      na página de cada recomp. O jogo fica marcado como externo: o Cerberus abre, conta tempo e faz
      backup, mas nunca atualiza nem apaga a pasta. Trazer duas vezes mantém um registro, e um jogo
      adicionado à mão vira o recomp mantendo o tempo de jogo. Conferido no app com o Zelda 64.
- [x] Busca do Descobrir procura em todas as versões de um jogo ("Zelda 64" acha o Zelda 64: Recompiled).
- [x] Personalizar arte de qualquer jogo (inclusive Steam e Epic): capa, fundo e logo do SteamGridDB,
      com a chave gratuita do próprio jogador, ou de um arquivo, enquadrados com arrastar e zoom numa
      moldura do formato certo. O logo aparece no lugar do nome na página do jogo. "Restaurar original"
      apaga só as imagens feitas pelo Cerberus. Conferido no app com o Alan Wake (Steam); a busca no
      SteamGridDB ainda precisa de uma chave real para ser vista funcionando.
- [ ] Vários discos, DLC e arquivos opcionais por jogo (item 4 da tabela): adiado. Em 18/11/2026
      nenhum dos 1899 jogos do catálogo usa mais de uma entrada (`boot.inputs`); retomar quando algum usar.
- [x] Aviso do catálogo para o jogador ("Como começar", 455 jogos) e selo de multijogador
      (local, online ou os dois, 156 jogos) na página do jogo.
- [x] Filtros visuais (Nítido, Cores vivas, CRT, Filme) com intensidade, na aba Ajustes, para jogos
      com Direct3D 10/11/12. Shaders próprios aplicados pelo ReShade, baixado do site oficial na primeira
      escolha. O `dxgi.dll` fica ao lado do jogo só durante a partida, nunca substitui um que já exista e
      é retirado no próximo início se o Cerberus fechar no meio. Conferido no SpaghettiKart (CRT e Filme),
      com a pasta do jogo idêntica ao backup depois. Vulkan e OpenGL ficam para depois.
- [x] Primeira curadoria dos candidatos do catálogo próprio: 62 promovidos (de 10 para 72 jogos).
      Critério: o repositório já está no catálogo do recomplauncher.com como jogo jogável, instala por
      arquivo (`portable`), é de console antigo e tem arquivo para Windows com SHA-256. Só fatos (jogo,
      console, programa) vieram de lá; capas e textos não. Os 52 restantes ficaram em `candidates.json`
      com o motivo em `review_note`.
- [x] Os 15 projetos de consoles recentes (Switch, Switch 2, Wii U, PS3, PS4) também entraram, por
      decisão do mantenedor: são os mais procurados agora e já estão no catálogo do site. O SHA-256
      garante o arquivo que o autor publicou, não a confiança no autor. Catálogo próprio: 87 jogos.
- [x] Mais 28 jogos (catálogo próprio: 115): os 22 que o site lista sem dizer como iniciar (o Cerberus
      acha o programa sozinho), Skate 3, Zelda TOTK "Weeping of the Empire", e Yu-Gi-Oh! FM, Kingdom
      Hearts 358/2 Days, Rayman 3 HD e Forza Horizon 1 conferidos à mão. Ficaram 9 em revisão: 7
      ferramentas (N64Recomp, bibliotecas, lojas, outros launchers) e 2 que não são para Windows.
- [x] O gerador escolhia arquivos auxiliares no lugar do jogo (editor, "Studio", tradução). Agora os
      evita e prefere a versão portátil, com testes, sem trocar jogos que são eles mesmos um mod.
- [x] Big Picture Mode refeita: abas (Início, Biblioteca, Descobrir, Downloads, Configurações) com LB/RB;
      Início com destaque do jogo em foco e fileiras; grade na Biblioteca e no Descobrir; página do jogo
      com botão principal que segue o estado (jogar, instalar, atualizar, progresso que cancela); painel de
      opções no X; teclado na tela para busca; Downloads com progresso e atualizações; B volta ao item de
      onde veio e, no Início, pergunta se quer sair; ícones de botão de Xbox, PlayStation ou Nintendo
      conforme o controle; sons de navegação; relógio. Segurar Menu ou F11 entra e sai. Conferido no app
      pelo teclado; o controle físico (Xbox 360 conectado) ainda precisa ser testado à mão.
- [x] Página do jogo mais completa: logo do catálogo no lugar do nome, jogadores recentes, versões
      lançadas, primeira versão, cartão do jogo original (resumo, desenvolvedora, publicadora, série,
      gêneros, modos, plataformas, nota, pelo IGDB) e "Jogos parecidos" (tags, gênero, série, console).
- [x] Descobrir lista por "Recomps" ou por "Jogos" (nome do jogo original), escolha lembrada.
- [x] MHGU Recomp e Budokai Tenkaichi 4 Tag Team no catálogo próprio (117 jogos).
- [x] Configurações refeitas com menu lateral em grupos (App, Jogos, Clássicos, Contas, Sistema), chaves
      de liga/desliga e opções novas: página inicial, rever a configuração inicial, menos animações, sons
      e "Abrir agora" do Big Picture Mode.
- [x] Biblioteca em abas Todos / Recomps / PC / Clássicos. Clássicos mostra as ROMs das pastas, com capa
      do libretro e o selo "Tem recomp" (abre o recomp).
- [x] RetroAchievements: conectar com usuário e chave da Web API (fica só no PC), perfil, pontos e os jogos
      jogados recentemente com o progresso de conquistas. Próximo: emulação (RetroArch primeiro) com login
      automático nos emuladores e conquistas na hora, como o Hydra faz.
- [x] Emulação pelo RetroArch: o Cerberus instala o RetroArch oficial do libretro numa pasta própria (ou usa
      o do jogador) e baixa um núcleo por console (NES, SNES, N64, Game Boy/Color, GBA, Mega Drive/Master
      System/Game Gear). Clássicos dessas consoles abrem direto da aba Clássicos. Um clássico que também tem
      recomp abre a página do recomp, com "Jogar" e "Original no RetroArch" lado a lado. Conferido no app:
      RetroArch 1.22.2 instalado, núcleo de N64 baixado e Mario Kart 64 aberto pela página do jogo.
- [x] RetroAchievements no RetroArch: a senha é trocada por um código de acesso no site e só o código vai para
      o retroarch.cfg (com cópia de segurança). Ainda não testado com uma conta real.
- [x] PlayStation, PSP e GameCube/Wii pelo RetroArch (núcleos pcsx_rearmed, ppsspp e dolphin, com os arquivos de
      sistema do PSP e do Dolphin). Crash Bandicoot original (PS1) aberto pela página do recomp.
- [x] Tempo de jogo dos clássicos (`classics.json`), mostrado na aba Clássicos e no histórico.
- [x] Conquistas na hora: o RetroArch escreve um log (`--log-file`) e o Cerberus mostra um aviso a cada
      "Awarding achievement". Texto conferido dentro do programa do RetroArch; ainda sem conquista real.
- [x] PS2 (pcsx2 com a BIOS do jogador), BIOS opcional do PS1 e fileira "Seus clássicos" no Big Picture Mode
      (conferida em tela cheia, 1920x1080).
- [ ] Validar a continuidade com um save real. Salvar progresso, fechar o jogo, conferir
      o backup, reabrir o Cerberus e carregar o mesmo progresso. Testar a restauração
      em uma cópia separada.
- [x] Repetir o envio dos backups locais pendentes ao abrir o Cerberus e a cada minuto.
      Testes com duas instalações e uma pasta compartilhada cobrem reinício, troca de destino,
      interrupção da cópia, ZIP danificado e retomada com progresso mais novo no outro PC.
      Configurações mostra quantos envios estavam pendentes na última verificação.

## Feito

- Biblioteca: Steam, Epic, jogos adicionados à mão (nome e capa automáticos, editáveis).
- Descobrir: instalar e atualizar recomps do GitHub, com checksum, desembrulhar a pasta do topo,
  atualização por cima (mantém saves) e cache de releases.

## Onda 1: o essencial para confiar no launcher (feita)

- [x] Backup de saves: depois de cada sessão, antes de atualizar e antes de desinstalar; os 10 mais
      novos por jogo; restaurar com um clique (fazendo backup do estado atual antes).
- [x] Sessões: saber quando o jogo está aberto, tempo de jogo e última vez jogado.
- [x] Página do jogo: arte de fundo, capa, jogar/instalar/atualizar, abas (visão geral, saves,
      gerenciar).
- [x] Downloads robustos: tentar de novo quando a conexão cai, continuar de onde parou, cancelar.
- [x] Interface no nível dos launchers de referência: barra lateral, Início com banner e fileiras,
      Biblioteca com busca/filtro/ordenação, Downloads, Saves, janela sem moldura.

## Onda 2: catálogo e biblioteca maduros (feita)

- [x] Versões: recomps do mesmo jogo aparecem uma vez, com menu "N versões" na página do jogo.
- [x] Mais formatos: `.7z` e `.tar.gz` (com proteção contra caminhos que saem da pasta).
- [x] Arte guardada em disco, aceitando só imagens de verdade (PNG/JPEG/WebP).
- [x] Arte para o Unleashed Recompiled (logo oficial do projeto).
- [x] Selo "Atualização" na biblioteca.
- [x] Configurações: pasta dos recomps (atualizações continuam na pasta antiga), dados, cache de imagens.
- [x] Desinstalar só apaga pastas marcadas como instaladas pelo Cerberus.
- [x] Catálogo atualizável sem nova versão do launcher (o app busca a lista deste
      repositório).

## Catálogo grande e suas ROMs (feito)

- [x] Catálogo do recomplauncher.com: ~1900 recomps, 32 consoles, versões agrupadas com a
      recomendada, arte em alta resolução. Cópia em disco, atualizada a cada 6 h; lista própria
      como reserva.
- [x] Download direto da release quando o catálogo traz arquivo + SHA-256 (sem API do GitHub).
- [x] Descobrir com busca, filtro por console, "Tenho o jogo", ordenação e paginação.
- [x] Pastas de ROMs: acha os jogos (N64 pelo cabeçalho, o resto pelo nome) e mostra quais têm recomp.
- [x] Fontes de catálogo em JSON (como o Hydra), só com jogos de repositórios no GitHub,
      baixados das releases oficiais. Listas Hydra podem ser consultadas em Configurações; links HTTP(S) diretos também entram no Descobrir e instalam sob demanda.
- [x] Recomps com instalador próprio: `single-exe` instala direto; `installer` roda no primeiro
      "Jogar" e depois o Cerberus acha o jogo (pasta própria, depois "Aplicativos" do Windows),
      ou pergunta onde ficou.
- [x] Textos do catálogo em português onde dá para traduzir com exatidão: as 80 tags e a linha
      "Sua cópia de...". Descrições e passos ficam como o desenvolvedor escreveu.
- [ ] ~~Traduzir descrições e passos~~: decidido manter em inglês (exigiria um serviço de tradução).

## Onda 3: experiência (feita)

- [x] Modo Big Picture Mode (Big Picture) com controle e teclado.
- [x] Mods do Thunderstore: comunidades conhecidas e as `*-recompiled` achadas pelo nome;
      instala dependências; só os arquivos que o jogo carrega.
- [x] Opções de inicialização por jogo.
- [x] Saves na nuvem por uma pasta sincronizada (OneDrive, Google Drive, Dropbox).
- [x] Easy Setup: a cópia achada nas pastas de ROMs é entregue ao recomp como o catálogo manda
      (`--rom`, variável de ambiente ou cópia na pasta), automaticamente ao instalar.
- [x] Easy Setup com pasta de arquivos extraídos ou imagem de disco de Xbox/Xbox 360 (XDVDFS),
      e checagem da versão da ROM de N64 (código, região e revisão) com mensagem clara.
- [x] Imagens ISO 9660 (PS2, PSP): cobre todos os formatos de extração que o catálogo usa hoje.
- [x] Catálogo próprio atualizável sem nova versão: o app busca `src-tauri/catalog/recomps.json`
      deste repositório a cada 6 h (a versão embutida fica de reserva).

## Onda 4: catálogo próprio, sem depender de servidor de ninguém

Hoje o catálogo grande vem do site do RecompLauncher, e os dois launchers de referência dependem de
um servidor fechado para montar o deles. A ideia é juntar o melhor dos dois sem servidor:
automação como o Hydra (bases públicas, arte da comunidade) e curadoria como o RecompLauncher
(entradas revisadas, só releases dos desenvolvedores, com checksum). O "servidor" é este
repositório: o app já busca `src-tauri/catalog/recomps.json` daqui a cada 6 h.

### 4.1 Gerador do catálogo (`tools/catalog-builder`)

- [x] Ferramenta em Rust no próprio repositório: `cargo run -p catalog-builder`.
- [x] Atualiza o que é automático em cada entrada a partir do GitHub do projeto: versão (tag),
      arquivo de cada sistema com SHA-256 (download direto), padrão do nome do arquivo, tamanho,
      estrelas, sistemas.
- [x] Descobre projetos novos no GitHub (busca por nomes `*Recomp*`/`*Recompiled*`, tópicos de
      recompilação/decompilação, ports nativos) e grava em `catalog/candidates.json` para revisão,
      sem colocar nada no catálogo sozinho.
- [x] Usa `GITHUB_TOKEN` quando houver (limite de 5000 consultas/h em vez de 60).

### 4.2 Revisão sem servidor (GitHub Actions)

- [x] Action diária (e manual) que roda o gerador e abre um Pull Request com as atualizações e os
      candidatos. Aprovar o PR = publicar no app.
- [x] Formulário de issue "Pedir um recomp" no repositório, e botão no Descobrir que abre esse
      formulário (o "Request a Recomp" do RecompLauncher, sem servidor).
- [x] Guia de curadoria no README: como completar uma entrada (jogo, console, como entregar a ROM,
      locais de save, passos de configuração).

### 4.3 Base de jogos e arte (estilo Hydra)

- [x] Arte da comunidade pelo SteamGridDB com a chave gratuita do jogador (Configurações › Arte dos jogos e
      "Personalizar arte" na página do jogo). Falta: o gerador do catálogo usar uma chave de CI.
- [ ] Base de jogos de console do LaunchBox Games Database (`Metadata.zip`): nomes alternativos,
      plataformas e imagens, para reconhecer melhor as ROMs. Confirmar os termos de uso antes.
- [ ] Busca com índice local (SQLite com busca de texto) se a base passar de algumas dezenas de
      milhares de jogos; para os recomps a busca em memória continua bastando.

### 4.3b Bandeja do Windows

- [x] Configurações › Geral › "Ao fechar a janela": continuar na bandeja (desligado por padrão). Com um jogo ou
      clássico aberto, fechar a janela sempre esconde o Cerberus, para não perder tempo de jogo, backup de
      saves e conquistas; ele fecha sozinho quando o último jogo termina. Conferido no app: Mario Kart 64 no
      RetroArch, janela fechada, partida registrada e Cerberus fechado 1 s depois; com a opção ligada, o
      ícone na bandeja reabre a janela.

### 4.3c Novidades

- [x] Página "Novidades" no menu (como as notificações do Hydra): atualizações disponíveis dos recomps instalados
      pelo Cerberus, com "Atualizar" ali mesmo e um número no menu, e a atividade recente de todos os jogos
      (instalações, partidas, conquistas, erros) a partir do histórico. Conferido no app com o histórico real.

- [x] Página própria para cada clássico: capa, console, região, tempo de jogo, última vez, arquivo com
      "Abrir pasta", histórico, "Jogar no RetroArch" e, quando existe, "Ver o recomp". Aberta pela aba
      Clássicos (jogos sem recomp) e pela Atividade recente das Novidades. Conferida com o Mario Kart 64.

- [x] Favoritos (como no Hydra): "Favoritar" na página do jogo, coração na capa, aba Favoritos na Biblioteca e
      atalhos no menu lateral. Guardados em favorites.json, à parte da biblioteca, para valer também para
      jogos da Steam e da Epic, que são lidos das lojas a cada vez.

- [x] Botão direito numa capa (como no Hydra): menu com Jogar (ou o próximo passo), Página do jogo,
      Favoritar/Tirar dos favoritos e Abrir pasta do jogo. Fica dentro da janela e fecha com clique fora ou Esc.

- [x] Big Picture Mode com mouse: abas, capas (abrem a página), botões da página do jogo, "Voltar" e
      painéis respondem a clique. Sair sempre desfaz a tela cheia, mesmo saindo no meio de um clique.

### 4.3d Paridade com Hydra e RecompLauncher (levantamento de 19/11/2026)

- [x] Uma instância só: abrir de novo traz a janela aberta (até da bandeja).
- [x] Avisos do Windows para conquistas e jogos prontos quando a janela não está em foco.
- [x] Jogo que fecha com erro: código de saída explicado (DLL faltando, Visual C++ antigo, travamento),
      log do jogo apontado, no histórico e como aviso.
- [x] Opções de inicialização: abrir com o Windows e começar escondido na bandeja (só quando o Windows abre).
- [x] Tempo de jogo dos jogos da Steam e da Epic: o Cerberus vigia os programas abertos (mesmo na bandeja). Falta conferir com uma partida real.
- [x] Instalador do Windows (0.10.0, NSIS): em português, com as imagens do Cerberus, só para o usuário (sem
      administrador), Menu Iniciar, atalho na área de trabalho e "Executar" ao concluir (cai na configuração inicial);
      WebView2 embutido para PCs sem ele. Desinstalar mantém os dados (a caixa para apagar diz o que apaga) e tira
      o "Abrir com o Windows" e as chaves do instalador. Conferido: instalar, primeira abertura e desinstalar.
      Falta: assinatura digital (o SmartScreen avisa) e publicar a release.
- [x] Atualização do próprio Cerberus e "O que há de novo" (precisa de releases assinadas no GitHub).
- [x] Atalho do jogo na área de trabalho (abre pelo Cerberus com --play).
- [x] Adicionar à Steam (Gerenciar): jogo de fora na biblioteca da Steam, com capa e fundo, abrindo pelo Cerberus;
      com a Steam fechada, backup do shortcuts.vdf, e "Tirar da Steam" desfaz. Conferido no app com o SpaghettiKart.
- [x] Configurações › Seu PC: Visual C++ 2015-2022 x64 (versão, em dia ou antigo) e DirectX antigo (D3DX, XInput 1.3,
      XAudio 2.7), instalados pelos links da Microsoft.
- [x] Jogo que fecha com 0xC0000139: diz qual DLL é antiga (cópia na pasta do jogo, Visual C++ ou Vulkan do driver).
- [x] Mods do GameBanana para Ship of Harkinian, Starship, SpaghettiKart e Ghostship (pacotes .otr/.o2r).
      Conferido no app: instalar e remover um mod do SpaghettiKart.
- [x] Mods do GameBanana para Unleashed e Marathon Recompiled: pasta com mod.ini ligada no ModsDB.ini que o
      cpkredir.ini aponta (a lista do Hedge Mod Manager é mantida, com backup). Conferido com um mod real numa pasta temporária.
- [x] Recomps no GitLab e em servidores Forgejo (11 do catálogo principal e os 3 da lista apps_min do site:
      Hookshot, Longshot, FE3H Recomp) e ids renomeados (replaced) do catálogo do site.
- [x] Pausar e continuar download de recomp (o arquivo parcial fica guardado; cancelar apaga), na página do jogo e no Big Picture.
- [x] Reparar instalação (baixa a versão de novo por cima, com backup dos saves) e mover o jogo para outra pasta ou disco (Gerenciar).
- [x] Imagens de disco compactadas na preparação dos recomps: CHD (de DVD e de CD, pela primeira faixa) e CSO
      são lidos como ISO, sem programa de fora (crate chd, BSD-3): os arquivos saem direto deles, ou viram .iso
      quando o recomp quer a imagem. Testado com imagens geradas; falta um CHD real do chdman. RVZ, ZSO (LZ4) e
      CHD "filho" ainda não.
- [x] Coleções e jogos ocultos: aba Coleções (criar, renomear, apagar) e Ocultos na biblioteca, pelo botão direito
      na capa; ocultos saem do Início, do menu lateral e do Big Picture (collections.json). Conferido no app.
- [x] Monitor onde o Cerberus e o Big Picture abrem (Configurações › Geral, só com mais de um monitor ligado).
      Conferido no app com um monitor (a escolha salva e o Big Picture em tela cheia); falta ver com dois monitores.
- [x] Clássicos com vários discos: "(Disc 1)", "(Disc 2)"... da mesma pasta viram um jogo só (ou a lista .m3u do
      próprio jogador); PlayStation e GameCube abrem por uma lista .m3u na pasta do Cerberus, com troca de disco
      no menu do RetroArch. Conferido no app com discos de teste (o RetroArch com um jogo real ainda falta).
- [x] Fotos do jogo: F12 com um jogo aberto pelo Cerberus na frente tira foto só da janela dele (o F12 continua
      chegando ao jogo e à Steam); aba Fotos na página do jogo, com abrir a foto e a pasta. Conferido no app com o
      SpaghettiKart. Falta: jogos em tela cheia exclusiva (saem pretos).
- [x] Menu por cima do jogo (Ctrl+F12): janela transparente do Cerberus sobre a do jogo, com tempo da partida e
      total, tirar foto (o menu sai da frente), abrir a página no Cerberus e fechar o jogo (com confirmação, pelo
      X da janela). Esc ou Ctrl+F12 devolve o jogo. Conferido no app com o SpaghettiKart. Só em janela ou sem
      bordas; tela cheia exclusiva não deixa nada por cima. Controle: View+Menu juntos abrem e fecham (XInput),
      direcional move, A aperta, B volta. Navegação conferida pelo teclado; falta testar com um controle ligado.
- [x] DLSS (Gerenciar): mostra as DLLs do DLSS do jogo (Super Resolution, Frame Generation, Ray Reconstruction)
      com a versão; "Trocar versão…" usa uma DLL de mesmo nome que o jogador já tem (o Cerberus não baixa nada),
      guarda a do jogo e "Voltar à original" desfaz. Testado com DLLs do Windows no lugar; a tela conferida com
      um jogo falso. Falta: um jogo real com DLSS.
- [x] Idiomas: português do Brasil e inglês em todo o app (interface, Big Picture, menu por cima do jogo, avisos,
      erros do backend, bandeja e instalador). Automático segue o Windows; escolha em Configurações › Aparência.
      `tr!` com o texto em português como chave e `shared/locales/en.json`; teste garante que nada fica sem tradução.
      Conferido no app trocando para inglês e voltando.

### 4.4 Transição

- [x] O catálogo do recomplauncher.com virou opcional: chave em Configurações › Catálogo e fontes, ligada
      por padrão (decisão do usuário em 19/11/2026). Desligado, ficam os 117 recomps do Cerberus e as fontes.
- [ ] Pedir permissão ao mantenedor do RecompLauncher para usar a lista de repositórios deles como
      ponto de partida da descoberta (só os fatos: quais projetos existem; nunca textos ou arte).


## Onda 5: próximas entregas (levantamento de 23/11/2026)

Levantamento do RecompLauncher (Beta 68, de 23/11/2026; GPL: só ideias e fatos, nunca código) e do Hydra
(MIT), cruzado com o que o Cerberus já tem. Ordem = prioridade dentro de cada bloco. O que depende de servidor
ou de conta de terceiros está marcado; o que só serve para jogos piratas fica de fora de vez (no fim).

### 5.1 Hub de conquistas (prioridade 1)

Uma página **Conquistas** no menu lateral que acompanha todas as plataformas, mais uma aba **Conquistas** na
página de cada jogo, com filtros como os da Biblioteca. Nem o Hydra tem um hub entre jogos (só totais no perfil).

**Fontes de dados (validadas neste PC em 23/11/2026):**

- [x] **Steam pelo cache local do próprio cliente, sem chave nem perfil público.**
      - `Steam\appcache\stats\UserGameStatsSchema_<appid>.bin` traz o catálogo de cada jogo: nome e descrição em
        vários idiomas (inclusive `brazilian`), ícone, ícone cinza e se a conquista é secreta.
      - `UserGameStats_<conta>_<appid>.bin` traz o que foi desbloqueado, com a data.
      - Os dois são KeyValues binário. Conferido aqui: 254 jogos em cache; no Balatro, 31 conquistas, 8 desbloqueadas
        e todas com data.
      - Ícone: `https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/<appid>/<hash>.jpg`.
      - O Hydra não lê esses arquivos: seria diferencial nosso.
      - Limite: o cache cobre os jogos que a Steam deste PC já sincronizou.
- [x] **Tempo de jogo e última vez da Steam pelo `userdata\<conta>\config\localconfig.vdf`** (`Playtime`, `LastPlayed`;
      259 jogos aqui). Hoje o Cerberus só conta o tempo enquanto está aberto; isso traz o histórico todo da Steam.
- [x] **Raridade das conquistas da Steam** (% de jogadores que desbloquearam) pelo endpoint público
      `ISteamUserStats/GetGlobalAchievementPercentagesForApp` (sem chave). *A validar.*
- [x] **Steam sem cache (opcional, em Configurações › Contas › Steam):** chave da Web API e SteamID do jogador (`GetPlayerAchievements`, `GetSchemaForGame`);
      exige perfil com detalhes de jogo públicos. *A validar.* (O Hydra usa um token tirado da sessão logada da loja
      e lê páginas da Comunidade; não vamos por esse caminho.)
- [x] **RetroAchievements pela Web API oficial**, com a chave que o jogador já conecta:
      - `API_GetGameInfoAndUserProgress` dá as conquistas do jogo com pontos, emblema e data, separando normal e hardcore.
      - `API_GetUserCompletionProgress` dá todos os jogos com o progresso de cada um, para o hub.
      - Falta descobrir o id do jogo no RA para cada clássico (pelo hash da ROM ou pela linha do log do RetroArch). *A validar.*
- [x] **Epic (fase 3, precisa de login na conta Epic):** feito em 25/11/2026 (conquistas e tempo de jogo, no banco local).
      - Login OAuth com o cliente público do launcher, o mesmo do Legendary, do Heroic e do Hydra.
      - Conquistas pela GraphQL do launcher (`playerAchievementGameRecordsBySandbox`).
      - Tempo de jogo pelo `library-service` (`/library/api/public/playtime/account/<id>/all`).
      - Falta o "sandbox" de cada jogo: o `CatalogNamespace` dos manifestos da Epic deve servir. *A validar*: não há jogo
        da Epic instalado neste PC, e a Epic não guarda conquistas nem tempo de jogo no disco.
- [ ] **Conquistas dos recomps (fase 3):**
      - Ler os arquivos de troféus que cada port já grava; o Hydra (MIT) mapeia os formatos: bitmask, linhas id+hora,
        jsonl e log.
      - Avaliar o canal de sessão do RecompLauncher (variável de ambiente com um pipe por partida, uma mensagem JSON por
        linha), para os recomps que adotarem a biblioteca deles.

**Telas:**

- [x] **Hub:** totais de todas as plataformas (desbloqueadas, % concluído, jogos 100%, pontos do RA) e **últimas
      conquistas de todos os jogos** numa linha do tempo.
- [x] **Hub, lista de jogos:**
      - Filtros por plataforma (Steam, Epic, RetroAchievements, Recomps).
      - Filtros por estado: em andamento, concluídos, não começados.
      - Ordem: recentes, % concluído, A a Z, mais raros.
      - Busca por nome.
- [x] **Página de conquistas do jogo** (falta a aba na página do jogo da Biblioteca):
      - Resumo X/Y com barra, e um troféu quando chega a 100%.
      - Busca e filtro Todas / Desbloqueadas / Bloqueadas.
      - Ordem: mais comuns, mais raras, recentes.
      - Secretas escondidas até clicar, com opção para mostrar todas.
      - Selo de hardcore e conjuntos (subsets) do RA.
      - % de raridade com "<0,1%" para as raríssimas.
- [x] **Biblioteca:** progresso de conquistas na capa e opção de ordenar por conquistas.
- [x] **Big Picture Mode:** a mesma página de conquistas.
- [x] **Aviso ao desbloquear por cima do jogo:** janela transparente, sem foco e sempre no topo, num canto escolhido,
      com ícone, raridade e "platina" quando completa o jogo.
      - Steam: vigiar a data de modificação do `UserGameStats` durante a partida.
      - RA: o log do RetroArch, que já lemos.
- [x] **Cartão "Seu tempo de jogo"** na página do jogo (ideia do RecompLauncher):
      - Barras das últimas 2 semanas, total, semana, mês, número de sessões e sessão mais longa.
      - Soma Cerberus, Steam e Epic, separando a origem.

### 5.2 Confiança e distribuição (prioridade 2)

- [x] **Atualização do próprio Cerberus** (já listada em 4.3d):
      - Um `version.json` com URL, SHA-256 e tamanho por plataforma, mais uma versão mínima obrigatória.
      - Instalação silenciosa do NSIS e reabertura do app.
      - Arquivo ruim marcado para não ser oferecido de novo, e conferência da versão exata dentro do instalador.
      - Cartão "O que há de novo" depois de atualizar e Configurações › Novidades.
- [ ] **Assinatura do instalador** (some o aviso do SmartScreen) e uma Action de conferência da release (versão certa,
      assinado, arquivo certo).
- [ ] **Janela de falha:**
      - As últimas linhas do log do jogo e a explicação que já temos.
      - "Usar a placa de vídeo mais rápida".
      - Botão para abrir uma issue com o log e o histórico.
- [x] Explicar quando o **Smart App Control** do Windows bloqueia um jogo sem assinatura.
- [ ] **Recuperar o WebView2** quando ele travar, sem derrubar jogos nem downloads; avisar que o RivaTuner é a causa comum.
- [x] Ignorar programas auxiliares (crash handler, atualizador, desinstalador) ao acompanhar a partida.
- [x] **Rede educada:** quando um site falha (tempo esgotado, 429, 5xx), esperar 30 s e ir dobrando até 1 h, com um
      pouco de aleatoriedade.
- [ ] Achar o arquivo da release pelo sistema e pela CPU quando o padrão do catálogo parar de bater (projeto que
      renomeou os arquivos).

### 5.3 Recomps e Easy Setup (prioridade 3)

- [x] **Fila única de downloads:** downloads e builds juntos, "Pausar tudo" e contador no menu.
- [ ] Por jogo: atualizar sozinho e optar por versões de teste (pre-release).
- [ ] "Liberar espaço": apagar arquivos de instalação que não são mais necessários.
- [ ] Recomp que mudou de página no catálogo: "Mudar para ele", levando saves, tempo de jogo e arquivos.
- [ ] **Recomps com vários jogos** numa instalação só, cada um com a própria capa, tempo, conquistas e atalhos.
- [x] **Ler o `recomplauncher.json`** que os desenvolvedores publicam na release: entradas, saves, arquivo e opções de
      ajustes, requisitos do sistema, preparação, dumps, ações, mods, filtros e códigos de saída.
- [x] `recomplauncher.json`: vários jogos numa release (`games`, com os saves de cada um no backup), chaves
      compartilhadas, modo Big Picture e controle, `lines` do filtro CRT e "Preparar agora" no cartão do jogo.
- [ ] `recomplauncher.json`: o DLSS que o launcher baixaria sozinho (precisa de aprovação), conquistas pelo
      `title_id` (o Cerberus ainda não lê Recomp Achievements).
- [ ] Conversão de mais formatos sem programa de fora: RVZ, WIA, WBFS, GCZ e NKit para ISO; CHD para BIN/CUE; ZSO.
- [x] Achar jogos nas pastas que os emuladores já conhecem (Dolphin, PCSX2, DuckStation, PPSSPP, RPCS3, Cemu), só lendo, e instalar esses emuladores das páginas oficiais. Ryujinx ficou de fora: o projeto foi encerrado.
- [x] Reconhecer o console de cada imagem de disco pelo próprio disco (PS1, PS2, PSP, GameCube, Wii, Xbox, Xbox 360) ou pelo nome da pasta.
- [ ] Trazer recomps que já estão na Steam ou em outro launcher, além da pasta escolhida.
- [ ] Notar uma instalação interrompida (cancelada, travada, falta de luz) e refazer antes de jogar.

### 5.4 Durante o jogo e integração (prioridade 4)

- [x] Conquistas dos emuladores só no Cerberus: RetroArch, DuckStation e PCSX2 entram na conta com o token,
  os avisos deles ficam desligados durante a partida (e voltam depois) e o Cerberus lê o log e mostra o próprio aviso.
- [x] O mesmo para Dolphin (só pela linha de comando) e PPSSPP (o PPSSPP não tem como esconder o próprio aviso; só o som sai).
- [ ] Troféus do RPCS3 (PS3) no hub de conquistas.

- [x] Combos do controle mesmo com o jogo em foco: View+RB tira foto e segurar View abre o menu (hoje: View+Menu).
- [ ] **Manter a Steam em dia:** todo jogo instalado entra na Steam sozinho, numa coleção "Cerberus", e sai ao desinstalar.
- [x] Menu da bandeja com os jogos recentes.
- [x] Impedir o PC de dormir durante downloads.
- [x] Informações do jogo original pela Wikipedia, guardadas por um mês.
- [ ] Prévia ao vivo dos filtros visuais sobre a imagem do próprio jogo, e um filtro "Studio".
- [ ] Arte de mídia: o jogo desenhado como o cartucho, cartão ou disco dele.
- [ ] Saves na nuvem de jogos de PC comuns (Steam, Epic, adicionados) pelos locais de save conhecidos (base do
      Ludusavi/PCGamingWiki; conferir a licença antes de usar).

### 5.5 Interface e acessibilidade (prioridade 5)

- [ ] **Tamanho do texto:** zoom da janela de 90% a 150%, com Ctrl +, Ctrl − e Ctrl 0.
- [ ] Respeitar o "reduzir movimento" do Windows sozinho (hoje só a chave "Menos animações").
- [ ] Mais idiomas além de português e inglês (o RecompLauncher tem 22): espanhol primeiro.
- [ ] Detectar portáteis (ROG Ally, Legion Go, Steam Deck) e ajustar padrões.
- [ ] Temas: cores e som de conquista personalizáveis.
- [ ] Formulário de opinião dentro do app (área, humor, mensagem, dados do PC opcionais).
- [ ] Modo portátil (tudo na pasta do programa).

### 5.6 Depende de servidor ou de outra decisão (avaliar)

- [ ] Relatos dos jogadores ("Como rodou?"), nota para o Steam Deck, aviso de "esta versão está travando muito",
      desenvolvedores verificados para seguir, "Avise-me" de lançamentos, notícias e aviso imediato de versão nova.
      No RecompLauncher tudo isso vem do site deles; para nós, só com um servidor próprio ou com permissão de usar o deles.
- [ ] Conta e sincronização entre PCs (biblioteca, tempo de jogo, saves) com login.
- [ ] Perfil, amigos, placares e avaliações de jogos (Hydra): precisam de servidor e moderação.
- [ ] Estatísticas de uso opcionais, perguntadas na primeira abertura, com página de privacidade.
- [ ] **Linux e Steam Deck:** Proton/umu para os recomps só de Windows; pacotes AppImage, Flatpak, .deb e Nix.
- [ ] Sistema de plugins e ferramentas para desenvolvedores de recomps (validar manifesto, pasta de build local).
- [ ] Emuladores fora do RetroArch (RPCS3, shadPS4, DuckStation, PCSX2, PPSSPP), com os troféus e conquistas deles.
- [ ] Importar a biblioteca do LaunchBox.

## Onda 6: consolidado de Playnite, Hydra e RecompLauncher (levantamento de 23/11/2026)

Três análises, uma por projeto, cruzadas com o que o Cerberus já tem e com a Onda 5 (o que já está lá não se repete
aqui). Licenças: **Playnite MIT** e **Hydra MIT** (ideias e código, com crédito); **RecompLauncher GPL** (só ideias e
fatos, nunca código). Origem entre colchetes: [P] Playnite, [H] Hydra, [R] RecompLauncher. Esforço: P, M, G.
Nada aqui precisa de servidor próprio, salvo onde está marcado.

### 6.1 Conquistas, a continuação do hub (prioridade 1)

- [x] **Aviso de conquista na tela durante o jogo** [H]: janela transparente, sem foco e por cima de tudo, em 4
      cantos; versões normal, rara (até 10%) e platina (a que completa o jogo); som próprio e botão de teste. — M
      (Texto corrigido em 26/11/2026 pelo J F11: antes dizia 6 posições, versão secreta, agrupamento e volume.)
- [ ] **O que falta do aviso**: versão secreta, "N conquistas em M jogos" quando vierem várias, volume ajustável e
      as posições do meio (em cima e embaixo). — P
- [x] **Conquista da Steam na hora** [H]: durante a partida, olhar `userdata/<conta>/config/librarycache/<app>.json`
      a cada 2 s e avisar o que mudou; reler a Steam 10 s e 30 s depois que o jogo fecha. — M
- [x] **Aba Conquistas na página do jogo da Biblioteca** (o que faltou da 5.1). — P
- [ ] **Steam sem chave** [H]: entrar na loja da Steam numa janela do app e usar o token que a própria página já tem
      (`webapi_token`) no lugar da chave da Web API; a chave continua como alternativa. — M
- [ ] **Biblioteca da família Steam** [H] (`IFamilyGroupsService`): jogos compartilhados, com o tempo de jogo. — M
- [ ] **Troféus dos recomps** [H]: ler os arquivos de troféus que alguns ports gravam (`trophies.dat`, `trophies.jsonl`,
      `earned.json`, linhas do log) e mostrar no hub; nomes e ícones num JSON nosso no catálogo. — M
- [ ] **Foto no momento da conquista** [H]: capturar a janela do jogo quando a conquista cai; no RetroArch, ligar a
      opção dele de foto ao desbloquear só durante a partida. — M
- [x] Conquistas da Epic [H] (GraphQL da loja da Epic), depois do login da Epic (6.2). — M (25/11/2026)
- [x] **Conquistas do Xbox** (conta Microsoft): todos os jogos da conta, com raridade, guardados no banco local. (25/11/2026)
- [x] **Banco local de conquistas** (`achievements.db`, SQLite): um desbloqueio visto nunca se perde; 5 min entre
      sincronizações e espera de 2 min a 6 h depois de recusa, gravadas. (25/11/2026)
- [x] Hub: cartão por plataforma, Ver todos em Platinados e Perto dos 100%, Próximas conquistas e Suas mais raras. (25/11/2026)
- [ ] Separar conquistas do jogo base e das DLCs [H]. — P

### 6.2 Biblioteca e lojas (prioridade 2)

- [x] **Jogos da Steam não instalados na Biblioteca** [H][P], pelo `GetOwnedGames` que o hub já usa; "Instalar" por
      `steam://install/<id>`, e o jogo vira instalado sozinho quando aparece o `appmanifest_*.acf`. — M
      - [ ] "Desinstalar" por `steam://uninstall/<id>`: ainda não existe (conferido no código em 25/11/2026).
- [ ] Opções de inicialização passadas à Steam: `steam://run/<id>//<argumentos>/` [H]. — P
- [ ] Ler o `appinfo.vdf` da Steam para saber o executável real de cada jogo [H] (medir a partida certo). — M
- [x] **Login da Epic e biblioteca completa** [H]: login na página da Epic, biblioteca inteira com os não instalados,
      tempo de jogo pelos servidores dela; instalar abre o launcher da Epic. (25/11/2026)
      - [ ] Instalar, verificar e abrir sem o launcher da Epic (Legendary é GPL: só como programa à parte). — G
- [x] **Conta Microsoft e PC Game Pass**: jogos do app Xbox instalados, catálogo do Game Pass e jogos de PC da conta,
      instalando pelo app Xbox; tempo de jogo e última vez da conta. (24–25/11/2026)
- [ ] **Importar o tempo de jogo das lojas** com escolha: sempre, só para jogos novos ou nunca; "última vez" da loja só
      vale se for mais nova [P]. Tempo separado por fonte (Cerberus, Steam, Epic), editar e zerar [H]. — P
- [ ] **Achar jogos fora das lojas** [P][H]: atalhos do Menu Iniciar (ignorando desinstaladores, setup, vc_redist,
      crash handlers), uma pasta escolhida, `C:\Games`/`D:\Games`/"GOG Games", e apps da Microsoft Store/Xbox
      (`shell:AppsFolder`). Jogo apagado entra numa lista de exclusão e não volta na próxima busca [P]. — M
- [x] O mesmo jogo na Steam e na Epic aparece como um só [H]. — M (27/11/2026, pedido do usuário: o mesmo jogo na Steam, no Xbox e na Epic é um card só, com o logo de cada loja; a cópia mostrada é a instalada, senão a jogada por último, senão Steam, Xbox, Epic; na página, "Steam | Xbox" ao lado de Jogar ou Instalar escolhe por qual loja; o filtro de uma loja mostra a cópia dela; cada cópia guarda seus dados; agrupa pela chave de nome, nunca duas cópias da mesma loja nem jogos adicionados à mão; testes; conferido no app: 16 jogos agrupados, Todos foi de 934 para 916, e Cyberpunk 2077 troca entre Steam e Xbox; 27/11/2026: a TV também junta as cópias)
- [ ] Ordem pelo nome sem "The"/"A" e com algarismos romanos na ordem certa [P]. — P
- [ ] Tamanho instalado de cada jogo, agrupar por disco e mover um jogo de PC para outro disco [P][H]. — M
- [x] **Página do jogo com dados da loja** [H]: descrição, requisitos mínimo e recomendado, idiomas, suporte a controle,
      imagens e trailers (`store.steampowered.com/api/appdetails`); Epic e Xbox pela Steam ou pela Microsoft Store
      (`ms_store.rs`). — M (25/11/2026)
- [ ] Atalho no Menu Iniciar além da área de trabalho [H][R]. — P

### 6.3 Organizar a coleção (prioridade 3)

- [x] **Situação do jogo** [P]: Não jogado, Jogado, Jogando, Zerado, Completo, Abandonado, Em pausa, Quero jogar,
      editáveis. Vira "Jogado" na primeira partida; os selos do hub (Zerado, Completo, Dominado) sugerem a situação. — P
- [x] Nota pessoal (de 1 a 5 estrelas; a ideia original era 0 a 100) e anotações por jogo [P]. — P
- [ ] **Filtros salvos** com ordem e agrupamento [P]: prontos "Todos", "Jogados recentemente", "Favoritos" e
      "Mais jogados"; aparecem também no Big Picture. — M
- [ ] **Agrupar** por plataforma, loja, situação, ano, disco, faixa de tempo de jogo (menos de 1 h, 1 a 10 h, 10 a
      100 h...) e última vez jogado (hoje, ontem, esta semana, este mês, este ano, nunca) [P]. — M
- [ ] Mais ordens: adicionado em, situação, tamanho, vezes jogado [P]. — P
      (Corrigido em 25/11/2026: estava marcado como feito, mas a Biblioteca só tem nome, recentes, tempo, nota e
      conquistas. Desenho em `docs/roadmap/metadados-filtros-e-estatisticas.md`.)
- [ ] Fixar jogos no topo e ordem manual por arrasto, também dentro das coleções [H][R]. — M
- [ ] **Editar vários jogos de uma vez** (situação, coleção, ocultar) [P]. — M
- [ ] **Página de estatísticas** [P]: total de jogos, instalados, tempo total e médio, os 50 mais jogados, quantos por
      situação e por loja, espaço ocupado, filtrável por loja ou plataforma. — M
- [x] **"Jogar algo aleatório"** dentro do filtro atual, com "sortear outro" e "jogar" [P][H]. — P
- [ ] **Busca global (Ctrl+F)** [P]: aceita erro de digitação e siglas ("gta"), sem acento; vazia mostra os 20 últimos
      jogados; resultados abrem ou jogam. — M
- [ ] Atalhos de teclado [P]: F5 atualizar biblioteca, F6 jogo aleatório, Ctrl+F busca, Enter jogar, Delete remover. — P
- [ ] Arte padrão quando falta: a da loja, depois a da plataforma, depois uma genérica [P]. — P

### 6.4 Partidas (prioridade 4)

- [ ] **Medir melhor** [P][H]: modos por jogo (programa, árvore de processos, qualquer programa na pasta do jogo,
      nome do processo) e programas extras a acompanhar; jogos abertos fora do Cerberus também contam; não contar o
      tempo em que o PC dormiu. — M
- [ ] **Várias formas de abrir o mesmo jogo** (DX12 e Vulkan, launcher de mods), com escolha ao jogar [P];
      variáveis nos caminhos: `{InstallDir}`, `{Name}`, `{ImagePath}` [P]. — M
- [ ] **Ações antes e depois de jogar**, globais ou por jogo, e a de antes pode cancelar [P]. — M
- [ ] **HDR do Windows ligado só durante o jogo** e devolvido no fim [P]. — M
- [ ] Fechar a Steam ou a Epic depois da partida (só se a partida passou de 2 min e nada mais dela está rodando) [P]. — P
- [ ] Ao abrir um jogo: nada, minimizar, ir para a bandeja ou fechar; ao fechar o jogo: restaurar [P][H]. — P
- [ ] Janelinha de abertura quando o jogo vem de um atalho: tempo jogado e verificações, fechando quando o jogo
      aparece [H]. — P
- [ ] Manter a tela acordada durante a partida e os downloads [H]. — P
- [ ] **Fotos de jogos em tela cheia exclusiva** pela Desktop Duplication do Windows [H]; tecla e combinação do
      controle escolhidas pelo jogador; copiar a foto. — M
- [ ] **Link `cerberus://`** (abrir jogo, página, busca), opções de linha de comando e lista de atalhos da barra de
      tarefas com os 10 jogos recentes [P][H]. — P
- [ ] Mais pacotes necessários com instalação num clique: DirectX de junho de 2010, OpenAL, .NET 4 e XNA 4 [H]. — P
- [ ] Central de notificações dentro do app (lidas e não lidas) [H]. — P

### 6.5 Clássicos e emuladores (prioridade 5)

- [ ] **Reconhecer ROMs pelo CRC** [P][H] com as bases No-Intro/Redump do libretro-database (também dentro de zip),
      depois pelo nome e pelo serial; região pelos códigos do nome do arquivo; nome, região e ano certos. — G
- [ ] Buscas de ROMs salvas por pasta (subpastas, dentro de arquivos, exclusões, ignorar arquivos só na nuvem),
      refeitas ao atualizar a biblioteca [P]. — M
- [ ] Escolher o disco ao jogar, com "não perguntar de novo" [H]. — P
- [ ] **Emuladores fora do RetroArch** [H][P]: instalar e atualizar DuckStation, PCSX2, PPSSPP, Dolphin, RPCS3 e
      shadPS4 pelas releases oficiais; conferir BIOS e firmware (sem baixar BIOS); levar o login do RetroAchievements
      para eles e ler as conquistas pelos logs; troféus do RPCS3 e do shadPS4; tempo de jogo deles. — G
- [ ] Cartões de memória de PS1 e PS2: listar e exportar saves [H]. — M
      - [x] Backup e restauração dos memory cards e estados dos emuladores (`emulator_saves.rs`, 24/11/2026).
- [x] Xenia e xemu para os clássicos de Xbox 360 e Xbox (24/11/2026).
- [x] Selo de origem na capa (loja, Recomp, emulador e console) e fotos reais dos consoles (25/11/2026).

### 6.6 Big Picture (prioridade 6)

- [ ] Teclado virtual para o controle e sons de navegação, com chave para desligar [H][R]. — M
- [ ] Menu de energia: desligar, reiniciar, suspender, bloquear [P][H]. — P
- [ ] Filtros salvos e mudar situação, nota e coleção pelo controle [P]. — M
- [ ] Música de fundo (muda quando um jogo abre), relógio e bateria, escurecer jogos não instalados [P]. — P
- [ ] Ícones dos botões (Xbox, PlayStation, Nintendo) e trocar confirmar/cancelar [P][R]. — P
- [ ] Diagnóstico do controle: botões e eixos ao vivo e teste de vibração [H]. — P

### 6.7 Recomps (complementa a 5.3) [R]

- [x] Pausar as atualizações de um jogo e limite de velocidade de download, mais lento enquanto se joga. — P
- [ ] Pasta de instalação por jogo e proxy em Configurações › Rede. — P
- [x] Repetir quando o Windows ou o antivírus prende um arquivo, com mensagem clara. — P
- [ ] Etiquetas no catálogo (Widescreen, 60 FPS, Mods, Online) com filtro, situação do projeto (encerrado, pausado)
      e "Parecidos com este". — P
- [ ] Chaves compartilhadas (Switch, Wii U) importadas uma vez e usadas por todos os jogos que pedem. — M
- [ ] Conferir a cópia do jogador contra o Redump ("Bate com o disco USA verificado"). — P
- [ ] **Resolver problemas** numa janela: arquivos, requisitos, caminho com acento, pasta do OneDrive, DLL faltando,
      com botões de correção; tirar do caminho DLLs antigas que o jogo traz. — M
- [ ] Mais causas na janela de falha: memória, placa de vídeo sem DX12/Vulkan, driver reiniciado, VRAM cheia,
      .NET ou Python faltando, um mod como culpado. — M
- [ ] Filtro visual que falha ao abrir tenta o outro jeito de carregar o ReShade da próxima vez. — P
- [ ] Guardar os saves ao desinstalar e devolver ao reinstalar; página de saves de todos os jogos. — P
- [ ] Segurar Jogar para escolher qual programa do recomp abrir. — P

### 6.8 Dados e interface

- [ ] **Backup da biblioteca** num zip (configurações, biblioteca, artes), automático diário ou semanal com rodízio, e
      restauração escolhendo o que voltar [P]. — M
- [ ] **Saves de qualquer jogo de PC** pelo Ludusavi (pré-visualizar, fazer backup e restaurar) [H]; junta com a nuvem
      por pasta que já temos. — M
- [ ] Menu lateral personalizável (esconder e reordenar), tamanho dos cards, abrir em Início, Biblioteca ou Conquistas
      [R]. — P
- [ ] Textos copiáveis (descrições, caminhos, erros) e layout que funciona em 640×480 [R]. — P

### 6.9 Precisa de servidor (só registro)

- Placares, amigos, chat, perfil público e "Wrapped" [H]; relatos "Como rodou?", aviso de versão que trava, seguir
  desenvolvedores e pedidos de recomp [R]; arte do SteamGridDB sem chave do jogador vem do servidor do
  Hydra [H]. Para nós, só com servidor próprio.

## Onda 7: consolidado de Heroic, Hydra, Playnite e RecompLauncher (levantamento de 25/11/2026)

Quatro agentes, um por launcher, leram o código de cada um e o do Cerberus. Aqui entra só o que **não** estava nas
ondas 5 e 6. Licenças: **Heroic GPL-3** e **RecompLauncher GPL** só ideias; **Hydra MIT**, **Playnite MIT** e
**RecompLauncher-SDK MIT** podem ser adaptados com crédito. Origem: [E] Heroic, [H] Hydra, [P] Playnite,
[R] RecompLauncher, [S] SDK. Esforço: P, M, G.

### 7.1 Saves de jogos de PC (prioridade 1)

- [ ] Achar os saves de qualquer jogo de PC pelos locais conhecidos (manifesto do Ludusavi/PCGamingWiki; conferir a
      licença dos dados) e usar o mesmo backup, rodízio e nuvem por pasta dos recomps [H][E][P]. — M
- [ ] Versões de backup com nome, conflito entre dois PCs resolvido com escolha clara, e caminho de save adicionado
      pelo jogador [H]. — M
- [ ] Backup automático da biblioteca do Cerberus (configurações, notas, tempo, artes próprias) diário ou semanal com
      rodízio; hoje só o zip manual (`data_backup.rs`) [P]. — P

### 7.2 Mais lojas e gerenciar os jogos delas (prioridade 2)

- [ ] **GOG**: jogos instalados pelo GOG Galaxy (registro do Windows), depois login, biblioteca com os não instalados,
      conquistas e tempo de jogo [E]. — P para os instalados, G para a conta
- [ ] Amazon (Prime Gaming), itch.io, Ubisoft Connect, EA app e Battle.net: primeiro os instalados, depois contas [E]. — P/M cada
- [ ] Desinstalar, verificar e mover jogos das lojas pelos próprios links ou janelas delas (Steam, Epic, Xbox) [E][P]. — P
- [ ] Argumentos de inicialização para jogos da Steam (`steam://run/<id>//<args>`); Epic e Xbox a verificar [E]. — P
- [ ] DLC: listar e, quando a loja deixar, instalar [E]. — M
- [ ] Fila de downloads com ordem e "desligar o PC ao terminar" [E]. — P/M

### 7.3 Biblioteca rica (prioridade 3)

- [ ] **Metadados guardados no jogo**: gênero, desenvolvedora, publicadora, ano, série, classificação, descrição, links e
      nota da crítica; baixados da loja ou do IGDB, escolhendo a fonte por campo e comparando antes de sobrescrever [P].
      É o que destrava filtros salvos, agrupar e editar vários jogos (6.3). — G
- [ ] Seleção de vários jogos na grade (caixas de seleção) para ocultar, coleção, situação [P]. — M
- [ ] Lista de exclusão na importação (o jogo removido não volta) [P]. — P
- [ ] Perfil local e "seu ano": tempo total, jogos e conquistas por mês, por loja e por situação, a partir de
      `sessions.jsonl` e `achievements.db` [H][P]. — M
- [ ] Descobrir também para jogos de PC: listas como Steam250 (joias escondidas, melhores do ano), filtros por ano e
      gênero, abrindo a página da loja [H]. — M

### 7.4 Partidas e avisos (prioridade 4)

- [ ] Script antes e depois de jogar e variáveis de ambiente por jogo, também para jogos de loja [E][P]. — P
- [ ] Aviso de sem internet e modo offline por jogo [E]. — P
- [ ] Som do aviso de conquista com volume e som próprio do jogador [H]. — P
- [ ] Controles além do XInput (DualSense, Switch Pro) para os combos e o menu por cima do jogo (SDL) [R]. — M/G
- [ ] Discord Rich Presence [R]. — P

### 7.5 Recomps (prioridade 5)

- [ ] Escolher uma versão anterior, fixar e voltar (hoje só "pausar atualizações") [R]. — M
- [ ] Campos do `recomplauncher.json` que o Cerberus ainda ignora: `extract`, `contains`, `destination` e
      `check.nup_build_id` [S]. — P/M
- [ ] Colocar as entradas como link em vez de cópia (ISOs grandes ocupam o dobro) [S]. — P
- [ ] Teclas do jogo nos ajustes (`settings[].bindings`) [S]. — M
- [ ] Liberar espaço depois de um build com `keeps_copy` [R]. — P
- [ ] Janela "Resolver problemas": guardar a saída do jogo num log, tirar a marca "baixado da internet", preferir a placa
      de vídeo mais rápida, avisar caminho com acento, exportar os logs num zip [R]. — M
- [ ] Conquistas dos recomps pelo canal de sessão e pelos arquivos de unlock do ReXGlue (`title_id`) [R]. — M/G

### 7.6 Página do jogo e aparência (prioridade 6)

- [ ] HowLongToBeat (não oficial, pode quebrar) e situação do anti-cheat [E][H]. — P/M
- [ ] Temas instaláveis (CSS) e editor de tema [H][P]. — M
- [ ] Promoções e lista de desejos [E]. — M
- [ ] Extensões/plugins com permissões; o primeiro passo são os scripts da 7.4 [P][R]. — G

### Onde o Cerberus já está à frente (segundo os quatro levantamentos)

- Hub de conquistas entre Steam, RetroAchievements, Xbox e Epic, guardado para sempre e com aviso por cima do jogo.
- Xbox app, Game Pass e conta Microsoft (nenhum dos outros tem).
- Recomps com catálogo, Easy Setup, ajustes por jogo, nuvem sem conta e listas por URL.
- Emuladores instalados com um clique e RetroAchievements levado para eles.
- Mods do GameBanana, troca de DLSS, filtros visuais e pacotes do Windows.

## Onda 8: deixar completo o que já foi entregue (revisão de 25/11/2026)

Três agentes revisaram o código das entregas de 23 a 25/11 procurando o que falta para cada feature ficar completa
(bordas, estados de tela, segurança, testes). Cada item abaixo tem, no documento ligado, o problema, como reproduzir,
como resolver, os arquivos, o critério de aceite e o teste. **Regra daqui em diante:** uma feature só é marcada [x]
quando os itens da sua revisão estiverem resolvidos ou conscientemente adiados aqui.

### 8.0 Perde dados ou mexe em contas (fazer antes de qualquer feature nova)

- [x] Remover ou reinstalar um emulador gerenciado apaga os saves dele (DuckStation, PCSX2, Dolphin, PPSSPP, RPCS3,
      Xenia) — [emulação L1](docs/roadmap/emulacao-e-saves-dos-emuladores.md). — P
      Feito em 25/11/2026: troca que guarda os arquivos do jogador para os 8 emuladores; Remover guarda à parte e devolve ao reinstalar. Validado numa cópia do DuckStation (3 itens) e do PCSX2 (12 itens), idênticos depois.
- [x] Refresh tokens da Epic e do Xbox vão para o zip do backup e a restauração troca as contas — [lojas L6](docs/roadmap/lojas-e-contas.md);
      erro de rede com a URL inteira pode vazar token no log ([plano Steam, etapa 0](docs/roadmap/steam-sem-chave-e-gerenciar-lojas.md)). — P
      Feito em 25/11/2026: tokens fora da cópia, contas deste PC intocadas na restauração, erros de rede sem o endereço. Conferido com o backup dos dados reais (11 arquivos, sem token).
- [x] `settings.json` gravado por várias tarefas ao mesmo tempo: Sair durante uma renovação devolve a conta — [lojas L1](docs/roadmap/lojas-e-contas.md). — M
      Feito em 25/11/2026: a renovação só grava se a conta ainda é a mesma; entrar e sair por `settings::update` e zerando as sessões guardadas da Epic e do Xbox. Testado.
- [x] Um dono só da sessão de cada conta; a sessão da Epic de 30 min não é zerada ao sair nem presa à conta — [lojas L2](docs/roadmap/lojas-e-contas.md), [conquistas L3](docs/roadmap/conquistas-e-tempo.md). — P/M
      Feito em 27/11/2026: accounts::Kept guarda a sessão de cada conta com o refresh token que veio com ela, uma renovação por vez para o app inteiro (quem pede durante a renovação espera e recebe a mesma); a Biblioteca e as conquistas da Epic e do Xbox usam a mesma sessão; outro refresh token (saiu, outra conta) nunca recebe a antiga; Sair esquece sem travar; testes (5 pedidos juntos, 1 renovação; outra conta renova); conferido no app, hub sem problemas.
- [x] Um jogo que falha derruba a sincronização da conta inteira e volta primeiro sempre (27/11/2026: no Xbox e na Epic, um jogo que falha ganha uma marca e espera 6 h, sem parar os outros nem voltar primeiro; a lista que ele tinha continua; duas falhas seguidas contam como da conta, e o que já veio fica gravado; teste da regra; sem servidor falso, a costura de rede da L16 continua faltando) — [conquistas L1](docs/roadmap/conquistas-e-tempo.md). — M
- [x] Sincronização pulada conta como sucesso e apaga a espera depois de uma recusa (27/11/2026: "sincronizado" ou "recusado" é gravado dentro da sincronização, com a trava na mão; quem encontra "ainda não é hora" não mexe na espera; teste; o hub daqui carrega os 571 jogos sem problemas) — [conquistas L2](docs/roadmap/conquistas-e-tempo.md). — P
- [x] Restaurar saves com o emulador aberto por fora (27/11/2026: restaurar recusa com o programa do emulador rodando, aberto de qualquer jeito, com "Feche o DuckStation antes"; teste) — [emulação L3](docs/roadmap/emulacao-e-saves-dos-emuladores.md). — P
- [x] Backup do Dolphin leva a BIOS do GameCube e deixa de fora os saves do Wii (27/11/2026: arquivos do console, IPL.bin, dsp_*.bin, font_*.bin, keys.bin, otp.bin, seeprom.bin, nunca entram num backup; o Dolphin copia também Wii/title/00010000 e 00010004, sem a memória do sistema; teste; sem Dolphin aqui) — [emulação L2](docs/roadmap/emulacao-e-saves-dos-emuladores.md). — P

### 8.1 Contas e lojas — [revisão completa](docs/roadmap/lojas-e-contas.md)

- [ ] Token revogado ou vencido vira "entre de novo" visível; erros distintos para 400/401/429/5xx (L3). — M (27/11/2026, parte: renovar o token da Epic e da Microsoft separa os casos: 400, 401 e 403 pedem para entrar de novo; 429 é uma pausa e 5xx é a loja fora do ar, os dois esperam sem tirar a conta; no Xbox, o 401 do perfil continua "crie um perfil Xbox"; teste; falta o cartão da conta mostrar o estado, junto com a L8)
- [ ] Conta identificada pelo id (xuid, account id, steamid), não pelo nome; Sair sem apagar conquistas (L4; conquistas L3). — M
- [ ] Resposta parcial, limites de uso e custo de cada sincronização (L5). — M
- [ ] Tokens fora do texto puro (Cofre de credenciais do Windows) (L6). — M
- [x] Endurecer a janela de login (L7). — P (27/11/2026: a passagem do código da Epic só vale com o segredo deste login, que só a página de retorno da Epic recebe; a janela da Epic e a da Microsoft só mostram os sites de login, Epic, Microsoft e os provedores da Epic, e o resto abre no navegador; pop-ups idem; teste; o login real não foi refeito, a conta daqui já está conectada; PKCE da Microsoft fica de fora)
- [ ] Cartões de conta com carregando, erro, "sincronizado há" e "Atualizar agora" (L8). — P/M (27/11/2026, parte: Sair pede um segundo clique; o Xbox conta os do Game Pass à parte dos da conta, 467 e 74 aqui; o texto da Epic não diz mais que não há conquistas; o cartão já se atualizava depois de entrar, pelo catalog-changed; falta "sincronizado há" e "Atualizar agora")
- [ ] Xbox: "jogos da conta" vem do histórico, não da posse (L9); pacotes instalados de verdade (L10). — M
- [ ] Epic: plataforma, demos e limite de páginas (L11). — M
- [ ] Game Pass: ~~padrão e mercado~~ (27/11/2026: "PC Game Pass na Biblioteca" começa desligado para quem nunca escolheu, e o cartão diz que é para quem assina; quem já ligou continua ligado, como aqui, 467 jogos; o catálogo vem do mercado da região do Windows, não do idioma; famílias comparadas sem maiúsculas; testes), ~~id estável~~ (27/11/2026: um jogo do Game Pass sem família de pacote, 108 dos 467 aqui, que é instalado pelo app Xbox leva favorito, nota e coleções do id antigo para o novo; teste), ~~selos~~ (27/11/2026: um jogo da conta não instalado que também está no Game Pass continua da conta, com a arte e o produto do catálogo, em vez de virar "Game Pass"; teste; aqui as contagens seguem 6, 467 e 74) (L12); ~~mesmo jogo com nomes diferentes entre lojas (L13)~~ (27/11/2026: uma chave de nome só, names::key em shared, no lugar das quatro cópias: tira marcas, "(PC)", "for Windows", "Windows 10 Edition", "Standard/Deluxe/Ultimate/Gold/GOTY/Definitive/Complete Edition", "Game of the Year Edition", e passa II a XX para algarismos; o ano entre parênteses fica, "DOOM (2016)" não é "DOOM"; só jogos de loja escondem o Game Pass, uma ROM ou um jogo adicionado à mão não; jogo Xbox com produto abre a página da Microsoft Store antes da Steam; testes; aqui o Game Pass deixou de repetir 10 jogos que já são seus, de Dead Island 2 a The Elder Scrolls Online, conferidos um a um, e a página do Halo Wars 2 vem da Microsoft Store). — M
- [x] Página da loja: vazio, fonte e "abrir na loja" (L14). — P (27/11/2026: a página diz de onde vieram as informações, Steam ou Microsoft Store; sem página, um cartão "Sem página da loja para este jogo" com "Abrir na Epic" ou "Abrir na Microsoft Store"; teste; conferido no app com "3 out of 10, EP 1")
- [x] Big Picture e Início coerentes com a Biblioteca (L15). — M (27/11/2026: a Biblioteca da TV ganhou a aba "Não instalados", com os jogos da conta e do Game Pass, que A instala pela loja; o Início da TV, como o do desktop, mostra só o que está no PC, por decisão; o mesmo jogo em várias lojas é um card só, com o selo de cada loja, e a página tem um botão por loja ("Instalar · Xbox"); a página de um jogo Xbox ou Epic busca a descrição pela página da loja e as conquistas pela conta, como no desktop; testes; não conferido ao vivo na TV)
- [x] Instalar, desinstalar e verificar pela loja com o launcher ausente (L16; ver também o plano da Steam). — P/M (27/11/2026: sem a Epic no PC, Instalar abre a página de download dela e explica; nenhum botão abre um link com id vazio: um jogo Xbox sem produto abre a busca da Microsoft Store pelo nome; Gerenciar de um jogo de loja instalado tem "Desinstalar pela Steam/Epic" ou "pelo Windows" (Xbox, em Aplicativos instalados) e "Verificar arquivos" na Steam e na Epic, cada um dizendo o que abre; a loja pede confirmação; testes das URLs; conferido no app no 9 Kings, Steam, e no Age of Empires II, Xbox, sem clicar; 27/11/2026, pedido do usuário: Desinstalar também no menu "…" e no clique direito, para todos os jogos, que pede o segundo clique e desinstala ali mesmo, sem ir para outra página, com o motivo e a saída dentro do menu quando o launcher para; o do Gerenciar também confirma; e a biblioteca vigia as pastas de instalação da Steam, Epic e Xbox: um jogo instalado ou desinstalado pela loja aparece em segundos, sem apertar nada; conferido com Another Crab's Treasure, desinstalado pela Steam)
- [ ] Testes que faltam (L17). — M

### 8.2 Conquistas e tempo de jogo — [revisão completa](docs/roadmap/conquistas-e-tempo.md)

- [x] Sessão vencida aparece como "loja ocupada" (L4). — P/M (27/11/2026: token recusado da Epic ou da Microsoft e 401 do Xbox são marcados como "entre de novo": espera de 5 min, sem dobrar a espera da loja, e o hub diz onde entrar; teste; conferido que o hub online daqui segue sem problemas; a privacidade do Xbox, 403, continua genérica)
- [x] Data desconhecida aparece como 31/12/1969 (L5). — P (27/11/2026: format::unlocked_when diz "Desbloqueada" quando a data não veio, na lista do jogo, na visão geral e na TV; "Última conquista" e "Platinado" sem data não aparecem; teste; conferido que a tela de Conquistas daqui não mostra 1969)
- [x] Conquistas revogadas ou zeradas nunca saem do hub (L6). — M (27/11/2026: migração 3 com revoked_at em unlocks; quando Xbox, Epic, RetroAchievements ou a Web API da Steam mandam a lista inteira, o que estava desbloqueado e sumiu vira revogado, sai de Últimas conquistas e da contagem do jogo, que passa a ser a da lista, e aparece numa seção recolhida "Revogadas"; desbloqueado de novo, volta com a data primeira; as listas dos arquivos da Steam neste PC não revogam, porque podem estar atrasadas; teste; a cópia do banco daqui migrou sem perder linhas)
- [x] Listas guardadas sem idioma (L7). — P/M (27/11/2026: migração 2 com a coluna lang em lists; uma lista em outro idioma conta como desatualizada e é relida aos poucos, na fila normal; as de antes contam como do idioma atual; teste; uma cópia do banco daqui migrou para v2 sem perder linhas: 2531 desbloqueios, 299 listas, 571 jogos)
- [x] Steam e RetroAchievements quase não alimentam Próximas e Mais raras; página da Epic só lê o banco (L8). — M (27/11/2026, parte: as listas da Steam lidas do disco vão para o banco, 253 aqui, guardando a raridade já conhecida, e 3 jogos em andamento por atualização ganham a deles; Próximas daqui passou a ter 4 da Steam; 27/11/2026, RA: a cada sincronização, as listas inteiras, com raridade, dos 5 jogos com conquista nova desde a última leitura, a mais recente primeiro, marcadas pela última conquista, também ao abrir o jogo; teste; Epic: a página do jogo pergunta à Epic quando a lista guardada não existe ou é de antes da última vez jogado, guarda a resposta, e numa falha mostra a guardada; conferido no app, tirando do banco a lista de um jogo com 45 conquistas, que voltou da Epic em cerca de 1 s)
- [x] Estados por fonte: quando sincronizou, quando tenta de novo, forçar (L9). — M (27/11/2026, parte: cada cartão de plataforma diz "Atualizado há 8 min", ou "A loja pediu para esperar; tenta de novo às 14:20"; Atualizar consulta as contas de novo se a última vez foi há mais de 1 min, e respeita a espera pedida pela loja; teste; conferido no app: Xbox, Epic e RA passaram de "há 8 min" e "há 1 h" para "agora"; 27/11/2026, resto: quando a Biblioteca já está sincronizando uma conta, o hub não espera, responde com o que tem e o cartão diz "Atualizando pela Biblioteca"; a TV lê o que está no PC e depois atualiza as contas, mostra quando atualizou e os problemas, e A abre a página da Biblioteca também dos jogos da Epic; testes; a TV não conferida ao vivo; o Xbox na TV ainda abre só a lista de conquistas)
- [ ] ~~Concorrência no SQLite com escrita que some em silêncio (L10)~~ (27/11/2026: transações Immediate, erros registrados em vez de engolidos, account_times fora do runtime; teste com dois escritores, 2000 linhas); ~~esquema com versão (L11)~~ (27/11/2026: migrações em ordem contadas pelo user_version, aplicadas uma vez por execução; o banco daqui virou v1 sem perder nada: 571 jogos, 2531 desbloqueios). — P/M
- [ ] ~~Banco fora do backup (L12)~~ (27/11/2026: a cópia de dados leva o banco de conquistas, copiado com VACUUM INTO; restaurar junta, não troca: desbloqueios e jogos de um e de outro ficam, o tempo fica no maior, e as esperas seguem as deste PC; o banco daqui vai antes para before-restore; teste; a exportação real daqui leva o banco, 3,4 MB no total) e órfãos acumulando. — P/M
- [ ] Menos consultas por sincronização (L13; tabela de orçamento no documento). — M (27/11/2026, parte: Epic guarda as definições por jogo e idioma por 30 dias, inclusive "sem conquistas", e um jogo jogado de novo só pergunta o progresso; a sincronização pula os jogos nunca jogados, que a página lê quando aberta; o 429 ou 5xx do tempo jogado do Xbox vira recusa com espera em vez de ser engolido; a Steam guarda por 3 dias os apps sem nome na loja; falta o contador de consultas no trail e o teste com servidor falso)
- [ ] Aviso ao vivo e releitura ao fechar o jogo também para Xbox e Epic (L14). — M/G (27/11/2026, primeiro passo: quando um jogo Xbox ou Epic acompanhado fecha, 20 s depois só a lista dele é lida de novo, uma consulta, guardada como lista com autoridade; o que é novo vira aviso, com raridade e 100%, e a tela de Conquistas aberta se atualiza do banco; respeita a espera da loja; não conferido ao vivo, porque precisa jogar um jogo Xbox ou Epic; falta o aviso durante a partida)
- [ ] DLC, Xbox 360 e totais (L15). — P (27/11/2026, parte: o jogo guarda quando chegou a 100% pela primeira vez; um DLC que acrescenta conquistas não o tira de Platinados, que mostra "Platinado antes do DLC" e 50/55; estatísticas zeradas tiram a marca; na Epic, o jogo e seus DLCs com o mesmo namespace viram uma entrada só, a do jogo base; teste merge_keeps_first_platinum_when_total_grows; aqui os 4 platinados seguem os mesmos, agora marcados; falta o Xbox 360 com conta real e o setId da Epic)
- [ ] Testes que faltam (L16). — M

### 8.3 Emulação e saves dos emuladores — [revisão completa](docs/roadmap/emulacao-e-saves-dos-emuladores.md)

- [ ] RPCS3, Cemu e Xenia sem backup, e o cartão some sem explicar (L4); xemu: arquivos do console, HD e saves (L5). — M
- [ ] Um lugar só decide a pasta de cada emulador (L6). — P
- [ ] Rodízio com limite de espaço e nuvem por pasta (L7). — M
- [ ] Saves por jogo, não só a pasta inteira (L8). — M
- [ ] Atualizar Xenia, xemu e os outros emuladores sem perder nada (L9, depende do 8.0). — M
- [ ] Xenia e xemu: formatos, configuração, controles e conquistas (L10). — M
- [ ] Uma tabela só de consoles: nomes, siglas e imagens (L11). — P
- [ ] Imagens que faltam (Wii U, 32X, Dreamcast, Saturn, Switch, Sega CD, PC Engine, PS4 têm foto em domínio público)
      e licença das existentes (L12). — P
- [ ] Estados da UI e testes (L13). — M

## Onda 9: auditoria completa do app (25/11/2026)

Oito agentes mapearam cada área do app (como funciona, como os outros launchers fazem) e procuraram falhas lendo o
código. **178 falhas: 17 críticas.** Cada uma tem arquivo:linha, reprodução, correção e teste no documento da área.
Aqui elas estão agrupadas **pela causa**, para corrigir um lote inteiro de uma vez em vez de uma por uma.
Códigos: [B] biblioteca, [BP] big-picture, [C] clássicos-e-retroarch, [J] durante-o-jogo-e-app, [Fu]
fundações-do-backend, [P] partidas-tempo-e-saves, [RC] recomps-catálogo-e-instalação, [RP] recomps-preparação-e-ajustes.

| Área | Documento | Críticas | Altas | Médias | Baixas |
| --- | --- | --- | --- | --- | --- |
| Biblioteca | [auditoria-biblioteca.md](docs/roadmap/auditoria-biblioteca.md) | 2 | 2 | 9 | 7 |
| Big Picture | [auditoria-big-picture.md](docs/roadmap/auditoria-big-picture.md) | 0 | 4 | 6 | 9 |
| Clássicos e RetroArch | [auditoria-classicos-e-retroarch.md](docs/roadmap/auditoria-classicos-e-retroarch.md) | 1 | 4 | 13 | 8 |
| Durante o jogo e app | [auditoria-durante-o-jogo-e-app.md](docs/roadmap/auditoria-durante-o-jogo-e-app.md) | 2 | 3 | 9 | 11 |
| Fundações do backend | [auditoria-fundacoes-do-backend.md](docs/roadmap/auditoria-fundacoes-do-backend.md) | 4 | 3 | 4 | 6 |
| Partidas, tempo e saves | [auditoria-partidas-tempo-e-saves.md](docs/roadmap/auditoria-partidas-tempo-e-saves.md) | 4 | 2 | 12 | 8 |
| Recomps: catálogo e instalação | [auditoria-recomps-catalogo-e-instalacao.md](docs/roadmap/auditoria-recomps-catalogo-e-instalacao.md) | 3 | 4 | 9 | 7 |
| Recomps: preparação e ajustes | [auditoria-recomps-preparacao-e-ajustes.md](docs/roadmap/auditoria-recomps-preparacao-e-ajustes.md) | 1 | 4 | 11 | 6 |

### Lote A — Nenhum arquivo do jogador apagado por uma leitura que falhou (primeiro)

Causa única: arquivo ilegível (travado, corrompido, outra codificação) vira "vazio" e a próxima gravação apaga tudo.
Correção única: um módulo comum de estado (ler distinguindo "não existe" de "não consegui ler"; nunca gravar por cima
do que não leu; gravar com temporário próprio, `sync_all` e rename; trava por arquivo; `.bak` da última versão boa).

- [x] `settings.json` [Fu F1, J F1, J F14, Fu F6, lojas L1] — Feito em 25/11/2026 (`state_file.rs`): lido com volta à última cópia boa (`.bak`), o arquivo ruim guardado à parte, gravação com trava; `settings::update` e `renew_token` para quem grava depois de uma espera. Falta migrar as outras ~35 gravações síncronas para `update`.
- [x] favoritos, coleções, ocultos, notas, arte [Fu F2, B F1, B F2, B F5] — Feito em 25/11/2026 (`state_file.rs`): mudanças por `update` (uma por vez, nunca sobre um arquivo que não leu). Lista de mods também (25/11/2026).
- [x] tempo de jogo das lojas e dos clássicos [P F1, C F1] — Feito em 25/11/2026 (`state_file.rs`): dois clássicos fechando juntos somam os dois; arquivo ilegível nunca é trocado por uma sessão só.
- [x] configuração dos emuladores (`retroarch.cfg`, `.ini`) [Fu F3] — corrigido em 25/11/2026: arquivo que não se lê (em uso, UTF-16) nunca é regravado; gravação em um passo; o que não voltou ao fim da partida fica registrado e volta na próxima (C F10).
- [x] arquivos do próprio jogo nos Ajustes [RP F1] — feito em 25/11/2026: arquivo que existe e não se lê (UTF-16, JSON com comentário, em uso) nunca é regravado; JSON com BOM lido; cópia `.cerberus-orig` do original na primeira mudança. Testado.
- [x] HedgeModManager [RP F2] — feito em 25/11/2026: `ModsDB.ini`/`cpkredir.ini` ilegível ou travado é recusado sem mudança; os dois ganham cópia antes da primeira mudança; gravação inteira ou nada; mod atualizado: o novo entra antes de sair o velho. Testado.
- [x] Valores de INI com `#`/`;` [RP F13] — feito em 25/11/2026: comentário só quando `;`/`#` está sozinho entre espaços (`60 ; nota`); `D:\Jogos #2\zelda.iso` e `C:\a#b.iso` ficam inteiros na leitura e na gravação. Testado.
- [x] JSONL com linha cortada, gravações sem `sync_all` [P F10, P F11, Fu F11] — feito em 25/11/2026: `state_file::write_bytes` (temporário próprio, `sync_all`, rename) na biblioteca, `retroarch.cfg`, histórico, partidas e restauração; `append_line` fecha a linha que um travamento deixou aberta; `json_lines` lê linha por linha (bytes inválidos só perdem a própria linha). `retroarch.cfg` ilegível é recusado em vez de regravado só com as chaves do Cerberus. Testado; conferido no app (950 jogos e histórico lidos).
- [x] Caches com `fs::write` direto [Fu F12] (27/11/2026: os 25 caches e listas gravados direto (conquistas, Steam, catálogo, mods, ROMs, notícias, Wikipedia) passam pelo state_file: temporário e troca de nome, um corte não deixa arquivo pela metade); [ ]; ~~`sessions.jsonl` sem limite [P F10, resto]~~ (27/11/2026: passando de 1 MiB, fica com as 1000 partidas mais novas de cada jogo; o daqui tem 19 linhas, longe disso)

### Lote B — Nada se perde quando algo é interrompido ou o app fecha

- [x] Reinstalação de emulador interrompida apaga os saves guardados em `.<nome>-old` [Fu F4] (regressão do commit 3d05150) — corrigido em 25/11/2026: os arquivos do jogador nunca passam pela pasta temporária; `recover_swap` termina a troca interrompida antes de qualquer coisa. Testado nos dois pontos de interrupção.
- [x] Fechar o app (janela, bandeja, atualização, fim do jogo) com download, instalação, backup ou jogo de loja aberto [J F4, RC F7, P F19, Fu F10] — feito em 25/11/2026: `background::busy` (jogo, clássico, jogo de loja, download/instalação); fechar a janela esconde e fecha sozinho quando acaba (com aviso se for só download); Sair na bandeja pergunta; atualizar espera. Não exercitado no app (fecharia o app em uso).
- [x] Cerberus fechado no meio da partida; próximo Jogar restaura saves sob o jogo aberto [P F6] — a parte que perde saves feita em 25/11/2026: Jogar e restaurar backup conferem se um programa da pasta do jogo está rodando (mesmo aberto fora do Cerberus) e recusam. Falta adotar a partida que ficou aberta (sessions-open.json) para gravar última vez e fazer o backup do fim.
- [x] Cópia/extração pela metade conta como pronta [RP F3] — feito em 25/11/2026: `fill_folder` copia/extrai numa pasta ao lado e só dá o nome no fim; falha deixa a escolha anterior intacta. Teste de interrupção. Espaço livre conferido antes de copiar uma pasta (25/11/2026).
- [x] Instalação nova parcial não é notada [RC F11] — feito em 25/11/2026: a primeira instalação é extraída em `<id>.cerberus-part` e só ganha o nome do jogo (com marcador e registro do que a versão trouxe) quando completa; sobras `.cerberus-part` e `.staging-*` somem ao abrir o app. Testado (instalação ponta a ponta e varredura).
- [x] Mover entre discos apaga a pasta antiga antes de gravar a biblioteca [RC F6] — corrigido em 25/11/2026: a biblioteca é gravada antes de apagar a antiga; se não gravar, a mudança é desfeita. Testado com a biblioteca bloqueada.
- [x] Backup do DLSS interrompido vira o "original"; mod atualizado apaga o antigo antes do novo [RP F9, RP F10] — feito em 25/11/2026: DLL guardada por cópia conferida (tamanho e versão) e nunca restaurada danificada; mods do Thunderstore: o novo entra antes de sair o velho; registro de mods pelo state_file. HedgeModManager feito em 25/11/2026 (o novo entra antes de sair o velho).
- [x] Preparação órfã, duas em paralelo [RP F12] — feito em 25/11/2026: a preparação roda num Job Object com KILL_ON_JOB_CLOSE (fecha com o Cerberus, filhos incluídos; o tempo esgotado encerra a árvore toda); `.recomplauncher-tmp` só é apagada com o marcador `.made-by-cerberus`; marcador de "pronto" que não grava vai para o histórico e um toast. Testado.
- [x] Restos `.part`/`.tmp` dos saves e da nuvem [P F23] — feito em 25/11/2026: ao abrir, `.part` e `.tmp*` com mais de 1 dia somem de `saves/` e da pasta da nuvem (os recentes ficam: outro PC pode estar gravando). Testado. Zips ilegíveis aparecem na lista como "Danificado", sem o botão de restaurar, e ficam fora da sincronização e do rodízio (26/11/2026).
- [x] Importar dados desfeito pelo cache na memória [Fu F5, P F5, P F17] — feito em 25/11/2026: a cópia é conferida e preparada à parte e trocada no próximo início, antes de qualquer leitura; recusada com jogo/download em andamento; guardar os dados atuais é obrigatório; tempo de jogo mesclado (maior por jogo); entradas acima de 256 MiB recusadas; só as 3 últimas before-restore ficam. Conferido com os dados reais.

### Lote C — Saves e nuvem sem confiar no relógio

- [x] Nuvem escolhe o "mais novo" pelo relógio; dois PCs se sobrescrevem [P F2] — feito em 25/11/2026: âncora por jogo e pasta de nuvem (`sync.json`), gravada só quando o envio termina ou ao trazer; só a nuvem mudou → traz; só aqui → envia depois; os dois → conflito, nada trocado, a cópia do outro PC fica em Backups. A primeira sincronização de um PC ainda usa a data (sem histórico). Falta o cartão de escolha na tela. (desenho de âncora em `saves-de-jogos-de-pc.md`)
- [x] Rodízio apaga o backup errado e as cópias de segurança [P F3] — feito em 25/11/2026: datas sempre crescentes, 10 regulares + 3 por motivo de segurança, o recém-feito nunca sai. Testado.
- [x] Restaurar não é atômico; restaurar com o jogo aberto por fora [P F13, P F14, P F16] — feito em 25/11/2026: cada save vivo é posto de lado antes da troca e, se um não puder ser trocado (outro programa o segura), os já trocados voltam: "Nada foi mudado"; 3 tentativas para quem segura por um instante. Backup manual recusado com o jogo aberto por fora (a restauração já recusava desde 6406731). Falha do backup "Restaurado" (o que vai para a nuvem) vira aviso no toast e no histórico. Testado com um save travado.
- [x] Cada Jogar abre os 20 zips da nuvem (OneDrive sob demanda) [P F12] — feito em 25/11/2026: o manifesto de cada backup é lido uma vez e guardado neste PC (`cache/backup-manifests.json`, por nome, tamanho e data; nada na pasta compartilhada); listar de novo não abre nenhum zip. Testado; conferido no app (2ª listagem em 2 ms). Limite de tempo (26/11/2026): a pasta da nuvem tem 10 s para responder, numa thread à parte que só lista; sem resposta, o jogo abre com os saves deste PC, com aviso e registro no histórico. Falta pedir confirmação antes do envio seguinte quando a conferência foi pulada.
- [x] UI que diz "na nuvem" sem saber [P F15] — feito em 25/11/2026: cada backup vem com `in_cloud` (conferido na pasta) e `waiting` (marca de envio); a linha do jogo diz "Na pasta da nuvem", "Esperando a pasta da nuvem" ou "Só neste PC"; "Tudo enviado" virou "Tudo copiado para a pasta", com a explicação de que o app da pasta leva para a internet. Testado. Mantida a decisão existente de que uma cópia esperando uma pasta não vai para outra escolhida depois; falta mostrar essas cópias no contador.
- [x] Reinstalar um recomp zera o tempo de jogo [P F4] — feito em 25/11/2026: a biblioteca guarda o tempo e o último dia de um jogo removido (`played-kept.json`) e devolve quando ele volta. Testado.

### Lote D — Instalar, atualizar e reparar recomps sem estragar a pasta do jogador

- [x] **Desinstalar um recomp tem que funcionar sempre, e fácil** — feito em 25/11/2026 (commit desta data):
      Desinstalar… no botão direito da capa (abre Gerenciar já na Zona de perigo), página de outra versão aponta para
      a instalada, backup que falha pergunta "Desinstalar sem backup dos saves", pasta que o Cerberus não fez oferece
      "Tirar da biblioteca e manter a pasta", nome temporário único por tentativa e sobras limpas. Falta: entrada do
      catálogo guardada na instalação [RC F4] e conferir se a sessão "aberta" ainda existe (item 5).
      Texto original do item: (relatado pelo jogador em 25/11/2026: não
      conseguiu desinstalar; **o botão nem aparece para ele**).
      **Por que o botão não aparece** (`ui/src/game.rs`, `Manage`):
      a. Ele só existe no fim da aba Gerenciar, em "Zona de perigo" (`{danger_h}{remove}`), abaixo de tudo.
      b. O menu do botão direito na capa (`ui/src/game_menu.rs`) não tem Desinstalar; o Big Picture tem (`tv/sheets.rs`).
      c. Só aparece quando a página tem o jogo da Biblioteca (`editable` exige `game`). Aberta pelo Descobrir ou numa
         versão diferente da instalada (id antigo, outra versão do mesmo jogo [RC F8]), `game` é `None` e não há botão.
      **Correção da tela:** Desinstalar no menu do botão direito e no menu "…" ao lado de Jogar; na página de qualquer
      versão, achar a instalação pelo jogo (todas as versões e ids antigos) e oferecer Desinstalar.
      **Por que, quando aparece, pode falhar** — `remove_game` (`src-tauri/src/main.rs`) para tudo no primeiro obstáculo:
      1. `recomp::backup_saves(...)?` falha → nada é removido. Falha quando o recomp saiu do catálogo ou o catálogo do
         site foi desligado (`entry_at` → "Esse recomp não está no catálogo.") [RC F4], ou quando a cópia dos saves dá
         erro. **Correção:** backup com a entrada guardada na instalação (`.cerberus-entry.json`); se ainda assim falhar,
         perguntar "Desinstalar sem backup dos saves?" em vez de travar.
      2. `check_install_folder` recusa pasta sem o marcador (copiada, adotada, marcador apagado). **Correção:** oferecer
         "Tirar da Biblioteca e manter a pasta" ou "Apagar a pasta" com o caminho à vista e confirmação.
      3. Arquivo em uso (Explorer, antivírus, o jogo) trava a renomeação. **Correção:** dizer qual programa/arquivo e
         "Tentar de novo"; nunca deixar meio apagado.
      4. Sobra `.cerberus-removing-<id>` de uma tentativa anterior faz as próximas falharem até reiniciar [RC F20].
         **Correção:** nome único por tentativa e limpeza das sobras.
      5. Sessão marcada como aberta por engano ("Feche o jogo antes de removê-lo."). **Correção:** conferir se o
         processo existe de verdade antes de recusar.
      **Aceite:** com cada um dos 5 casos reproduzido em pasta temporária, Desinstalar termina (ou pergunta e termina),
      o jogo sai da Biblioteca e a mensagem diz o que aconteceu. Conferir no app com um recomp real de teste.

- [x] Instalação nova adota pasta alheia com o mesmo nome e depois a apaga inteira [RC F1] — feito em 25/11/2026: pasta existente sem a nossa marca nunca é usada; a instalação vai para `<id>-2`, `<id>-3`... Testado. (Uma instalação nossa interrompida antes da marca também vai para a próxima livre; limpar fica com RC F11.)
- [x] Reparar/Atualizar sobrescrevem configurações e mods [RC F2] — feito em 25/11/2026: `.cerberus-files.json` com o hash do que cada versão trouxe; arquivo que o jogador mudou fica, o da versão nova vai para `<nome>.cerberus-new`. Vale a partir da próxima instalação/atualização (as antigas não têm o registro). Testado.; arquivos removidos pela versão nova ficam [RC F12]
- [x] Reparar falha quando a pasta sumiu [RC F5] — feito em 25/11/2026: pasta que não existe mais é instalada de novo no mesmo lugar, com o mesmo jogo na biblioteca (tempo e cópia ficam). Teste ponta a ponta com servidor local.
- [x] Recomp que saiu do catálogo não desinstala [RC F4] — feito em 25/11/2026: cada instalação guarda `.cerberus-entry.json` (nome, saves, repo, tipo, programa) e usa essa cópia quando o catálogo não tem mais o recomp; backup, desinstalar e a página seguem funcionando. As instalações antigas ganham a cópia ao abrir o app (conferido: os 9 recomps instalados).
- [x] Versão comparada como texto; ids antigos [RC F8, RC F9] — feito em 25/11/2026: `cerberus_shared::is_update` compara pelos números (e pelo que vem depois deles, como "R3" → "R4"); sem números, só se diferente; a `RecompEntry` leva os ids antigos (`aliases`) e a tela usa `is()`/`update_to()` em Downloads, selos, página do jogo, TV e novidades. Testado; conferido nos 9 recomps instalados (só o MHGU, 0.20.0 → 0.20.3, é atualização).
- [x] Checksum velho do catálogo em loop [RC F10] — feito em 25/11/2026: o download que não bate fica de lado (`.unverified`, nunca com o nome real); se o checksum era o do catálogo e o servidor do projeto dá ao arquivo o checksum que ele tem, é usado sem baixar de novo; senão, nada é instalado e a mensagem diz que o catálogo pode estar desatualizado. Testado.
- [x] `.cue`/`.gdi`/`.m3u` sem as faixas [RP F4] — feito em 25/11/2026: as faixas da folha vão junto, com os nomes delas (só arquivos ao lado da folha). Testado; conferido com o .cue real do Crash Bandicoot. Falta o selo de verificado por faixa.
- [x] CHD de PS1 Mode 2 Form 2 [RP F15] — feito em 26/11/2026: ao converter um CHD de CD para ISO, cada setor Mode 2 tem o submode conferido; um setor Form 2 (vídeo STR, áudio XA) ou uma faixa de áudio recusa a conversão e pede BIN/CUE, em vez de gerar uma ISO com vídeo ou som faltando. A leitura que só identifica o console continua igual. Testado (sem CHD real neste PC).
- [x] Instalador que pede administrador (erro 740) [RP F5] — feito em 25/11/2026: no 740 o instalador abre pelo pedido de permissão do Windows e o fim é aguardado; recusar explica o que fazer. Vale para recomps e runtimes da Microsoft. Testado o mapeamento; não exercitado com um instalador real (abriria o pedido do Windows).
- [x] Programa de outro jogo adotado [RP F6] — feito em 25/11/2026: pelo registro do Windows só vale um nome com todas as palavras do recomp, nunca numa pasta da Steam/Epic/Xbox/GOG nem na de outro jogo da biblioteca; na dúvida, o jogador escolhe o programa. Testado. Falta o "fechou cedo demais" (bootstrapper).
- [x] Mods mexem em arquivos que não são deles [RP F11] — feito em 25/11/2026: instalar recusa um nome que já está na pasta e não é deste mod (de outro mod ou do jogador), antes de gravar qualquer coisa; remover deixa um arquivo que outro mod também lista; vale para Thunderstore e pacotes do GameBanana. Testado.
- [x] Filtro aplicado no programa errado [RP F7] — feito em 25/11/2026: a partida põe o ReShade ao lado do mesmo programa que a aba Ajustes avalia (o acompanhado, quando um launcher abre o jogo); retirar a DLL tenta por ~3 s; antes de cada partida sem filtro, um que sobrou é retirado. Testado (DLL presa por 600 ms); não exercitado com o ReStreet.
- [x] Disco cheio e rede caída [RC F14] — feito em 26/11/2026: disco cheio no download diz qual disco encheu e que o baixado ficou guardado para continuar; uma queda de rede espera 2, 4, 8, 16, 30 e 30 s antes de desistir (~1,5 min). Conversão para ISO e cópias de pasta conferem o espaço antes. Testado.
- [x] Fila de downloads no backend, persistida [RC F13] — feito em 26/11/2026: `downloads.json` guarda o estado de cada download quando ele muda (não a cada porcentagem); ao abrir, os pausados e os que não terminaram voltam para Downloads (um que estava baixando volta pausado); um download por vez, os outros "Na fila" (pausar ou cancelar na fila funciona); cancelar e dispensar tiram da lista. Testado; conferido no app que a página abre e a fila é lida.
- [x] Cópias grandes com progresso e cancelamento [RP F14] — feito em 26/11/2026: ao escolher a cópia do jogo, a página mostra "Copiando o jogo" com a barra e os GB copiados, e um botão Cancelar; as cópias de arquivo, de pasta, a extração de ISO/XISO e a conversão para ISO contam o que gravam e param no bloco seguinte ao Cancelar (só a cópia em andamento: um Cancelar antigo nunca para a próxima). Testado; não exercitado com uma cópia real (pede o seletor de arquivo).
- [x] Reordenar a fila de downloads — feito em 27/11/2026: a fila tem ordem (quem chegou primeiro baixa primeiro, e um download novo não fura a fila); "Baixar primeiro" num que espera o põe logo depois do de agora. Teste da ordem; não testado com downloads de verdade.

### Lote E — Segurança

- [x] Catálogo de terceiros manda no repositório e no checksum do catálogo próprio [RC F3] — feito em 25/11/2026: para os jogos do catálogo próprio, repositório, servidor, versão, arquivos/checksum, sistemas, programa e tipo vêm só dele; o site completa texto e arte. Testado.
- [ ] Assinar o `recomps.json` do catálogo próprio (minisign/ed25519) e recusar cópia sem assinatura [RC F3, 2ª parte]
- [x] Autoatualização às vezes sem hash; instalador no nome final [J F2, RC F15, Fu F15] — feito em 25/11/2026: checksum obrigatório (sem ele, não instala), download em `.part` renomeado só depois de conferido. Conferido com a release real do GitHub.
- [ ] Autoatualização: exigir assinatura (Authenticode ou minisign) quando as releases forem assinadas [J F2]
- [x] Núcleos e ReShade gravados pela metade [C F23, RP F8 parte] — feito em 25/11/2026: download lido com limite (256 MB); núcleo do RetroArch e as DLLs do ReShade gravados inteiros ou nada (`write_bytes`), com a versão do ReShade anotada por último (um download interrompido nunca conta como pronto).
- [x] Executáveis de terceiros sem hash nem assinatura [RP F8] — feito em 26/11/2026: ReShade fixo na 6.8.0 com o SHA-256 do instalador no código (uma versão nova vem por PR; a assinatura do ReShade não é de uma raiz que o Windows confia, então o checksum é a conferência); instaladores da Microsoft (Visual C++, DirectX) só rodam com assinatura Authenticode válida da "Microsoft Corporation" (`authenticode.rs`, WinVerifyTrust) e são lidos com limite e gravados inteiros. Conferido com os arquivos reais: o ReShade bate e rende as duas DLLs, o dxwebsetup é da Microsoft Corporation.
- [ ] Núcleos do RetroArch conferidos por tamanho/ETag do buildbot [C F23, resto]
- [x] Trocar/desconectar conta do RetroAchievements deixa o token antigo no RetroArch, PCSX2 e Dolphin [C F2, C F3] — feito em 25/11/2026: trocar de conta apaga o token anterior; desconectar e trocar tiram o token do Cerberus do RetroArch, PCSX2, Dolphin e DuckStation (o do jogador fica). Testado.
- [x] Respostas sem limite e redirecionamento para http [Fu F8]; `..` nos lugares de saves do manifesto [Fu F9] — feito em 25/11/2026: `net::read_limited` lê em streaming até o limite (mods do Thunderstore e arte); o `Client` segue redirecionamentos mas nunca de https para http; um lugar de saves com `..`, raiz ou unidade é recusado, e `{home}`/`{documents}`/`{appdata}`/`{local}`/`{config}` sem subpasta também. Testado; conferido que nenhum lugar do catálogo e dos 3 manifestos instalados é recusado. Falta aplicar o limite nos núcleos e na lista do Thunderstore.
- [x] Nomes reservados do Windows vindos do disco [RP F17]; 7z/tar sem teste de path traversal [Fu F17] — feito em 25/11/2026: `disk::plain_name`/`safe_path` recusam `CON`/`NUL`/`COM1`…, `:`, ponto ou espaço no fim, `..`, raiz e unidade; usados no ISO9660, XDVDFS, 7z (todos os nomes conferidos antes de extrair) e tar (entrada por entrada; links só para um caminho seguro). Testado.
- [x] Sem CSP [J F25, Fu F13, 1ª parte] — feito em 25/11/2026: CSP ligada (`default-src 'self'`, scripts só do app + `wasm-unsafe-eval`, imagens/mídia https/asset/data/blob, IPC); o player de trailers deixou de montar código a partir de texto (`vendor/hls-play.js`); imagens `http://` das lojas (Xbox store-images) pedidas por https (`net::secure`/`secure_all`); teste que falha com permissão `*`, capability remota ou CSP desligada. Validado na versão compilada: script injetado bloqueado, as 7 telas sem violação, trailer do Portal 2 tocando.
- [x] Overlay e aviso com as permissões da janela principal [J F25, Fu F13, 2ª parte] — feito em 25/11/2026: `build.rs` lê o `generate_handler!` e dá a cada comando uma permissão `allow-*` (AppManifest), com o conjunto `main-window` só para a janela principal; o menu sobre o jogo só tem os `overlay_*` e o aviso só o `unlock_next`. Conferido no app: na janela do aviso `list_games`, `import_data`, `get_settings` e `overlay_state` são recusados e `unlock_next` passa; a principal segue com tudo.
- [ ] Fontes no formato RecompLauncher de qualquer servidor [RC F16] — não feito: as mesmas fontes trazem as listas de downloads diretos (Hydra), área em que não trabalho; restringir só uma parte não fecha a brecha.

### Lote F — Panics e travamentos da interface

- [x] Sinais descartados lidos em timers e depois de `await` [BP F1, BP F8, J F5] — feito em 25/11/2026: `try_` no fundo da TV e depois de desinstalar; `on_event_here` desliga o ouvinte quando a tela sai (Fotos, progresso dos emuladores). Conferido no app: 4 visitas e evento depois de sair, sem erro.
- [x] Comandos síncronos pesados na thread principal [Fu F7, B F12] — feito em 25/11/2026: 75 comandos que tocam disco, registro ou processos viraram `#[tauri::command(async)]`; ficam síncronos só os de memória, janela e bandeja. Conferido no app.
- [ ] (27/11/2026, parte: a montagem da lista de jogos, que lê arquivos e o banco de conquistas, roda numa thread de bloqueio; os outros comandos de disco já rodam fora do runtime) I/O bloqueante em async [Fu F16]; guardar em memória o tamanho da arte e as pendências da nuvem do `settings_view`
- [x] `list_games` refeito a cada clique e em rajadas [B F4, passos 1 e 3] — feito em 25/11/2026: favoritar, ocultar e coleção mudam só o jogo na tela (`Library::patch_game`); vários `catalog-changed` viram uma leitura 300 ms depois do último. Medido: `list_games` ~310 ms com 950 jogos, agora 0 chamadas ao favoritar.
- [x] Cache da montagem no backend [B F4, passo 2] — feito em 25/11/2026: os jogos das lojas ficam guardados e são lidos de novo quando chega `catalog-changed` ou depois de 20 s; a arte baixada é conferida com uma listagem da pasta em vez de ~1.900 consultas; só os recomps da lista são cruzados com as ROMs. Medido no app (build de desenvolvimento, 950 jogos): ~300 ms → ~75–105 ms quente; aviso de loja refaz (228 ms).
- [ ] (27/11/2026, parte: as cópias do jogo nas pastas de ROMs, cruzadas com o catálogo inteiro, ficam guardadas até as ROMs ou o catálogo mudarem, e o tamanho do cache de arte até a pasta mudar: Configurações abre em 4 ms em vez de 30; a lista das 2.135 imagens do cache é guardada e só relida quando a pasta muda, e a conferência das capas próprias olha o nome antes do disco; a lista de jogos com o cache quente caiu de 84 para uns 69 ms, com as mesmas capas: 620 do cache, 323 locais, 326 da rede) `settings::load` uma vez, arte da Steam por um `read_dir`, `owned-<id>.json` uma vez [B F4, passos 4 a 6]; arte pedida de novo [B F6]
- [x] Varredura de ROMs que baixa arquivos só na nuvem [C F4] — feito em 25/11/2026: arquivo marcado pelo Windows como só na nuvem é reconhecido pelo nome e pela pasta, sem ser aberto. Testado; conferido no app (os 5 clássicos continuam).
- [x] Varredura sem cache e que cobre pastas demais [C F7, C F8, C F18] — feito em 25/11/2026: `rom-cache.json` guarda o que cada arquivo tinha (tamanho e data), arquivo sem mudança não é aberto de novo; pastas e varredura fora do executor async; só a varredura mais recente publica; Downloads, Documentos, Área de Trabalho, Imagens/Vídeos/Músicas e a raiz do OneDrive não entram como "última pasta" de emulador. Testado; conferido no app (cache com 6 arquivos, os 5 clássicos iguais). Falta progresso/cancelamento.
- [x] Biblioteca da TV desenha tudo [BP F5] — feito em 25/11/2026 (ff4464d): 48 capas e cresce até onde o foco vai; conferido no app.
- [x] Menu escondido lendo o controle a 30 Hz [J F10] — resolvido junto com BP F3 (2c3474b): a leitura só acontece com a janela em foco, e a janela escondida nunca tem; A/B não agem no menu escondido. Fica só o timer, que pergunta pelo foco e volta.

### Lote G — Funciona como promete (por área)

- Biblioteca: ~~DLCs na página do jogo~~ (27/11/2026, pedido do usuário: as DLCs instaladas aparecem num cartão na página do jogo, na Steam pelos dlcappid do appmanifest, com o nome da loja guardado, e no Xbox pelos pacotes que dizem a que jogo pertencem; conferido: Age of Empires II com 3, Dark Souls II com 3, Vampire Survivors com 6 e outros; as DLCs que são só licença, sem arquivos, a Steam não deixa no disco), ~~B F3~~ (25/11/2026: `library.json` ilegível tira só os jogos dele; as lojas continuam e a Biblioteca explica), ~~B F8, F9, F10~~ (26/11/2026: ocultos fora do "Adicionar todos à Steam" e do destaque do Início; anotação gravada 800 ms depois de digitar e mantida quando a página é refeita, conferido no app), ~~B F7~~ (26/11/2026: o `header.jpg` da Steam é a última capa a tentar; na sua Biblioteca, de 6 capas vazias sobrou 1, que não tem arte nenhuma na Steam), ~~B F11, F13, F14, F17~~ (26/11/2026: setas do menu do jogo passam pelas coleções e não tiram as teclas da caixa de nome, Esc ali fecha só a caixa; arte própria e capa escolhida guardadas pelo nome e achadas na pasta de capas atual depois de restaurar em outro PC; "Sua biblioteca está vazia" só sem clássicos; decidido que remover um jogo mantém favorito, coleções, notas e arte própria, como já mantém o tempo de jogo, para quem o adicionar de novo), ~~B F18~~ (26/11/2026: nota como grupo de rádio "N de 5 estrelas" com a marcada; favorito anunciado no cartão; abas da Biblioteca com setas e foco itinerante; busca com rótulo; Esc fecha o sorteio e o "nada para sortear" é um diálogo; conferido no app: setas, rótulos, estrelas e Esc). ~~B F16 (parte)~~ (26/11/2026: adicionar um .exe que já está na biblioteca abre o jogo existente com "já está na biblioteca", sem mostrar dados que não foram gravados; um .exe dentro da pasta de um jogo instalado pela Steam, Epic ou Xbox é recusado com o nome da loja; falta "Localizar programa" que mantém o id e o Game Pass pelo produto). ~~B F19 (parte)~~ (26/11/2026: um jogo do Xbox que a Steam não conhecia guarda a data e é procurado de novo depois de 30 dias, 40 por vez; as faltas antigas sem data entram aos poucos; conferido no app: 120 das 167 faltas daqui já com data). ~~B F15~~ (26/11/2026: nomes de outros programas fora de UTF-8 voltam byte a byte; shortcuts.vdf.cerberus-original.bak feito uma vez só, além do de rodízio; readicionar um jogo renomeado apaga a arte do número antigo; o histórico diz quando nenhuma arte foi copiada; conferido: o shortcuts.vdf deste PC lido e regravado dá os mesmos 558 bytes). ~~B F19~~ (26/11/2026: imagens que não baixam vão para o histórico, em "Capas e imagens", uma linha por site a cada execução, com quantas e o motivo em português; conferido no app: "12 imagens de shared.akamai.steamstatic.com não baixaram (o site respondeu 404)"). ~~B F16 (localizar)~~ (26/11/2026: "Localizar programa" já mantinha o id, com favorito, nota e tempo; agora a pasta do jogo adicionado à mão também acompanha o novo lugar; fica de fora o Game Pass chaveado pelo produto, que muda o id de jogos que já existem e precisa de migração). ~~B F20 (parte)~~ (26/11/2026: testes das abas e dos chips de loja da Biblioteca (oculto, favorito, coleção, recomps, PC, release com vários jogos) e de que um jogo da conta Steam já instalado não aparece de novo como não instalado, nem repetido; o shortcuts.vdf real já tem teste de ida e volta; falta separar a montagem inteira de list_games num assemble testável). ~~B F20~~ (27/11/2026: a montagem da lista virou assemble, testada: ordem pelo nome, cada jogo uma vez, favorito, imagem em https e o tempo da Steam em cada jogo; conferido no app: 949 jogos únicos, em ordem, favoritos iguais ao favorites.json, nenhuma imagem http). Falta: F16 (Game Pass chaveado pelo produto, precisa de migração dos ids).
- Clássicos: ~~C F5~~ (25/11/2026: `.zip` pelo cartucho dentro, `.7z` pela pasta), ~~C F6, F9, F12~~ (26/11/2026: pasta fora do ar mantém os clássicos dela na lista; `.md`/`.gen` só com a assinatura SEGA, então um README não vira jogo; antes de uma partida o RetroArch não recebe a conta se estiver em outra ou com conquistas desligadas, só quando o jogador liga em Configurações), ~~C F10, F11~~ (26/11/2026: as mudanças de uma sessão guardam o processo do emulador; ao abrir, elas só voltam depois que aquele emulador fechar, em vez de serem desfeitas por baixo dele e regravadas por ele; um segundo clássico ao mesmo tempo é recusado com aviso), ~~C F15~~ (26/11/2026: no RetroArch, o save feito pelo disco 1 é copiado para o nome da lista de discos, sem apagar nem sobrescrever), ~~C F14~~ (26/11/2026: nos emuladores que não trocam disco por lista (PCSX2 e outros), cada disco tem o próprio Jogar, e o texto diz isso em vez de prometer a troca pelo menu), ~~C F13~~ (26/11/2026: a busca guarda se achou pelo hash ou pelo título, com o tamanho e a data do arquivo; "não achou" vale 30 dias; a página do clássico avisa quando foi pelo nome; conferido: N64 pelo hash, PS1 e PS2 pelo título), ~~C F16~~ (26/11/2026: um clássico que muda de caminho com o mesmo nome e console, e um só candidato, leva o tempo, as partidas, a nota e o favorito), ~~C F17~~ (26/11/2026: núcleos e arquivos de sistema vão para as pastas que o RetroArch usa, `libretro_directory` e `system_directory`, como a BIOS; conferido que o N64 continua pronto), ~~C F19~~ (26/11/2026: região lida de todas as etiquetas No-Intro, "(Japan, USA)" vira "US, Japan", e World, Brazil, Korea, Asia passam a contar; N64 B é Brasil; a página mostra o nome traduzido; os clássicos daqui relidos uma vez, FIFA Street agora US e Europe). ~~C F20~~ (26/11/2026: discos cada um na sua pasta com o mesmo nome, "FF7 (Disc 1)\FF7 (Disc 1).chd", se juntam; "(Disc Edition) (Disc 2)" é lido pelo segundo). ~~C F21~~ (26/11/2026: BIOS pelo conteúdo além do tamanho: a do PS1 traz "Sony Computer Entertainment", a do PS2 o ROMDIR com RESET; um jogo de 512 KB não é mais importado como BIOS; os sinais conferidos nas 5 BIOS deste PC, PS1 SCPH1001 e PS2 japonesa, chinesa, americana e europeia). ~~C F26~~ (26/11/2026: as conquistas do clássico usam o console lido do arquivo, como o Jogar; o da página só quando o arquivo não diz). ~~C F25~~ (26/11/2026: o Big Picture mostra também os clássicos sem emulador pronto, com o selo "Falta o emulador", e A diz o que falta; conferido com o Monster Hunter Generations, que antes sumia). ~~C F22~~ (26/11/2026: "Procurar de novo" e adicionar/remover pasta dizem quando a busca falhou ou parou nos 300 mil arquivos, em vez de "Pastas verificadas de novo."; os achados ficam na lista; as edições do RetroArch que não se gravam já não entravam no arquivo pendente; conferido no app: as 5 ROMs daqui sem aviso falso). ~~C F24~~ (26/11/2026: durante a sessão, "Fechar conteúdo" encerra o RetroArch (quit_on_close_content, desfeito no fim como os outros ajustes de sessão), então o tempo para com o jogo; conferido com uma cópia do retroarch.cfg daqui: 10 ajustes na sessão, arquivo idêntico depois). Clássicos sem pendências da auditoria.
- Durante o jogo e app: ~~J F3~~ (25/11/2026: fechar a janela de vez encerra o app, com as janelas escondidas do menu e do aviso), ~~J F9~~ (27/11/2026: "Atalhos do controle" separado do teclado, ligado, e "Segurar View abre o menu" separado e desligado por padrão, porque muitos jogos seguram Select para o mapa; teste da combinação com a opção desligada; conferido no app: os dois aparecem em Durante o jogo com os valores padrão; e o Ctrl+F12 registrado com RegisterHotKey só enquanto um jogo do Cerberus roda, o que dá ao menu o direito de vir para a frente com o teclado; se outro programa já o tem, volta a leitura de antes; teste com o Windows de verdade; não testado com um jogo aberto), ~~J F22~~ (27/11/2026: a atualização só fecha o Cerberus depois de ver o script de troca rodando (um arquivo-sinal em até 5 s); com o PowerShell bloqueado, o Cerberus fica aberto e diz onde está o instalador; teste com um programa que sai sem rodar o script), ~~J F12 (atalhos)~~ (27/11/2026: a lista "?" ganhou "Durante o jogo": F12, Ctrl+F12, View+Menu ou segurar View, View+RB; conferida no app, cabe na janela), ~~J F12 (Restaurar)~~ (26/11/2026: com a janela maximizada o botão diz "Restaurar", no rótulo e na dica; conferido que diz "Maximizar" com ela normal; maximizada não testado, para não mexer na janela), ~~J F19~~ (26/11/2026: a janela nasce invisível e só aparece no fim da preparação, já no monitor escolhido, e não aparece ao começar escondido com o Windows; conferido que ela abre normal; o começo com o Windows não testado), ~~J F17 (troca)~~ (26/11/2026: trocar o idioma recarrega também o menu por cima do jogo e o aviso de conquista, se abertos, e a escolha é guardada antes, para eles já a lerem; fica de fora o automático da interface seguir o do Windows pelo backend; não trocado no app, para não mudar a sua configuração), ~~J F16~~ (26/11/2026: a foto do F12 deixa de fora o aviso de conquista do Cerberus, só durante a captura (streams e gravações continuam vendo); o PNG é gravado fora do laço do controle; a mensagem de foto que não saiu cita também a tela preta de carregamento; não testado com um jogo aberto), ~~J F23~~ (26/11/2026: "Fechar o jogo" no menu por cima do jogo confere depois de 6 s se a janela continua lá e, se sim, avisa que o jogo não fechou e como fechar; sem "forçar", que perderia o que não foi salvo; não testado com um jogo real), ~~J F20~~ (26/11/2026: uma bandeja que falha ao abrir, com o Windows ainda subindo, não impede o app de abrir e é tentada de novo a cada 5 s; sem bandeja, fechar a janela não a esconde sem caminho de volta; conferido que a bandeja sobe sem erro aqui; a falha não foi simulada), ~~J F15, F18, F21, F24~~ (26/11/2026: com dois jogos abertos, fechar o primeiro não traz o Cerberus por cima do outro, a janela volta quando o último fecha; textos que estavam fora da tradução ("livres neste disco", "(principal)", "arquivos copiados", "Monitor"); uma versão de teste instalada recebe a estável dela e as seguintes; a lista de monitores mostra a escolha nova na hora e avisa; testes; a troca de monitor não testada, há um monitor só aqui), ~~J F7 (aviso)~~ (26/11/2026: com um jogo em tela cheia exclusiva, onde nada aparece por cima, o aviso de conquista fica guardado e sai quando o jogo deixa a tela, um só ou um resumo "N conquistas durante a partida"; falta o mesmo para as respostas de F12/Ctrl+F12), ~~J F12 (parte)~~ (26/11/2026: os diálogos de atualização e "O que há de novo" fecham com Esc, dão o foco ao botão principal e anunciam o título; falta o rótulo "Restaurar" no botão de maximizar e os atalhos de jogo na lista do "?"), ~~J F8~~ (26/11/2026: vigia da fila de avisos: janela que fica 10 s sem pedir o próximo com avisos esperando é recriada, e na segunda vez os avisos viram notificação do Windows; um aviso que chega enquanto a janela recebe "não há mais" é pedido de novo), ~~J F14~~ (26/11/2026: os 35 pontos do main.rs que liam, mudavam e gravavam o settings.json passaram para settings::update, com trava; um teste falha se aparecer um settings::save direto; a restauração do backup é a única exceção, de propósito), ~~J F6, F9 (parte 1), F11, F13~~ (26/11/2026: o pedido de jogar de um atalho fica guardado até a janela pegar, uma vez; Alt+Tab esconde o menu sobre o jogo sem puxar o jogo de volta; texto do aviso no 6.1 corrigido; Enter num botão da primeira configuração faz só o que o botão diz), F7, F8, F9 (resto), F12, F15–F24.
- Big Picture: ~~BP F2, BP F3~~ (25/11/2026: botões só contam depois de vistos soltos, Menu precisa ser solto depois de sair, F11 sem repetição, controle só com o Cerberus em foco; o controle real ainda precisa ser conferido à mão), ~~BP F10~~ (26/11/2026: testes do foco da TV: pular fileira vazia, a fileira lembra onde estava, a grade mantém a coluna e a última linha curta segura o foco, parar nas pontas, foco que cai num item que existe depois de a lista mudar; e, em 27/11/2026, a máquina de estados do controle virou PadState::advance, testada: repetição 400/120 ms, analógico que só conta depois de visto em repouso, botão já segurado ao abrir, Menu tocado × segurado, e o Menu que abriu a TV ainda apertado), ~~BP F17~~ (26/11/2026: sair do Big Picture devolve a janela ao lugar de antes, maximizada se estava, quando ele a levou a outro monitor; com o monitor escolhido desligado, a TV avisa que abriu neste; entrar e sair conferido aqui, que tem um monitor só; a troca entre dois não testada), ~~BP F19~~ (26/11/2026: na página do jogo na TV, quando o foco não tem para onde ir, cima e baixo rolam o texto (descrição, "Como começar"); conferido com um texto longo posto na página: 0 → 2425 → 0), ~~BP F7~~ (26/11/2026: todos os controles são lidos e vale o último usado; um virtual parado no primeiro lugar não bloqueia o real), ~~BP F14, F15~~ (26/11/2026: B aparece nas dicas de toda seção (Sair no Início, Início nas outras) e na lista de conquistas; Descobrir mostra X Opções; "Indisponível" avisa por quê; passar o mouse numa capa move o foco), ~~BP F6~~ (26/11/2026: fileira "Todos os jogos" na tela de Conquistas da TV, pela última conquista e depois pelo nome, crescendo conforme o foco avança; a mensagem de vazio só aparece sem jogo nenhum e cita Steam, Xbox, Epic e RetroAchievements), ~~BP F9~~ (26/11/2026: o teclado da busca ganhou - : ' & . !, aceita acentos do teclado físico e responde ao clique; a busca já dobrava acentos), ~~BP F8, F9 (espaço), F11, F12, F13, F16, F18~~ (26/11/2026: o desinstalar já não dava panic desde 42e284b; Espaço digita espaço no teclado da busca (conferido) e é A fora dele; textos traduzidos; LB/RB na página do jogo contam da aba de origem; "Fechar o Cerberus" pergunta antes; o som retoma o contexto de áudio suspenso; ROMs relidas ao entrar na TV, não a cada aba), ~~BP F4~~ (25/11/2026: chip Buscar na Biblioteca com busca própria e sem acento; LT/RT (Home/End) saltam de letra em A a Z com a letra grande, ou uma página; conferido no app), F6, F7, F9–F19.
- Partidas: ~~nuvem não conferida~~ (27/11/2026: quando a pasta da nuvem não responde antes da partida, o backup do fim fica só neste PC e o histórico explica por quê e como enviar depois com "Fazer backup agora"; o "jogado/jogados" do histórico passou pela tradução), ~~P F8, P F9~~ (26/11/2026: "Última sessão" dos jogos de loja é a partida inteira, não o último minuto; a partida em andamento é anotada a cada minuto e entra no histórico mesmo se o Cerberus fechar no meio; o cartão não culpa mais "fora do Cerberus"), ~~P F19, F21, F22, F25~~ (26/11/2026: fechar com jogo de loja aberto já esconde a janela desde c4fcd8c; o gráfico divide a partida pela meia-noite e as linhas viraram "Últimos 7/30 dias"; o PC não dorme na preparação do jogo nem na cópia grande; o histórico guarda as 100 linhas mais novas de cada jogo, e um jogo cheio de conquistas não apaga o dos outros), ~~P F7~~ (26/11/2026: o tempo de partida, de jogo de loja e de clássico é medido por um relógio que para com o PC dormindo, QueryUnbiasedInterruptTime; com executável acompanhado, a duração do processo perde o tempo de sono; testes; não testado com uma suspensão real). ~~P F20~~ (26/11/2026: o checkpoint grava a biblioteca sem travar as sessões; uma trava só de gravação, junto com o fechamento da partida, impede contar o mesmo trecho duas vezes; teste: com uma gravação de 2 s, "está rodando?" responde em menos de 50 ms). ~~P F24~~ (26/11/2026: depois de restaurar, "Os saves de antes estão guardados nos backups" (verdade também quando já estavam no mais recente); o botão de restaurar desarma em 5 s ou ao perder o foco; o texto do rodízio cita as cópias de antes de restaurar; não clicado no app, pela regra de não apertar botões destrutivos). ~~P F18~~ (26/11/2026: a contagem por pasta das lojas ignora crash handlers e updaters e os programas na pasta de um jogo que o Cerberus já acompanha, então um jogo de loja adicionado à mão não conta em dobro; a lista de processos cresce quando enche). ~~P F26~~ (26/11/2026: um launcher que sai com sucesso em menos de 30 s faz a partida acompanhar os programas da pasta dele, esperando até 60 s o jogo aparecer fora do job, e somar o tempo; teste com um launcher que sai na hora e o jogo aberto por fora; custo: um jogo fechado em segundos aparece como aberto até 60 s a mais). Partidas sem pendências da auditoria.
- Recomps: ~~RC F23~~ (26/11/2026: teste do 7z com ../; os outros já existiam: tar com link, catálogo de outro repositório, fila depois de reiniciar, desinstalar e fazer backup de um recomp fora do catálogo). ~~RC F17~~ (26/11/2026: com a pasta de instalação em outro lugar, os recomps baixam em <instalações>\.cerberus-downloads, no disco do destino; a limpeza de downloads velhos olha os dois lugares; o espaço livre em Configurações mostra o do lugar certo; a busca de recomps instalados por fora ignora essa pasta). ~~RP F16~~ (26/11/2026: com a cópia certa e outra entrada faltando (disco 2, uma chave), o próximo passo é "Escolher o que falta" com o nome dela; o Jogar, o aviso da capa e a página apontam essa entrada em vez de dizer que a cópia não serve; os 9 recomps daqui seguem no mesmo estado). ~~RP F18~~ (26/11/2026: adotar recusa uma pasta dentro de, ou em volta da, pasta de outro recomp da biblioteca; um programa em bin\ fica com a pasta acima que tem recomplauncher.json). ~~RP F19~~ (26/11/2026: "Seu PC" confere o Visual C++ x86 e o DirectX em SysWOW64; "Instalar" instala só o que falta, x64 e/ou x86; conferido neste PC sem alarme falso). ~~RC F18, RP F21~~ (26/11/2026: RC F18 já estava feito; a cópia do jogo que não pôde voltar para a pasta, ao abrir ou depois de instalar, e a cópia achada nas pastas que não serviu vão para o histórico do jogo com a causa; a pasta de dados que não se cria impede o início com mensagem). ~~RC F19, F20, F21, F22, RP F20, F22~~ (26/11/2026: os avisos ao abrir a biblioteca se somam em vez de o segundo sumir; o catálogo trocado atualiza a tela mesmo com o mesmo número de jogos; sem tamanho do servidor o progresso anda a cada 4 MB; a chave compartilhada nova é gravada ao lado e trocada, a antiga fica se a gravação falhar; RC F20 e RP F22 já estavam feitos).

### Divergências do próprio roadmap (corrigir o texto)

- 5.2 marca como feitos `version.json`, versão mínima e arquivo ruim marcado: não existem em `self_update.rs` [RC F15, J F2]. Corrigir o texto da 5.2.
- ~~6.1 diz que o aviso de conquista tem 6 posições, versão secreta, agrupamento e volume [J F11]~~: texto corrigido em 26/11/2026; o que falta virou um item próprio na 6.1.

### Ordem

A (dados do jogador) → B (interrupções) → C (saves) → E (segurança) → D (recomps) → F (travamentos) → G.
Cada lote fecha com testes novos para cada item e conferência no app sem clicar em nada destrutivo.

## Planos detalhados (25/11/2026)

Cada tópico novo ganhou um plano escrito por um agente que leu o código do Cerberus e as referências. Os planos
têm etapas pequenas com critério de aceite e testes; o que não foi confirmado está marcado "não verificado".

| Tópico | Plano | Primeira etapa |
| --- | --- | --- |
| Saves de jogos de PC | [saves-de-jogos-de-pc.md](docs/roadmap/saves-de-jogos-de-pc.md) | Ler o manifesto do Ludusavi baixado sob demanda (não embutir: licença dos dados ambígua) |
| GOG e outras lojas | [gog-e-outras-lojas.md](docs/roadmap/gog-e-outras-lojas.md) | GOG instalados pelo registro e pelo `galaxy-2.0.db` (biblioteca e tempo sem login) |
| Steam sem chave e gerenciar lojas | [steam-sem-chave-e-gerenciar-lojas.md](docs/roadmap/steam-sem-chave-e-gerenciar-lojas.md) | Etapa 0 (segredos no backup e na URL), depois teste descartável do `webapi_token` |
| Metadados, filtros e estatísticas | [metadados-filtros-e-estatisticas.md](docs/roadmap/metadados-filtros-e-estatisticas.md) | E1: tipos de filtro, agrupamento e ordem no `shared`, medidos com 1.500 jogos |
| Troféus e lembranças | [trofeus-e-lembrancas.md](docs/roadmap/trofeus-e-lembrancas.md) | Parsers puros do `TROPUSR.DAT` e `TROPCONF.SFM` com arquivos sintéticos |
| Recomps robustos | [recomps-robustos.md](docs/roadmap/recomps-robustos.md) | `extract`, `contains`, `destination` e entradas como link |

## Fora de vez

- Ler arquivos de conquistas de emuladores de Steam usados em jogos piratas (CODEX, RUNE, Goldberg, EMPRESS etc.),
  torrent, debrid, resolvedores de hosters e instalação de PKG de PS4 baixado. O Hydra tem; o Cerberus não terá.
