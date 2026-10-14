// Floating windows (Quick, tray menu, popup) are transparent and sized to
// what they show: the card, plus a menu hanging below it, plus the body
// padding that leaves room for the shadow.

import { inApp } from './ipc';

/** Keeps the window the size of the card (and any open `.menu`). Returns a
    function that re-fits on demand. `card` can be a getter: a page that
    swaps its card (Quick's Окно / Строка) is measured by the one shown now
    — the old, detached one measures 0 × 0 and shrank the window to a
    sliver. */
export async function fitWindow(card: HTMLElement | (() => HTMLElement | undefined)): Promise<() => void> {
  if (!inApp) return () => {};
  const { getCurrentWindow, LogicalSize } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  const pad = getComputedStyle(document.body);
  const current = typeof card === 'function' ? card : () => card;
  let watched: HTMLElement | undefined;
  const observer = new ResizeObserver(() => fit());
  const fit = () => {
    const card = current();
    if (!card?.isConnected) return;
    if (card !== watched) {
      if (watched) observer.unobserve(watched);
      observer.observe(card);
      watched = card;
    }
    const menu = document.querySelector('.menu');
    const rect = card.getBoundingClientRect();
    const bottom = Math.max(rect.bottom, menu?.getBoundingClientRect().bottom ?? 0);
    const right = Math.max(rect.right, menu?.getBoundingClientRect().right ?? 0);
    void win.setSize(
      new LogicalSize(Math.ceil(right + parseFloat(pad.paddingRight)), Math.ceil(bottom + parseFloat(pad.paddingBottom))),
    );
  };
  window.addEventListener('hl:layout', fit);
  fit();
  return fit;
}

/** Hides the window when it loses focus (after a short grace period: a
    tray click hands focus back to the taskbar right after opening). */
export async function hideOnBlur(keep: () => boolean = () => false): Promise<{ shown: () => void }> {
  let shownAt = performance.now();
  if (!inApp) return { shown: () => {} };
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  await win.onFocusChanged(({ payload: focused }) => {
    if (focused || keep()) return;
    if (performance.now() - shownAt < 600) {
      void win.setFocus();
      return;
    }
    void win.hide();
  });
  return { shown: () => (shownAt = performance.now()) };
}

/** Runs `f` every time Rust shows this window. */
export async function onShown<T>(event: string, f: (payload: T) => void) {
  if (!inApp) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().listen<T>(event, (e) => f(e.payload));
}

export async function hideWindow() {
  if (!inApp) return;
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  await getCurrentWindow().hide();
}
