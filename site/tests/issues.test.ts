import assert from 'node:assert/strict';
import { test } from 'node:test';
import { App } from '../src/lib/demo/app';
import { inputKey } from '../src/lib/demo/client';
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
