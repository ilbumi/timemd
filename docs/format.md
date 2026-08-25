# The file format

This is the specification, not a description of an implementation detail. The
markdown tree **is** the database — the web app, the CLI and the MCP server are
three views onto these files, and editing them by hand is a supported, first-class
way to use timemd.

## Guarantees

1. **Reads are lenient.** A line the app cannot parse is preserved verbatim,
   moved to the end of its section, and reported — never dropped, and never a
   reason to fail the whole file. One typo costs you one misplaced line.
2. **The app rewrites only what it understands.** Unrecognised `##` sections,
   prose, and unknown frontmatter keys survive a write untouched. Add
   `## Retrospective` to a day file and it will still be there tomorrow.
3. **Derived values are display-only.** The `(25m)` on a session line is
   recomputed from the times on every write and ignored on read. If a hand-edit
   makes them disagree, the times win.
4. **Writes are atomic.** Every file is written to a temporary and renamed, so a
   concurrent reader sees the old file or the new one, never a half-written one.
5. **Times are local wall-clock.** No offsets appear anywhere. The timezone lives
   once, in `settings.md`.

A caveat on guarantee 2: unknown frontmatter *keys and values* survive, but YAML
formatting may be normalised (quoting, indentation). Body content is preserved
byte-for-byte.

## Layout

```
data/
  projects/<slug>.md              one file per project
  days/YYYY/YYYY-MM-DD.md         tracked time, planned blocks, free notes
  schedule/recurring.md           weekly-repeating blocks
  todos.md                        the todo list, in Obsidian Tasks format
  settings.md                     pomodoro lengths, timezone, reminder default
  state/active.md                 the running timer
  state/reminders.md              reminders already sent
  state/push.md                   VAPID key and subscriptions — mode 0600, secret
  state/ntfy.md                   ntfy server, topic and token — mode 0600, secret
```

`state/` holds machine state and the files that carry credentials. Gitignore it.
Everything else is meant to be read, diffed and committed.

## Shared conventions

Every structured line is a markdown list item built from the same pieces:

| Piece | Form | Notes |
|---|---|---|
| Time | `HH:MM` | Zero-padded, 24-hour. `9:00` is rejected. |
| Range | `HH:MM-HH:MM` | End earlier than start means it crossed midnight. |
| Duration | `25m`, `1h`, `1h30m` | Whole minutes. An unlabelled `90` is rejected. |
| Project | `[[slug]]` | Slug is `[a-z0-9]` and dashes, no leading or trailing dash. |
| Reminder | `!5m` | Lead time, at the end of the line. `!0m` disables. |
| Block id | `` `deep-work` `` | Backtick-quoted. |
| Checkbox | `[x]` or `[ ]` | Opens a milestone or todo line. `[X]` is read, `[x]` written. |
| Date | `YYYY-MM-DD` | Zero-padded. `2026-8-1` is rejected. |

**One `##` section, one line grammar.** A parser never has to guess what a line
inside a section is meant to be.

## Day files — `days/YYYY/YYYY-MM-DD.md`

```markdown
---
date: 2026-08-01
---

# 2026-08-01

## Schedule

- 16:00-17:00 [[reading]] Paper club !15m

## Skipped

- `deep-work`

## Sessions

- 09:00-09:25 (25m) [[timemd]] file store layer
- 10:30-10:45 (15m) email

## Notes

Free-form. Yours.
```

- `## Schedule` — one-off blocks, this day only:
  `- HH:MM-HH:MM [[project]] Title !5m`, project and reminder optional.
- `## Skipped` — repeating blocks suppressed today: `` - `block-id` ``.
- `## Sessions` — tracked time:
  `- HH:MM-HH:MM (duration) [[project]] note`, project and note optional.
  Sorted by start time on write. Only focus time is logged; breaks are timer
  state and never appear.
- `## Notes` and anything else — untouched.

There is one kind of session. A pomodoro and a hand-entered meeting are the same
thing.

### Two ambiguities worth knowing

A note that *opens* with `[[something]]` is read as a project link. A hand-written
line whose note opens with a duration-shaped `(1h)` has that group read as the
display duration and dropped. Both are consequences of a grammar with no escaping,
and both cost you the token once — after the app rewrites the line, the canonical
form round-trips exactly.

## Recurring schedule — `schedule/recurring.md`

```markdown
---
---

# Recurring schedule

## Blocks

- `deep-work` mon-fri 09:00-11:00 [[timemd]] Deep work !5m
- `review` wed 14:00-15:00 [[admin]] Weekly review !10m
```

Day specs accept `daily`, single days (`wed`), ranges (`mon-fri`), and
comma-separated combinations (`mon-fri,sun`). They render canonically: runs of
three or more collapse to a range, so `mon-fri` survives a rewrite rather than
expanding into a list.

## Projects — `projects/<slug>.md`

```markdown
---
name: Thesis
color: '#245a8d'
mark: square
target: 10h
status: active
created: 2026-08-01
---

# Thesis

Free-form project notes.

## Milestones

- [x] Ch. 1 — lit review
- [ ] Ch. 4 — first draft
```

The filename stem is the canonical identity; `name` is only for display. `status`
is `active` or `archived`. Any value that is missing or unreadable falls back to a
default rather than failing the file.

`mark` is the shape the project is drawn as — `square`, `circle`, `triangle`,
`diamond` or `bar`, defaulting to `square`. It carries the project's identity
alongside `color`, so two projects stay distinguishable at a glance and in
greyscale. `target` is how many hours a week you mean to spend on it, in the usual
duration form, and is absent when there is no target.

