import { createContext, type RefObject, useContext } from 'react';

/** The content panel's scroll container; virtualized lists scroll with it. */
export const ScrollRoot = createContext<RefObject<HTMLDivElement | null> | null>(null);

export function useScrollRoot(): RefObject<HTMLDivElement | null> {
  const root = useContext(ScrollRoot);
  if (!root) throw new Error('useScrollRoot outside the shell');
  return root;
}
