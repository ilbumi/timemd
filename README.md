# timemd

A phone-first time tracker whose database is a tree of markdown files.

The point is the files. Projects, tracked time and your schedule are plain
markdown you can read, grep, commit and hand-edit — and so can an agent. The web
app, the CLI and the MCP server are three views onto the same tree; whichever one
writes, the others see it on their next read.

```
data/days/2026/2026-08-01.md

## Sessions

- 09:00-09:25 (25m) [[timemd]] file store layer
- 10:30-10:45 (15m) email
```

[`docs/format.md`](docs/format.md) is the specification. It is worth reading
before pointing an agent at the tree.

## What it does

- **Projects** — each with a shape, a colour, a weekly hour target and a
  milestone list. Create, edit, archive, delete.
- **Pomodoro timer** — server-authoritative, so a session completes, gets logged
  and notifies even while your phone is asleep. Assign a project and a note.
- **Schedule** — weekly-repeating blocks plus one-offs, with per-day skips.
- **Reminders** — web push before a block starts, and when a session completes.
- **Log** — every session with its note, banded by day, with weekly totals.
- **CLI** — `timemd start`, `stop`, `today`, `log`, `report` for shell use.
- **MCP server** — nine tools so Claude and other agents get first-class tooling.

### The app is three screens

A project is drawn as a shape as well as a colour, so the running timer can be
read at a glance, and the tab bar is those same shapes:

| Tab | Screen |
|---|---|
| ● | **Timer** — pick a project, run a session, log what got done and tick a milestone |
| ■ | **Projects** — targets and milestones; archived projects collapse into a footer |
| ▲ | **Schedule** — `Day` timeline · `Week` raster · `Log`, with the repeating pattern behind the week |

Settings hangs off the timer's header. Nothing is fetched at runtime — the font
is bundled — so the installed app works with no signal.

It is phone-first but not phone-only: from 900px the tab bar becomes a sidebar
and the screens use the width — the project detail puts its identity panel beside
the milestones, the day timeline sits next to its block list, and the week raster
simply gets bigger. Same rules, same palette, more room.

## Installing it

Every release ships one self-contained binary per platform, with the web UI
compiled in. Pick your target from `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` or `x86_64-apple-darwin`:

```sh
TARGET=aarch64-apple-darwin
BASE=https://github.com/ilbumi/timemd/releases/latest/download

curl -LO "$BASE/timemd-$TARGET.tar.gz"
curl -LO "$BASE/timemd-$TARGET.tar.gz.sha256"
shasum -a 256 -c "timemd-$TARGET.tar.gz.sha256"

tar xzf "timemd-$TARGET.tar.gz"
./timemd-$TARGET/timemd --version
```

The Linux builds need glibc 2.39 or newer — Ubuntu 24.04, Debian 13, Fedora 40.
On anything older, build from source. There is deliberately no Docker image and
nothing on crates.io: this is one binary and a directory of markdown.

## Running it

From a release, that is `./timemd --data ./data serve`. From source:

```sh
make frontend          # build the web UI into the server crate
cargo build --release  # one binary, UI embedded
./target/release/timemd --data ./data serve
```

Or for development, with the Vite dev server proxying to a running `make serve`:

```sh
make serve   # terminal one
make dev     # terminal two
```

| Variable | Default | Meaning |
|---|---|---|
| `TIMEMD_DATA` | `./data` | Root of the markdown tree |
| `TIMEMD_ADDR` | `0.0.0.0:8080` | Bind address |
| `TIMEMD_LOG` | `info` | Log filter |

### There is no authentication

This is deliberate, and it means **the port must not be exposed to the internet**.
Anyone who can reach it can read and rewrite your data. Run it on a tailnet or a
home LAN and reach it that way:

```sh
tailscale serve --bg 8080     # https://<machine>.<tailnet>.ts.net
```

Tailscale terminates TLS, which push notifications need anyway — browsers refuse
to register a service worker over plain HTTP on anything but `localhost`.

### Notifications on iOS

iOS delivers web push only to apps added to the Home Screen. Open the app, tap
Share → **Add to Home Screen**, open it from there, then turn notifications on in
Settings. Skipping the install step leaves notifications silently doing nothing;
the Settings screen detects this and says so rather than pretending to work.

## Using it with agents

Point an agent at the MCP server:

```json
{
  "mcpServers": {
    "timemd": {
      "command": "/path/to/timemd",
      "args": ["--data", "/path/to/data", "mcp"]
    }
  }
}
```

Tools, by what they touch:

| | |
|---|---|
| Timer | `start_session`, `stop_session`, `cancel_session`, `current_session` |
| Logged time | `log_time`, `edit_session`, `delete_session`, `day` |
| Projects | `list_projects`, `project`, `upsert_project`, `delete_project` |
| Milestones | `add_milestone`, `update_milestone`, `remove_milestone` |
| Schedule | `schedule`, `recurring`, `set_recurring_block`, `remove_recurring_block`, `add_block`, `edit_block`, `remove_block`, `skip_block`, `unskip_block` |
| Everything else | `report`, `settings` |

Sessions and one-off blocks are addressed by index, milestones by title, and
repeating blocks by id. Every tool that writes a session or a block answers with
the whole day, renumbered, because both lists re-sort on write.

Agents can equally just edit the files. Anything the app does not understand —
your own `##` sections, prose, extra frontmatter keys — survives untouched, and a
line it cannot parse is preserved and reported rather than dropped.

## Development

```sh
make test    # Rust and frontend suites
make lint    # clippy -D warnings, rustfmt, svelte-check, prettier
make cov     # coverage, failing under 85%
make e2e     # alignment and adaptive layout in a real browser
make help
```

`make e2e` is separate because it downloads a Chromium and compiles the server.
It checks the things the design language promises — that a screen's rules end
where its content ends, that two rules meeting draw one line, that nothing is
rounded that is not a circle — at five widths either side of the breakpoints. It
seeds its own markdown tree and never touches your `./data`.

Layout:

```
crates/core     domain types and the markdown store
crates/server   HTTP API, reminder ticker, embedded web UI
crates/mcp      Model Context Protocol server
crates/cli      the `timemd` binary
frontend/       SvelteKit, built into crates/server/assets
```

### Releasing

Releases are cut by hand, from the Actions tab: run the **release** workflow and
pick a bump, or leave it on `auto` to take the version from the commits. It sets
the version, writes the changelog, tags, builds the four binaries and publishes
the release. Nothing releases on its own.

Commit subjects are what `auto` reads: `feat:` bumps the minor, `fix:` and
`perf:` the patch. While the version is below 1.0 a `!` or a `BREAKING CHANGE:`
footer also bumps the minor rather than the major, so nothing reaches 1.0 by
accident. Every rule lives in [`cliff.toml`](cliff.toml), and

```sh
git cliff --bumped-version    # what would the next release be?
git cliff --unreleased        # what would its notes say?
```

gives the same answer locally that the workflow will. The workflow also takes a
`dry_run` input if you would rather ask it directly.

CI runs the same `make` targets, so a change to a gate belongs in the Makefile
rather than in a workflow. The `rust` job installs no Node — `cargo test` and
`cargo clippy` must keep working on a clone where the UI has never been built.
