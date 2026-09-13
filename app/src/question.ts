import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import {
  questionActionsEl,
  questionBodyEl,
  questionCardEl,
  questionCountEl,
  questionKickerEl,
} from "./elements";
import { isRecord, stringField } from "./json";
import { Question, QuestionRequest, QuestionResolved } from "./types";
import { childLabel, jumpToId, render, resetIdle, showCard, showError } from "./main";

export let pendingQuestion: QuestionRequest | null = null;
let questionAnswers: string[][] = [];
let resolvingQuestion = false;
let questionDeadline = 0;
let countdownTimer = 0;
let renderedQuestionKey = "";

export function parseQuestion(value: unknown): QuestionRequest | null {
  const questionId = stringField(value, "question_id");
  const sessionId = stringField(value, "session_id");
  const agent = stringField(value, "agent");
  if (!questionId || !sessionId || !agent || !isRecord(value)) return null;
  const raw = value.questions;
  if (!Array.isArray(raw) || raw.length === 0) return null;
  const questions = raw.map((entry): Question => {
    const options = isRecord(entry) && Array.isArray(entry.options) ? entry.options : [];
    return {
      question: stringField(entry, "question") ?? "",
      ...(stringField(entry, "header") ? { header: stringField(entry, "header") } : {}),
      options: options.flatMap((option) => {
        const label = stringField(option, "label");
        if (!label) return [];
        const description = stringField(option, "description");
        return [{ label, ...(description ? { description } : {}) }];
      }),
      multi_select: isRecord(entry) && entry.multi_select === true,
      custom: isRecord(entry) && entry.custom === true,
    };
  });
  return {
    question_id: questionId,
    session_id: sessionId,
    agent,
    questions,
    answerable: value.answerable === true,
    ...(typeof value.expires_in_ms === "number" ? { expires_in_ms: value.expires_in_ms } : {}),
  };
}

export function parseQuestionResolution(value: unknown): QuestionResolved | null {
  const questionId = stringField(value, "question_id");
  const sessionId = stringField(value, "session_id");
  const outcome = stringField(value, "outcome");
  if (!questionId || !sessionId) return null;
  if (outcome !== "answered" && outcome !== "cancelled" && outcome !== "expired") return null;
  return { question_id: questionId, session_id: sessionId, outcome };
}

const questionQueue: QuestionRequest[] = [];
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
    return;
  }
  pendingQuestion = question;
  questionAnswers = question.questions.map(() => []);
  resolvingQuestion = false;
  clearInterval(countdownTimer);
  questionDeadline = question.expires_in_ms ? Date.now() + question.expires_in_ms : 0;
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
    button.classList.toggle("selected", on);
    button.setAttribute("aria-pressed", String(on));
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
    button.addEventListener("click", () =>
      toggleAnswer(index, option.label, question.multi_select),
    );
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
  questionActionsEl.replaceChildren();
  if (!pendingQuestion) return;

  if (pendingQuestion.answerable) {
    const submit = document.createElement("button");
    submit.type = "button";
    submit.className = "question-submit";
    submit.textContent = strings.question.submit;
    submit.disabled = resolvingQuestion || !answersComplete();
    submit.addEventListener("click", submitAnswers);
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
  jump.addEventListener("click", () => jumpToId(pendingQuestion?.session_id ?? ""));
  questionActionsEl.append(fallback, jump);
}

export function renderQuestion(): void {
  if (!pendingQuestion) {
    showCard(questionCardEl, false, () => {
      questionCardEl.classList.remove("focused");
      questionBodyEl.replaceChildren();
      questionActionsEl.replaceChildren();
    });
    return;
  }
  const renderKey = JSON.stringify({
    question: pendingQuestion,
    resolving: resolvingQuestion,
  });
  showCard(questionCardEl, true);
  questionCardEl.setAttribute("aria-label", strings.question.label);
  const owner = childLabel(pendingQuestion.session_id);
  questionKickerEl.textContent = strings.question.kicker(pendingQuestion.agent) + (owner ? ` · ${owner}` : "");
  questionCountEl.textContent = strings.question.count(pendingQuestion.questions.length);
  if (renderedQuestionKey === renderKey) return;
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
  if (!pendingQuestion || resolvingQuestion || !answersComplete()) return;
  const question = pendingQuestion;
  const answers = questionAnswers.map((labels) => [...labels]);
  resolvingQuestion = true;
  renderQuestion();
  void invoke("answer_question", { questionId: question.question_id, answers })
    .then(() => {
      if (pendingQuestion?.question_id === question.question_id) closeQuestion();
      render();
      resetIdle();
    })
    .catch((error: unknown) => {
      if (pendingQuestion?.question_id !== question.question_id) return;
      resolvingQuestion = false;
      renderQuestion();
      showError(strings.question.failed(String(error)));
    });
}
