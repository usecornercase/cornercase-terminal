import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
import { ISSUES } from '../src/lib/demo/data';
import { fitIssueButtons } from '../src/lib/demo/ui';
import { world } from '../src/lib/demo/world';

const key = (k: string) => ({ key: k, metaKey: false, isComposing: false, ctrlKey: false, altKey: false, shiftKey: false });

const type = (app: App, text: string) => {
  for (const ch of text) inputKey(app, key(ch));
};

test('the GitLab tab asks for the server, then the token, then lists its issues', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.advance(500);
  app.issuesTab(app.config.sources.indexOf('gitlab'));
  app.render();
  assert.ok(app.textAt('Leave it empty for gitlab.com.'));
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('Connecting to https://gitlab.com.'));
  type(app, 'glpat');
  inputKey(app, key('Enter'));
  app.advance(1500);
  app.render();
  assert.ok(app.textAt('Cache the product images at the edge'));
  assert.ok(app.textAt('@you in gitlab.com'));
  assert.equal(app.branchFor({ source: 'gitlab', number: 36, key: '#36', title: 'Cache the product images at the edge', labels: [], state: 'open', author: 'marta', age: '2d', url: '', body: '', mine: false }), '36-cache-the-product-images-at-the-edge');
  app.destroy();
});

test('a self-hosted GitLab server is checked and its back button returns to it', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.issuesTab(app.config.sources.indexOf('gitlab'));
  type(app, 'gitlab.acme.dev');
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('a URL starts with https://, such as https://gitlab.acme.dev'));
  for (let i = 0; i < 'gitlab.acme.dev'.length; i++) inputKey(app, key('Backspace'));
  type(app, 'https://gitlab.acme.dev/');
  inputKey(app, key('Enter'));
  app.render();
  assert.equal(app.config.gitlabUrl, 'https://gitlab.acme.dev');
  assert.ok(app.textAt('Connecting to https://gitlab.acme.dev.'));
  app.issuesButton('back');
  app.render();
  assert.ok(app.textAt('https://gitlab.acme.dev'));
  assert.ok(app.textAt('Leave it empty for gitlab.com.'));
  app.destroy();
});

test('an http GitLab server on another machine is saved with a warning', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.issuesTab(app.config.sources.indexOf('gitlab'));
  type(app, 'http://gitlab.acme.dev');
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('Careful: the address starts with http://, so the token travels unencrypted.'));
  app.destroy();
});

test('a host name that starts like a loopback address still gets the warning', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.issuesTab(app.config.sources.indexOf('gitlab'));
  type(app, 'http://127.example.com');
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('Careful: the address starts with http://, so the token travels unencrypted.'));
  app.destroy();
});

test('the issue list offers start a flow, which needs a real cornercase', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.advance(500);
  app.render();
  assert.ok(app.textAt('start a flow'));
  app.issuesButton('start a flow');
  app.render();
  assert.ok(app.textAt('the demo only plays the flow in payments'));
  app.destroy();
});

test('a narrow issue list drops schedule, then start a flow, and keeps start, refresh and cancel', () => {
  const labels = ['start', 'schedule', 'start a flow', 'refresh', 'cancel'];
  const room = (shown: string[]) => shown.reduce((sum, label) => sum + [...label].length + 3, 0) - 1;
  const without = ['start', 'start a flow', 'refresh', 'cancel'];
  assert.deepEqual(fitIssueButtons(labels, room(labels)), labels);
  assert.deepEqual(fitIssueButtons(labels, room(without)), without);
  assert.deepEqual(fitIssueButtons(labels, room(without) - 1), ['start', 'refresh', 'cancel']);
});

test('the Asana tab takes a token, lists its tasks and names the branches after them', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.config.asanaProjects = '1201';
  app.openIssues();
  app.advance(500);
  app.issuesTab(app.config.sources.indexOf('asana'));
  app.render();
  assert.ok(app.textAt('Connect Asana.'));
  assert.ok(app.textAt('Create a personal access token in Asana under Settings → Apps → Developer apps'));
  assert.ok(app.textAt('It is saved in secrets.json'));
  type(app, 'tok');
  inputKey(app, key('Enter'));
  app.advance(1500);
  app.render();
  assert.ok(app.textAt('Offer gift wrapping at checkout'));
  assert.ok(app.textAt('Fix the empty cart message'));
  assert.ok(app.textAt('@you in Acme'));
  const [withId, withoutId] = ISSUES.filter((i) => i.source === 'asana');
  assert.equal(app.branchFor(withId), 'SHOP-23-offer-gift-wrapping-at-checkout');
  assert.equal(app.branchFor(withoutId), 'asana-1205551234567890-fix-the-empty-cart-message');
  app.destroy();
});

test('the Asana tab lists your tasks without projects, and the tasks of the projects set otherwise', () => {
  const open = (projects: string) => {
    const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
    app.config.accounts.asana = true;
    app.config.asanaWorkspaceName = 'Acme';
    app.config.asanaProjects = projects;
    app.openIssues();
    app.advance(500);
    app.issuesTab(app.config.sources.indexOf('asana'));
    app.advance(1500);
    app.render();
    return app;
  };
  const yours = open('');
  assert.ok(yours.textAt('Offer gift wrapping at checkout'));
  assert.ok(!yours.textAt('Fix the empty cart message'));
  yours.destroy();
  const project = open('1201');
  assert.ok(project.textAt('Offer gift wrapping at checkout'));
  assert.ok(project.textAt('Fix the empty cart message'));
  project.destroy();
  const other = open('https://app.asana.com/1/1200/project/9999/list');
  assert.ok(!other.textAt('Offer gift wrapping at checkout'));
  assert.ok(!other.textAt('Fix the empty cart message'));
  other.destroy();
});

test('start asks for the branch and the workspace name, prefilled, and starts with what was typed', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.advance(500);
  inputKey(app, key('Enter'));
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('Start #482 Returns page crashes on empty address in a new worktree'));
  assert.ok(app.textAt('issue-482-returns-page-crashes-on-empty-address'));
  for (let i = 0; i < 'issue-482-returns-page-crashes-on-empty-address'.length; i++) inputKey(app, key('Backspace'));
  type(app, 'ana/482');
  inputKey(app, key('Tab'));
  for (let i = 0; i < 60; i++) inputKey(app, key('Backspace'));
  type(app, 'Returns');
  inputKey(app, key('Enter'));
  app.advance(1000);
  const shop = app.projects[app.active];
  const made = shop.workspaces.find((w) => w.branch === 'ana/482');
  assert.equal(made?.name, 'Returns');
  app.destroy();
});

test('a branch git would refuse is not started', () => {
  const app = new App(); app.virtual = true; app.resize(110, 38); world(app, { scripted: true });
  app.openIssues();
  app.advance(500);
  inputKey(app, key('Enter'));
  inputKey(app, key('Enter'));
  type(app, ' x');
  inputKey(app, key('Enter'));
  app.render();
  assert.ok(app.textAt('cannot be a branch'));
  assert.equal(app.overlay?.kind, 'issues');
  app.destroy();
});
