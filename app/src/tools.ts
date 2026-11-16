/**
 * The one list of tools (decision 0076, workstream 4): everything the app can be asked to do from its toolbar, its
 * menus, the Panels menu and the workflow canvas, each tool once, in the stage of the work it belongs to, and the row
 * of tools under each stage's tab. The toolbar, the menus and the canvas are drawn from it, and Find a command searches it, so a new tool is added here and
 * nowhere else. Kept free of React, so the tests can read it: an icon is a name (`toolIcons.tsx` holds the pictures) and
 * what a tool does is a panel to open, or an action App carries out. Nothing is imported at run time, for the same reason.
 */
import type { PanelId } from './workspace/panels';

/** The stages of the work, in the order it's done, and `always` for what's there whatever the stage. */
export type Stage = 'project' | 'strings' | 'walls' | 'solids' | 'geology' | 'reserves' | 'always';

export const STAGES: readonly { id: Exclude<Stage, 'always'>; title: string }[] = [
  { id: 'project', title: 'Project' },
  { id: 'strings', title: 'Strings' },
  { id: 'walls', title: 'Walls' },
  { id: 'solids', title: 'Solids' },
  { id: 'geology', title: 'Geology' },
  { id: 'reserves', title: 'Reserves' },
];

/** The pictures tools are drawn with, by name. */
export type ToolIcon =
  | 'new'
  | 'open'
  | 'save'
  | 'save-as'
  | 'undo'
  | 'redo'
  | 'import'
  | 'export'
  | 'relink'
  | 'offset'
  | 'number'
  | 'polygons'
  | 'grade'
  | 'elevation'
  | 'drape'
  | 'split'
  | 'join'
  | 'smooth'
  | 'filter'
  | 'reverse'
  | 'close'
  | 'face'
  | 'builder'
  | 'rebuild'
  | 'join-faces'
  | 'cut'
  | 'between'
  | 'strips'
  | 'seams'
  | 'benches'
  | 'by-blocks'
  | 'blocks'
  | 'repair'
  | 'solids-in'
  | 'solids-out'
  | 'grids'
  | 'suggest'
  | 'seam-model'
  | 'reserves'
  | 'compare'
  | 'reserves-table'
  | 'parameters'
  | 'report'
  | 'workflow'
  | 'panels'
  | 'more'
  | 'select'
  | 'draw'
  | 'snap'
  | 'plan'
  | 'zoom-selection'
  | 'zoom-all'
  | 'section';

/** What App carries out itself: files, undo, the seam model, the layout. */
export type ToolAction =
  | 'project.new'
  | 'project.open'
  | 'project.open_folder'
  | 'project.save'
  | 'project.save_as'
  | 'edit.undo'
  | 'edit.redo'
  | 'import.surfaces'
  | 'import.vulcan_grid'
  | 'import.strings'
  | 'import.solids'
  | 'import.solids_folder'
  | 'import.solids_repaired'
  | 'import.solids_folder_repaired'
  | 'import.deswik'
  | 'import.spry'
  | 'seam.suggest'
  | 'seam.build'
  | 'seam.relink'
  | 'export.selected_dxf'
  | 'export.selected_strings'
  | 'export.selected_solids'
  | 'export.visible_dxf'
  | 'export.visible_strings'
  | 'export.visible_solids'
  | 'solids.repair'
  | 'panels.hide'
  | 'panels.reset'
  | 'view.select'
  | 'view.draw'
  | 'view.snap'
  | 'view.plan'
  | 'view.zoom_selection'
  | 'view.zoom_all'
  | 'view.section';

/** What a tool needs before it can be used: something to undo, or something of a kind selected. It's off without it. */
export type ToolNeeds = 'undo' | 'redo' | 'selected-dxf' | 'selected-strings' | 'selected-solids';

export type MenuId = 'project' | 'import' | 'export';

