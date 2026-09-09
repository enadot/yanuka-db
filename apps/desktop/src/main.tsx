import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClientProvider } from '@tanstack/react-query';
import { Direction } from 'radix-ui';
import { HashRouter } from 'react-router-dom';
import { TooltipProvider, Toaster } from '@yanuka/ui';
import { createQueryClient } from './lib/query-client';
import { RepositoryProvider } from './lib/repository';
import { App } from './App';
import './styles/globals.css';

// Configured in lib/query-client.ts — and tested there, because the one
// setting that matters (never wait for the network) once went missing.
const queryClient = createQueryClient();

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
            <Toaster
              position="bottom-left"
              dir="rtl"
              richColors
              // Above the phone layout's bottom bar (ADR-040).
              mobileOffset={{ bottom: '5.5rem' }}
            />
          </TooltipProvider>
        </RepositoryProvider>
      </QueryClientProvider>
    </Direction.DirectionProvider>
  </StrictMode>,
);
