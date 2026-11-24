# Roadmap

Comparado ao RecompLauncher (referência de comportamento) e ao Hydra. Cada onda só fecha com teste
real, não só compilação.

## Referência Hydra e próximas entregas

Comparação com o checkout local do Hydra em `6b7ad7bf2`.

| Área | Comportamento observado no Hydra | Entrega no Cerberus |
| --- | --- | --- |
| Fontes | O cliente cadastra a URL via HydraApi e persiste a fonte. As publicações têm título, tamanho e data. [Cadastro](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/main/events/download-sources/add-download-source.ts) e [tipos](https://github.com/hydralauncher/hydra/blob/6b7ad7bf2/src/types/index.ts) | Implementado. Configurações importa os metadados por HTTPS, guarda a última cópia válida e permite pesquisar por fonte. Cada fonte mostra a última sincronização e eventuais erros. |
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
      baixados das releases oficiais. Listas Hydra podem ser consultadas em Configurações por título, tamanho e data.
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
- [ ] **Epic (fase 3, precisa de login na conta Epic):**
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
- [ ] `recomplauncher.json`: vários jogos numa release (`games`), e as chaves compartilhadas e o DLSS que o launcher
      baixaria sozinho.
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

- [x] **Aviso de conquista na tela durante o jogo** [H]: janela transparente, sem foco e por cima de tudo, em 6
      posições; versões normal, rara (até 10%), secreta e platina (a que completa o jogo); "N conquistas em M jogos"
      quando vierem várias; som e volume próprios e botão de teste. — M
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
- [ ] Conquistas da Epic [H] (GraphQL da loja da Epic), depois do login da Epic (6.2). — M
- [ ] Separar conquistas do jogo base e das DLCs [H]. — P

### 6.2 Biblioteca e lojas (prioridade 2)

- [x] **Jogos da Steam não instalados na Biblioteca** [H][P], pelo `GetOwnedGames` que o hub já usa; "Instalar" e
      "Desinstalar" por `steam://install|uninstall/<id>`, e o jogo vira instalado sozinho quando aparece o
      `appmanifest_*.acf`. — M
- [ ] Opções de inicialização passadas à Steam: `steam://run/<id>//<argumentos>/` [H]. — P
- [ ] Ler o `appinfo.vdf` da Steam para saber o executável real de cada jogo [H] (medir a partida certo). — M
- [ ] **Login da Epic e biblioteca completa** [H]: login numa janela da Epic e o Legendary para listar, instalar,
      verificar e abrir (conferir a licença do Legendary, GPL, antes de distribuir junto); tempo de jogo da Epic pelos
      servidores dela. — G
- [ ] **Importar o tempo de jogo das lojas** com escolha: sempre, só para jogos novos ou nunca; "última vez" da loja só
      vale se for mais nova [P]. Tempo separado por fonte (Cerberus, Steam, Epic), editar e zerar [H]. — P
- [ ] **Achar jogos fora das lojas** [P][H]: atalhos do Menu Iniciar (ignorando desinstaladores, setup, vc_redist,
      crash handlers), uma pasta escolhida, `C:\Games`/`D:\Games`/"GOG Games", e apps da Microsoft Store/Xbox
      (`shell:AppsFolder`). Jogo apagado entra numa lista de exclusão e não volta na próxima busca [P]. — M
- [ ] O mesmo jogo na Steam e na Epic aparece como um só [H]. — M
- [ ] Ordem pelo nome sem "The"/"A" e com algarismos romanos na ordem certa [P]. — P
- [ ] Tamanho instalado de cada jogo, agrupar por disco e mover um jogo de PC para outro disco [P][H]. — M
- [ ] **Página do jogo com dados da loja** [H]: descrição, requisitos mínimo e recomendado, idiomas, suporte a controle,
      imagens e trailers (`store.steampowered.com/api/appdetails`). — M
- [ ] Atalho no Menu Iniciar além da área de trabalho [H][R]. — P

### 6.3 Organizar a coleção (prioridade 3)

- [x] **Situação do jogo** [P]: Não jogado, Jogado, Jogando, Zerado, Completo, Abandonado, Em pausa, Quero jogar,
      editáveis. Vira "Jogado" na primeira partida; os selos do hub (Zerado, Completo, Dominado) sugerem a situação. — P
- [x] Nota pessoal (0 a 100) e anotações por jogo [P]. — P
- [ ] **Filtros salvos** com ordem e agrupamento [P]: prontos "Todos", "Jogados recentemente", "Favoritos" e
      "Mais jogados"; aparecem também no Big Picture. — M
- [ ] **Agrupar** por plataforma, loja, situação, ano, disco, faixa de tempo de jogo (menos de 1 h, 1 a 10 h, 10 a
      100 h...) e última vez jogado (hoje, ontem, esta semana, este mês, este ano, nunca) [P]. — M
- [x] Mais ordens: adicionado em, nota, situação, tamanho, vezes jogado [P]. — P
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

## Fora de vez

- Ler arquivos de conquistas de emuladores de Steam usados em jogos piratas (CODEX, RUNE, Goldberg, EMPRESS etc.),
  fontes de download, torrent, debrid e hosters, e instalar PKG de PS4 baixado. O Hydra tem; o Cerberus nunca terá.