/** Where a tool sits in one of the toolbar's menus. */
export interface MenuPlace {
  in: MenuId;
  /** Its place down the menu. */
  at: number;
  /** As the menu words it, where that's not its label ("Surfaces…" under Import). */
  name?: string;
  /** A menu inside the menu that holds it, with what the tip on its name says. */
  under?: { name: string; tip: string };
  /** A rule above it. */
  rule?: boolean;
  /** A heading above it. */
  heading?: string;
}

export interface Tool {
  /** Unique, and stable: a drag onto the canvas carries it. */
  key: string;
  stage: Stage;
  /** Its name in words, as a row of tools or Find a command shows it. */
  label: string;
  /** What it does, in a line. For a panel's choice this is the line at the panel's head. */
  tip: string;
  icon?: ToolIcon;
  /** The keys that do the same, as written for people ("Ctrl+S"). */
  shortcut?: string;
  /** The panel it opens. A tool whose key is its panel's id opens the panel as it stands. */
  panel?: PanelId;
  /** Which of what the panel does it sets the panel to, as the panel's choice holds it. */
  mode?: string;
  /** As the panel's choice words it, where that's not its label. */
  choice?: string;
  /** The engine command it runs, through its panel or its action. */
  command?: string;
  /** What App does for it, where it opens no panel. */
  action?: ToolAction;
  needs?: ToolNeeds;
  menu?: MenuPlace;
  /** It can add its step to a workflow (decision 0065): what the step works on, in a few words. */
  step?: { takes: string };
  /** Its name in a row of tools, where that's shorter than its label. */
  short?: string;
}

const REPAIRED = 'Sheets and fins off, cracks joined, holes up to 10 % filled';
const SOLIDS_MENU = { name: 'Solids', tip: 'Closed triangulations in Vulcan .00t or DXF files, or a zip or folder of them' };

