/**
 * PWA Service Worker registration helper.
 *
 * Registers `/sw.js` (relative to the site root) when running in a
 * production browser context. In dev, or when no service worker exists,
 * the call is a no-op (the failed `register` is swallowed).
 *
 * Usage:
 *   import { registerServiceWorker } from '@/adapters/pwaServiceWorker';
 *   registerServiceWorker();
 */

/** Minimal subset of the ServiceWorkerContainer we depend on. */
interface SwContainerLike {
  register(scriptURL: string, options?: RegistrationOptions): Promise<unknown>;
  getRegistrations?(): Promise<
    Iterable<unknown> & {
      forEach(cb: (r: { unregister(): Promise<boolean> }) => void): void;
    }
  >;
}

/** Minimal subset of `navigator` we depend on. */
interface NavigatorLike {
  serviceWorker?: SwContainerLike;
}

/** Read the navigator global defensively (SSR/no-DOM safe). */
function getNavigator(): NavigatorLike | undefined {
  if (typeof navigator === 'undefined') return undefined;
  return navigator as unknown as NavigatorLike;
}

/**
 * Register the application service worker.
 *
 * @param scriptUrl Service worker script URL (defaults to `/sw.js`).
 * @returns `true` when registration succeeded, `false` otherwise.
 */
export async function registerServiceWorker(
  scriptUrl = '/sw.js',
): Promise<boolean> {
  const nav = getNavigator();
  if (!nav?.serviceWorker) return false;
  try {
    await nav.serviceWorker.register(scriptUrl, { scope: '/' });
    return true;
  } catch {
    // No sw.js present, or registration blocked — ignore.
    return false;
  }
}

/**
 * Unregister all service workers controlled by this origin.
 * Useful for tests / feature-flag teardown.
 */
export async function unregisterServiceWorkers(): Promise<void> {
  const nav = getNavigator();
  const container = nav?.serviceWorker;
  if (!container?.getRegistrations) return;
  try {
    const regs = await container.getRegistrations();
    regs.forEach((reg) => void reg.unregister());
  } catch {
    // ignore
  }
}
