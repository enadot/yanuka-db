import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Direction } from 'radix-ui';
import { HashRouter } from 'react-router-dom';
import { TooltipProvider, Toaster } from '@yanuka/ui';
import { RepositoryProvider } from './lib/repository';
import { App } from './App';
import './styles/globals.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // The data source is a local database, not a network. Refetching on
      // window focus would be pure waste, and retrying a failed local query
      // just delays showing the user a real error.
      refetchOnWindowFocus: false,
      retry: false,
      staleTime: 30_000,
    },
  },
});

const container = document.getElementById('root');
if (!container) throw new Error('Root element not found');

createRoot(container).render(
  <StrictMode>
    {/*
      Radix computes its own directionality and does not read the `dir`
      attribute from the document. Without this provider, dropdown alignment,
      select positioning and arrow-key navigation all stay left-to-right even
      though the page renders right-to-left. This is the single most commonly
      missed piece of RTL setup in a shadcn application.
    */}
    <Direction.DirectionProvider dir="rtl">
      <QueryClientProvider client={queryClient}>
        <RepositoryProvider>
          <TooltipProvider delayDuration={300}>
            <HashRouter>
              <App />
            </HashRouter>
            <Toaster position="bottom-left" dir="rtl" richColors />
          </TooltipProvider>
        </RepositoryProvider>
      </QueryClientProvider>
    </Direction.DirectionProvider>
  </StrictMode>,
);
