import { Window } from "happy-dom";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const APP_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

let mounted = false;
const frames = new Map<number, FrameRequestCallback>();
let cancelFrame: (id: number) => void = (): void => {};
/** Simulate one browser paint boundary; callbacks scheduled during it wait for the next frame. */
export function paintFrame(): void {
  const ready = [...frames];
  for (const [id, callback] of ready) {
    if (!frames.delete(id)) continue;
    cancelFrame(id);
    callback(performance.now());
  }
}

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
  const requestFrame = window.requestAnimationFrame.bind(window);
  let nextFrame = 0;
  const nativeFrames = new Map<number, ReturnType<typeof requestFrame>>();
  cancelFrame = (id): void => {
    const handle = nativeFrames.get(id);
    if (handle !== undefined) window.cancelAnimationFrame(handle);
    nativeFrames.delete(id);
  };
  Object.assign(globalThis, {
    window,
    document: window.document,
    navigator: window.navigator,
    screen: window.screen,
    performance: window.performance,
    requestAnimationFrame: (callback: FrameRequestCallback): number => {
      const id = ++nextFrame;
      nativeFrames.set(id, requestFrame((time): void => { frames.delete(id); nativeFrames.delete(id); callback(time); }));
      frames.set(id, callback); return id;
    },
    cancelAnimationFrame: (id: number): void => { frames.delete(id); cancelFrame(id); },
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
