import { Window } from "happy-dom";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

let mounted = false;

function mount(page: string, reducedMotion: boolean): void {
  if (mounted) return;
  const html = readFileSync(join(APP_ROOT, page), "utf8").replace(/<link\b[^>]*>/g, "");
  const window = new Window({ url: "http://localhost/" });
  window.document.write(html);
  window.matchMedia = (() => ({
    matches: reducedMotion,
    addEventListener: () => {},
    removeEventListener: () => {},
  })) as unknown as Window["matchMedia"];
  Object.assign(globalThis, {
    window,
    document: window.document,
    navigator: window.navigator,
    screen: window.screen,
    performance: window.performance,
    requestAnimationFrame: window.requestAnimationFrame.bind(window),
    cancelAnimationFrame: window.cancelAnimationFrame.bind(window),
    getComputedStyle: window.getComputedStyle.bind(window),
    matchMedia: window.matchMedia,
    Element: window.Element,
    HTMLElement: window.HTMLElement,
    SVGElement: window.SVGElement,
  });
  mounted = true;
}

export function mountIsland(options: { reducedMotion?: boolean } = {}): void {
  mount("index.html", options.reducedMotion ?? false);
}

export function mountSettings(): void {
  mount("settings.html", false);
}

export function setScreenHeight(height: number): void {
  (window.screen as unknown as { height: number }).height = height;
}
