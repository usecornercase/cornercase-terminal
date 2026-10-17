import { Grid, rect } from './term/grid';
import { themes } from './term/palette';
import { toSvg } from './term/svg';
import { App } from './demo/app';
import { diagramHeight, drawDiagram, plan, type Flow } from './demo/flow';
import { Painter } from './demo/ui';
import type { RegistryFlow } from './registry';

const COLS = 60;

export function diagramSvg(flow: RegistryFlow, label: string): string {
  const shown: Flow = {
    name: flow.name,
    orchestrator: { name: 'orchestrator', agent: flow.orchestrator.agent, model: flow.orchestrator.model },
    roles: flow.roles.map((r) => ({ name: r.name, agent: r.agent, model: r.model, own: r.worktree === 'own' })),
    stages: flow.stages,
    end: flow.end === 'commits' ? 'leave the commits' : 'open a pull request',
  };
  const d = plan(shown);
  const rows = diagramHeight(d, COLS);
  const app = new App();
  app.virtual = true;
  const grid = new Grid(COLS, rows);
  drawDiagram(new Painter(app, grid), rect(0, 0, COLS, rows), d, 0, () => {});
  app.destroy();
  return toSvg(grid, { label, theme: themes.tokyo, className: 'term-svg' });
}
