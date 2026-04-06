// Install the bridge before anything else
import './bridge/omninet';

// Castle HMR — listens for file change events from the Rust watcher
import './hmr';

import { render } from 'solid-js/web';
import App from './App';

render(() => <App />, document.getElementById('app')!);
