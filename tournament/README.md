# Cross-engine tournament vs brianberns/Hearts (Deep CFR)

`CfrShim` bridges this crate's driver to the Deep CFR model of
[brianberns/Hearts](https://github.com/brianberns/Hearts), the strongest
open-source Hearts player we know of.  The Rust side is
[`examples/vs_cfr.rs`](../examples/vs_cfr.rs), which mirrors Brian's own
benchmark methodology (`Hearts/Tournament.fs`): duplicate 2v2 deals scored
by the per-deal zero-sum payoff `mean(others' points) − own points`.

## Setup

Brian's repo has **no license**, so it is referenced in place as a sibling
clone and never vendored or redistributed:

```sh
cd ../..   # the directory containing hearts-engine
git clone --recurse-submodules https://github.com/brianberns/Hearts.git brianberns-Hearts
cd hearts-engine/tournament/CfrShim
dotnet build -c Release          # .NET SDK 10+; clone path overridable with -p:HeartsRepo=...
```

## Running

```sh
# polite smoke test against Brian's live server (default endpoint)
cargo run --release --example vs_cfr -- --deals 4 --throttle-ms 500

# full run against a locally hosted model (needs AdvantageModel.pt; run
# Brian's Hearts.Web.Harness, then point the shim at it)
cargo run --release --example vs_cfr -- --deals 10000 --throttle-ms 0 \
    --shim "dotnet tournament/CfrShim/bin/Release/net10.0/CfrShim.dll http://localhost:8080"
```

The trained model is not published; the default endpoint is Brian's
personal server at <https://www.bernsrite.com>.  **Keep runs against it
small and throttled.**

## Protocol

One JSON request per line on the shim's stdin; one response per line on
stdout.  The shim replays the public history through Brian's own library
to rebuild his `InformationSet`, so nothing about his wire format is
reimplemented:

```json
{"kind":"play","seat":"N","dir":"left","hand":["QS","TH"],
 "outgoing":["2C","3C","4C"],"incoming":["AH","KH","QH"],
 "plays":[["E","2C"],["S","5C"]]}
{"card":"QS","legal":["2C","QS"]}
```

`legal` echoes Brian's legal-action set; `vs_cfr` compares it with ours on
every decision and aborts on any mismatch, so rules drift between the two
engines cannot silently skew results.  Seat letters, pass-direction names,
and card codes (rank char + `CDHS`) map one-to-one between the engines;
deal-level rules were verified identical (game-to-100, L/R/A/Hold
rotation, 2♣ leads, no first-trick penalties, add-26 moon scoring) with
one exception the drift detector caught: in Brian's engine the **Q♠ also
breaks hearts**; in ours only a heart does.  The shim overrides his
`HeartsBroken` flag to our semantics — it feeds only his legality mask,
not the model's input encoding — so both sides play under identical
rules, at the cost of denying his model a heart lead it was trained to
consider legal after a bare Q♠.  Results should note this caveat.
