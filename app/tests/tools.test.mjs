// The one tool list (decision 0076, workstream 4): every tool once, in one stage, doing something that exists: a panel
// in the panel list, an action, and where it names an engine command, one the engine has. Run with `pnpm test`.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { ALWAYS_ROW, choiceTool, choiceWords, inUse, menuTools, menuWords, panelTool, rowName, ROWS, rowTools, STAGES, stageTools, stepTools, tipOf, toolByKey, TOOLS } from '../src/tools.ts';
import { rememberStage, stageOf } from '../src/workspace/stage.ts';
import { PANELS } from '../src/workspace/panels.ts';
import { CANVAS_TOOLS } from '../src/panels/workflowTools.ts';

/** Every command the engine names: `const NAME: &'static str = "…";` under crates/zd-commands. */
function engineCommands() {
  const names = new Set();
  const walk = (folder) => {
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      const path = join(folder, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.rs')) for (const m of readFileSync(path, 'utf8').matchAll(/const NAME: &'static str = "([a-z_.]+)";/g)) names.add(m[1]);
    }
  };
  walk(join(import.meta.dirname, '..', '..', 'crates', 'zd-commands', 'src'));
  return names;
}

test('every tool is listed once, in one stage', () => {
  const keys = TOOLS.map((t) => t.key);
  assert.equal(new Set(keys).size, keys.length, 'a key twice');
  const stages = new Set([...STAGES.map((s) => s.id), 'always']);
  for (const tool of TOOLS) assert.ok(stages.has(tool.stage), `${tool.key} is in ${tool.stage}, which is no stage`);
  for (const { id } of STAGES) assert.ok(stageTools(id).length > 0, `${id} has no tools`);
  assert.equal(STAGES.map((s) => s.title).join(' · '), 'Project · Strings · Walls · Solids · Geology · Reserves');
});

test('every tool opens a panel that exists or names an action, and says what it does', () => {
  const panels = new Set(PANELS.map((p) => p.id));
  for (const tool of TOOLS) {
    assert.ok(tool.panel || tool.action, `${tool.key} does nothing`);
    assert.ok(!(tool.panel && tool.action), `${tool.key} both opens a panel and names an action`);
    if (tool.panel) assert.ok(panels.has(tool.panel), `${tool.key} opens ${tool.panel}, which is no panel`);
    assert.ok(tool.label.trim() && tool.tip.trim(), `${tool.key} has no name or no line`);
    if (tool.mode) assert.ok(tool.panel, `${tool.key} sets a panel it doesn't open`);
  }
});

test('every command a tool names is one the engine has', () => {
  const commands = engineCommands();
  assert.ok(commands.size > 50, `only ${commands.size} commands found in the engine's sources`);
  const named = TOOLS.filter((t) => t.command);
  assert.ok(named.length > 30);
  for (const tool of named) assert.ok(commands.has(tool.command), `${tool.key} names ${tool.command}, which the engine doesn't have`);
});

test('every panel has one tool that opens it as it stands, under its own name', () => {
  for (const panel of PANELS) {
    assert.equal(TOOLS.filter((t) => t.key === panel.id).length, 1, panel.id);
    assert.equal(panelTool(panel.id).label, panel.title);
    assert.equal(panelTool(panel.id).mode, undefined);
  }
  assert.throws(() => panelTool('nothing'));
});

test('every stage has a row of tools that exist, named in words', () => {
  for (const { id } of STAGES) {
    assert.ok(ROWS[id].length > 0, id);
    for (const item of ROWS[id]) {
      if (item === '|') continue;
      if (typeof item === 'string') {
        assert.ok(toolByKey(item), `${id} row: no tool ${item}`);
        assert.ok(rowName(toolByKey(item)).length <= 22, `${item}: a long name for a row`);
        assert.ok(toolByKey(item).icon, `${item} has no picture for its row`);
      } else {
        assert.ok(item.label && item.tip && (item.menu || item.keys?.length), `${id} row: a menu with nothing in it`);
        for (const key of item.keys ?? []) assert.ok(toolByKey(key)?.menu, `${id} row, ${item.label}: ${key} has no words for a menu`);
      }
    }
    assert.notEqual(ROWS[id][0], '|');
    assert.notEqual(ROWS[id].at(-1), '|');
  }
  const names = (stage) => ROWS[stage].filter((i) => typeof i === 'string' && i !== '|').map((key) => rowName(toolByKey(key)));
  assert.deepEqual(names('solids'), ['Between surfaces', 'Strips by faces', 'By surface or RL', 'By seams', 'By benches', 'By blocks', 'Block table', 'Repair']);
  assert.deepEqual(names('project'), ['New', 'Open', 'Save as', 'Relink geology']);
  assert.deepEqual(names('geology'), ['Import seam grids', 'Suggest definition', 'Build seam model', 'Relink geology']);
  assert.deepEqual(names('reserves'), ['Reserves', 'Parameters', 'Reserves table', 'Compare', 'Report']);
  assert.deepEqual(names('walls'), ['Face design', 'Builder', 'Rebuild from here', 'Join faces']);
  assert.deepEqual(names('strings'), ['Offset', 'Number across', 'Polygons from lines', 'Set grade', 'Set elevation', 'Drape', 'Split', 'Join', 'Smooth', 'Filter', 'Reverse', 'Close']);
});

