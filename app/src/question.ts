import { pendingKey } from "./island-state";
import type { ActionIdentity } from "./daemon-state";
import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import {
  questionActionsEl,
  questionBodyEl,
  questionCardEl,
  questionCountEl,
  questionKickerEl,
} from "./elements";
import { Question, QuestionRequest } from "./types";

interface QuestionContext {
  childLabel(id: string): string;
  jumpToId(id: string, expected?: ActionIdentity): void;
  render(): void;
  resetIdle(): void;
  showCard(card: HTMLElement, visible: boolean, onHidden?: () => void): void;
  showError(message: string): void;
}
let context: QuestionContext;
export function initializeQuestions(value: QuestionContext): void { context = value; }

let connected = false;
export function setQuestionConnection(online: boolean): void {
  if (connected === online) return;
  connected = online;
  context.render();
}
function guardedReady(question: QuestionRequest | null): boolean {
  return connected && question?.action_identity !== undefined && question.pending_generation !== undefined;
}

export let pendingQuestion: QuestionRequest | null = null;
let questionAnswers: string[][] = [];
let resolvingQuestion = false;
let questionDeadline = 0;
let countdownTimer = 0;
let renderedQuestionKey = "";
let renderedActionsKey = "";

const questionQueue: QuestionRequest[] = [];
export function replaceQuestions(questions: QuestionRequest[]): void {
  const next = questions.find((question): boolean => pendingKey(question) === pendingKey(pendingQuestion)) ?? questions[0];
  questionQueue.length = 0;
  if (!next) { closeQuestion(); return; }
  if (pendingKey(next) !== pendingKey(pendingQuestion)) closeQuestion();
  openQuestion(next);
  questionQueue.push(...questions.filter((question): boolean => question !== next));
}

export function openQuestion(question: QuestionRequest): void {
  if (pendingQuestion && pendingQuestion.question_id !== question.question_id) {
    if (!questionQueue.some((entry) => entry.question_id === question.question_id)) {
      questionQueue.push(question);
    }
    return;
  }
  if (pendingQuestion?.question_id === question.question_id) {
    // Repeated hook delivery can update text, but must not erase a draft or restart its deadline.
    pendingQuestion = question;
    if (question.expires_in_ms !== undefined) {
      const remainingDeadline = Date.now() + Math.max(0, question.expires_in_ms);
      questionDeadline = questionDeadline === 0 ? remainingDeadline : Math.min(questionDeadline, remainingDeadline);
    }
    return;
  }
  pendingQuestion = question;
  questionAnswers = question.questions.map(() => []);
  resolvingQuestion = false;
  clearInterval(countdownTimer);
  questionDeadline = question.expires_in_ms !== undefined ? Date.now() + Math.max(0, question.expires_in_ms) : 0;
  if (questionDeadline > 0) {
    countdownTimer = window.setInterval(tickCountdown, 1000);
  }
}

export function closeQuestion(id?: string): void {
  if (id && pendingQuestion?.question_id !== id) {
    const index = questionQueue.findIndex((entry) => entry.question_id === id);
    if (index >= 0) questionQueue.splice(index, 1);
    return;
  }
  pendingQuestion = null;
  questionAnswers = [];
  resolvingQuestion = false;
  questionDeadline = 0;
  clearInterval(countdownTimer);
  renderedQuestionKey = "";
  const next = questionQueue.shift();
  if (next) openQuestion(next);
}

function toggleAnswer(index: number, label: string, multiSelect: boolean): void {
  const current = questionAnswers[index] ?? [];
  if (!multiSelect) {
    questionAnswers[index] = current[0] === label ? [] : [label];
  } else if (current.includes(label)) {
    questionAnswers[index] = current.filter((entry) => entry !== label);
  } else {
    questionAnswers[index] = [...current, label];
  }
  paintAnswers(index);
  renderQuestionActions();
}

