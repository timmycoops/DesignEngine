import { createContext, useContext } from 'react';

/**
 * Where menus, lists and tips draw: the page of the window their panel is in. The Tables side popped out into a window
 * of its own (decision 0109) gives its page here, so a list opened there opens there; none, the main window's page.
 */
export const PortalRoot = createContext<HTMLElement | null>(null);

/** The page to draw a menu, list or tip into, or undefined for the main window's. */
export const usePortalRoot = (): HTMLElement | undefined => useContext(PortalRoot) ?? undefined;
