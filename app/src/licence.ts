/**
 * The licence this copy runs under, as the shell reads it (decision 0098), and the words the app shows for it. Kept free
 * of the bridge so the tests can read it.
 */

/** A licence as issued: who it's for, its modules, the machines it runs on and its dates (`YYYY-MM-DD`). */
export interface Licence {
  format: number;
  licence: string;
  licensed_to: string;
  kind: 'machine';
  modules: string[];
  machines: string[];
  seats: number;
  issued: string;
  runs_until: string | null;
  updates_until: string | null;
}

export type LicenceState =
  | { state: 'current'; days_left: number | null }
  | { state: 'ends_soon'; days_left: number }
  | { state: 'grace'; grace_left: number }
  | { state: 'viewer'; reason: string };

/** Where this copy's licence stands, the modules on and this machine's code. */
export type LicenceStatus = LicenceState & { licence: Licence | null; modules: string[]; machine: string; path: string | null };

/** Sent on the window when a licence is loaded, so the status bar reads it again. */
export const LICENCE_CHANGED = 'zd-licence-changed';

export const MODULE_TITLES: Record<string, string> = {
  viewer: 'Viewer',
  design: 'Design',
  solids: 'Solids and blocks',
  reserves: 'Reserves',
  python: 'Python',
};

/** Every module, in the order a licence lists them. */
export const MODULES = Object.keys(MODULE_TITLES);

const days = (n: number) => `${n} day${n === 1 ? '' : 's'}`;

/** The status bar's word on the licence, or none while it's simply running. */
export function licenceWarning(s: LicenceStatus): string | null {
  switch (s.state) {
    case 'current':
      return null;
    case 'ends_soon':
      return s.days_left === 0 ? 'Licence ends today' : `Licence ends in ${days(s.days_left)}`;
    case 'grace':
      return `Licence ended: ${days(s.grace_left)} of grace left`;
    case 'viewer':
      return 'Viewer only: no licence for this machine';
  }
}

/** One line on where the licence stands, for the Licence panel. */
export function licenceStanding(s: LicenceStatus): string {
  switch (s.state) {
    case 'current':
      return s.days_left === null ? 'Running, with no end date' : `Running until ${s.licence?.runs_until}`;
    case 'ends_soon':
      return `Ends ${s.licence?.runs_until}, in ${days(s.days_left)}`;
    case 'grace':
      return `Ended ${s.licence?.runs_until}; it runs on for ${days(s.grace_left)} of grace`;
    case 'viewer':
      return s.reason;
  }
}
