import { useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/**
 * Subscribes to a Tauri backend event and automatically unsubscribes on unmount.
 */
export function useTauriEvent<T>(
  event: string,
  handler: (payload: T) => void,
): void {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen<T>(event, (e) => handler(e.payload)).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [event, handler]);
}
