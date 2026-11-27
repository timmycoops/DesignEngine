import { useSyncExternalStore } from 'react';
import { type Look, lookNow, onLook } from '../viewport/look';

/** The look in use, light or dark, for a part that draws differently in each (dockview's frame, the workflow canvas). */
export function useLook(): Look {
  return useSyncExternalStore(
    (changed) => onLook(changed),
    lookNow,
  );
}
