# Run ledger

One JSON file per campaign run, written by `campaign_main.record_run` and kept
here rather than under `target/`.

A run's own artifacts — programs, oracles, reduced reproducers, the whole
finding directory a defect node cites — live under `target/generative-campaign`,
which an ordinary `cargo clean` deletes. It did, on 2026-09-15, taking the
2026-09-14 full run with it; the next run reported ninety-two findings against
the previous thirty-seven and there was no way to say which were new, because
the baseline no longer existed.

The bulk stays disposable, since the campaign is seeded and a run is
reproducible in principle. What does not survive regeneration cheaply is the
*identity* of a run: which findings it had, at which revision, with which
coverage obligations outstanding. That is a few kilobytes and it is what a
comparison between two runs actually needs, so it is version-controlled.

Each entry carries the profile and whether it passed, the revision and whether
the tree was dirty, the per-class counts, the fault kinds, the outstanding
coverage obligations, and one line per finding: item, run class, verdict and
program digest. It deliberately records no path into `target`, since a
reference to a directory `cargo clean` will remove is the problem rather than
the record of it.

To compare two runs, diff their `findings` lists by item.