test("a stage's tools are on its row, but for the few reached another way", () => {
  const elsewhere = [];
  for (const { id } of STAGES) {
    const onRow = new Set(rowTools(id).map((t) => t.key));
    for (const tool of stageTools(id)) if (!onRow.has(tool.key)) elsewhere.push(tool.key);
  }
  // The older project folder is in the project's menu; Build a step and Interrogate are what Builder and Reserves open
  // on, listed for the canvas; Cut solids as it stands is in the Panels menu, its six choices on the row.
  assert.deepEqual(elsewhere.sort(), ['builder:step', 'cut', 'project.open_folder', 'reserves:interrogate']);
  // A row shows a tool from another stage only where the plan has it: Relink geology on Project, and the Import and
  // Export menus there, which gather their tools from every stage.
  const visiting = STAGES.flatMap(({ id }) => rowTools(id).filter((t) => t.stage !== id && t.stage !== 'always').map((t) => `${id}: ${t.key}`));
  // Relink geology is there twice: in the Import menu, as it always was, and on the row.
  assert.deepEqual([...new Set(visiting)].sort(), [
    'project: export.selected_solids',
    'project: export.visible_solids',
    'project: import.solids',
    'project: import.solids_folder',
    'project: import.solids_folder_repaired',
    'project: import.solids_repaired',
    'project: import.vulcan_grid',
    'project: seam.build',
    'project: seam.relink',
    'project: seam.suggest',
  ]);
});

test('every tool on the old one row is one click from its stage, and every import and export from Project', () => {
  const opens = (stage, panel) => ROWS[stage].some((item) => typeof item === 'string' && item !== '|' && toolByKey(item).panel === panel);
  assert.ok(opens('walls', 'face') && opens('walls', 'builder'));
  assert.ok(opens('solids', 'cut') && opens('solids', 'blocks'));
  assert.ok(opens('reserves', 'reserves') && opens('reserves', 'restable') && opens('reserves', 'params'));
  assert.deepEqual(ALWAYS_ROW, ['workflow']);
  const project = new Set(rowTools('project').map((t) => t.key));
  for (const tool of [...menuTools('import'), ...menuTools('export')]) assert.ok(project.has(tool.key), tool.key);
  assert.equal(tipOf(toolByKey('cut')), 'Cut solids: into strips by faces, by a surface, grid or RL, or one between two surfaces');
  assert.equal(tipOf(toolByKey('project.save')), 'Save (Ctrl+S)');
  assert.equal(tipOf(toolByKey('edit.undo')), 'Undo (Ctrl+Z)');
  assert.equal(tipOf(toolByKey('cut:seams')), "Cuts solids by the seam model's horizons, in one run.");
});

test("the tool in use is its panel's own, or the one for what the panel is set to", () => {
  assert.ok(inUse(toolByKey('blocks'), undefined));
  assert.ok(inUse(toolByKey('cut:seams'), 'seams'));
  assert.ok(!inUse(toolByKey('cut:seams'), 'strips'));
  assert.ok(!inUse(toolByKey('cut:join_faces'), 'strips'), 'Join faces opens Cut solids at strips, but is not the strips tool');
  assert.ok(!inUse(toolByKey('builder:rebuild'), undefined));
  assert.ok(!inUse(toolByKey('string.offset'), 'string.offset'), 'Properties in front lights no string tool');
  assert.ok(!inUse(toolByKey('project.save'), undefined));
});

test('the stage is remembered a project, Project for one never opened', () => {
  const held = new Map();
  const storage = { getItem: (k) => held.get(k) ?? null, setItem: (k, v) => held.set(k, v) };
  assert.equal(stageOf('D:/pits/a.zdp', storage), 'project');
  rememberStage('D:/pits/a.zdp', 'solids', storage);
  rememberStage(null, 'strings', storage);
  rememberStage('D:/pits/b.zdp', 'reserves', storage);
  assert.equal(stageOf('D:/pits/a.zdp', storage), 'solids');
  assert.equal(stageOf('D:/pits/b.zdp', storage), 'reserves');
  assert.equal(stageOf(null, storage), 'strings');
  rememberStage('D:/pits/a.zdp', 'walls', storage);
  assert.equal(stageOf('D:/pits/a.zdp', storage), 'walls');
  assert.equal(JSON.parse(held.get('zd.ribbon.stage.v1')).length, 3, 'a project once');
  held.set('zd.ribbon.stage.v1', '[["x","nowhere"],"junk"]');
  assert.equal(stageOf('x', storage), 'project', 'a stage that is none is not kept');
  held.set('zd.ribbon.stage.v1', 'not json');
  assert.equal(stageOf('x', storage), 'project');
});

