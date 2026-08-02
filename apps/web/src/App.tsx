import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useState, type ReactElement } from 'react';
import type { ThemeMode } from '@aurora/shared-types';

const DEFAULT_THEME: ThemeMode = 'light';

/**
 * Root application component.
 *
 * Wires up the top-level providers (React Query) and renders the app shell.
 * Subsequent Phase 5 tasks will mount the router, layout, and feature views here.
 */
export function App(): ReactElement {
  const [queryClient] = useState(() => new QueryClient());

  return (
    <QueryClientProvider client={queryClient}>
      <div className="app" data-theme={DEFAULT_THEME}>
        <h1>Aurora Note</h1>
        <p>Phase 5 view-layer foundation is ready.</p>
      </div>
    </QueryClientProvider>
  );
}
