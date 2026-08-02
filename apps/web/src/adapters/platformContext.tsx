/**
 * Platform context.
 *
 * Provides the active {@link PlatformAPI} implementation to the React tree.
 * On the web host the default implementation is the {@link wasmPlatform}
 * (Web Crypto + IndexedDB + fetch). Tests or specialized hosts can override
 * it via the `<PlatformProvider api={...}>` prop.
 */

import {
  createContext,
  useContext,
  useMemo,
  type ReactElement,
  type ReactNode,
} from 'react';
import type { PlatformAPI } from '@aurora/shared-types';
import { wasmPlatform } from './wasmPlatform';

const PlatformContext = createContext<PlatformAPI | null>(null);

export interface PlatformProviderProps {
  /** Optional override; defaults to the web {@link wasmPlatform}. */
  api?: PlatformAPI;
  children: ReactNode;
}

/**
 * Provide a {@link PlatformAPI} to the React tree.
 *
 * @example
 * <PlatformProvider>
 *   <App />
 * </PlatformProvider>
 */
export function PlatformProvider({
  api,
  children,
}: PlatformProviderProps): ReactElement {
  const value = useMemo<PlatformAPI>(() => api ?? wasmPlatform, [api]);
  return (
    <PlatformContext.Provider value={value}>
      {children}
    </PlatformContext.Provider>
  );
}

/**
 * Access the active {@link PlatformAPI}.
 *
 * Throws when used outside of a `<PlatformProvider>`.
 */
export function usePlatform(): PlatformAPI {
  const api = useContext(PlatformContext);
  if (!api) {
    throw new Error(
      'usePlatform must be used within a <PlatformProvider>',
    );
  }
  return api;
}

export { PlatformContext };
