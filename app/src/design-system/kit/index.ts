/**
 * The kit (UI plan): the building blocks every tool panel is made of, so each one looks and works the same. A new panel
 * takes its inputs, choices, sections, run result and button from here, and its colours from the theme's tokens.
 */
export { IconButton, RoadBadge, StatusBadge } from './badges';
export { Choice, Field, NumberInput, Select, TargetInput, TargetOptions } from './fields';
export { type Range, readNumber, showNumber } from './number';
export { type RunReport, RunResult, type ToolAction, ToolPanel, ToolSection } from './ToolPanel';
export { warns } from './words';