function paintAnswers(index: number): void {
  const options = questionBodyEl.querySelector<HTMLElement>(
    `.question-options[data-question="${index}"]`,
  );
  if (options === null) return;
  const selected = questionAnswers[index] ?? [];
  for (const button of options.querySelectorAll<HTMLButtonElement>(".question-option")) {
    const on = selected.includes(button.dataset.option ?? "");
    if (button.classList.contains("selected") !== on) button.classList.toggle("selected", on);
    const pressed = String(on);
    if (button.getAttribute("aria-pressed") !== pressed) button.setAttribute("aria-pressed", pressed);
  }
  const custom = questionBodyEl.querySelector<HTMLInputElement>(
    `.question-custom[data-question="${index}"]`,
  );
  if (custom !== null && selected.length > 0) custom.value = "";
}

function answersComplete(): boolean {
  return questionAnswers.every((labels) => labels.length > 0);
}

function questionItem(question: Question, index: number): HTMLDivElement {
  const ownerKey = pendingKey(pendingQuestion);
  const current = (): boolean => ownerKey === pendingKey(pendingQuestion) && !resolvingQuestion;
  const item = document.createElement("div");
  item.className = "question-item";

  const text = document.createElement("span");
  text.className = "question-text";
  text.textContent = question.header
    ? `${question.header} — ${question.question}`
    : question.question;
  item.append(text);

  const options = document.createElement("div");
  options.className = "question-options";
  options.dataset.question = String(index);
  options.setAttribute("aria-label", strings.question.optionsLabel);
  for (const option of question.options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "question-option";
    button.dataset.option = option.label;
    button.textContent = option.label;
    if (option.description) button.title = option.description;
    const selected = (questionAnswers[index] ?? []).includes(option.label);
    button.classList.toggle("selected", selected);
    button.setAttribute("aria-pressed", String(selected));
    button.disabled = resolvingQuestion || !pendingQuestion?.answerable;
    button.addEventListener("click", (): void => {
      if (current()) toggleAnswer(index, option.label, question.multi_select);
    });
    options.append(button);
  }
  item.append(options);

  if (question.multi_select) {
    const hint = document.createElement("span");
    hint.className = "question-hint";
    hint.textContent = strings.question.multiSelectHint;
    item.append(hint);
  }

  if (question.custom && pendingQuestion?.answerable) {
    const custom = document.createElement("input");
    custom.type = "text";
    custom.className = "question-custom";
    custom.dataset.question = String(index);
    custom.placeholder = strings.question.customPlaceholder;
    custom.disabled = resolvingQuestion;
    const selected = questionAnswers[index] ?? [];
    const optionLabels = new Set(question.options.map((option) => option.label));
    const savedCustom = selected.find((answer) => !optionLabels.has(answer));
    if (savedCustom) custom.value = savedCustom;
    custom.addEventListener("input", () => {
      if (!current()) return;
      const value = custom.value.trim();
      questionAnswers[index] = value ? [value] : [];
      renderQuestionActions();
      for (const button of options.querySelectorAll("button")) {
        button.classList.remove("selected");
        button.setAttribute("aria-pressed", "false");
      }
    });
    item.append(custom);
  }

  return item;
}

function countdownText(): string {
  const remaining =
    questionDeadline > 0 ? Math.max(0, Math.ceil((questionDeadline - Date.now()) / 1000)) : 0;
  return remaining > 0
    ? `${strings.question.terminalFallback} · ${strings.question.countdown(remaining)}`
    : strings.question.terminalFallback;
}

function tickCountdown(): void {
  const fallback = questionActionsEl.querySelector<HTMLElement>(".question-fallback");
  if (fallback === null) {
    renderQuestionActions();
    return;
  }
  const text = countdownText();
  if (fallback.textContent !== text) fallback.textContent = text;
}