export const TOOLS: readonly Tool[] = [
  // Project: the file, and getting data in and out.
  { key: 'project.new', stage: 'project', label: 'New project', tip: 'Starts an empty project', short: 'New', icon: 'new', action: 'project.new', menu: { in: 'project', at: 1 } },
  { key: 'project.open', stage: 'project', label: 'Open a project', tip: 'Opens a .zdp project file', short: 'Open', icon: 'open', action: 'project.open', menu: { in: 'project', at: 2, name: 'Open a project (.zdp)…' } },
  { key: 'project.open_folder', stage: 'project', label: 'Open an older project folder', tip: 'Opens a .zdproj folder, which saves as a .zdp beside it', icon: 'open', action: 'project.open_folder', menu: { in: 'project', at: 3, name: 'Open an older project folder (.zdproj)…' } },
  { key: 'project.save_as', stage: 'project', label: 'Save as', tip: 'Saves the project as another .zdp file', short: 'Save as', icon: 'save-as', action: 'project.save_as', menu: { in: 'project', at: 5, name: 'Save as…' } },
  { key: 'import.surfaces', stage: 'project', label: 'Import surfaces', tip: 'Vulcan .00t, DXF 3DFACE or polyface, or an XYZ or CSV grid', command: 'surface.import', action: 'import.surfaces', menu: { in: 'import', at: 1, name: 'Surfaces…' } },
  { key: 'import.strings', stage: 'project', label: 'Import strings', tip: 'DXF polylines and lines, or CSV with x, y and z columns', command: 'string.import', action: 'import.strings', menu: { in: 'import', at: 3, name: 'Strings…' } },
  { key: 'import.deswik', stage: 'project', label: 'Import a Deswik drawing', tip: 'Layers of a Deswik drawing: strings with their projection details, surfaces and solids', command: 'deswik.import', action: 'import.deswik', menu: { in: 'import', at: 8, name: 'Deswik drawing (.dcf)…', rule: true } },
  { key: 'import.spry', stage: 'project', label: 'Import a Spry project', tip: "Block solids named by their node paths with Spry's values, and design-layer strings and solids", command: 'spry.import', action: 'import.spry', menu: { in: 'import', at: 9, name: 'Spry project (.spry)…' } },
  { key: 'export.selected_dxf', stage: 'project', label: 'Export what is selected to DXF', tip: 'The selected strings, surfaces and solids as one DXF', action: 'export.selected_dxf', needs: 'selected-dxf', menu: { in: 'export', at: 1, name: 'Selected to DXF…' } },
  { key: 'export.selected_strings', stage: 'project', label: 'Export selected strings to CSV', tip: 'The selected strings as x, y and z rows', action: 'export.selected_strings', needs: 'selected-strings', menu: { in: 'export', at: 2, name: 'Selected strings to CSV…' } },
  { key: 'export.visible_dxf', stage: 'project', label: 'Export everything visible to DXF', tip: 'Every string, surface and solid on show as one DXF', action: 'export.visible_dxf', menu: { in: 'export', at: 4, name: 'Everything visible to DXF…' } },
  { key: 'export.visible_strings', stage: 'project', label: 'Export visible strings to CSV', tip: 'Every string on show as x, y and z rows', action: 'export.visible_strings', menu: { in: 'export', at: 5, name: 'Visible strings to CSV…' } },

  // Strings: the Strings panel's twelve, each opening it set to that tool, on the string picked.
  { key: 'strings', stage: 'strings', label: 'Strings', tip: 'offset, number, grade, drape, split, join and tidy the string picked', panel: 'strings' },
  { key: 'strings:offset', stage: 'strings', label: 'Offset', tip: 'Offsets the string to one side, once or several times.', icon: 'offset', panel: 'strings', mode: 'offset', command: 'string.offset' },
  { key: 'strings:number_across', stage: 'strings', label: 'Number across', tip: 'Numbers the strips by a line drawn across the strings.', icon: 'number', panel: 'strings', mode: 'number_across', command: 'string.number_across' },
  { key: 'strings:polygons_from_lines', stage: 'strings', label: 'Polygons from lines', tip: 'Makes named block polygons from the lines picked.', icon: 'polygons', panel: 'strings', mode: 'polygons_from_lines', command: 'string.polygons_from_lines' },
  { key: 'strings:set_grade', stage: 'strings', label: 'Set grade', tip: 'Sets the string to a grade from a vertex.', icon: 'grade', panel: 'strings', mode: 'set_grade', command: 'string.set_grade' },
  { key: 'strings:set_elevation', stage: 'strings', label: 'Set elevation', tip: 'Sets every vertex of the string to one RL.', icon: 'elevation', panel: 'strings', mode: 'set_elevation', command: 'string.set_elevation' },
  { key: 'strings:drape', stage: 'strings', label: 'Drape', tip: 'Drapes the string onto a grid or surface.', icon: 'drape', panel: 'strings', mode: 'drape', command: 'string.drape' },
  { key: 'strings:split', stage: 'strings', label: 'Split', tip: 'Splits the string at a vertex.', icon: 'split', panel: 'strings', mode: 'split', command: 'string.split' },
  { key: 'strings:join', stage: 'strings', label: 'Join', tip: 'Joins another open string onto this one.', icon: 'join', panel: 'strings', mode: 'join', command: 'string.join' },
  { key: 'strings:smooth', stage: 'strings', label: 'Smooth', tip: 'Smooths the string, a vertex every so many metres.', icon: 'smooth', panel: 'strings', mode: 'smooth', command: 'string.smooth' },
  { key: 'strings:filter', stage: 'strings', label: 'Filter', tip: "Takes out vertices the string doesn't need.", icon: 'filter', panel: 'strings', mode: 'filter', command: 'string.filter' },
  { key: 'strings:reverse', stage: 'strings', label: 'Reverse', tip: 'Turns the string to run the other way.', icon: 'reverse', panel: 'strings', mode: 'reverse', command: 'string.reverse' },
  { key: 'strings:set_closed', stage: 'strings', label: 'Close', tip: 'Closes the string, or opens it.', icon: 'close', panel: 'strings', mode: 'set_closed', command: 'string.set_closed' },

  // Walls.
  { key: 'face', stage: 'walls', label: 'Face design', tip: 'lines, profiles and rules, run as one', icon: 'face', panel: 'face' },
  { key: 'builder', stage: 'walls', label: 'Builder', tip: 'pick a line, build one wall or berm from it, edit between steps', icon: 'builder', panel: 'builder' },
  { key: 'builder:step', stage: 'walls', label: 'Build a step', tip: 'Builds one wall or berm from the line you pick.', panel: 'builder', command: 'face.build_step', step: { takes: 'the line picked' } },
  { key: 'builder:rebuild', stage: 'walls', label: 'Rebuild from here', tip: 'Builds the steps above an edited line again, each with its settings', icon: 'rebuild', panel: 'builder', command: 'face.rebuild' },
  { key: 'cut:join_faces', stage: 'walls', label: 'Join faces', tip: 'Makes one face a strip from up and down faces, several runs or a build’s steps', icon: 'join-faces', panel: 'cut', mode: 'strips', command: 'face.join' },

  // Solids. Cut solids' six, in the order the work is done; the panel's own choice keeps its order.
  { key: 'cut', stage: 'solids', label: 'Cut solids', tip: 'into strips by faces, by a surface, grid or RL, or one between two surfaces', icon: 'cut', panel: 'cut' },
  { key: 'cut:between', stage: 'solids', label: 'Between surfaces', tip: 'Makes one solid inside a closed string, between two surfaces.', icon: 'between', panel: 'cut', mode: 'between', command: 'solid.between_surfaces', step: { takes: 'a closed string, top and bottom' } },
  { key: 'cut:strips', stage: 'solids', label: 'Strips by faces', tip: 'Slices solids into strips by faces, from the pit out.', icon: 'strips', panel: 'cut', mode: 'strips', command: 'solid.slice_by_faces', step: { takes: 'solids and a face a strip' } },
  { key: 'cut:cut', stage: 'solids', label: 'By surface or RL', tip: 'Cuts solids by a surface, a grid or an RL.', icon: 'cut', panel: 'cut', mode: 'cut', command: 'solid.cut_by_surface', step: { takes: 'solids and a surface, grid or RL' } },
  { key: 'cut:seams', stage: 'solids', label: 'By seams', tip: "Cuts solids by the seam model's horizons, in one run.", icon: 'seams', panel: 'cut', mode: 'seams', command: 'solid.cut_by_seams', step: { takes: 'solids and a seam model' } },
  { key: 'cut:benches', stage: 'solids', label: 'By benches', tip: 'Cuts pieces at bench levels, by the bench rules.', icon: 'benches', panel: 'cut', mode: 'benches', command: 'solid.cut_by_benches', step: { takes: 'solids and bench rules' } },
  { key: 'cut:polygons', stage: 'solids', label: 'By blocks', tip: 'Cuts solids vertically into blocks, by named polygons.', icon: 'by-blocks', panel: 'cut', mode: 'polygons', command: 'solid.cut_by_polygons', step: { takes: 'solids and block polygons' } },
  { key: 'blocks', stage: 'solids', label: 'Block table', tip: 'solids and their attributes to sort, filter, add up, select or delete', icon: 'blocks', panel: 'blocks' },
  { key: 'solids.repair', stage: 'solids', label: 'Repair', tip: 'Fixes closed meshes kept as surfaces because they could not be solids', icon: 'repair', command: 'solid.repair', action: 'solids.repair' },
  { key: 'import.solids', stage: 'solids', label: 'Import solids', tip: 'A zip of solids, or files of them', command: 'solid.import_many', action: 'import.solids', menu: { in: 'import', at: 4, name: 'A zip or files…', under: SOLIDS_MENU } },
  { key: 'import.solids_folder', stage: 'solids', label: 'Import a folder of solids', tip: 'Every solid in a folder', command: 'solid.import_many', action: 'import.solids_folder', menu: { in: 'import', at: 5, name: 'A folder…', under: SOLIDS_MENU } },
  { key: 'import.solids_repaired', stage: 'solids', label: 'Import solids, repaired', tip: REPAIRED, command: 'solid.import_many', action: 'import.solids_repaired', menu: { in: 'import', at: 6, name: 'A zip or files…', under: SOLIDS_MENU, rule: true, heading: 'Repaired as they come in' } },
  { key: 'import.solids_folder_repaired', stage: 'solids', label: 'Import a folder of solids, repaired', tip: REPAIRED, command: 'solid.import_many', action: 'import.solids_folder_repaired', menu: { in: 'import', at: 7, name: 'A folder…', under: SOLIDS_MENU } },
  { key: 'export.selected_solids', stage: 'solids', label: 'Export selected solids', tip: 'The selected solids as a zip or a folder, a DXF each', command: 'solid.export_many', action: 'export.selected_solids', needs: 'selected-solids', menu: { in: 'export', at: 3, name: 'Selected solids to a zip…' } },
  { key: 'export.visible_solids', stage: 'solids', label: 'Export visible solids', tip: 'Every solid on show as a zip or a folder, a DXF each', command: 'solid.export_many', action: 'export.visible_solids', menu: { in: 'export', at: 6, name: 'Visible solids to a zip…' } },

  // Geology.
  { key: 'import.vulcan_grid', stage: 'geology', label: 'Import seam grids', tip: 'Pick a roof, floor or topography, and the grids of that name come with it', icon: 'grids', command: 'grid.import_vulcan_seam', action: 'import.vulcan_grid', menu: { in: 'import', at: 2, name: 'Vulcan grid…' } },
  { key: 'seam.suggest', stage: 'geology', label: 'Suggest a stack definition', tip: "Writes a starting definition from the grids' geometry, as a CSV to edit", short: 'Suggest definition', icon: 'suggest', command: 'seam.suggest_definition', action: 'seam.suggest', menu: { in: 'import', at: 10, name: 'Suggest a stack definition (CSV)…', rule: true, heading: 'Seam model' } },
  { key: 'seam.build', stage: 'geology', label: 'Build a seam model', tip: 'Builds and checks a seam model from a definition', short: 'Build seam model', icon: 'seam-model', command: 'seam.build_model', action: 'seam.build', menu: { in: 'import', at: 11, name: 'Build and check from a definition…' } },
  { key: 'seam.relink', stage: 'geology', label: 'Relink geology', tip: 'Points a grid folder at its new place', icon: 'relink', command: 'seam.relink', action: 'seam.relink', menu: { in: 'import', at: 12, name: 'Relink geology…' } },

  // Reserves.
  { key: 'reserves', stage: 'reserves', label: 'Reserves', tip: "interrogate blocks against a seam model, or set them beside Spry's or Deswik's numbers", icon: 'reserves', panel: 'reserves' },
  { key: 'reserves:interrogate', stage: 'reserves', label: 'Interrogate', tip: "Writes each seam grid's value onto blocks, from a seam model.", panel: 'reserves', mode: 'interrogate', command: 'solid.interrogate', step: { takes: 'blocks and a seam model' } },
  { key: 'reserves:compare', stage: 'reserves', label: 'Compare', tip: "Sets blocks beside Spry's or Deswik's numbers.", icon: 'compare', panel: 'reserves', mode: 'compare', command: 'solid.compare' },
  { key: 'params', stage: 'reserves', label: 'Parameters', tip: 'parameter tables and dataflows edited as grids, a picked solid\'s values marked where they come from', icon: 'parameters', panel: 'params' },
  { key: 'restable', stage: 'reserves', label: 'Reserves table', tip: "a node's fields down the side, broken down by its children across, from Imported to Scheduled", icon: 'reserves-table', panel: 'restable' },
  { key: 'workflow:report', stage: 'reserves', label: 'Report', tip: "Writes a workflow's report, every case from its latest run", icon: 'report', panel: 'workflow' },

  // Always there, whatever the stage.
  { key: 'project.save', stage: 'always', label: 'Save', tip: 'Saves the project', icon: 'save', shortcut: 'Ctrl+S', action: 'project.save', menu: { in: 'project', at: 4, rule: true } },
  { key: 'edit.undo', stage: 'always', label: 'Undo', tip: 'Takes back the last change', icon: 'undo', shortcut: 'Ctrl+Z', action: 'edit.undo', needs: 'undo' },
  { key: 'edit.redo', stage: 'always', label: 'Redo', tip: 'Puts back the change just undone', icon: 'redo', shortcut: 'Ctrl+Y', action: 'edit.redo', needs: 'redo' },
  { key: 'workflow', stage: 'always', label: 'Workflow', tip: 'saved steps run again in one go, captured from the Run log or added from the tool panels', icon: 'workflow', panel: 'workflow' },
  { key: 'canvas', stage: 'always', label: 'Workflow canvas', tip: 'a workflow as nodes and wires: what each step reads', panel: 'canvas' },
  { key: 'layers', stage: 'always', label: 'Layers', tip: 'the layer tree: show, hide, select and arrange', panel: 'layers' },
  { key: 'properties', stage: 'always', label: 'Properties', tip: "what's picked: its name, layer, attributes and measures", panel: 'properties' },
  { key: 'runlog', stage: 'always', label: 'Run log', tip: 'every run, what it made and what it said', panel: 'runlog' },
  { key: 'panels.hide', stage: 'always', label: 'Hide every panel, or bring them back', tip: 'The view fills the window', shortcut: '`', action: 'panels.hide' },
  { key: 'panels.reset', stage: 'always', label: 'Reset layout', tip: 'Puts the panels back where they start', action: 'panels.reset' },

  // The view's own tools, which the view bar draws itself (workstream 2); listed so Find a command reaches them.
  { key: 'view.select', stage: 'always', label: 'Select', tip: 'Select and edit strings', icon: 'select', action: 'view.select' },
  { key: 'view.draw', stage: 'always', label: 'Draw string', tip: 'Draw a string: snaps to string vertices and segments, then surfaces', icon: 'draw', action: 'view.draw' },
  { key: 'view.snap', stage: 'always', label: 'Snap', tip: 'Turns snap on, or off', icon: 'snap', shortcut: 'S', action: 'view.snap' },
  { key: 'view.plan', stage: 'always', label: 'Plan or 3D', tip: 'Plan view, straight down and north up, or back to 3D', icon: 'plan', shortcut: 'T', action: 'view.plan' },
  { key: 'view.zoom_selection', stage: 'always', label: "Zoom to what's selected", tip: "From the way you're looking", icon: 'zoom-selection', shortcut: 'F', action: 'view.zoom_selection' },
  { key: 'view.zoom_all', stage: 'always', label: 'Zoom to everything', tip: 'The whole project in the view', icon: 'zoom-all', shortcut: 'E', action: 'view.zoom_all' },
  { key: 'view.section', stage: 'always', label: 'Section', tip: 'Pick two points for a true vertical slice, or close the one open', icon: 'section', shortcut: '2', action: 'view.section' },
];

