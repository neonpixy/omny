import { createSignal } from 'solid-js';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Custom macOS-style traffic light buttons (close, minimize, maximize). */
export function TrafficLights() {
  const [hovered, setHovered] = createSignal(false);
  const win = getCurrentWindow();

  return (
    <div
      class="traffic-lights"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <button
        class="traffic-light close"
        onClick={() => win.close()}
        title="Close"
      >
        {hovered() && <i class="ri-close-line" />}
      </button>
      <button
        class="traffic-light minimize"
        onClick={() => win.minimize()}
        title="Minimize"
      >
        {hovered() && <i class="ri-subtract-line" />}
      </button>
      <button
        class="traffic-light maximize"
        onClick={() => win.toggleMaximize()}
        title="Maximize"
      >
        {hovered() && <i class="ri-add-line" />}
      </button>
    </div>
  );
}