function renderQuestionActions(): void {
  const key = JSON.stringify([
    pendingKey(pendingQuestion),
    pendingQuestion?.answerable ?? false,
    resolvingQuestion,
    connected,
    questionAnswers,
  ]);
  if (renderedActionsKey === key) return;
  renderedActionsKey = key;
  questionActionsEl.replaceChildren();
  if (!pendingQuestion) return;

  if (pendingQuestion.answerable) {
    const submit = document.createElement("button");
    submit.type = "button";
    submit.className = "question-submit";
    submit.textContent = strings.question.submit;
    submit.disabled = resolvingQuestion || !answersComplete() || !guardedReady(pendingQuestion);
    const ownerKey = pendingKey(pendingQuestion);
    submit.addEventListener("click", (): void => {
      if (ownerKey === pendingKey(pendingQuestion)) submitAnswers();
    });
    questionActionsEl.append(submit);
    return;
  }

  const fallback = document.createElement("span");
  fallback.className = "question-fallback";
  fallback.textContent = countdownText();
  const jump = document.createElement("button");
  jump.type = "button";
  jump.className = "question-jump";
  jump.textContent = strings.question.jump;
  const question = pendingQuestion;
  jump.disabled = !guardedReady(question);
  jump.addEventListener("click", (): void => {
    if (!guardedReady(question) || pendingKey(question) !== pendingKey(pendingQuestion)) return;
    context.jumpToId(question.session_id, question.action_identity);
  });
  questionActionsEl.append(fallback, jump);
}

export function renderQuestion(): void {
  if (!pendingQuestion) {
    context.showCard(questionCardEl, false, () => {
      questionCardEl.classList.remove("focused");
      questionBodyEl.replaceChildren();
      questionActionsEl.replaceChildren();
    });
    return;
  }
  const renderKey = JSON.stringify({
    question: { ...pendingQuestion, expires_in_ms: undefined },
    resolving: resolvingQuestion,
  });
  context.showCard(questionCardEl, true);
  if (questionCardEl.getAttribute("aria-label") !== strings.question.label) {
    questionCardEl.setAttribute("aria-label", strings.question.label);
  }
  const owner = context.childLabel(pendingQuestion.session_id);
  const kicker = strings.question.kicker(pendingQuestion.agent) + (owner ? ` · ${owner}` : "");
  if (questionKickerEl.textContent !== kicker) questionKickerEl.textContent = kicker;
  const count = strings.question.count(pendingQuestion.questions.length);
  if (questionCountEl.textContent !== count) questionCountEl.textContent = count;
  if (renderedQuestionKey === renderKey) { renderQuestionActions(); return; }
  const focused = document.activeElement;
  const focusedIndex = focused instanceof HTMLElement && focused.matches(".question-custom")
    ? focused.dataset.question
    : undefined;
  questionBodyEl.replaceChildren(...pendingQuestion.questions.map(questionItem));
  if (focusedIndex !== undefined) {
    questionBodyEl.querySelector<HTMLInputElement>(`.question-custom[data-question="${focusedIndex}"]`)?.focus();
  }
  renderQuestionActions();
  renderedQuestionKey = renderKey;
}

function submitAnswers(): void {
  if (!pendingQuestion || resolvingQuestion || !answersComplete() || !guardedReady(pendingQuestion)) return;
  const question = pendingQuestion;
  const answers = questionAnswers.map((labels) => [...labels]);
  resolvingQuestion = true;
  renderQuestion();
  void invoke("answer_question_v2", { questionId: question.question_id, answers, identity: question.action_identity, pendingGeneration: question.pending_generation })
    .then(() => {
      if (pendingKey(pendingQuestion) === pendingKey(question)) closeQuestion();
      context.render();
      context.resetIdle();
    })
    .catch((error: unknown) => {
      if (pendingKey(pendingQuestion) !== pendingKey(question)) return;
      resolvingQuestion = false;
      renderQuestion();
      context.showError(strings.question.failed(String(error)));
    });
}
