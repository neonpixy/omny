import type { JSX } from 'solid-js';

interface CircleButtonProps {
  icon: string;
  onClick?: () => void;
  disabled?: boolean;
  class?: string;
  title?: string;
}

/** Glass circle button — 34px, used for nav controls and chrome actions. */
export function CircleButton(props: CircleButtonProps) {
  return (
    <button
      class={`chrome-circle-btn ${props.class ?? ''}`}
      classList={{ disabled: props.disabled }}
      onClick={props.onClick}
      title={props.title}
      disabled={props.disabled}
    >
      <i class={`ri-${props.icon}`} />
    </button>
  );
}
