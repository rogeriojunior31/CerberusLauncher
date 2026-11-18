# Roadmap

Comparado ao RecompLauncher (referência de comportamento) e ao Hydra. Cada onda só fecha com teste
real, não só compilação.

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
- Colocar os testes de ciclo de vida ao lado dos módulos Rust e executá-los em pull requests,
  junto com a compilação WASM. A Action atual cuida do catálogo. Um cenário de referência deve
  instalar uma release de teste, simular falha na atualização, reiniciar e ler os dados da versão anterior.
  O teste manual com mídia própria cobre a interação final com o jogo.

### Primeira entrega e próximos testes

- [x] Publicar alterações de `LocalLibrary` em memória somente depois da gravação em disco.
      A regressão em `library.rs` força uma falha real no arquivo temporário ao adicionar,
      renomear e remover jogos. Depois libera o destino, repete as operações e reabre a biblioteca.
- [ ] Recuperar a biblioteca na abertura. Um registro incompatível hoje faz toda a lista
      parecer vazia. Preservar o original, recuperar entradas legíveis e informar onde ficou a
      cópia. Cobrir arquivo ausente, JSON truncado, leitura negada e um registro inválido entre
      dois válidos. Uma tentativa frustrada de recuperação deve preservar os dados para nova tentativa.
- [ ] Levar as etapas de atualização de `recomp/mod.rs` para um módulo que responda pela
      preparação, validação e confirmação. Exercitá-lo com duas releases locais e falhas antes e
      depois da troca. Manter os comandos Tauri como pontos de entrada usados pela UI.
- [ ] Fechar o ciclo com um instalador real e uma imagem própria. O teste do ReStreet chegou
      ao checksum e ao registro do instalador pendente. Ainda faltam preparar a ISO de FIFA Street,
      iniciar o jogo, fechar, reabrir o launcher e conferir executável e histórico da sessão.

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
      baixados das releases oficiais. Listas de repacks (magnet, torrent, sites de arquivos) são recusadas.
- [x] Recomps com instalador próprio: `single-exe` instala direto; `installer` roda no primeiro
      "Jogar" e depois o Cerberus acha o jogo (pasta própria, depois "Aplicativos" do Windows),
      ou pergunta onde ficou.
- [x] Textos do catálogo em português onde dá para traduzir com exatidão: as 80 tags e a linha
      "Sua cópia de...". Descrições e passos ficam como o desenvolvedor escreveu.
- [ ] ~~Traduzir descrições e passos~~: decidido manter em inglês (exigiria um serviço de tradução).

## Onda 3: experiência (feita)

- [x] Modo tela grande (Big Picture) com controle e teclado.
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

- [ ] Arte da comunidade pelo SteamGridDB (chave gratuita do usuário em Configurações; o gerador
      pode usar uma chave de CI para preencher capas/fundos do catálogo).
- [ ] Base de jogos de console do LaunchBox Games Database (`Metadata.zip`): nomes alternativos,
      plataformas e imagens, para reconhecer melhor as ROMs. Confirmar os termos de uso antes.
- [ ] Busca com índice local (SQLite com busca de texto) se a base passar de algumas dezenas de
      milhares de jogos; para os recomps a busca em memória continua bastando.

### 4.4 Transição

- [ ] Quando o catálogo próprio cobrir os recomps principais, o do recomplauncher.com vira opcional
      (fonte nas Configurações) em vez de padrão.
- [ ] Pedir permissão ao mantenedor do RecompLauncher para usar a lista de repositórios deles como
      ponto de partida da descoberta (só os fatos: quais projetos existem; nunca textos ou arte).
