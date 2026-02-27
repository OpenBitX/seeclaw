import { useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * Subscribes to a Tauri backend event and automatically unsubscribes on unmount.
 *
 * Uses an `active` flag to handle React StrictMode's double-mount behavior:
 * `listen` is async, so the cleanup may run before the promise resolves.
 * Without this guard, the unlisten call would be a no-op and the first
 * listener would leak, causing each event to fire twice.
 */
export function useTauriEvent<T>(
  event: string,
  handler: (payload: T) => void,
): void {
  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;

    listen<T>(event, (e) => {
      if (active) handler(e.payload);
    }).then((fn) => {
      if (!active) {
        fn();
      } else {
        unlisten = fn;
      }
    });

    return () => {
      active = false;
      unlisten?.();
    };
  }, [event, handler]);
}
