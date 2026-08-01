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

## Running it

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

Tools: `start_session`, `stop_session`, `current_session`, `log_time`, `day`,
`schedule`, `report`, `list_projects`, `upsert_project`.

Agents can equally just edit the files. Anything the app does not understand —
your own `##` sections, prose, extra frontmatter keys — survives untouched, and a
line it cannot parse is preserved and reported rather than dropped.

## Development

```sh
make test    # Rust and frontend suites
make lint    # clippy -D warnings, rustfmt, svelte-check, prettier
make cov     # coverage, failing under 85%
make help
```

Layout:

```
crates/core     domain types and the markdown store
crates/server   HTTP API, reminder ticker, embedded web UI
crates/mcp      Model Context Protocol server
crates/cli      the `timemd` binary
frontend/       SvelteKit, built into crates/server/assets
```
