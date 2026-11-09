# Website › The landing's show

Part of [Website](../website.md).

Code: `site/src/lib/show/`, `site/src/layouts/Landing.astro`, `site/src/components/landing/` (`Stage.astro`, `Hero.astro`, `Features.astro`, `Why.astro`, `Play.astro`, `Flows.astro`)

The landing page as a show driven by the scroll, asked for on 2026-11-09 ("much more eye-catching, full motion", like the page of the GPUI window at `gui.usecornercase.dev`, whose own `design/website.md` this follows where it can). On the landing only, it reverses two decisions of the October brand: motion that only explains, and a page that stays still until asked (#80). Its texts, sections, anchors, nav labels and numbers did not change, and neither did `/plugins/`, `/flows/`, `/stats/` or the docs.

## One page, two forms

- **The plain page is the document, the show is the same markup with `html.motion`**: tall sections whose content is pinned, a fixed stage, a field behind everything. A section's rules for the show sit under `:global(.motion)` in the component that owns its markup, so a text or a link exists once. Rejected: a second page, or a second set of components, which would need the three locales and the docs links kept in step twice.
- **The order**: the hero, Open a folder (the nine built-ins), the statement of Why and its four steps, Or hand all of it to an agent, Your turn (the live demo), Flows; then Everything else, the speed and the closing flow as an ordinary page. After six pinned chapters a reader wants to scroll normally, and those are reference material.
- **Dark only**, like `/stats/`: `data-theme="dark"` on `<html>` sets the dark tokens whatever the system prefers (`Race.astro` takes its bar colours from it too). The field adds light to a dark ground and the terminal pictures were always dark. A light field, ink on paper, was not tried.
- **Square stays**: corners, 1 px lines, a solid nav bar. What comes back from what October removed is depth: a shadow and a cobalt glow under the stage window, scrims behind text, the field. No glass and no blur: the GPUI page has them because its window is glass, and this one is a terminal.
- **The corner** is still used twice: the two cobalt bars outside the stage window's top-left, and the mark beside the closing title, which the field draws in cells.
- **No runtime dependency**: `lib/show/` is plain TypeScript. `lib/show/field/` (`gl.ts`, `shaders.ts`, `field.ts`, `life.ts`) is the GPUI page's `client/field/`, taken as it is (same organisation, MIT or Apache-2.0); a fix to one belongs in both.
- **`og.png` and `readme.png` stay on the brand's light page**: the brand, the hero's text and their scenes did not change.

## The field

- **A fluid drawn in terminal cells** behind the whole page (WebGL2): Navier-Stokes on a small grid, shown by a pass that picks a glyph per cell from an atlas of JetBrains Mono, by the dye's density or, where it runs fast, by its direction (`─ ╱ │ ╲`). Five walkers keep it alive, the pointer pushes it and the scroll's speed drifts it. Its colours are the app's: cobalt, and yellow, orange and green while Ten agents, one glance shows who works, who waits and who is done, with a burst at each beat of the story (`stage.ts` `FLOODS`).
- **The mark is drawn by the same pass**: it assembles at load, is stamped into the dye and blown away as the headline rises, and assembles again beside the closing title (`story.ts` `Finale`).
- **It gives up by itself**: under 38 fps for 1.5 s it lowers the pressure iterations, then drops to one device pixel (`.lite`), and the stage's canvas with it. A lost context pauses it and it is rebuilt when the context comes back.
- **It dims where there is text to read** (`main.ts` `settle`): each chapter sets its gain, and the flowing sections sit on a dark layer (`.backed`).

## The stage: the simulation, recorded

- **The window on the stage is the simulation itself** (`lib/demo/`), not a drawing. The GPUI page drew its window in HTML because it has no simulation; this site has one that mirrors the app and is kept in step by rule, and a drawing would be a second thing to keep in step.
- **Recorded, not run** (`reel.ts`): the simulation only goes forward, and a page driven by the scroll must go back, jump to an anchor and reload midway. So the story is played once on the simulation's virtual clock and every 50 ms tick is kept as the cells that changed, with what they were, so a `Screen` reaches any tick by applying deltas or undoing them. Cells are shared (`pool`), so a whole story is about 3 MB. Recording takes about half a second (2026-11-09, an M5) and is spread over frames, 5 ms at a time, ahead of the scroll. Rejected: rebuilding the world and replaying up to the scroll's position on every frame of a scroll back (up to about 8 ms at the story's end on that M5, several times that on a slower machine, against a seek that costs what changed); pictures made at build time and cross-faded (no typing, no spinner, megabytes of SVG); running the simulation with triggers (it cannot go back).
- **The same frame for the same scroll**: the recording takes its jitter from a seeded source (`App.random`, which `programs.ts` now reads instead of `Math.random`; the live demo keeps `Math.random`), so two recordings are equal and a test can say what is on screen at a mark.
- **The story is the tour's** (`script.ts`): the four chapters of `boot.ts` as timed steps, so the stage and the live demo tell one story. It finds what it clicks by its text, like the tour, and `expect` steps say what must be true by then (the trust question, the issue in the prompt, the fix). `tests/reel.test.ts` records it at every size the page can pick and fails on a missed click or an unmet expectation. It differs from the tour in what a scroll needs: every step has a fixed length, waits that the tour sped up are `rate` steps (the agent fixing, the 25 minutes away), the pointer's path is computed (`pointerAt`) instead of a CSS transition, and there is no narration strip, since the captions are the page's.
- **Nine seconds of lead play in real time under the hero** (`LEAD`), then the scroll owns the clock. So the first screen is alive (a spinner, a toast) without the page moving anything a reader is reading, and from the first chapter on a scroll position is one frame. While nobody scrolls, the lead's last second plays back and forth (`IDLE`, `idleAt`): a second in which only the agent's spinner turns, so the terminal never freezes and nothing in it changes.
- **Sharp at every zoom**: the canvas is drawn at the camera's closest zoom times the device's pixel ratio, capped at 9 million pixels (`stage.ts`), and only the rows a tick changed are redrawn (`CanvasView.draw` takes a span of rows).
- **Sizes** (`track.ts` `sizeFor`): under 700 px the compact layout at 34 rows, which is taller than the room above the caption, so the camera frames its top or its bottom (the `More` menu); to 980 px the columns that fit at a 7.2 px cell; above, the columns and rows that fit the free area at a cell of 7.2 to 9.4 px, at most 196×48.

## The camera

- **A camera, not a layout** (`track.ts`, `shots.ts`), as on the GPUI page: a shot names a rectangle of the window, in cells, and the side left free for the caption, and its pose is worked out for the viewport. On a phone the caption is a sheet at the bottom and the window is framed above it.
- **Keys sit on the scroll, and some on the story**: `mark('picked', -300, …)` places a key where the scroll reaches that moment (`whereIs`), so the camera moves when the story does and a change of timing in `script.ts` moves both.
- **Each element carries its whole transform, perspective included** (`lens`), and the window and the nine layers are siblings in one stacking order, lowest first. The GPUI page puts `perspective` on the scene and `preserve-3d` down the tree; here that would ask every browser to keep a 3D context across a canvas, opacity and clipping, and the order of nine flat layers is known anyway.
- **Large screens** (`--big`, `bigOf`: 1.25 from 1900 px, 1.5 from 2400 px) scale the captions and the room the camera leaves them together.

## The chapters

- **Open a folder** is an exploded view: the window turns isometric and the nine built-ins rise as layers, one per tile and in the tiles' order (`layers.ts`), while the list beside it opens the one that is up. The layers are the simulation's own pictures (`stills.ts`: the issues dialog, the changes, files and flow panels, the compact window as a phone), the server is a slab under the window, and scripts are a command line above it. Rejected: a ring of screens, as the GPUI page has for its sheets: these nine are parts of one window, and the stack shows that.
- **The statement** of Why stands alone over the dimmed window before its four steps, which are the story.
- **Close the window** shows the outer shell on the stage and, beside it, the diagram the plain page animates by itself, here driven by the story: closed, a clock that runs to 25 minutes, the programs finishing.
- **Or hand all of it to an agent** types its transcript by the scroll.
- **Your turn** is the live demo as it was (a still picture, mounted on the first click, the tour on request): after watching, a reader can try it.
- **Flows** pins its diagram and runs `frameAt` by the scroll, without the pause button: nothing there moves by itself. The plain page keeps the timed run and its button.

## Without the show

- **The plain page comes first**: with reduced motion (also when it is turned on with the page open), without JavaScript or WebGL2, under 420 px of height, if the script has not run after 4 s or if it throws, the page is the document (`html:not(.motion)`). The inline script in `Landing.astro` adds `.motion` before the first paint, so the show has no flash of the plain page. Giving up puts back every style, class and text the show wrote (`paint.ts` `wipe`) and starts the flow diagram's own clock.
- **A screen reader reads the same page in both**: the stage and the field are `aria-hidden`; captions fade with `opacity` and `pointer-events`, never `visibility`, so they stay in the tree and in the tab order, and a link focused in a chapter that is not on screen scrolls it in (`main.ts` `reach`); the headline is split in letters only inside `aria-hidden` spans, with the whole text as its `aria-label`.
- **Anchors land inside their chapter**: a pinned section's top is its empty start, so the nav's targets take a negative `scroll-margin-top`; `#install` lands at the top, where the install box is.

## Checked

2026-11-09, on an M5 with macOS 26.6, headless with Playwright 1.65:

- **Chrome for Testing 154** (GPU through ANGLE Metal): every chapter at several scroll positions at 1440×900 and 2560×1440 (both at two device pixels), 1920×1080, 1280×720, 1024×768, 820×1180 and 390×844 (touch); the intro frame by frame; the three locales; the built site; a scroll of the whole page at 1,400 px/s held 60 fps in every section, with a worst frame of 16.8 ms. Also reduced motion at load and turned on midway, no JavaScript, no WebGL2, a refused context, every anchor, a system in light mode, and the live demo with its tour.
- **Firefox 159**, 1440×900: the same pictures as Chrome, the 3D included.
- **WebKit 27.2** (Playwright's build): the page runs, and the computed matrices and boxes of the window and the layers equal Chrome's. Its screenshots flatten a 3D transform to an affine one, so the 3D look could not be seen there.
- **Not checked**: Safari itself, a real phone, a weak GPU, browsers on Windows or Linux.
