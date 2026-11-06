// A click on two strings lying on each other picks the one you're editing, and a face design's own lines say
// which row's ✎ redraws them. Run with `pnpm test`.
import assert from 'node:assert/strict';
import test from 'node:test';
import { designLineMessage, lineToPick, rowEndingOn, TIE_PX } from '../src/viewport/designLines.ts';

const PICK_PX = 8;

test('a click on your line lying on the design\'s keeps your line, selected or not', () => {
  // Tim, 2026-11-06: Edit from here put "Toe Steps your line 8" on top of "Toe Steps toe 5", and a click picked
  // the design's, so 11 Deletes went to a line the next run made again.
  const design = { id: 'toe 5', distance: 3.2, madeByDesign: true };
  const yours = { id: 'your line 8', distance: 3.2, madeByDesign: false };
  for (const near of [[design, yours], [yours, design]]) {
    assert.equal(lineToPick(near, 'your line 8', PICK_PX), 'your line 8', 'selected stays selected');
    assert.equal(lineToPick(near, null, PICK_PX), 'your line 8', 'nothing selected: your own over the design\'s');
    assert.equal(lineToPick(near, 'Ramp A', PICK_PX), 'your line 8', 'something else selected');
  }
  // A pixel apart, as two lines a hair apart on screen, is still a tie.
  assert.equal(lineToPick([{ ...design, distance: 2.5 }, { ...yours, distance: 2.5 + TIE_PX }], null, PICK_PX), 'your line 8');
});

test('the selected line stays picked among lines on top of each other, even a design\'s', () => {
  const near = [
    { id: 'toe 5', distance: 1, madeByDesign: true },
    { id: 'your line 8', distance: 1, madeByDesign: false },
  ];
  assert.equal(lineToPick(near, 'toe 5', PICK_PX), 'toe 5', 'selected in the tree, then clicked on');
});

test('a line clearly nearer wins, whoever made it, and nothing past the pick distance counts', () => {
  const near = [
    { id: 'crest 3', distance: 0.5, madeByDesign: true },
    { id: 'your line 4', distance: 6, madeByDesign: false },
  ];
  assert.equal(lineToPick(near, null, PICK_PX), 'crest 3');
  assert.equal(lineToPick(near, 'your line 4', PICK_PX), 'crest 3', 'a click well off the selected line picks the one under it');
  assert.equal(lineToPick([{ id: 'far', distance: 8.5, madeByDesign: false }], null, PICK_PX), null);
  assert.equal(lineToPick([], 'x', PICK_PX), null);
  // Two of the design's own: the nearer.
  assert.equal(lineToPick([{ id: 'a', distance: 2, madeByDesign: true }, { id: 'b', distance: 1, madeByDesign: true }], null, PICK_PX), 'b');
});

test('a design\'s line names the row whose ✎ redraws it', () => {
  const d = 'design id';
  // Tim's step template: rows 1 wall, 2 berm, ... 9 wall, 10 berm. Toe 5 is the toe of the wall on row 9.
  assert.equal(rowEndingOn({ name: 'Toe Steps toe 5', design: d, step: 9 }), 8);
  assert.equal(rowEndingOn({ name: 'Toe Steps crest 4', design: d, step: 7 }), 7);
  assert.equal(rowEndingOn({ name: 'Toe Steps next toe', design: d, step: 8 }), 8);
  assert.equal(rowEndingOn({ name: 'Toe Steps toe 1', design: d, step: 1 }), null, 'the first toe is the design\'s string');
  assert.equal(rowEndingOn({ name: 'Toe Steps road A pit edge', design: d, step: null }), null);
  // A string named with digits of its own still reads its row from the end of its name.
  assert.equal(rowEndingOn({ name: 'Strip 12 Grid walls crest 2', design: d, step: 3 }), 3);
});

test('editing a design\'s own line says why not and what to do', () => {
  const toe5 = { name: 'Toe Steps toe 5', design: 'id', step: 9 };
  assert.equal(
    designLineMessage(toe5, 'Steps from Toe'),
    'Toe Steps toe 5 is made by the design Steps from Toe and made again each time it runs, so an edit to it would be lost. In Face design, ✎ on row 8 gives you a line of your own to change.',
  );
  const edge = { name: 'Toe Steps road A pit edge', design: 'id', step: null };
  assert.match(designLineMessage(edge, undefined), /made by a face design .* Change the design and run it again instead\.$/);
});
