// The script reference's parts, by kind of script: pure, so the tests can load it.

/** The reference's part for a kind of script: a table script's own section, or how formulas and their functions are written. */
export function guideFor(reference: string, table: boolean): string {
  const at = reference.indexOf('# Writing a table script');
  if (at < 0) return reference;
  const allocation = reference.indexOf('# Writing an allocation script');
  return (table ? reference.slice(at, allocation > at ? allocation : undefined) : reference.slice(0, at)).trim();
}

/** The reference's part for an allocation step (Waste allocation plan, decision 4), else all of it. */
export function allocationGuide(reference: string): string {
  const at = reference.indexOf('# Writing an allocation script');
  return at < 0 ? reference : reference.slice(at).trim();
}
