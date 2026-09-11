# Paridade do macOS — acompanhamento

Objetivo: implementar as lacunas funcionais do macOS preservando Linux/Hyprland,
macOS 12+, Apple Silicon/Intel, português e integração com o notch.

| Requisito | Implementação | Evidência necessária |
| --- | --- | --- |
| Ícone real do terminal nos cartões | Em implementação: AppKit, PNG 64 px, busca pelos ancestrais | Build nativo, imagem de aplicativo real e fallback para processo encerrado |
| Escala automática por monitor | Em implementação: largura lógica e dimensão física via CoreGraphics | Testes Retina/sem Retina/EDID ausente, troca de monitor no Mac |
| Seguir Não Perturbe/Foco | Em implementação: INFocusStatusCenter, permissão nos ajustes e estado enviado ao daemon com TTL | Autorização, recusa, indisponibilidade, ativar/desativar Foco, atualização no daemon |
| Silenciar com tela desligada/sessão indisponível | Em validação: CGDisplayIsAsleep e consulta atual da sessão para bloqueio/troca de usuário | Daemon iniciado antes/depois da suspensão e retorno sem estado preso |
| Ocultar durante tela cheia | Em implementação: currentSystemPresentationOptions do aplicativo ativo | Tela cheia real, maximizada, troca de Spaces, várias telas e permissão recusada |
| Abrir sessão no terminal preferido, incluindo Warp | Implementado: preferência persistente para Terminal, iTerm2, Warp, WezTerm e Kitty; validação nativa pendente | Escolha persistente, terminal ausente, caminhos especiais, recusa de Automação |
| Instalar atualização pelo app | Em validação: botão conectado ao download assinado, validação do bundle, troca atômica e reinício do daemon | Artefato autenticado, instalação atômica, rollback, permissões, relançamento e daemon atualizado |
| Envio de texto no Warp e outros terminais | Implementado com ponte PTY por sessão: botão + e integração opcional de Bash/Zsh/Fish | Sete testes nativos e regressões do core/daemon passaram em Apple Silicon/Intel; validar no Warp real e reabrir sessões antigas |
| Linux e integração AppKit/notch | Preservar | Regressões automatizadas e validação visual no Mac |
| Distribuição | DMGs por arquitetura, assinatura ad hoc, sem notarização | Bundle/daemon/sons, checksums, build e testes nativos |

## Regras de implementação

- Não consultar arquivos privados do macOS para Foco/Não Perturbe.
- Permissões são solicitadas quando o usuário habilita o recurso; recusa deve ser
  indicada nos ajustes e não interpretada como estado conhecido do sistema.
- A interface não pode anunciar suporte apenas porque um botão foi adicionado.
- A validação em Linux ou em mocks não comprova interação com o macOS físico.
- Integração com Hyprland é substituída por AppKit; não há motivo para copiar
  configurações exclusivas de outro compositor para os ajustes do Mac.

## Evidência de desenvolvimento

- Ícones: API pública `NSRunningApplication.icon`, conversão PNG no thread principal,
  limite de 64 KiB e busca dos ancestrais para processos auxiliares.
- Densidade: a largura de NSScreen já está em pontos; backingScale não é dividido
  novamente. Testes incluem Retina, monitor comum, monitor denso e dimensão ausente.
- Foco: `INFocusStatusCenter` está disponível no macOS 12 segundo a documentação
  oficial da Apple. O pedido de autorização usa `NSFocusStatusUsageDescription`.
  Estado indisponível é diferente de Foco desligado nos ajustes; os relatórios ao
  daemon expiram em 10 segundos para não manter silêncio após desconexão.
- Tela: consulta atual via CoreGraphics permite iniciar o daemon com a tela já
  em repouso. Bloqueio e troca de usuário são consultados mesmo se a interface não estiver aberta; a chave de bloqueio do WindowServer não é documentada e mantém a detecção experimental.
- Testes Linux da implementação inicial: workspace Rust e frontend aprovados.
  Builds nativos e confirmação física continuam necessários antes de conclusão.

- Atualizador: chave privada fora do repositório em `~/.local/share/open-island-release/updater.key` e secret `TAURI_SIGNING_PRIVATE_KEY` no CI; preservar backup dessa chave para futuras versões. A chave pública está na configuração macOS. Assinatura do atualizador é independente da assinatura ad hoc do aplicativo.
- Transação do pacote: testes locais cobrem commit, restauração após falha/panic, recusa de symlink e preservação do backup se a restauração falhar. Download autenticado, validação do bundle e reinício/verificação do daemon foram conectados; teste nativo de ponta a ponta ainda pendente.

- Bloqueio: `CGSessionCopyCurrentDictionary` consulta a sessão do próprio processo; `kCGSessionOnConsoleKey` cobre troca de usuário. O campo `CGSSessionScreenIsLocked` não é documentado pela Apple; fica isolado em uma consulta defensiva, com tipos inesperados/sessão inexistente tratados como desconhecidos. O WindowServer omite a chave quando desbloqueado. Validar bloqueio antes/depois de iniciar o daemon em um Mac físico.

- Warp: a [especificação oficial de Warp Control](https://github.com/warpdotdev/warp/blob/master/specs/warp-control-cli/PRODUCT.md) exclui execução/submissão de comandos e prompts. `input.insert` e `input.replace` apenas preparam texto. O suporte da ilha exige entrega ao processo certo; portanto, não se usa essa interface para simular envio. tmux/Zellij/Kitty/WezTerm continuam sendo os canais suportados.

- Entrada universal: a nova [ponte PTY](terminal-input.md) entrega texto e Enter ao
  processo da sessão sem usar a API do Warp, foco de janela ou clipboard. Os canais
  anteriores permanecem disponíveis para sessões abertas sem a ponte.

## Validação automatizada do conjunto completo

Build `2e980b62d2d5cb4ce0c0e228cbe3298e7c34f2d9`,
[execução 34625129963](https://github.com/matheus-cintra/open-island/actions/runs/34625129963):
Apple Silicon e Intel concluídos com sucesso. Os dois jobs compilaram os bundles,
verificaram assinatura ad hoc/arquitetura/daemon/sons e executaram o workspace Rust.
O teste nativo com o arquivo real de atualização validou a assinatura, recusou uma
alteração de bytes e comprovou a troca do pacote e do PID do daemon nas duas arquiteturas.
Checksums dos DMGs, tarballs e assinaturas foram conferidos novamente após o download.

Frontend local: 157 testes aprovados; build Vite aprovado. Workspace Rust local aprovado,
sem processos de teste remanescentes. A comparação dos arquivos confirmou que o código
entregue corresponde ao workspace; a única diferença era formatação de um helper de testes.

Ainda falta a confirmação física: notch/hover/teclado, ícones, monitores/Retina,
Spaces/tela cheia, suspensão, Foco/bloqueio, Automação/Warp e login. O roteiro foi
entregue junto aos DMGs e o resultado foi solicitado ao usuário. O rótulo experimental
permanece até essa confirmação.
