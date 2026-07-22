<div align="center">

<img width="160" alt="LocalKit" src="../assets/logo.png" />

# LocalKit

**Crie sites WordPress locais com um clique.**

Um app de desktop enxuto (pense no LocalWP, mas mais leve) que executa cada site WordPress
como seu próprio projeto isolado de Docker Compose — com o `wp-content` montado
numa pasta comum do host, para você editar o código no seu próprio editor.

[English](../README.md) | [Español](README.es.md) | [中文版](README.zh-CN.md) | Português

<br>

![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)
![Docker](https://img.shields.io/badge/Docker-2496ED?style=for-the-badge&logo=docker&logoColor=white)
[![Discord](https://img.shields.io/discord/1470639209059455008?style=for-the-badge&logo=discord&logoColor=white&label=Discord&color=5865F2)](https://discord.gg/ZKk6tkCQfG)

[![GitHub Stars](https://img.shields.io/github/stars/jhd3197/LocalKit?style=flat-square&color=f5c542)](https://github.com/jhd3197/LocalKit/stargazers)
[![Downloads](https://img.shields.io/github/downloads/jhd3197/LocalKit/total?style=flat-square)](https://github.com/jhd3197/LocalKit/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](../LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-756ce3?style=flat-square)](https://github.com/jhd3197/LocalKit/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![React](https://img.shields.io/badge/react-18-61DAFB.svg?style=flat-square&logo=react&logoColor=black)](https://reactjs.org)

<br>

[Início Rápido](#-início-rápido) · [Capturas de Tela](#-capturas-de-tela) · [Funcionalidades](#-funcionalidades) · [Arquitetura](#-arquitetura) · [Roadmap](#-roadmap) · [Documentação](#-documentação) · [Contribuir](#-contribuir) · [Discord](#-comunidade)

</div>

---

## 🚀 Início Rápido

> ⏱️ Do clone a um site WordPress rodando em minutos

### Requisitos

- **Docker Desktop** (em execução) com Compose v2+
- **Node.js 20+** e **Rust** (estável, toolchain MSVC no Windows) — apenas para compilar a partir do código-fonte
- Para sincronização: um servidor **[ServerKit](https://github.com/jhd3197/ServerKit)** com a **extensão `serverkit-localkit`** instalada

### Desenvolvimento

```bash
git clone https://github.com/jhd3197/LocalKit.git
cd LocalKit
npm install
npm run tauri dev        # inicia o Vite + a janela do Tauri
```

### Build de produção

```bash
npm run tauri build
```

<!-- LOCALKIT:SHOTS:START -->
## 📸 Capturas de Tela

> Capturadas de um build com dados fictícios — todos os sites, credenciais e servidores abaixo são inventados. Consulte [`docs/screenshots/CAPTURE.md`](screenshots/CAPTURE.md) para a lista de capturas e como regenerá-las com `npm run shots`.

|                            Painel                             |                            Visualização em Lista                            |
| :--------------------------------------------------------------: | :------------------------------------------------------------: |
|      ![Painel](screenshots/dashboard.png)       |      ![Visualização em lista](screenshots/dashboard-list.png)       |
|   _Todos os seus sites num relance — WordPress, PHP e Docker — com selos de status ao vivo_   |   _Uma visão em tabela mais densa para muitos sites_   |

|                             Detalhe do Site                              |                           Ferramentas                            |
| :-------------------------------------------------------------: | :------------------------------------------------------------: |
|         ![Detalhe do site](screenshots/site-detail.png)         |      ![Ferramentas](screenshots/site-tools.png)     |
| _Credenciais, banco de dados, informações do wp-cli, snapshots e sincronização_ | _Navegador de BD Adminer, busca-substituição segura, WP\_DEBUG e editor de configuração — no app_ |

|                             Snapshots                             |                           Novo Site                            |
| :-------------------------------------------------------------: | :------------------------------------------------------------: |
|         ![Snapshots](screenshots/snapshots.png)         |      ![Novo site](screenshots/new-site.png)     |
| _Restauração num clique; um é feito antes de cada push, pull e exclusão_ | _WordPress, PHP/Laravel ou um projeto Docker — em branco ou a partir de um blueprint_ |

|                             Importar                             |                           Configurações                            |
| :-------------------------------------------------------------: | :------------------------------------------------------------: |
|         ![Importar site remoto](screenshots/import-site.png)         |      ![Configurações](screenshots/settings.png)     |
| _Clone um site remoto do ServerKit como uma nova cópia local_ | _Status do Docker, atualizações, caminhos de dados e padrões_ |

|                           Domínios Locais                            |                           ServerKit                           |
| :--------------------------------------------------------------: | :-------------------------------------------------------------: |
|            ![Domínios locais](screenshots/settings-domains.png)            |      ![ServerKit](screenshots/settings-serverkit.png)      |
|      _Sirva sites como `http://<slug>.test` através de um roteador Caddy compartilhado_      |     _Navegue pelos sites de um servidor e faça push/pull ou importe-os_     |
<!-- LOCALKIT:SHOTS:END -->

## 🎯 Funcionalidades

### 🚀 Sites e Docker

| | |
|---|---|
| **Sites WordPress com Um Clique**<br>Escolha um nome, uma versão do WordPress e uma versão do PHP — pronto. | **Projeto Docker Compose por Site**<br>`wordpress:<wp>-php<php>-apache` + `mariadb:11`, totalmente isolado por site. |
| **Instalação Automática do WordPress**<br>Instalado via wp-cli, com credenciais de administrador geradas para você. | **Portas Únicas no Host**<br>Sites em `http://localhost:8081+`, bancos de dados em `18081+` — sem conflitos. |
| **Ciclo de Vida e Logs**<br>Iniciar / parar / excluir, selos de status dos contêineres ao vivo e visualizador de logs dos contêineres. | **Domínios Locais**<br>URLs opcionais `http(s)://<slug>.test` através de um roteador Caddy compartilhado nas portas 80/443, bloco gerenciado do arquivo hosts (aprovação de administrador uma única vez) e confiança na CA local com um clique para HTTPS. |

### 🔁 Sincronização com ServerKit

| | |
|---|---|
| **Enviar Código**<br>Envie seu `wp-content` local diretamente para um site remoto no seu servidor ServerKit. | **Enviar / Baixar Banco de Dados**<br>Envie o banco de dados, ou baixe um banco remoto para o seu site local com substituição automática de URLs. |
| **Histórico de Sincronização**<br>Cada operação de sincronização é registrada por site, com seu resultado. | **Conexões**<br>Salve, teste e exclua conexões de servidores; navegue por sites remotos e provisione novos — tudo através da extensão `serverkit-localkit`. |

### 🖥️ Desktop e CLI

| | |
|---|---|
| **Visualizações do Painel**<br>Grade ou lista densa para o painel, lembrada entre execuções. | **Página de Detalhe do Site**<br>Abrir site / wp-admin, credenciais de admin e banco de dados copiáveis, informações do wp-cli (versão do core, plugins). |
| **CLI `lk`**<br>Gerencie sites pelo terminal: `lk create`, `start/stop/restart`, passagem direta de `wp`, exportações `env`, `doctor`, saída JSON — compartilha o diretório de dados do app. | **Código Montado do Host**<br>O `wp-content` fica numa pasta comum do host, então você edita temas e plugins no seu próprio editor. |

---

## 🏗️ Arquitetura

```
React frontend (Zustand stores)
        │  invoke / events
        ▼
Tauri commands (src-tauri/src/lib.rs)
        │
        ├─► SQLite (rusqlite, forward-only migrations)
        ├─► docker compose CLI  ──► per-site project dir (compose + .env + wp-content/)
        └─► ServerKit API (reqwest, X-API-Key) ──► serverkit-localkit extension (push/pull)
```

O backend chama a CLI do `docker compose` — sem cliente da API do Docker, sem necessidade de direitos de administrador. Operações longas transmitem eventos de progresso `site-event` (`files → containers → waiting → install → done`) para a UI.

---

## 🖥️ A CLI `lk`

Binário complementar sem interface que compartilha o diretório de dados e o banco de dados do app:

```bash
cd src-tauri
cargo run -p lk -- list                 # ou: cargo build -p lk → target/debug/lk
lk create "My Blog"                     # criação completa do site, imprime a URL
lk wp my-blog plugin list               # passagem direta do wp-cli
lk env my-blog                          # exportações avaliáveis: eval $(lk env my-blog)
lk doctor                               # diagnostica Docker / compose / diretório de dados
lk list --json                          # saída legível por máquina
```

---

## 🛠️ Desenvolvimento

Apenas o frontend (para iterar a UI sem o shell):

```bash
npm run dev              # Vite em http://localhost:1420
npm run dev:mock         # Vite com dados fictícios, sem Docker/Tauri (porta 1426)
npm run shots            # regenera docs/screenshots/*.png via Chrome headless
npm run build            # tsc + vite build
```

Backend em Rust:

```bash
cd src-tauri
cargo check
cargo build
```

> **Nota para Windows:** se o `cargo` no seu PATH for uma instalação GNU não gerenciada pelo rustup
> (por exemplo, do chocolatey) e você encontrar `dlltool.exe: program not found`,
> coloque os shims do rustup primeiro: `export PATH="$HOME/.cargo/bin:$PATH"`
> (ou use `rustup run stable cargo check`).

---

## 📁 Estrutura

```
src/                     React 18 + TS + Vite frontend
  lib/ipc.ts             typed wrappers for all Tauri commands (invoke + events)
  lib/types.ts           shared TS types mirroring Rust payloads
  stores/                Zustand stores (nav, sites)
  pages/                 Dashboard (grid + list views), SiteDetail, Settings (modal)
  components/            Sidebar, StatusBadge, CopyButton, NewSiteDialog, icons
  mock/                  fake @tauri-apps/* modules for `vite --mode mock` (screenshots)
src-tauri/               Rust backend
  src/lib.rs             AppState, command registration, app entry
  src/db.rs              rusqlite, forward-only migrations (PRAGMA user_version)
  src/docker.rs          `docker compose` CLI wrapper
  src/site.rs            Site model, lifecycle, compose/env templates
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli`
  src/router.rs          local domains: shared Caddy router + hosts block + CA trust
  src/serverkit.rs       ServerKit API client (X-API-Key)
  src/sync.rs            push/pull orchestration + sync history
scripts/                 capture-screenshots.mjs (npm run shots), generate-funding-qr.mjs
docs/
  plans/                 ROADMAP.md + numbered implementation plans
  screenshots/           README screenshots + CAPTURE.md
  images/funding/        donation QR codes
```

---

## 📂 Onde as Coisas Ficam

- Dados do app: `%APPDATA%/LocalKit/` (macOS: `~/Library/Application Support/LocalKit/`, Linux: `~/.local/share/LocalKit/`)
  - `localkit.db` — banco de dados SQLite de sites, conexões e histórico de sincronização
  - `sites/<slug>/` — projeto por site: `docker-compose.yml`, `.env`, `wp-content/` (edite seu código aqui)
  - `router/` — roteador Caddy compartilhado para domínios locais (projeto compose + Caddyfile gerado), apenas enquanto os domínios locais estiverem ativados

---

## 🔁 Notas de Sincronização com ServerKit

- A autenticação é via `X-API-Key` (crie uma chave em ServerKit → configurações de API).
- O teste de conexão = o endpoint público `/api/v1/system/health` + validação da chave contra `/api/v1/setup-health/account` + uma sonda `/api/v1/localkit/pair` que detecta a extensão.
- Todos os endpoints de sincronização ficam na extensão `serverkit-localkit` (`/api/v1/localkit/...`); sem ela, o LocalKit diz exatamente o que está faltando.
- **Enviar código** = tar.gz em memória do `wp-content/` → POST multipart. **Enviar BD** = `wp db export` → POST multipart. **Baixar BD** = baixar o dump → `wp db import` → `wp search-replace` da URL remota para a local.
- Cada operação de sincronização é registrada no histórico por site com seu resultado.
- As chaves de API são armazenadas em **texto plano** no banco SQLite local do LocalKit — aceito na v1; armazenamento no chaveiro do SO está no roadmap.

---

## 🗺️ Roadmap

- **M1 — Ciclo de vida de sites locais** ✅ criar/iniciar/parar/excluir, projetos compose, alocação de portas
- **M2 — Instalação do WordPress e detalhe** ✅ instalação com wp-cli, credenciais, logs, wp info
- **M3 — Conexão com ServerKit** ✅ salvar/testar conexões, detecção da extensão, navegar por sites remotos
- **M4 — Enviar / baixar** ✅ enviar código, enviar BD, baixar BD com reescrita de URL, histórico de sincronização
- **M5 — Polimento para lançamento** ⬜ instaladores, atualização automática, chaveiro do SO para as chaves de API, suíte de testes
- **M6 — Domínios locais** ✅ `http(s)://<slug>.test` através de um roteador Caddy compartilhado, bloco de hosts gerenciado + confiança na CA local (plano 6)
- **M7 — CLI (`lk`)** ✅ binário complementar sem interface: ciclo de vida, passagem de wp, `env`, `doctor`, saída JSON (plano 7)

Detalhes completos, fases por plano e ordem de construção: [`docs/plans/ROADMAP.md`](plans/ROADMAP.md).

---

## 📖 Documentação

| Documento | Descrição |
|----------|-------------|
| [Roadmap](plans/ROADMAP.md) | Marcos, fases por plano e ordem de construção |
| [Captura de Screenshots](screenshots/CAPTURE.md) | Lista de capturas e como regenerá-las com `npm run shots` |
| [Planos de Implementação](plans/) | Planos de implementação numerados por funcionalidade |

---

## 🧱 Stack Tecnológico

| Camada | Tecnologia |
|-------|------------|
| Shell do App | Tauri 2, Rust |
| Frontend | React 18, TypeScript, Vite, Tailwind CSS v3, Zustand |
| Banco de Dados | rusqlite (SQLite embutido, migrações apenas para frente) |
| Contêineres | CLI do Docker Compose (sem cliente da API do Docker) |
| Sincronização | reqwest (rustls) + flate2/tar (arquivos de sincronização) |

---

## 🤝 Contribuir

Contribuições são bem-vindas!

```
fork → feature branch → commit → push → pull request
```

---

## 💛 Apoie o LocalKit

O LocalKit é livre e de código aberto. Se ele economiza seu tempo, você pode ajudar a mantê-lo vivo:

- ⭐ [Dê uma estrela no repositório](https://github.com/jhd3197/LocalKit) — não custa nada e ajuda muito
- 💖 [GitHub Sponsors](https://github.com/sponsors/jhd3197)
- ☕ [Buy Me a Coffee](https://buymeacoffee.com/jhd3197)

### 💎 Criptomoedas

| | Ativo | Rede | Endereço |
|:---:|---|---|---|
| <img src="images/funding/usdt-trc20.png" width="110" alt="Código QR do endereço de doação USDT TRC-20" /> | **USDT** | **TRC-20** · Tron | `TTiCtqLauF1iSW2YGB3b78KmRxRqoLCgeL` |
| <img src="images/funding/usdt-erc20.png" width="110" alt="Código QR do endereço de doação USDT e ETH ERC-20" /> | **USDT / ETH** | **ERC-20** · Ethereum | `0xD13D5355Fa214e8317fea2ff192a065BaeC13527` |
| <img src="images/funding/btc.png" width="110" alt="Código QR do endereço de doação de Bitcoin" /> | **BTC** | **Bitcoin** | `bc1qatx67n3qxdvuv3arc9j8aytk34f22g02k9c7vr` |
| <img src="images/funding/sol.png" width="110" alt="Código QR do endereço de doação de Solana" /> | **SOL** | **Solana** | `AWXzqtBEgUfteHPQtDegsZ6D5y57M3GGdKPD8rR7h6xu` |

TRC-20 tem as taxas mais baixas — geralmente menos de um dólar — então é a
opção mais amigável para uma doação pequena. O gas de ERC-20 pode custar mais
do que a própria doação.

<sub>Os códigos QR são gerados localmente pelo [`scripts/generate-funding-qr.mjs`](../scripts/generate-funding-qr.mjs), que valida a soma de verificação de cada endereço antes de codificá-lo.</sub>

---

## 🔭 Projetos Relacionados

**[ServerKit](https://github.com/jhd3197/ServerKit)** — Um painel de controle de servidores leve e moderno para gerenciar aplicações web, bancos de dados, contêineres Docker e segurança — sem a complexidade do Kubernetes nem o custo das plataformas gerenciadas. Combine com o LocalKit através da extensão `serverkit-localkit` para enviar código e enviar/baixar bancos de dados entre sites locais e remotos.

**[Faro](https://github.com/jhd3197/faro)** — Um cliente de desktop moderno para SFTP, FTP, SSH e armazenamento compatível com S3, do mesmo autor. Salve um servidor uma vez e depois navegue pelos arquivos numa visualização de painel duplo e abra um terminal sobre a mesma sessão SSH — além de transferências por arrastar e soltar, sincronização de diretórios em um sentido e edição in-loco. Ele até tem um **Agent Bridge** que permite ao Claude Code (ou qualquer agente MCP) executar comandos num servidor através da sua sessão autenticada, com aprovação por comando e sem compartilhar credenciais.

**[DeviceKit](https://github.com/jhd3197/DeviceKit)** — Uma plataforma unificada de frota de dispositivos Android e automação de testes. Controle uma frota de dispositivos a partir de um único painel — execute automações, transmita telas em tempo real, capture regressões visuais e depure falhas com análise assistida por IA.

---

## 💬 Comunidade

[![Discord](https://img.shields.io/badge/Discord-Join_Us-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/ZKk6tkCQfG)

Entre no Discord para fazer perguntas, compartilhar feedback ou obter ajuda com sua configuração.

---

## 📄 Licença

MIT — consulte [LICENSE](../LICENSE).

---

<div align="center">

**LocalKit** — Desenvolvimento local de WordPress, sem o inchaço.

[Reportar um Bug](https://github.com/jhd3197/LocalKit/issues) · [Solicitar uma Funcionalidade](https://github.com/jhd3197/LocalKit/issues)

Feito com ❤️ por [Juan Denis](https://juandenis.com)

</div>
