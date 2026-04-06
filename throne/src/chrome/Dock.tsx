import { For } from 'solid-js';
import { A, useLocation } from '@solidjs/router';
import { dockItems } from '../castle-init';

/** Floating glass dock at the bottom center. */
export function Dock() {
  const location = useLocation();

  return (
    <nav class="chrome-dock">
      <div class="chrome-dock-pill">
        <For each={dockItems}>
          {(item) => {
            const active = () => location.pathname === item.path;
            return (
              <A
                href={item.path}
                class="chrome-dock-item"
                classList={{ active: active() }}
                title={item.label}
              >
                <i class={item.icon} />
              </A>
            );
          }}
        </For>
      </div>
    </nav>
  );
}
