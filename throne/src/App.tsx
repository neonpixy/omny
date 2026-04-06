import { createSignal, onMount } from 'solid-js';
import { Router, Route, Navigate } from '@solidjs/router';
import { Theme } from '@omnidea/ui';
import Shell from './Shell';
import { routes, defaultPath } from './castle-init';

export default function App() {
  console.log(`[Throne] ${routes.length} routes, defaultPath="${defaultPath}"`);
  console.log('[Throne] routes:', routes.map(r => r.path));

  const [crystal, setCrystal] = createSignal<unknown>(null);

  onMount(() => {
    import('@omnidea/crystal')
      .then(({ Crystal }) => Crystal.init())
      .then((instance) => {
        setCrystal(instance);
        console.log('Crystal WebGPU initialized');
      })
      .catch((e) => console.warn('Crystal not available:', e));
  });

  return (
    <Theme mode="neu" crystal={crystal()}>
      <Router root={Shell}>
        {routes.map((r) => (
          <Route path={r.path} component={r.component} />
        ))}
        <Route path="*" component={() => <Navigate href={defaultPath} />} />
      </Router>
    </Theme>
  );
}
