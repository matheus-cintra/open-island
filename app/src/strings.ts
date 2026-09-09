const AGENT_LABELS: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  opencode: "OpenCode",
};

const CLAUDE_MODEL = /^claude-([a-z]+)-(\d+)(?:-(\d+))?$/;

export const strings = {
  island: {
    label: "Open Island",
    sessions: (count: number): string =>
      count === 1 ? "1 sessão ativa" : `${count} sessões ativas`,
    compactSessions: (count: number): string => (count === 1 ? "sessão" : "sessões"),
  },
  usage: {
    label: "Limites de uso",
    stale: "antigo",
    unavailable: "sem dados de uso",
    separator: "|",
    resetIn: (milliseconds: number): string => {
      const minutes = Math.max(0, Math.round(milliseconds / 60_000));
      if (minutes < 60) return `${minutes}m`;
      const hours = Math.floor(minutes / 60);
      const rest = minutes % 60;
      if (hours < 24) return rest === 0 ? `${hours}h` : `${hours}h${rest}m`;
      return `${Math.floor(hours / 24)}d`;
    },
    percent: (value: number): string => `${Math.round(value)}%`,
    resetCards: (count: number): string =>
      count === 1 ? "1 Reset Card" : `${count} Reset Cards`,
    credits: (balance: number): string => `${balance.toLocaleString("pt-BR")} créditos`,
    dollars: (balance: number): string =>
      `US$${(balance / 25).toLocaleString("pt-BR", {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
      })}`,
    creditsUnlimited: "créditos ilimitados",
  },
  session: {
    messageOpen: "Mandar mensagem para a sessão",
    messagePlaceholder: "Mensagem para o agente. Enter envia, Shift+Enter quebra a linha.",
    messageQueued: (count: number): string =>
      count === 1 ? "1 mensagem na fila" : `${count} mensagens na fila`,
    messageCancel: "Cancelar esta mensagem",
    messageFailed: (reason: string): string => `Não deu para mandar a mensagem: ${reason}`,
    messageBlocked: (code: string): string =>
      ({
        host_unsupported: "Este terminal não tem como receber texto pela ilha.",
        kitty_remote_control_off:
          "Ligue allow_remote_control e listen_on no kitty para escrever daqui.",
        wezterm_socket_missing: "O socket da GUI do wezterm não foi encontrado.",
        pane_gone: "O pane desta sessão não existe mais.",
      })[code] ?? code,
    promptPrefix: "Você:",
    done: "Concluído",
    tasksLabel: "Tarefas",
    tasks: (done: number, progress: number, open: number): string =>
      `(${done} concluídas, ${progress} em andamento, ${open} em aberto)`,
    subagents: (count: number): string => `Agentes (${count})`,
    subagentTool: "└",
    separator: "·",
    bypass: "BYPASS",
    /// The activity line never goes empty, or the row loses a line and the list stops
    /// having one rhythm. `strings.attention.working` is deliberately blank for the pill.
    working: "trabalhando",
    waitingApproval: "esperando sua permissão",
    waitingAnswer: "esperando sua resposta",
    branchLabel: (branch: string): string => `worktree ${branch}`,
    agent: (agent: string): string => AGENT_LABELS[agent] ?? agent,
    /// `claude-opus-5[1m]` reads as `Opus 5`; anything the pattern does not know keeps the
    /// id the agent reported, minus a context-window suffix and a build date.
    model: (model: string): string => {
      const id = model.replace(/\[[^\]]*\]$/, "").replace(/-\d{8}$/, "");
      const claude = CLAUDE_MODEL.exec(id);
      if (!claude) return id;
      const [, family, major, minor] = claude;
      const name = family.charAt(0).toUpperCase() + family.slice(1);
      return minor ? `${name} ${major}.${minor}` : `${name} ${major}`;
    },
    elapsed: (milliseconds: number): string => {
      const seconds = Math.max(0, Math.floor(milliseconds / 1000));
      if (seconds < 60) return `${seconds}s`;
      const minutes = Math.floor(seconds / 60);
      if (minutes < 60) return `${minutes}m`;
      const hours = Math.floor(minutes / 60);
      return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
    },
  },
  attention: {
    waiting_for_input: "esperando você",
    needs_attention: "parou e ninguém viu",
    working: "",
    idle: "parada",
  },
  approval: {
    label: "Permissão pendente",
    actionsLabel: "Ações da permissão",
    kicker: "Permissão solicitada",
    missingTool: "Ferramenta do agente",
    fallback: "Aguardando sua decisão",
    planTool: "Plano",
    planFallback: "O agente quer sair do modo plano",
    allow: "Permitir",
    always: "Sempre",
    alwaysTitle: "Permitir e não perguntar de novo nesta sessão",
    deny: "Negar",
    diffLabel: "Alteração proposta",
    invalid: "Pedido de permissão inválido",
    failed: (decision: "allow" | "deny" | "allow_always", reason: string): string =>
      `Falha ao ${decision === "deny" ? "negar" : "permitir"}: ${reason}`,
  },
  question: {
    label: "Pergunta pendente",
    kicker: (agent: string): string => `${agent} está esperando uma resposta`,
    count: (count: number): string =>
      count === 1 ? "1 pergunta" : `${count} perguntas`,
    optionsLabel: "Opções da resposta",
    multiSelectHint: "Escolha uma ou mais",
    customPlaceholder: "Outra resposta",
    submit: "Responder",
    terminalFallback: "Responda no terminal do agente",
    countdown: (seconds: number): string => `expira em ${seconds}s`,
    jump: "Ir para a sessão",
    invalid: "Pergunta inválida",
    failed: (reason: string): string => `Falha ao responder: ${reason}`,
  },
  jump: {
    failed: (title: string, reason: string): string =>
      `Falha ao focar ${title}: ${reason}`,
  },
  header: {
    settings: "Abrir ajustes",
    mute: "Silenciar os sons",
    unmute: "Voltar a tocar os sons",
    update: (version: string): string => `${version} disponível. Clique para atualizar.`,
    updatePrompt: "Pressione Enter para fechar.",
    updateFailed: (reason: string): string =>
      `Não deu para abrir um terminal (${reason}). Rode você mesmo: curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh`,
  },
  settings: {
    title: "Ajustes",
    panes: {
      general: "Geral",
      integrations: "Integrações",
      display: "Exibição",
      sound: "Som",
      usage: "Uso",
      filters: "Notificações",
      about: "Sobre",
    },
    about: {
      name: "Open Island",
      removeAutoConfig: "Remover Toda a Configuração Automática",
      removeAutoConfigConfirm: "Tocar de novo para remover",
      removeAutoConfigHint:
        "Tira os hooks de todos os agentes configurados, o atalho do Hyprland e as duas units do systemd. A ilha e o daemon são encerrados no processo.",
      removeDone: (count: number): string =>
        count === 1
          ? "1 arquivo removido. O autostart está sendo desfeito e a ilha vai fechar."
          : `${count} arquivos removidos. O autostart está sendo desfeito e a ilha vai fechar.`,
      quit: "Sair do Open Island",
      quitConfirm: "Tocar de novo para sair",
      acknowledgements: "Agradecimentos",
      credits: "Departure Mono, Tauri, GTK",
      updateCheck: "Avisar quando sair uma versão nova",
      updateCheckHint:
        "Uma vez por dia o daemon pergunta ao GitHub qual é a versão publicada mais recente. Nada seu vai junto. Desligue para que essa consulta não seja feita.",
      updateAvailable: "Versão nova disponível",
      updateAvailableHint:
        "Para atualizar, rode de novo: curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh",
      actionFailed: (reason: string): string => `Não deu para concluir: ${reason}`,
    },
    filters: {
      panel: "Notificações do painel",
      panelFooter: "Estas configurações não mudam o comportamento das aprovações.",
      expandOnCompletion: "Expandir o painel para notificações de conclusão",
      expandOnCompletionHint:
        "Desligue para manter o painel recolhido quando uma sessão termina.",
      expandOnQuestion: "Expandir o painel para perguntas",
      expandOnQuestionHint: "Desligue para manter o painel recolhido até você abri-lo.",
      subagentTiming: "Subagentes e Agent Team",
      subagentRoot: "Conforme o agente principal responde",
      subagentAllFinished: "Depois que todos os subagentes terminam",
      subagentEvery: "A cada subagente concluído",
      reminders: "Lembretes de acompanhamento",
      reminderDelay: "Lembrar novamente",
      reminderDelayHint:
        "Lembra uma vez depois do atraso escolhido. Mudança de estado cancela; a sessão que você está olhando fica em silêncio.",
      reminderOff: "Desativado",
      reminderAfter: (label: string): string => `Depois de ${label}`,
      reminderInclude: "Quando ativado, incluir",
      reminderNeedsResponse: "Precisa da sua resposta",
      reminderNeedsResponseHint: "Aprovações e perguntas.",
      reminderCompletedTasks: "Tarefas concluídas",
      reminderCompletedTasksHint: "Só enquanto o resultado não foi lido.",
      quietScenes: "Cenários silenciosos",
      quietScenesHint:
        "Fica em silêncio enquanto qualquer cenário abaixo estiver ativo — sem expansão automática, sem som, incluindo aprovações.",
      quietFocusMode: "Modo Foco",
      quietFocusModeHint: "Enquanto o Não Perturbe do swaync estiver ligado.",
      quietScreenOff: "Tela apagada ou bloqueada",
      quietScreenOffHint:
        "Enquanto todos os monitores estiverem com o DPMS desligado, ou a sessão marcada como bloqueada.",
      launchers: "Aplicativos iniciadores bloqueados",
      launchersHint:
        "Descarta a sessão iniciada pelo aplicativo escolhido antes que ela apareça na ilha.",
      launchersFooter:
        "Use para sonda em segundo plano e aplicativo auxiliar. Filtro normal de projeto vai em Diretório ou Primeiro prompt.",
      launchersEmpty: "Nenhum aplicativo iniciador bloqueado.",
      launcherPlaceholder: "ex.: kitty",
      launcherSeen: "Visto agora",
      directory: "Diretório",
      directoryHint: "Oculta a sessão cujo diretório de trabalho casar com o padrão.",
      prompt: "Primeiro prompt",
      promptHint: "Oculta a sessão cujo primeiro prompt do usuário casar com o padrão.",
      footer:
        "A sessão filtrada não aparece na ilha, não toca som e não segura o agente esperando aprovação.",
      preset: "Predefinido",
      add: "Adicionar",
      remove: "Remover",
      empty: "Nenhum filtro aqui ainda.",
      patternCwd: "ex.: /chronicle/dev-experiments",
      patternPrompt: "ex.: ## Memory Writing Agent",
      matchContains: "Contém",
      matchPrefix: "Começa com",
      matchEquals: "É igual a",
    },
    usage: {
      limits: "Limites de Uso",
      showLimits: "Mostrar Limites de Uso",
      showLimitsHint: "Exibe os limites de uso da assinatura no cabeçalho do painel da ilha.",
      useClaudeLogin: "Usar o login do Claude Code para o uso",
      useClaudeLoginHint:
        "Lê o login do Claude Code guardado nesta máquina só para buscar o uso do seu plano na Anthropic. Nada é enviado para outro lugar. Desligue para cortar todo o acesso à credencial; o uso do Codex continua.",
      valueMode: "Valor Exibido",
      valueUsed: "Usado",
      valueRemaining: "Restante",
      preferredProvider: "Provedor Preferido",
      providerAuto: "Auto (seguir sessão)",
      providerAnthropic: "Anthropic",
      providerCodex: "Codex",
      showResetCards: "Mostrar Reset Cards",
      showResetCardsHint:
        "Mostra os Reset Cards do Codex disponíveis e o vencimento mais próximo no cabeçalho de uso.",
      codexCredits: "Créditos extras do Codex",
      codexCreditsCredits: "Créditos",
      codexCreditsDollars: "US$",
      codexCreditsHint: "A estimativa usa 25 credits = US$1. A taxa pode mudar.",
      alert: "Aviso",
      warnThreshold: "Tocar o som de limite ao passar de",
      warnThresholdHint: "O som escolhido na aba Som, uma vez por travessia e não a cada leitura.",
      refreshInterval: "Buscar o uso a cada",
      bridge: "De onde vem o número",
      bridgeAnthropic:
        "A Anthropic responde o uso do plano para o token que o Claude Code já guardou em ~/.claude/.credentials.json. A ilha só lê esse arquivo, nunca escreve nem renova.",
      bridgeCodex:
        "O Codex responde pelo próprio codex app-server, então a ilha não toca na credencial dele e não faz chamada de rede nenhuma por ele.",
    },
    integrations: {
      agents: "Agentes",
      claude: "Claude Code",
      codex: "Codex",
      opencode: "OpenCode",
      agentsFooter:
        "A ilha instala os hooks sozinha em cada agente que encontra. Desligue o que não quiser.",
      autoConfigure: "Configurar novos agentes automaticamente",
      autoConfigureHint: "Configure automaticamente os agentes compatíveis recém-detectados.",
      codexSteps: "Passos manuais do Codex",
      codexTrust:
        'Na próxima vez que o Codex abrir, ele mostra "Hooks need review". Aperte t para confiar nos hooks da ilha; sem isso eles ficam instalados e nunca rodam.',
      codexUserInput:
        "As perguntas do agente precisam de experimental_request_user_input=true e features.default_mode_request_user_input=true no ~/.codex/config.toml.",
    },
    display: {
      notch: "Ilha",
      compactClean: "Limpo",
      compactCleanHint: "Só o ícone do agente e a contagem.",
      compactDetailed: "Detalhado",
      compactDetailedHint: "Com o nome do projeto antes da contagem.",
      monitor: "Tela",
      monitorAuto: "Automática",
      monitorHint: "Em qual saída a ilha aparece. Automática segue o compositor.",
      notchTuning: "Ajuste fino",
      notchWidth: "Largura da ilha",
      notchHeight: "Altura da ilha",
      islandHeight: "Altura fixa da ilha",
      islandHeightHint:
        "Fixa a altura da ilha em pixels. 0 usa a altura que o compositor reserva.",
      notchTuningHint:
        "Corrige a ilha quando ela não encaixa na sua barra. 0 usa o valor do compositor.",
      sessionCard: "Linha da sessão",
      project: "Mostrar o nome do projeto",
      worktree: "Mostrar o worktree",
      agentIcons: "Mostrar agentes como ícones",
      agentIconsHint: "Liga o ícone do agente no lugar da pílula com o nome.",
      terminalIcons: "Mostrar terminais como ícones",
      terminalIconsHint: "Usa o ícone do próprio aplicativo. Sem ícone instalado, fica a pílula.",
      model: "Mostrar o modelo",
      effort: "Mostrar o esforço de raciocínio",
      effortHint: "O Claude e o OpenCode informam; o Codex não.",
      tasks: "Mostrar tarefas",
      tasksHint: "Mostra a lista de tarefas em cada cartão de sessão.",
      activity: "Mostrar o detalhe da atividade",
      activityHint: "A terceira linha: a ferramenta em uso, ou a última resposta do agente.",
      subagents: "Mostrar subagentes",
      subagentsHint: "O que cada agente filho está fazendo. Desligado, fica só a contagem.",
      size: "Tamanho",
      panelSize: "Tamanho do painel",
      contentFont: "Tamanho da fonte do conteúdo",
      contentFontHint: "Vale para o texto da linha de sessão. O cabeçalho e a pílula não mudam.",
      panelMaxWidth: "Largura máxima do painel",
      panelMaxHeight: "Altura máxima do painel",
      panelMaxHeightHint: "Teto real; a ilha nunca passa de um terço da altura da tela.",
      completionCardHeight: "Altura do cartão de conclusão",
      uiScale: "Escala da interface",
      uiScaleHint: "Automática segue a densidade da tela. A ilha aplica na hora.",
      uiScaleAuto: (value: string | null): string =>
        value === null ? "Automática" : `Automática (${value})`,
      uiScaleValue: (value: number): string =>
        `${value.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}×`,
    },
    status: {
      active: "Ativo",
      notDetected: "Não detectado",
    },
    stepper: {
      increase: "Aumentar",
      decrease: "Diminuir",
    },
    pickerDefault: (label: string): string => `${label} (Padrão)`,
    lockedByEnv: (variable: string): string => `Definido por ${variable}`,
    loadFailed: (reason: string): string => `Não deu para ler os ajustes: ${reason}`,
    saveFailed: (reason: string): string => `Não deu para salvar: ${reason}`,
    general: {
      system: "Sistema",
      autostart: "Iniciar com a sessão",
      autostartHint:
        "Sobe o daemon e a ilha junto com o Hyprland, e põe o Open Island no lançador de aplicativos.",
      hyprland: "Integração com o Hyprland",
      hyprlandHint:
        "Instala o atalho SUPER + I e a regra de janela desta tela. Reabra esta janela depois de instalar.",
      island: "Ilha",
      expandOnHover: "Expandir ao passar o mouse",
      collapseOnLeave: "Recolher ao tirar o mouse",
      hideInFullscreen: "Sumir com janela em tela cheia",
      hideInFullscreenHint: "Vale para qualquer janela em tela cheia, não só vídeo.",
      hideWhenIdle: "Sumir quando ociosa",
      hideWhenIdleHint: "Some depois de 30s sem nenhum agente trabalhando. Volta quando algum voltar.",
      clickToJump: "Clicar na sessão pula para o terminal",
      clickToJumpHint: "Desligado, a linha continua clicável mas não muda o foco.",
      cleanupAfter: "Sumir com a sessão parada depois de",
      cleanupAfterHint: "Só some da lista; volta inteira se o agente responder. 0 nunca some.",
      hoverDwell: "Expandir quando o ponteiro parar por",
      smartSuppression: "Supressão inteligente",
      smartSuppressionHint:
        "Não expandir automaticamente quando o terminal do agente já estiver em foco.",
      autoCollapse: "Fechar sozinha depois de",
      idleFade: "Escurecer quando ociosa depois de",
      sessions: "Sessões",
      idleAfter: "Considerar a sessão ociosa depois de",
      idleAfterHint: "Vale para a cor da bolinha na linha da sessão.",
      idleReminderAfter: "Tocar o lembrete de sessão parada depois de",
      idleReminderAfterHint:
        "O som escolhido na aba Som. A ilha não manda card para o centro de notificações.",
    },
    sound: {
      output: "Saída",
      enabled: "Ativar sons",
      behaviour: "Como tocar",
      volume: "Volume",
      quiet: "Modo silencioso",
      quietHint: "Cala todos os sons. A ilha continua mostrando tudo.",
      followDnd: "Respeitar o Não Perturbe",
      followDndHint: "Não toca nada enquanto a swaync estiver em Não Perturbe.",
      events: "Som por evento",
      off: "Desligado",
      sessionStart: "Sessão começou",
      taskComplete: "Agente terminou",
      approvalNeeded: "Permissão ou pergunta",
      taskAcknowledge: "Prompt enviado",
      idleReminder: "Lembrete de sessão parada",
      contextLimit: "Limite de uso perto do fim",
      userSpam: "Prompts em rajada",
      userSpamHint: "Toca quando você dispara vários prompts em poucos segundos.",
      preview: (label: string): string => `Ouvir: ${label}`,
      previewFailed: (reason: string): string => `Não deu para tocar: ${reason}`,
      quietHours: "Horário silencioso",
      quietHoursHint: "Cala tudo no período. Se o fim vier antes do início, passa da meia-noite.",
      quietHoursStart: "Começa às",
      quietHoursEnd: "Termina às",
      spam: "Rajada de prompts",
      spamThreshold: "Considerar rajada a partir de",
      spamThresholdUnit: "prompts",
      spamWindow: "Dentro de",
      mySounds: "Meus sons",
      mySoundsHint: (dir: string): string =>
        `Ponha arquivos .oga, .ogg, .wav, .flac ou .mp3 em ${dir} e eles aparecem nas listas acima.`,
    },
    units: {
      milliseconds: "ms",
      seconds: "s",
      minutes: "min",
      percent: "%",
    },
  },
} as const;
