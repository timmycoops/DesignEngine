// The coarser versions buffer (decision 0097, workstream 2) read back as the shell writes it: indices chunk after chunk,
// padded to 4 bytes, the JSON header, its length. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { decodeLevels } from '../src/viewport/levels.ts';

/** A buffer as `encode_levels` lays it out, from each chunk's versions (index lists and errors). */
function levelsBuffer(chunks) {
  const all = chunks.flat().flatMap((l) => l.indices);
  const bytes = Math.ceil((all.length * 2) / 4) * 4;
  let start = 0;
  const header = {
    kind: 'levels',
    name: 'Lidar',
    triangles: 100,
    level_triangles: all.length / 3,
    coarsest_triangles: chunks.reduce((n, c) => n + (c.length ? c[c.length - 1].indices.length / 3 : 0), 0),
    chunks: chunks.map((c) => ({
      levels: c.map((l) => {
        const meta = { index_start: start, index_count: l.indices.length, error: l.error };
        start += l.indices.length;
        return meta;
      }),
    })),
  };
  const json = new TextEncoder().encode(JSON.stringify(header));
  const buffer = new ArrayBuffer(bytes + json.length + 4);
  new Uint16Array(buffer, 0, all.length).set(all);
  new Uint8Array(buffer, bytes, json.length).set(json);
  new DataView(buffer).setUint32(bytes + json.length, json.length, true);
  return buffer;
}

test("each chunk's versions come back in order, with their indices and errors", () => {
  const chunks = [
    [
      { indices: [0, 1, 2, 2, 1, 3, 4, 5, 6], error: 0.25 },
      { indices: [0, 1, 3], error: 0.5 },
    ],
    [],
    [{ indices: [7, 8, 9, 9, 8, 10], error: 1.5 }],
  ];
  const decoded = decodeLevels(levelsBuffer(chunks));
  assert.equal(decoded.chunks.length, 3);
  assert.deepEqual(
    decoded.chunks.map((c) => c.map((l) => [Array.from(l.indices), l.error])),
    chunks.map((c) => c.map((l) => [l.indices, l.error])),
  );
  assert.equal(decoded.header.coarsest_triangles, 1 + 2);
});

test('a buffer of another kind is refused', () => {
  const json = new TextEncoder().encode(JSON.stringify({ kind: 'triangulation' }));
  const buffer = new ArrayBuffer(json.length + 4);
  new Uint8Array(buffer, 0, json.length).set(json);
  new DataView(buffer).setUint32(json.length, json.length, true);
  assert.throws(() => decodeLevels(buffer), /not a levels buffer/);
});
