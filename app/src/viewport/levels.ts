// The coarser versions of a drawn surface's chunks (decision 0097, workstream 2), as the shell's `stage_levels` sends
// them: every version's u16 indices, chunk after chunk, padded to 4 bytes, then the JSON header and its u32 length, as
// every render buffer ends. Chunk k here is chunk k of the full mesh `stage_object` sent, and a version's indices point
// at that chunk's vertices, so the card holds only the extra index lists.

/** One coarser version of a chunk. */
export interface ChunkLevel {
  /** Its triangles, three indices each, into the chunk's own vertices: a view onto the buffer. */
  indices: Uint16Array;
  /** The farthest any vertex of the full chunk lies from it (m): how far it can look wrong. */
  error: number;
}

export interface LevelsHeader {
  kind: 'levels';
  name: string;
  /** Triangles in the full mesh. */
  triangles: number;
  /** Triangles in every version of every chunk together. */
  level_triangles: number;
  /** Triangles in each chunk's coarsest version together: the least the surface can be drawn with. */
  coarsest_triangles: number;
  chunks: { levels: { index_start: number; index_count: number; error: number }[] }[];
  timings?: Record<string, number>;
}

export interface DecodedLevels {
  header: LevelsHeader;
  /** Each chunk's versions, coarser as they go; a chunk too small to thin has none. */
  chunks: ChunkLevel[][];
}

/** Views over a levels buffer, no copying. */
export function decodeLevels(buffer: ArrayBuffer): DecodedLevels {
  const view = new DataView(buffer);
  const headerLength = view.getUint32(buffer.byteLength - 4, true);
  const headerStart = buffer.byteLength - 4 - headerLength;
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, headerStart, headerLength))) as LevelsHeader;
  if (header.kind !== 'levels') throw new Error(`not a levels buffer: ${header.kind}`);
  const chunks = header.chunks.map((c) =>
    c.levels.map((l) => {
      if (2 * (l.index_start + l.index_count) > headerStart) throw new Error('a version runs past the indices');
      return { indices: new Uint16Array(buffer, 2 * l.index_start, l.index_count), error: l.error };
    }),
  );
  return { header, chunks };
}