`## Milestones` is a list section like `## Sessions`: one `- [x] Title` or
`- [ ] Title` per line, in whatever order you keep them. A line the app cannot
read is preserved and reported, as everywhere else. The app writes a title back
exactly as given, so it must be non-empty and on one line — anything else is
refused at the point of writing rather than silently mangled.

## Todos — `todos.md`

```markdown
---
---

## Todos

- [ ] [[timemd]] Draft the release notes ⏫ 🆔 dcf64c ➕ 2026-08-24 ⏳ 2026-08-30 14:00 📅 2026-08-31 #writing
- [x] [[timemd]] Fix the ticker drift 🔺 🆔 0h17ye ⛔ dcf64c ✅ 2026-08-23
- [-] Rewrite the CSS 🆔 8kq2mv ❌ 2026-08-20
- [ ] Water the plants 🔁 every day when done ⏳ 2026-08-25
```

One global file rather than a section per project, because a todo outlives the
project it belongs to and many belong to no project at all. Milestones are
something else and stay where they are: a project's spine, short and ordered.

The line grammar is [Obsidian Tasks' emoji format][tasks], so the same file is
editable by hand, by an agent, and by Obsidian. After the checkbox comes an
optional `[[project]]`, then the description, then the fields:

| Signifier | Field | Value |
|---|---|---|
| `🔺 ⏫ 🔼 🔽 ⏬` | Priority | Highest, high, medium, low, lowest. No signifier means normal. |
| `➕` | Created | A date |
| `🛫` | Start | A date |
| `⏳` | Scheduled | A date |
| `📅` | Due | A date |
| `✅` | Done | A date |
| `❌` | Cancelled | A date |
| `🔁` | Recurrence | A rule, kept verbatim — see below |
| `🆔` | Id | Letters, digits, dashes and underscores |
| `⛔` | Depends on | Comma-separated ids |
| `🏁` | On completion | `keep` or `delete`, kept for Obsidian's benefit |

A `#tag` is description text, not a field: it stays where you put it.

**A date may be narrowed to a time**, as `⏳ 2026-08-30 14:00`. This is the one
deliberate departure from Obsidian, because a scheduled todo has to be able to
mean a slot on the day timeline and not just a day. A date with no time is
written back exactly as Obsidian writes it, so a file that never uses one stays
byte-identical.

**Reads accept the fields in any order; writes emit one order** — description,
priority, `🔁`, `🆔`, `⛔`, `➕`, `🛫`, `⏳`, `📅`, `❌`, `✅`, `🏁`. That is what
makes a second write a no-op, and an idempotent write is what lets Obsidian and
timemd both hold the file open.

The checkbox is `[ ]` open, `[x]` done, `[-]` cancelled. Any other single
character — Obsidian lets you define your own — is kept as typed and counts as
not yet done.

**A todo is addressed by its id.** A milestone is addressed by its title
because a project has a handful in a deliberate order; a todo list has hundreds
in none, and the same words come round again next week. Every todo the app
creates gets an id, and any todo the app *writes* that has none is given one.
A hand-written todo is left exactly as typed until something edits that file —
at which point it becomes addressable, and `⛔` can name it.

**Recurrence is preserved, not executed.** `🔁 every day when done` survives a
round trip untouched, and ticking a recurring todo here does not spawn the next
one. Obsidian already does that.

A description is refused at the point of writing if the app could not read it
back: blank, spanning lines, containing a signifier, or opening with `[[` —
which would be swallowed as the project link.

[tasks]: https://publish.obsidian.md/tasks/Reference/Task+Formats/Tasks+Emoji+Format

## Settings — `settings.md`

```markdown
---
timezone: Europe/Berlin
focus: 25m
short_break: 5m
long_break: 15m
long_break_every: 4
remind_before: 5m
---
```

`timezone` is an IANA name and defaults to the host's. Everything else falls back
to the values above.

## The running timer — `state/active.md`

```markdown
---
started: 2026-08-01T09:00:00
kind: focus
duration: 25m
project: timemd
note: file store layer
---

# Active session
```

Deliberately readable: "what is the user working on right now" is answerable by
reading one small file, with no server running. A file without a `started` key
means idle.

A break carries the `project` and `note` of the block it is a break *from*, so
the same block can be offered again when it ends. That never makes it loggable —
only `kind: focus` becomes a session line.

## ntfy — `state/ntfy.md`

```markdown
---
server: https://ntfy.sh
topic: timemd-a7f3c9e1
token: tk_…
app_url: https://box.tailnet.ts.net
---
```

The second notification channel, for a phone that a browser will not wake.
`topic` is the only key that has to be there: without one the channel is off.
`server` defaults to `https://ntfy.sh`, and `token` and `app_url` are both
optional — the first for an access-controlled topic, the second to make a
notification tappable, since the server cannot work out its own external
address.

**A topic on a public server is a bearer capability.** Anyone who knows the name
can subscribe and read every notification it carries. Pick one nobody would
guess, or run a server with access control and set `token`. This is why the file
is written mode 0600 and why `state/` is gitignored.

`server` and `topic` are apart rather than one URL because ntfy accepts a JSON
body only at the server root; publishing to `/{topic}` would put the title in a
header, which cannot carry a block called `Café admin`.

## Durations across a clock change

Session length is wall-clock arithmetic on the stored times. On the two days a
year the clocks change, a session spanning the change is off by an hour. That is
the price of keeping offsets out of the files, and it is tested rather than
accidental.