test('the menus read top to bottom as they always have', () => {
  const words = (menu) => menuTools(menu).map((t) => t.menu.name ?? t.label);
  assert.deepEqual(words('project'), ['New project', 'Open a project (.zdp)…', 'Open an older project folder (.zdproj)…', 'Save', 'Save as…']);
  assert.deepEqual(words('import'), [
    'Surfaces…',
    'Vulcan grid…',
    'Strings…',
    'A zip or files…',
    'A folder…',
    'A zip or files…',
    'A folder…',
    'Deswik drawing (.dcf)…',
    'Spry project (.spry)…',
    'Suggest a stack definition (CSV)…',
    'Build and check from a definition…',
    'Relink geology…',
  ]);
  assert.deepEqual(words('export'), ['Selected to DXF…', 'Selected strings to CSV…', 'Selected solids to a zip…', 'Everything visible to DXF…', 'Visible strings to CSV…', 'Visible solids to a zip…']);
  for (const menu of ['project', 'import', 'export']) {
    const places = menuTools(menu).map((t) => t.menu.at);
    assert.equal(new Set(places).size, places.length, `two tools at one place in ${menu}`);
  }
  // The solids sit together in a menu within Import, the repaired ones under their heading; rules where they were.
  const imports = menuTools('import');
  assert.deepEqual(imports.filter((t) => t.menu.under).map((t) => t.menu.at), [4, 5, 6, 7]);
  assert.equal(imports.find((t) => t.menu.heading === 'Repaired as they come in').key, 'import.solids_repaired');
  assert.deepEqual(imports.filter((t) => t.menu.rule).map((t) => t.key), ['import.solids_repaired', 'import.deswik', 'seam.suggest']);
  assert.equal(imports.find((t) => t.menu.heading === 'Seam model').key, 'seam.suggest');
  assert.deepEqual(menuTools('project').filter((t) => t.menu.rule).map((t) => t.key), ['project.save']);
});

test('a menu item says how many it would take, past one', () => {
  const dxf = toolByKey('export.selected_dxf');
  assert.equal(menuWords(dxf, 0), 'Selected to DXF…');
  assert.equal(menuWords(dxf, 1), 'Selected to DXF…');
  assert.equal(menuWords(dxf, 3), 'Selected to DXF (3)…');
  assert.equal(menuWords(toolByKey('project.save'), null), 'Save');
  for (const tool of TOOLS.filter((t) => t.needs?.startsWith('selected'))) assert.equal(tool.menu?.in, 'export', tool.key);
});

test("a panel's choices take their names and their lines from the list", () => {
  const cut = choiceWords('cut', ['strips', 'seams', 'benches', 'polygons', 'cut', 'between']);
  assert.deepEqual(Object.values(cut.labels), ['Into strips', 'By seams', 'By benches', 'By polygons', 'By a surface', 'Between surfaces']);
  assert.equal(cut.lines.seams, "Cuts solids by the seam model's horizons, in one run.");
  const reserves = choiceWords('reserves', ['interrogate', 'compare']);
  assert.deepEqual(Object.values(reserves.labels), ['Interrogate', 'Compare']);
  // A line at a panel's head is a sentence short enough for the 400 px tool dock (the shell bench measures it on screen).
  for (const line of [...Object.values(cut.lines), ...Object.values(reserves.lines)]) assert.ok(line.endsWith('.') && line.length <= 62, line);
  assert.equal(choiceTool('cut', 'polygons').command, 'solid.cut_by_polygons');
  assert.throws(() => choiceTool('cut', 'nothing'));
});

test('the canvas lists the tools that add a step, as the list has them', () => {
  assert.deepEqual(CANVAS_TOOLS.map((t) => t.key), stepTools().map((t) => t.key));
  assert.deepEqual(CANVAS_TOOLS.map((t) => `${t.group}: ${t.label}`), [
    'Builder: Build a step',
    'Cut solids: Between surfaces',
    'Cut solids: Into strips',
    'Cut solids: By a surface',
    'Cut solids: By seams',
    'Cut solids: By benches',
    'Cut solids: By polygons',
    'Reserves: Interrogate',
  ]);
  for (const tool of stepTools()) assert.ok(tool.command && tool.panel && tool.step.takes, tool.key);
});
