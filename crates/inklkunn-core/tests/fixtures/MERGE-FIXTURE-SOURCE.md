# Merge fixture source

`merge-spike-b-pairs.json` is the 120 pairs of M0's spike B (RFC 001 D-10 b), as the three sides of a merge:
the base, the tip's pages (edit A, which landed on `heads/main`) and the proposal's (edit B, which sat on the
branch). `tests/merge_corpus.rs` merges every pair with the product's function and requires exactly 99 clean
merges and 21 overlaps (RFC 004 D-5).

The spike's report (`rfcs/handoffs/001-foundations/spikes/spike-b-prikk-0.54.1.json`) holds the 120 pairs of
edits but not the page texts. The texts are a pure function of the run's seed, so they are regenerated, without
prikk, by replaying the generator that made the run:

```text
cargo run -p inklkunn-corpus -- merge-fixture \
    --report <absolute path of>/rfcs/handoffs/001-foundations/spikes/spike-b-prikk-0.54.1.json \
    --out <absolute path of>/crates/inklkunn-core/tests/fixtures/merge-spike-b-pairs.json
```

The replay draws the same scenarios and edits in the same order (seed 7, 40 pages of 4096 bytes), carrying
the wiki's state from trial to trial as the run did (edit A always landed; edit B landed when prikk's own merge
was confluent), and **refuses to write the fixture if any drawn pair of edits differs from the recorded one**:
the committed file is therefore the texts of the recorded run, not an approximation of them. Each pair also
records what the spike's own block model said (`spike_block_clean`), and the test requires the product
function to agree with it on every pair, not only in total.

In the file, a page of the tip or of the proposal has no `text` when it is the base's text.

source report sha256: 033175e2e1d073f7bd78968260cd4b0bea6b3735766366b6a69f8a94695eca2a
fixture sha256: 29fb28176bd6e9a01ca5d99f7176b823d55ca7c2b2279933b3c8d27effe6e29d
generated: 2026-10-31 (the same bytes on a second run)
