import { mount } from 'svelte';
import App from './app.svelte';
import './app.css';

const target = document.getElementById('app');
if (!target) {
  throw new Error('Root element #app not found in index.html');
}

const app = mount(App, { target });

export default app;
