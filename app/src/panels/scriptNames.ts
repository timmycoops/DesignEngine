import { useEffect, useMemo, useState } from 'react';
import { objectDetails } from '@/bridge';
import type { ScriptName, ScriptNames } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';

/**
 * The names a script box offers as it's typed (decision 0099): the project's scripts for `load("…")`, a table's columns
 * for `t["…"]`, parameter tables' columns for `p.`, and a checked formula's own names. Names only, never a value.
 */

/** The project's scripts, by name, for `load("…")`. */
export function useProjectScripts(): string[] {
  const { summary } = useWorkspace();
  return useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'script').map((o) => o.name), [summary]);
}

/** A table's columns with their units; none while it's read or when there's no table. */
export function useTableColumns(table: string | null | undefined): ScriptName[] {
  const [columns, setColumns] = useState<ScriptName[]>([]);
  useEffect(() => {
    if (!table) return setColumns([]);
    let live = true;
    void objectDetails(table)
      .then((d) => live && d.measures.kind === 'table' && setColumns(d.measures.columns.map((c) => ({ name: c.name, unit: c.unit }))))
      .catch(() => live && setColumns([]));
    return () => {
      live = false;
    };
  }, [table]);
  return columns;
}

/** The columns of parameter tables, each once, the first table's first. */
export function useParameterColumns(tables: string[]): ScriptName[] {
  const [columns, setColumns] = useState<ScriptName[]>([]);
  const key = tables.join(',');
  useEffect(() => {
    let live = true;
    void Promise.all(tables.map((t) => objectDetails(t).catch(() => null))).then((all) => {
      if (!live) return;
      const out: ScriptName[] = [];
      for (const d of all) {
        if (d?.measures.kind !== 'parameter_table') continue;
        for (const c of d.measures.parameters.columns) if (!out.some((o) => o.name === c.name)) out.push({ name: c.name, unit: c.text ? 'words' : c.unit });
      }
      setColumns(out);
    });
    return () => {
      live = false;
    };
  }, [key]);
  return columns;
}

/** The project's parameter tables, each with its columns and the keys its rows filter on, for `lookup("…")`. */
export function useParameterTables(): { name: string; columns: ScriptName[]; filters: string[] }[] {
  const { summary } = useWorkspace();
  const ids = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'parameter_table').map((o) => `${o.id}@${o.revision}`), [summary]);
  const [tables, setTables] = useState<{ name: string; columns: ScriptName[]; filters: string[] }[]>([]);
  const key = ids.join(',');
  useEffect(() => {
    let live = true;
    void Promise.all(ids.map((i) => objectDetails(i.split('@')[0]).catch(() => null))).then((all) => {
      if (!live) return;
      setTables(
        all.flatMap((d) =>
          d?.measures.kind === 'parameter_table' ? [{ name: d.name, columns: d.measures.parameters.columns.map((c) => ({ name: c.name, unit: c.text ? 'words' : c.unit })), filters: d.measures.parameters.filters }] : [],
        ),
      );
    });
    return () => {
      live = false;
    };
  }, [key]);
  return tables;
}

export { allocationGuide, guideFor } from './scriptGuide';

/** A formula's names as its check gives them: fields with units, labels, parameters (words marked). */
export function formulaNames(names: { fields: [string, string][]; labels: string[]; parameters: [string, string, boolean][] } | null | undefined, scripts: string[]): Omit<ScriptNames, 'calls'> {
  return {
    kind: 'formula',
    fields: (names?.fields ?? []).map(([name, unit]) => ({ name, unit })),
    labels: names?.labels ?? [],
    parameters: (names?.parameters ?? []).map(([name, unit, text]) => ({ name, unit: text ? 'words' : unit })),
    scripts,
  };
}
