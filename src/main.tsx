import '@fontsource/inter/400.css';
import '@fontsource/inter/600.css';
import './index.css';
import './i18n';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { CaptchaHint } from './auth/CaptchaHint';

const view = new URLSearchParams(window.location.search).get('view');

createRoot(document.getElementById('root')!).render(
  <StrictMode>{view === 'captcha-hint' ? <CaptchaHint /> : <App />}</StrictMode>,
);
