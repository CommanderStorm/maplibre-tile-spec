# THROWAWAY prototype — the annotated hexdump view

Three variants of the Inspector's annotated hexdump on one route, switchable with
`?variant=A|B|C` and the floating bar (← / → also cycle). Built for
[wayfinder ticket #15](https://github.com/CommanderStorm/maplibre-tile-spec/issues/15).
**Not for merging.** It lives in a dot-dir so zensical ignores it.

```sh
./gen-fixtures.sh   # once: dumps tiles + JSON into public/fixtures/
npm install
npm run dev         # http://localhost:5175/?variant=A
```

## The variants

| key | name                          | the disagreement                                                        |
| --- | ----------------------------- | ----------------------------------------------------------------------- |
| A   | Dump — CLI stream             | one monospace stream, `offset hex ascii \| annotation`, exactly the CLI  |
| B   | Outline — tree with inline bytes | the region tree is the page; every leaf carries its own bytes, no hex column |
| C   | Split — hex map + inspector   | continuous virtualized hex map left, one region's detail right           |

Each variant places the Source picker and the render controls itself, because where
those sit is one of the questions.

## URL parameters

`variant`, `fixture` (a path from `public/fixtures/index.json`), `width`, `maxBlob`,
`data`, `bits=0`, `layer`, and `chunk` (variant A: how many regions to render up front).
So `?variant=A&fixture=real/omt_z14_8298_10748&chunk=99999&maxBlob=0` is the
pathological case.

## What stands in for the wasm

There is no `annotate` binding yet, so `src/annotated.ts` mirrors the #14 contract
verbatim and wraps the JSON that `cargo run -p mlt-core --example dump_json` writes.
Two places knowingly diverge:

- `decodeBlob` reads the renderer's decoded *text* and parses it back into
  `DecodedBlob`. The real binding returns the arrays directly.
- An upload cannot be annotated at all; the prototype says so instead of guessing.

`filterLayer` is a TS copy of `filter_layer` (`rust/mlt/src/hexdump.rs:104`), which #14
moves into `mlt-core`.

## Measuring

`node measure.mjs` prints load-to-ready, a real-clock re-render time and DOM node counts
per variant against the real tile; `node shots.mjs <dir>` writes screenshots with a
selection made. Both need the dev server up and `npm install --no-save puppeteer-core`.
Chrome's `--virtual-time-budget` fakes `performance.now()`, so plain
`google-chrome --headless --screenshot` reports 0 ms for everything — don't trust it.
