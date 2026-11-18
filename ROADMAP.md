# Roadmap

Comparado ao RecompLauncher (referência de comportamento) e ao Hydra. Cada onda só fecha com teste
real, não só compilação.

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
