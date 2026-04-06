import { createSignal, Show, onCleanup } from 'solid-js';
import { useNavigate, useLocation } from '@solidjs/router';
import { identity } from '../stores/identity';

function crownGradientHues(id: string): [number, number] {
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (Math.imul(hash, 31) + id.charCodeAt(i)) >>> 0;
  }
  return [hash % 360, Math.floor(hash / 360) % 360];
}

/** Crown button with profile avatar and context menu. */
export function CrownMenu() {
  const navigate = useNavigate();
  const location = useLocation();
  const [open, setOpen] = createSignal(false);
  let menuRef: HTMLDivElement | undefined;

  const initials = () =>
    identity.displayName
      .trim()
      .split(/\s+/)
      .map((w) => w[0]?.toUpperCase() ?? '')
      .slice(0, 2)
      .join('');

  const hues = () => crownGradientHues(identity.crownId);

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    setOpen(!open());
  }

  function go(path: string) {
    setOpen(false);
    navigate(path);
  }

  async function lock() {
    setOpen(false);
    const path = location.pathname;
    if (path !== '/crown-unlock' && path !== '/crown-setup') {
      sessionStorage.setItem('omny:lastPage', path);
    }
    try {
      await window.omninet.run({ method: 'chamberlain.lock', params: {} });
    } catch { /* best effort */ }
    navigate('/crown-unlock', { replace: true });
  }

  function handleClickOutside(e: MouseEvent) {
    if (menuRef && !menuRef.contains(e.target as Node)) {
      setOpen(false);
    }
  }

  function refCallback(el: HTMLDivElement) {
    menuRef = el;
    document.addEventListener('click', handleClickOutside);
    onCleanup(() => document.removeEventListener('click', handleClickOutside));
  }

  return (
    <div class="chrome-crown-menu" ref={refCallback}>
      <Show
        when={identity.unlocked && initials()}
        fallback={
          <button class="chrome-circle-btn" onClick={toggle} title="Crown">
            <i class="ri-vip-crown-2-fill" />
          </button>
        }
      >
        <button
          class="chrome-crown-avatar"
          onClick={toggle}
          title={identity.displayName || 'Crown'}
          style={{
            background: `linear-gradient(135deg, hsl(${hues()[0]}, 70%, 55%), hsl(${hues()[1]}, 70%, 45%))`,
          }}
        >
          {initials()}
        </button>
      </Show>
      <Show when={open()}>
        <div class="chrome-crown-dropdown">
          <button class="chrome-crown-item" onClick={() => go('/account')}>
            <i class="ri-user-line" />
            <span>Account</span>
          </button>
          <button class="chrome-crown-item" onClick={() => go('/settings')}>
            <i class="ri-settings-3-line" />
            <span>Settings</span>
          </button>
          <div class="chrome-crown-divider" />
          <button class="chrome-crown-item" onClick={lock}>
            <i class="ri-lock-line" />
            <span>Lock</span>
          </button>
        </div>
      </Show>
    </div>
  );
}
