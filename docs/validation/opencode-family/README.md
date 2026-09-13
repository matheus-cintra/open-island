# OpenCode: agrupamento de subagentes

Validação local em 2026-09-13, Linux/Hyprland, OpenCode 1.18.30.
Worktree: /home/matheus/dev/pessoal/open-island, branch master,
base 71f315e. Evidência da implementação incluída na release v0.6.2.

## Implementação

O plugin normaliza os dois formatos de identidade e consulta session.get com
cache e compartilhamento de consultas por instância. Metadados confirmam o
parentesco (parent_id nulo confirma uma raiz); eventos sem metadados continuam
compatíveis. Falhas têm timeout e permitem novas tentativas.

O store mantém cada sessão e projeta seus descendentes no cartão da raiz.
A ponte Claude continua usando opencode:<id>. PID e diretório não determinam
parentesco. Relações cíclicas são rejeitadas. Atividade, conclusão e pendências
dos filhos não sobrescrevem os dados da conversa principal.

Perguntas e permissões simultâneas ficam em fila, identificadas pelo filho, com
IDs de resposta originais. Navegação usa o cartão principal. A seção existente
e as opções de visibilidade/notificação continuam sendo utilizadas.

## Testes automatizados

- cargo test --workspace: 560 testes Rust aprovados.
- bun run test: 183 testes aprovados em 30 arquivos.
- TypeScript e build Vite aprovados.
- bun run tauri build --no-bundle aprovado; o prebundle recompila explicitamente
  open-islandd em release para x86_64-unknown-linux-gnu.
- Testes com processos executados com setsid, timeout --kill-after e saída
  redirecionada para arquivo. Verificação de vazamentos sem daemon de teste.

Cobertura nova: eventos reais info/sessionID, ausência/falha de metadados,
timeout, cache por instância, consultas concorrentes, ancestrais, ciclos,
ponte Claude, filho antes do pai, parentesco tardio, duas raízes no mesmo PID,
conclusão/retomada, pendências, novo prompt, exclusão, limpeza por inatividade
e encerramento do processo. Testes DOM verificam visibilidade, dois filhos,
navegação e perguntas/permissões simultâneas com os IDs corretos.

## OpenCode real e evidência visual

Uma instância separada foi iniciada com opencode serve na porta local 4098.
O prompt principal usou o tool Task para criar exatamente dois subagentes
general, Research e Review, que leram um README de teste e executaram sleep 20.

IDs verificados na API do OpenCode:

- Pai: ses_f668ae424ffesNE7WFdHqXEuGF
- Research: ses_f668aa514ffep6O14KLORoyQMa
- Review: ses_f668aa528ffecXvUzqCscgxLdM

Os dois filhos informaram o ID do pai em parentID. Na captura de list_sessions,
a família aparece como um cartão com dois subagentes. A captura nativa mostra
“Agentes (2)” e os dois nomes marcados como concluídos.

- [Captura visual nativa](completed.png)
- [list_sessions após conclusão](completed.json)
- [Próximo prompt: filhos concluídos removidos](next-prompt.json)
- [Após encerrar o processo: família removida](after-exit.json)

Também foi observada a fase intermediária com Research concluído e Review
ainda trabalhando, ambos no mesmo cartão.

Perguntas/permissões simultâneas e o roteamento ao terminal foram verificados
automaticamente. A instância real de QA era headless (host_unsupported);
não se afirma validação manual de foco/envio em um terminal para essa instância.

## Aplicação local

Daemon e interface atualizados em ~/.local/bin, com checksum conferido contra
os builds. Plugin atualizado por open-islandd hooks install --agent opencode.
Serviços open-islandd.service e open-island.service reiniciados e ativos.

A conversa OpenCode que já estava em uso não foi reiniciada. A ativação do
plugin atualizado foi validada na nova instância de QA. O processo de QA foi
encerrado ao final, incluindo seu grupo de processos. Nenhum pacote de
terceiros foi alterado e nenhum banco SQLite do OpenCode foi usado.
