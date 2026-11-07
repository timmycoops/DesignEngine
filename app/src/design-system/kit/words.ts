/** How a run's messages read to the panel that shows them (UI plan: one way of working, item 5). */

/** Words in a run's message that mean the planner should look. */
const WARNING_WORDS = ['stopped', 'no height', 'no target', 'cross', 'pinched', 'edited by hand', 'has gone', 'off its grid', "doesn't cross", 'made no road', 'steeper', 'stops short', "doesn't cover", 'stands up to', 'below the top', 'filled near', 'nothing of'];

export function warns(line: string): boolean {
  return WARNING_WORDS.some((word) => line.includes(word)) && !line.includes('crosses the berm');
}
