// Starts a window page: base styles, settings (theme, language), the
// component, then tells Rust the page is drawn so the window can show.

import './base.css';
import { mount, tick, type Component } from 'svelte';
import { inApp } from './ipc';
import { loadSettings } from './settings.svelte';

export async function boot(App: Component) {
  await loadSettings();
  mount(App, { target: document.body });
  await tick();
  // Two frames: the first paint has happened.
  requestAnimationFrame(() =>
    requestAnimationFrame(async () => {
      if (!inApp) return;
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('ready');
    }),
  );
}
