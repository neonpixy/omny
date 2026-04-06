import { For } from 'solid-js';

export interface Tab {
  id: string;
  path: string;
  label: string;
}

interface TabBarProps {
  tabs: Tab[];
  activeTabId: string;
  onNewTab: () => void;
  onCloseTab: (id: string) => void;
  onTabClick: (tab: Tab) => void;
}

/** Tab strip — glass tabs with active highlight. */
export function TabBar(props: TabBarProps) {
  return (
    <div class="chrome-tab-strip">
      <For each={props.tabs}>
        {(tab) => {
          const active = () => tab.id === props.activeTabId;
          return (
            <div
              class="chrome-tab"
              classList={{ active: active() }}
              onClick={() => props.onTabClick(tab)}
            >
              <img
                src="/omnidea-pinwheel.svg"
                class="chrome-tab-icon"
                alt=""
                onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }}
              />
              <span class="chrome-tab-label">{tab.label}</span>
              <button
                class="chrome-tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  props.onCloseTab(tab.id);
                }}
              >
                <i class="ri-close-line" />
              </button>
            </div>
          );
        }}
      </For>
      <button class="chrome-tab-new" title="New tab" onClick={props.onNewTab}>
        <i class="ri-add-line" />
      </button>
    </div>
  );
}
