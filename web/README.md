# hearts-web

Play Hearts against three [hearts-engine](..) bots in the browser. The whole
game runs client-side as WebAssembly; there is no server and no external asset.

## Build

Install `wasm-pack`, then build the package from this directory:

```console
wasm-pack build --release --target web
```

This writes the generated JavaScript and WebAssembly to `pkg/`.

## Play

Serve this directory over HTTP — ES modules and wasm do not load from
`file://`:

```console
python3 -m http.server
# open http://localhost:8000/
```

Pick Easy, Medium, Hard, or Expert in the header; the saved choice applies to
the next new game. Click cards to pass or play. `h` opens a solver hint, `l`
toggles the log, and `n` starts a new game (or continues after a round).

The hint depth adapts to measured latency on the device. Equity is the chance
to win the game; round EV is expected penalty points, so lower is better.

## Deploy

`pkg/`, `index.html`, `app.js`, and `style.css` are static. The repository's
[Pages workflow](../.github/workflows/pages.yml) builds and publishes them on
pushes to `main`; any static host works too.

## Test

The wasm wrapper's native tests can be run with:

```console
cargo test
```
