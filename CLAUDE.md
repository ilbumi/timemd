# timemd

A time tracker whose database is a tree of markdown files. Read
[`docs/format.md`](docs/format.md) before touching anything that reads or writes
the tree — it is the specification, and the file grammar is a public interface
that users and agents edit by hand.

## The rules that shape this codebase

- **Reads are lenient, writes are strict.** An unparseable line is preserved
  verbatim and reported, never dropped and never fatal. If you add a parser,
  follow this.
- **The app owns only what it understands.** Unknown `##` sections and unknown
  frontmatter keys must survive a write. `Document` enforces this; go through it.
- **No cache, no file watcher.** Every read hits disk. At single-user scale that
  costs microseconds and means an agent's edit is visible on the next request.
  Do not add an index without a measured reason.
- **The server owns the clock, not the phone.** A session must complete, log and
  notify while the tab is suspended. Anything that depends on the client being
  awake is wrong.
- **One binary.** `serve`, `mcp` and the shell commands all ship in `timemd`.

## Layout

| Path | What |
|---|---|
| `crates/core` | Domain types, the file grammar, the store, timer, reports |
| `crates/server` | HTTP API, reminder ticker, push, embedded UI |
| `crates/mcp` | MCP tools for agents |
| `crates/cli` | The `timemd` binary; `main.rs` is a shim, logic is in `lib.rs` |
| `frontend/` | SvelteKit, built into `crates/server/assets` |

## Working here

```sh
make test lint cov     # all three must pass
make e2e               # layout in a real browser; needs a downloaded Chromium
make serve             # build the UI and run locally
```

- Store writes go through `Store::transaction`; the write methods live on `Tx`
  so taking the lock twice is not expressible.
- Handlers define their own `*View` types. Core stays free of HTTP naming.
- Tests: property tests pin the grammar's round-trip and preservation
  guarantees. If you change the grammar, they should fail — read them first.
- The design language is enforced by `make e2e`, not by eye: one pair of edges
  per screen, one rule where two meet, no radius that is not a circle, 44px of
  reach under a thumb. If you change the sheet, run it. It seeds its own tree
  and never reads `./data`.
- Coverage floors are >85% overall and >80% per file. `crates/cli/src/main.rs`
  is excluded as a process shim; the reason is recorded in the `Makefile`.
- Never log to stdout: `timemd mcp` speaks JSON-RPC there.
- CI runs these same `make` targets, split into `-rust` and `-web` halves. A
  change to what a gate *checks* belongs in the `Makefile`; a workflow may only
  supply toolchains, caches and artifacts.
- The `rust` CI job installs no Node. `cargo test` and `cargo clippy` must keep
  working on a clone where the UI has never been built — that is what
  `#[allow_missing]` on `Assets` buys, so do not undo it.
- Commit subjects pick the version and write the changelog: `feat:` → minor,
  `fix:`/`perf:` → patch, `!` or a `BREAKING CHANGE:` footer → minor while below
  1.0, everything else → no release. Releases are cut by merging the
  `chore(main): release X.Y.Z` pull request; nothing is tagged by hand.
- release-please edits `Cargo.toml` through the GitHub API and cannot run cargo,
  so `release.yml` pushes the matching `Cargo.lock` onto the release branch. If
  you change how the version is stored, that step has to follow.
