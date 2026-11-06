import '@fontsource/inter/400.css';
import '@fontsource/inter/600.css';
import '@fontsource/inter/700.css';
import './index.css';
import './i18n';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { CaptchaHint } from './auth/CaptchaHint';

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 60_000, retry: 1, refetchOnWindowFocus: false } },
});

const view = new URLSearchParams(window.location.search).get('view');

if (view === 'drm') {
  // The hidden DRM player window: no UI, just the player.
  void import('./drm/player').then((player) => player.start());
} else {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        {view === 'captcha-hint' ? <CaptchaHint /> : <App />}
      </QueryClientProvider>
    </StrictMode>,
  );
}