export const toolByKey = (key: string): Tool | undefined => TOOLS.find((t) => t.key === key);

/** The tool that opens a panel as it stands: the one keyed by the panel's id. */
export const panelTool = (panel: PanelId): Tool => {
  const tool = TOOLS.find((t) => t.key === panel && t.panel === panel);
  if (!tool) throw new Error(`no tool opens the ${panel} panel`);
  return tool;
};

/** The tool for one of what a panel does. */
export const choiceTool = (panel: PanelId, mode: string): Tool => {
  const tool = TOOLS.find((t) => t.panel === panel && t.mode === mode && t.key === `${panel}:${mode}`);
  if (!tool) throw new Error(`no tool for ${panel} set to ${mode}`);
  return tool;
};

/** A panel's choices as its own choice words them, and the line each puts at the panel's head. */
export function choiceWords<M extends string>(panel: PanelId, modes: readonly M[]): { labels: Record<M, string>; lines: Record<M, string> } {
  const labels = {} as Record<M, string>;
  const lines = {} as Record<M, string>;
  for (const mode of modes) {
    const tool = choiceTool(panel, mode);
    labels[mode] = tool.choice ?? tool.label;
    lines[mode] = tool.tip;
  }
  return { labels, lines };
}

/** A stage's tools, in the order they're listed. */
export const stageTools = (stage: Stage): Tool[] => TOOLS.filter((t) => t.stage === stage);

