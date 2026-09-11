# Paridade do macOS — acompanhamento

Objetivo: implementar as lacunas funcionais do macOS preservando Linux/Hyprland,
macOS 12+, Apple Silicon/Intel, português e integração com o notch.

| Requisito | Implementação | Evidência necessária |
| --- | --- | --- |
| Ícone real do terminal nos cartões | Em implementação: AppKit, PNG 64 px, busca pelos ancestrais | Build nativo, imagem de aplicativo real e fallback para processo encerrado |
| Escala automática por monitor | Em implementação: largura lógica e dimensão física via CoreGraphics | Testes Retina/sem Retina/EDID ausente, troca de monitor no Mac |
| Seguir Não Perturbe/Foco | Pendente: INFocusStatusCenter e autorização explícita | Autorização, recusa, indisponibilidade, ativar/desativar Foco, atualização no daemon |
| Silenciar com tela desligada/sessão indisponível | Pendente: APIs públicas de tela e sessão | Daemon iniciado antes/depois da suspensão e retorno sem estado preso |
| Ocultar durante tela cheia | Pendente | Tela cheia real, maximizada, troca de Spaces, várias telas e permissão recusada |
| Abrir sessão no terminal preferido, incluindo Warp | Pendente | Escolha persistente, terminal ausente, caminhos especiais, recusa de Automação |
| Instalar atualização pelo app | Pendente | Artefato autenticado, instalação atômica, rollback, permissões, relançamento e daemon atualizado |
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
