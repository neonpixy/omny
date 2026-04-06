/**
 * Castle HMR — listens for file change events from the Rust watcher
 * and reloads the appropriate resources.
 *
 * CSS changes: hot-swap the stylesheet (no page reload)
 * JS/HTML changes: full page reload (programs are lazy-loaded, so it's fast)
 */
import { listen } from '@tauri-apps/api/event';

listen<string>('castle:reload', (event) => {
  const path = event.payload;
  console.log(`[hmr] changed: ${path}`);

  if (path.endsWith('.css')) {
    // Hot-swap: find the stylesheet link and bust its cache
    const links = document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]');
    for (const link of links) {
      const href = link.getAttribute('href');
      if (href) {
        const base = href.split('?')[0];
        link.href = `${base}?t=${Date.now()}`;
      }
    }
  } else {
    // JS or HTML change — full reload
    location.reload();
  }
});