/** A menu's tools, top to bottom. */
export const menuTools = (menu: MenuId): (Tool & { menu: MenuPlace })[] =>
  TOOLS.filter((t): t is Tool & { menu: MenuPlace } => t.menu?.in === menu).sort((a, b) => a.menu.at - b.menu.at);

/** A menu on a row of tools: one of the toolbar's menus whole, or a few tools under a name. */
export interface RowMenu {
  label: string;
  icon: ToolIcon;
  tip: string;
  /** One of the toolbar's menus, top to bottom. */
  menu?: MenuId;
  /** Or these tools, as their menu words them. */
  keys?: readonly string[];
}

/** What sits on a row: a tool by its key, a rule between groups, or a menu. */
export type RowItem = string | '|' | RowMenu;

const IMPORT_MENU: RowMenu = { label: 'Import', icon: 'import', tip: 'Bring in surfaces, grids, strings, solids, Deswik and Spry, and build a seam model', menu: 'import' };
const EXPORT_MENU: RowMenu = { label: 'Export', icon: 'export', tip: 'Strings, surfaces and solids out to DXF, CSV or a zip', menu: 'export' };

/**
 * The row of tools under each stage's tab, left to right, in the order the work is done (decision 0076). A tool is in
 * one stage, but a row may show one from another where the work wants it there too: Relink geology on Project.
 */
