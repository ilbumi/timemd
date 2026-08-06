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

[`docs/surfaces.md`](docs/surfaces.md) says which operations the API, the MCP
tools, the command and the web app each offer, and how each thing is addressed.

## What it does

- **Projects** — each with a shape, a colour, a weekly hour target and a
  milestone list. Create, edit, archive, delete.
- **Pomodoro timer** — server-authoritative, so a session completes, gets logged
  and notifies even while your phone is asleep. Assign a project and a note.
- **Schedule** — weekly-repeating blocks plus one-offs, with per-day skips.
- **Reminders** — before a block starts, when a session completes, and when a
  break is over, over web push or ntfy.
- **Log** — every session with its note, banded by day, with the week's tracked
  total read against what the schedule set aside for it.
- **CLI** — `timemd start`, `stop`, `today`, `log`, `report` for shell use.
- **MCP server** — 27 tools so Claude and other agents get first-class tooling.

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
On anything older, build from source. There is deliberately nothing on
crates.io: this is one binary and a directory of markdown.

That same binary is also published as a container image, for `linux/amd64` and
`linux/arm64`, tagged with the release version and with `latest`:

```sh
mkdir -p data

docker run -d --name timemd \
  -p 127.0.0.1:8080:8080 \
  -v "$PWD/data:/data" \
  ghcr.io/ilbumi/timemd:latest
```

It runs as uid 65532 and never as root. On a Linux host that means the mounted
directory has to be writable by it — `sudo chown 65532:65532 data` once, before
the first run. Docker Desktop maps ownership for you and needs no such thing.

The image holds the binary and nothing else — no shell, no package manager — so
it is the file-copy deploy in a different wrapper, not a different program. Every
subcommand works: `docker run --rm -v "$PWD/data:/data" ghcr.io/ilbumi/timemd today`.

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

The container inherits the rule, which is why the `docker run` above publishes to
`127.0.0.1` — a bare `-p 8080:8080` reaches past the host firewall on most Docker
installs.

### Notifications on iOS

iOS delivers web push only to apps added to the Home Screen. Open the app, tap
Share → **Add to Home Screen**, open it from there, then turn notifications on in
Settings. Skipping the install step leaves notifications silently doing nothing;
the Settings screen detects this and says so rather than pretending to work.

### Notifications over ntfy

Web push on a phone depends on the browser being willing to wake a service
worker, which iOS treats as optional. [ntfy](https://ntfy.sh) does not: it is a
push app with a topic you subscribe to. Both channels run independently, so you
can use either or both.

Install the ntfy app, then pick a topic nobody would guess and tell timemd about
it:

```sh
timemd ntfy --topic "timemd-$(LC_ALL=C tr -dc 'a-z0-9' </dev/urandom | head -c 12)"
# prints the URL to subscribe to in the app
```

Or type it into Settings, which sends a test notification when you save.

**A topic on the public server is a bearer capability** — anyone who knows the
name can read your notifications. Pick an unguessable one, or run your own ntfy
with access control and set `--token`. Add `--app-url https://<your-host>` to
make a notification tappable; the server cannot work out its own external
address, so without it there is nothing to open.

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
| Everything else | `report`, `settings`, `ntfy` |

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
