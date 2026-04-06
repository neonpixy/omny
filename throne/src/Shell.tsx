import { createSignal, createEffect, onMount, batch, type ParentProps } from 'solid-js';
import { useNavigate, useLocation } from '@solidjs/router';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { TrafficLights } from './chrome/TrafficLights';
import { CircleButton } from './chrome/CircleButton';
import { AddressBar } from './chrome/AddressBar';
import { CrownMenu } from './chrome/CrownMenu';
import { TabBar, type Tab } from './chrome/TabBar';
import { Dock } from './chrome/Dock';
import { initIdentity } from './stores/identity';

let nextTabId = 1;

function pathToLabel(path: string): string {
  if (!path || path === '/') return 'Home';
  return path.slice(1).split('-').map(w => w[0]?.toUpperCase() + w.slice(1)).join(' ');
}

const Shell = (props: ParentProps) => {
  const [daemonConnected, setDaemonConnected] = createSignal(false);
  const [tabs, setTabs] = createSignal<Tab[]>([]);
  const [activeTabId, setActiveTabId] = createSignal<string>('');
  const navigate = useNavigate();
  const location = useLocation();

  // When the route changes, update the active tab to reflect the new path
  createEffect(() => {
    const path = location.pathname;
    if (!path || path === '/') return;

    const current = tabs();
    const id = activeTabId();

    // If no tabs yet, create the first one
    if (current.length === 0) {
      const newId = String(nextTabId++);
      setTabs([{ id: newId, path, label: pathToLabel(path) }]);
      setActiveTabId(newId);
      return;
    }

    // If a different tab already points to this path, switch to it
    const existing = current.find((t) => t.path === path);
    if (existing) {
      setActiveTabId(existing.id);
      return;
    }

    // Otherwise update the active tab's path (dock/nav changed it)
    setTabs((prev) =>
      prev.map((t) => t.id === id ? { ...t, path, label: pathToLabel(path) } : t),
    );
  });

  function handleNewTab() {
    const id = String(nextTabId++);
    const path = '/home';
    setTabs((prev) => [...prev, { id, path, label: pathToLabel(path) }]);
    setActiveTabId(id);
    navigate(path);
  }

  function handleCloseTab(id: string) {
    const current = tabs();
    const idx = current.findIndex((t) => t.id === id);
    if (idx === -1) return;

    const next = current.filter((t) => t.id !== id);

    if (next.length === 0) {
      handleNewTab();
      return;
    }

    if (id === activeTabId()) {
      const target = next[Math.min(idx, next.length - 1)];
      batch(() => {
        setTabs(next);
        setActiveTabId(target.id);
      });
      navigate(target.path);
    } else {
      setTabs(next);
    }
  }

  // Clicking a tab switches to it
  function handleTabClick(tab: Tab) {
    setActiveTabId(tab.id);
    navigate(tab.path);
  }

  // Gate: no crown → crown-setup, locked → crown-unlock
  async function checkCrownGate() {
    try {
      const result = await window.omninet.run({
        method: 'chamberlain.state',
        params: {},
      }) as any;

      const path = location.pathname;
      if (!result.exists) {
        if (path !== '/crown-setup') navigate('/crown-setup', { replace: true });
      } else if (!result.unlocked) {
        if (path !== '/crown-unlock') navigate('/crown-unlock', { replace: true });
      } else if (path === '/' || path === '/crown-setup' || path === '/crown-unlock') {
        const lastPage = sessionStorage.getItem('omny:lastPage');
        if (lastPage) {
          sessionStorage.removeItem('omny:lastPage');
          navigate(lastPage, { replace: true });
        } else {
          navigate('/home', { replace: true });
        }
      }
    } catch (e) {
      console.warn('Crown gate check failed (daemon not ready?):', e);
    }
  }

  onMount(async () => {
    initIdentity();

    try {
      const result = await window.omninet.platform('daemon.status') as any;
      setDaemonConnected(result.ok === true);
    } catch {
      setDaemonConnected(false);
    }

    await checkCrownGate();

    // Re-gate when crown is deleted or locked
    window.omninet.on('crown/deleted', () => checkCrownGate());
    window.omninet.on('crown/locked', () => checkCrownGate());
    window.omninet.on('crown/created', () => checkCrownGate());
    window.omninet.on('crown/unlocked', () => checkCrownGate());
  });

  return (
    <div class="shell">
      <header class="chrome-header" onMouseDown={(e) => {
        if ((e.target as HTMLElement).closest('button, a, .chrome-address-bar, .traffic-lights')) return;
        e.preventDefault();
        getCurrentWindow().startDragging();
      }}>
        <div class="chrome-titlebar-inner">
          <div class="chrome-titlebar-left">
            <TrafficLights />
            <CircleButton icon="arrow-left-s-line" onClick={() => history.back()} title="Back" />
            <CircleButton icon="arrow-right-s-line" onClick={() => history.forward()} title="Forward" />
          </div>
          <AddressBar />
          <div class="chrome-titlebar-right">
            <div
              class="chrome-status-dot"
              classList={{ connected: daemonConnected(), disconnected: !daemonConnected() }}
              title={daemonConnected() ? 'Daemon connected' : 'Daemon offline'}
            />
            <CrownMenu />
          </div>
        </div>

        <div class="chrome-divider" />

        <div class="chrome-tabrow-inner">
          <TabBar
            tabs={tabs()}
            activeTabId={activeTabId()}
            onNewTab={handleNewTab}
            onCloseTab={handleCloseTab}
            onTabClick={handleTabClick}
          />
        </div>
      </header>

      <main class="shell-content">
        {props.children}
      </main>

      <Dock />
    </div>
  );
};

export default Shell;