export const ROWS: Record<Exclude<Stage, 'always'>, readonly RowItem[]> = {
  project: ['project.new', 'project.open', 'project.save_as', '|', IMPORT_MENU, EXPORT_MENU, '|', 'seam.relink'],
  strings: [
    'strings:offset',
    'strings:number_across',
    'strings:polygons_from_lines',
    '|',
    'strings:set_grade',
    'strings:set_elevation',
    'strings:drape',
    '|',
    'strings:split',
    'strings:join',
    'strings:smooth',
    'strings:filter',
    'strings:reverse',
    'strings:set_closed',
  ],
  walls: ['face', 'builder', 'builder:rebuild', 'cut:join_faces'],
  solids: [
    'cut:between',
    'cut:strips',
    'cut:cut',
    'cut:seams',
    'cut:benches',
    'cut:polygons',
    '|',
    'blocks',
    'solids.repair',
    '|',
    { label: 'Import solids', icon: 'solids-in', tip: 'Closed triangulations in Vulcan .00t or DXF files, or a zip or folder of them', keys: ['import.solids', 'import.solids_folder', 'import.solids_repaired', 'import.solids_folder_repaired'] },
    { label: 'Export solids', icon: 'solids-out', tip: 'Solids out as a zip or a folder, a DXF each', keys: ['export.selected_solids', 'export.visible_solids'] },
  ],
  geology: ['import.vulcan_grid', 'seam.suggest', 'seam.build', 'seam.relink'],
  reserves: ['reserves', 'params', 'restable', 'reserves:compare', 'workflow:report'],
};

