import { useMemo } from 'react';
import type { ObjectSummary } from '@/bridge';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { useWorkspace } from '@/workspace/context';
import { kindLabel } from './format';

interface ObjectRow {
  id: string;
  kind: string;
  name: string;
  layer: string;
  revision: number;
  note?: string;
}

const helper = columnHelper<ObjectRow>();
const columns = helper.columns([
  helper.accessor('kind', { header: 'Kind', cell: (c) => <span className="text-muted-foreground">{c.getValue()}</span> }),
  helper.accessor('name', {
    header: 'Name',
    cell: (c) => (
      <span title={c.row.original.note}>
        {c.getValue()}
        {c.row.original.note && <span className="ml-1 text-muted-foreground">·</span>}
      </span>
    ),
  }),
  helper.accessor('layer', { header: 'Layer' }),
  helper.accessor('revision', { header: 'Rev', cell: (c) => <span className="tabular-nums">{c.getValue()}</span> }),
]);

/** Every live object; click one to select it. */
export function ObjectsPanel() {
  const { summary, selected, select } = useWorkspace();
  const rows = useMemo(() => {
    const layers = new Map(summary?.layers.map((l) => [l.id, l.name]));
    return (summary?.objects ?? []).map(
      (o: ObjectSummary): ObjectRow => ({
        id: o.id,
        kind: kindLabel[o.kind],
        name: o.name,
        layer: layers.get(o.layer) ?? '',
        revision: o.revision,
        note:
          o.kind === 'grid' && !o.drawn
            ? 'Property grid: listed, not drawn'
            : o.kind === 'seam_model'
              ? 'Seam model: its grids stay in their files'
              : o.kind === 'face_design'
                ? 'Face design: open it in the Face design panel'
                : undefined,
      }),
    );
  }, [summary]);
  return (
    <DataTable
      data={rows}
      columns={columns}
      rowId={(r) => r.id}
      selected={selected}
      onRowClick={(r) => select(r.id)}
      empty="Nothing here yet: import a surface or draw a string."
      windowed={['4.5rem', undefined, '6rem', '2.5rem']}
    />
  );
}
