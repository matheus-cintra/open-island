# Interface do Open Island

## Tokens e componentes

A ilha usa os tokens de `app/src/styles.css`: fundo `--bg`, texto `--fg`,
texto secundário `--muted`/`--fg-secondary`, separadores `--separator` e
`--row-stroke`, foco `--accent`, estados `--danger`/`--warning`/`--success`.
Tipografia de conteúdo usa `--content-font`, `--content-sm`, `--content-md` e
`--font-mono`. Ícones de controles são SVG de 16 unidades, com `currentColor`
e `aria-hidden`; o botão fornece o nome acessível.

As áreas compacta e expandida compartilham a janela. Apenas a área ativa
fica acessível: a outra recebe `inert` e `aria-hidden=true`. Abrir por hover
preserva o foco do terminal. Abertura explícita e seleção de um editor seguem
as regras de teclado de cada plataforma. Escape no ditado cancela o job;
fora dele, o editor mantém seu comportamento de liberar foco.

## Tamanho e movimento

`island-window.ts` conserva a mola de 320 ms, amortecimento 0,7 e frequência
10 usados pela ilha. Reduced motion aplica o destino imediatamente, sem RAF.
O resize nativo admite uma chamada em voo e um destino pendente substituível.
Somente respostas bem-sucedidas confirmam dimensões. Falhas aguardam outro
evento de layout; callbacks de animações substituídas não podem restaurar
um destino antigo. Escala, notch e limite de altura continuam calculados na
ilha a partir das métricas da plataforma.

Sessões preservam sua rolagem. Voz, recuperação e estado da conexão ficam
numa área auxiliar com rolagem própria, limitada a 55% da altura expandida.
Sua altura participa do cálculo da janela. Isso mantém os resultados
recuperáveis acessíveis quando existem várias sessões ou transcrições.

## Voz e recuperação

A ilha é hidratada pelo cache de snapshots do shell. Até a conexão ficar pronta,
ações que dependem do daemon permanecem desabilitadas, incluindo o botão de som.
Eventos legados de sessão, pendência, configuração, uso e atualização não alteram
a ilha. Troca de monitor consulta apenas as métricas nativas e reaplica a
configuração já recebida. Uma resposta tardia não fecha uma pendência substituída.
As linhas conservam a identidade exibida para navegação; linhas saindo não
aceitam cliques. O botão de navegação de uma pergunta não troca seu destino por
uma instância que reutilizou o mesmo ID. `main.ts` fornece estado e ações às
views de linha, mensagem e pergunta, sem imports dessas views de volta a main.

O deslocamento OLED mantém uma única cadeia de RAF. Visibilidade do documento,
tela cheia com ocultação, idle e movimento reduzido cancelam o frame pendente;
callbacks antigos não reativam a cadeia. O transform só é escrito quando o valor
arredondado muda. O relógio das sessões usa um timer apenas enquanto a ilha está
expandida e visível, mantendo os mesmos períodos e limites do deslocamento.
O aviso de tela desligada do daemon também suspende esse trabalho enquanto a
conexão está ativa. Um aviso antigo não mantém a suspensão após desconectar.
Ao voltar à área expandida visível, o tempo decorrido é recalculado imediatamente;
ticks com o mesmo texto não alteram o nó. O sinal do daemon depende do filtro
`quiet_screen_off`; a visibilidade do documento continua sendo observada.

Os indicadores de uso conservam nós por provedor e chave da janela/modelo.
Percentual, severidade, prazo, créditos e estado desatualizado são comparados
antes de escrever no DOM. Reordenar mantém os nós; remover limpa o cache local.
O formato e as classes visuais existentes continuam sendo usados.

Snapshots atualizam o estado e as guardas imediatamente; as invalidações da
lista, uso, pendências, conexão e configuração compartilham uma pintura por RAF.
Uma rajada conserva o estado final e os eventos de conclusão relevantes, mesmo
que uma sessão volte a trabalhar antes da pintura. Abrir explicitamente a ilha
consome a pintura pendente antes de medir sua geometria. Respostas antigas de
métricas não reaplicam uma configuração substituída.

Os caches de atividade pertencem à identidade da sessão, incluindo epoch e
instância. Cada sessão viva conserva apenas o último token de conclusão (também
durante intervalos de trabalho) e o conjunto atual de subagentes concluídos.
Remoção descarta a entrada; hidratação refaz a referência inicial sem disparar
animações. A deduplicação usa snapshots ordenados já aceitos pelo shell e os
tokens novos emitidos pelo daemon para cada Stop aceito, sem interpretar seu formato.

