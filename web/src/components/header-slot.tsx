import { createContext, type ReactNode, useContext } from 'react';
import { createPortal } from 'react-dom';

/** A place in the header that a page fills with its own controls (the map's model and layer). */
export const HeaderSlotContext = createContext<HTMLElement | null>(null);

/** Puts `children` into the header's slot when there is one and `when` holds, else where it stands. */
export function InHeader({ when, children }: { when: boolean; children: ReactNode }) {
  const slot = useContext(HeaderSlotContext);
  return when && slot ? createPortal(children, slot) : children;
}
