import { createSignal, createEffect } from 'solid-js';
import { useLocation, useNavigate } from '@solidjs/router';

/** Pill-shaped address bar — editable, navigates on Enter. */
export function AddressBar() {
  const location = useLocation();
  const navigate = useNavigate();

  function handleRefresh() {
    const current = location.pathname;
    navigate('/', { replace: true });
    setTimeout(() => navigate(current, { replace: true }), 0);
  }
  const [editing, setEditing] = createSignal(false);
  const [value, setValue] = createSignal('');
  let inputRef: HTMLInputElement | undefined;
  let committing = false;

  const displayUrl = () => {
    const path = location.pathname;
    const search = location.search;
    if (!path || path === '/') return 'omny://home';
    // Show net:// URL when viewing an Omninet note
    if (path === '/tome' && search) {
      const params = new URLSearchParams(search);
      const net = params.get('net');
      if (net) return `net://${net}`;
    }
    return `omny://system${path}`;
  };

  createEffect(() => {
    if (!editing()) setValue(displayUrl());
  });

  function startEditing() {
    setValue(displayUrl());
    setEditing(true);
    requestAnimationFrame(() => inputRef?.select());
  }

  function commitNavigation() {
    const url = value().trim();
    committing = true;
    setEditing(false);
    committing = false;
    if (!url) return;

    if (url.startsWith('net://')) {
      // Resolve Omninet name → open in Tome
      const name = url.replace('net://', '').split('/')[0];
      navigate(`/tome?net=${encodeURIComponent(name)}`);
    } else if (url.startsWith('omny://system/')) {
      navigate('/' + url.replace('omny://system/', ''));
    } else if (url.startsWith('omny://')) {
      navigate('/' + url.replace('omny://', ''));
    } else if (url.startsWith('/')) {
      navigate(url);
    } else {
      navigate('/' + url);
    }
    committing = false;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      commitNavigation();
    } else if (e.key === 'Escape') {
      setEditing(false);
    }
  }

  function handleBlur() {
    // Don't reset if we're in the middle of committing
    if (!committing) {
      setEditing(false);
    }
  }

  return (
    <div class="chrome-address-bar" onClick={() => !editing() && startEditing()}>
      <i class="ri-shield-check-fill chrome-address-icon" />
      {editing() ? (
        <input
          ref={inputRef}
          class="chrome-address-input"
          value={value()}
          onInput={(e) => setValue(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onBlur={handleBlur}
          spellcheck={false}
          autocapitalize="off"
          autocomplete="off"
          autocorrect="off"
        />
      ) : (
        <span class="chrome-address-text">{displayUrl()}</span>
      )}
      <i
        class="ri-refresh-line chrome-address-icon chrome-address-refresh"
        title="Refresh"
        onClick={(e) => { e.stopPropagation(); handleRefresh(); }}
      />
    </div>
  );
}