Controles de aprovação e pergunta conferem a chave exibida antes de agir.
Opções e editores antigos não alteram respostas de uma pendência substituída.
Envio, navegação, cancelamento e voz conferem também a identidade atual no momento
da ação; adiar a pintura não estende a autorização de um destino antigo.

Mensagens admitidas mostram Enfileirada, Enviando, Entregue ao canal, Falha ou
Entrega não confirmada. O item conserva seu nó durante mudanças de estado.
Enviando desabilita descarte; confirmação do canal não significa execução da
tarefa. A confirmação desaparece após cinco segundos na UI. Falhas e resultados
incertos oferecem copiar/descartar, sem reenvio automático. Texto recuperável
usa campo somente leitura e foco visível; selecionar o texto permite cópia
manual quando o clipboard falha.

Mensagens cuja instância não tem linha própria ficam em “Mensagens de outras
sessões”, incluindo subagentes agrupados e sessões encerradas ou alteradas.
O descarte usa a identidade original do registro. Se o registro desaparecer do
daemon sem confirmação, a cópia do shell permanece como entrega não confirmada;
essa indicação distingue registro indisponível de conexão anterior.

O botão de microfone alterna gravar/parar e mora dentro do campo do composer,
no canto direito. Sem modelo configurado, o botão fica esmaecido e o clique abre
Ajustes na aba Voz local; a seleção do modelo existe apenas nas preferências.
A gravação encerra sozinha cerca de um segundo e meio depois que a voz para
abaixo do limiar de voz; clicar de novo encerra na hora, e Escape cancela.
Enquanto o worker está ativo, o destino e o estado permanecem visíveis, e a
ilha não colapsa. Durante a gravação, o campo é substituído por uma linha
horizontal que ondula com o nível do microfone; o rascunho permanece no estado
e volta quando a gravação termina. O nível vem do evento `voice-level`, lido da
cauda do buffer de captura a cada ~60 ms; o desenho respeita
`prefers-reduced-motion` e para com a gravação. A transcrição não tem faixa
própria: o microfone mostra um indicador girando até o texto ficar pronto, e o
resultado entra no rascunho ou vira cartão. O timer
acompanha a captura; a transcrição ocorre localmente e exige revisão.
Estados: idle, requesting_permission, recording, transcribing, ready,
cancelled e error. Os rótulos ficam em `strings.ts`.

Ao disponibilizar a transcrição, o shell revalida epoch, instância e nascimento
do processo. Destino removido, reciclado ou desconectado mantém o resultado em
ready com `target_unavailable`. O cartão explica a indisponibilidade e conserva
o texto; a inserção exige nova seleção de sessão e clique explícito. A checagem
não segura o lock da voz, e cancelar durante ela impede a publicação do resultado.

O shell emite `voice-state` somente para a janela principal. A UI assina antes
de iniciar e lê `voice_state` para hidratação tardia; `voice_get_state` permanece
como alias. Revisões antigas não substituem eventos novos. Se ready anteceder a
resposta de start, o resultado aguarda a associação ao job antes de preencher o
rascunho. Leituras de recuperação ficam limitadas ao job ativo: um segundo na
gravação para atualizar o tempo e cinco nas demais fases; não são benchmark de
latência ou prova de consumo nativo.

A transcrição só entra automaticamente no rascunho de origem se sua revisão
e a identidade do destino não mudaram. O cartão mantém o texto para copiar
ou inserir explicitamente em um editor selecionado. Nenhuma dessas ações
envia a mensagem. Fechar o cartão não apaga texto já inserido no editor.
A escolha, a verificação e a remoção da configuração local do modelo ficam
exclusivamente nas preferências, na aba Voz local; mudanças lá avisam a ilha
pelo evento `voice-model-status`. Remover cancela o worker ativo, sem apagar
o arquivo do modelo.

Textos de mensagens recuperáveis pertencem ao shell, em memória, e oferecem
copiar/descartar. Não há reenvio automático ou promessa de persistência após
fechar o aplicativo.

## Evidência e limites

Testes de DOM verificam identidade, rascunho, teclado, atributos acessíveis
e contagem de chamadas. Testes de resize usam latência e falhas controladas.
Esses testes não comprovam geometria, hitbox, permissões ou legibilidade em
WebKit/GTK/AppKit reais. Comparação visual e de desempenho nativa Linux e
macOS permanece necessária; macOS continua experimental.
