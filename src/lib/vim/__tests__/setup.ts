import { Window } from "happy-dom";

const window = new Window();
Object.assign(globalThis, {
  window,
  document: window.document,
  navigator: window.navigator,
  HTMLElement: window.HTMLElement,
  HTMLDivElement: window.HTMLDivElement,
  Event: window.Event,
});
