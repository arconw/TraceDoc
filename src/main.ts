import { mount } from 'svelte';
import '@xyflow/svelte/dist/style.css';
import App from './App.svelte';
import './lib/styles/theme.css';

const target = document.getElementById('app');

if (!target) {
  throw new Error('Application root was not found');
}

const app = mount(App, { target });

export default app;
