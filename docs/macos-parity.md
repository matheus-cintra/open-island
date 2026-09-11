# Paridade do macOS — acompanhamento

Objetivo: implementar as lacunas funcionais do macOS preservando Linux/Hyprland,
macOS 12+, Apple Silicon/Intel, português e integração com o notch.

| Requisito | Implementação | Evidência necessária |
| --- | --- | --- |
| Ícone real do terminal nos cartões | Em implementação: AppKit, PNG 64 px, busca pelos ancestrais | Build nativo, imagem de aplicativo real e fallback para processo encerrado |
| Escala automática por monitor | Em implementação: largura lógica e dimensão física via CoreGraphics | Testes Retina/sem Retina/EDID ausente, troca de monitor no Mac |
| Seguir Não Perturbe/Foco | Em implementação: INFocusStatusCenter, permissão nos ajustes e estado enviado ao daemon com TTL | Autorização, recusa, indisponibilidade, ativar/desativar Foco, atualização no daemon |
| Silenciar com tela desligada/sessão indisponível | Em implementação: CGDisplayIsAsleep; bloqueio com tela acesa ainda pendente | Daemon iniciado antes/depois da suspensão e retorno sem estado preso |
| Ocultar durante tela cheia | Em implementação: currentSystemPresentationOptions do aplicativo ativo | Tela cheia real, maximizada, troca de Spaces, várias telas e permissão recusada |
| Abrir sessão no terminal preferido, incluindo Warp | Implementado: preferência persistente para Terminal, iTerm2, Warp, WezTerm e Kitty; validação nativa pendente | Escolha persistente, terminal ausente, caminhos especiais, recusa de Automação |
| Instalar atualização pelo app | Em implementação: pacotes assinados, índice por arquitetura e troca atômica com restauração; botão ainda usa DMG manual | Artefato autenticado, instalação atômica, rollback, permissões, relançamento e daemon atualizado |
| Envio de texto no Warp | Investigar integração pública; sem simular sucesso | Identificação da sessão de destino e confirmação de entrega sem atingir outra aba |
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
  em repouso. Não anuncia detecção de bloqueio quando a tela continua acesa.
- Testes Linux da implementação inicial: workspace Rust e frontend aprovados.
  Builds nativos e confirmação física continuam necessários antes de conclusão.

- Atualizador: chave privada fora do repositório em `~/.local/share/open-island-release/updater.key` e secret `TAURI_SIGNING_PRIVATE_KEY` no CI; preservar backup dessa chave para futuras versões. A chave pública está na configuração macOS. Assinatura do atualizador é independente da assinatura ad hoc do aplicativo.
- Transação do pacote: testes locais cobrem commit, restauração após falha/panic, recusa de symlink e preservação do backup se a restauração falhar. Download autenticado, validação do bundle e reinício/verificação do daemon ainda precisam ser conectados.
