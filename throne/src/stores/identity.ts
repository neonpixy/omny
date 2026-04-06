/**
 * Shell identity store — reactive Crown state for Throne chrome.
 * Uses the bridge directly (shell is not a Castle program).
 */
import { createSignal } from 'solid-js';

const [exists, setExists] = createSignal(false);
const [unlocked, setUnlocked] = createSignal(false);
const [crownId, setCrownId] = createSignal('');
const [displayName, setDisplayName] = createSignal('');

function applyState(data: any) {
  setExists(data.exists ?? false);
  setUnlocked(data.unlocked ?? false);
  setCrownId(data.crown_id ?? data.crownId ?? '');
  setDisplayName(data.display_name ?? '');
}

/** Fetch crown state and start listening for changes. */
export function initIdentity() {
  window.omninet.run({ method: 'chamberlain.state', params: {} })
    .then((data: any) => { if (data) applyState(data); })
    .catch(() => {});

  const refresh = () => {
    window.omninet.run({ method: 'chamberlain.state', params: {} })
      .then((data: any) => { if (data) applyState(data); })
      .catch(() => {});
  };

  window.omninet.on('crown/created', refresh);
  window.omninet.on('crown/unlocked', refresh);
  window.omninet.on('crown/locked', refresh);
  window.omninet.on('crown/deleted', refresh);
}

export const identity = {
  get exists() { return exists(); },
  get unlocked() { return unlocked(); },
  get crownId() { return crownId(); },
  get displayName() { return displayName(); },
};