/** At the right of every row, whatever the stage. */
export const ALWAYS_ROW: readonly string[] = ['workflow'];

/** The tools a row holds, its menus' tools among them: what's one click, or two through a menu, from its tab. */
export function rowTools(stage: Exclude<Stage, 'always'>): Tool[] {
  return ROWS[stage].flatMap((item) => {
    if (item === '|') return [];
    if (typeof item === 'string') return [toolByKey(item)!];
    return item.menu ? menuTools(item.menu) : (item.keys ?? []).map((key) => toolByKey(key)!);
  });
}

/**
 * Whether a tool is the one in use when its panel is in front, set to `mode`: a panel's own tool, or the tool for the
 * choice the panel is set to. A tool that only opens a panel at something (Rebuild from here) never is.
 */
export const inUse = (tool: Tool, mode: string | undefined): boolean => !!tool.panel && (tool.key === tool.panel || (!!tool.mode && tool.key === `${tool.panel}:${tool.mode}` && tool.mode === mode));

/** What a toolbar button's tip says: a panel's name then what it's for, a tool's name and its keys, or what it does. */
export const tipOf = (tool: Tool): string => (tool.panel && tool.key === tool.panel ? `${tool.label}: ${tool.tip}` : tool.shortcut ? `${tool.label} (${tool.shortcut})` : tool.tip);

/** A tool's name on a row. */
export const rowName = (tool: Tool): string => tool.short ?? tool.label;

/** A menu item's words, with how many it would take where that's more than one ("Selected to DXF (3)…"). */
export function menuWords(tool: Tool, count: number | null): string {
  const name = tool.menu?.name ?? tool.label;
  if (count === null || count <= 1) return name;
  return name.endsWith('…') ? `${name.slice(0, -1)} (${count})…` : `${name} (${count})`;
}

/** A stage's name, and Always there for what isn't in one. */
export const stageTitle = (stage: Stage): string => STAGES.find((s) => s.id === stage)?.title ?? 'Always there';

/** What Find a command matches a tool by, besides its name: its other names, its stage, what it does and its command. */
export const findWords = (tool: Tool): string[] =>
  [tool.short, tool.choice, tool.menu?.name, tool.menu?.under?.name, stageTitle(tool.stage), tool.tip, tool.command, tool.shortcut].filter((w): w is string => !!w);

/**
 * How well a tool answers what's typed into Find a command, from 0 (not at all) to 1. Every word typed must be in the
 * tool's name, its other names, its stage, its command or what it does, as typed: nothing is guessed from scattered
 * letters. A name that starts with a word comes before one that holds it, a name before a stage or a command, and those
 * before what the tool does.
 */
export function findScore(tool: Tool, typed: string): number {
  const whole = typed.trim().toLowerCase();
  const words = whole.split(/\s+/).filter(Boolean);
  if (words.length === 0) return 1;
  const name = tool.label.toLowerCase();
  if (name.startsWith(whole)) return 1;
  const nameWords = name.split(/[^a-z0-9]+/);
  const [does, ...others] = findWords(tool)
    .map((w) => w.toLowerCase())
    .sort((a, b) => Number(b === tool.tip.toLowerCase()) - Number(a === tool.tip.toLowerCase()));
  const other = others.join(' | ');
  let score = 0.95;
  for (const word of words) {
    const here = nameWords.some((w) => w.startsWith(word)) ? 0.9 : name.includes(word) ? 0.8 : other.includes(word) ? 0.6 : does.includes(word) ? 0.4 : 0;
    if (here === 0) return 0;
    score = Math.min(score, here);
  }
  return score;
}

/** Every tool for Find a command, a stage at a time in the order the work is done, then what's always there. */
export const findGroups = (): { title: string; tools: Tool[] }[] => [...STAGES.map((s) => s.id), 'always' as const].map((stage) => ({ title: stageTitle(stage), tools: stageTools(stage) }));

/** The tools that can add a step to a workflow, for the canvas. */
export const stepTools = (): (Tool & { panel: PanelId; command: string; step: { takes: string } })[] =>
  TOOLS.filter((t): t is Tool & { panel: PanelId; command: string; step: { takes: string } } => !!t.step && !!t.panel && !!t.command);
