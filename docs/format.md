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
  settings.md                     pomodoro lengths, timezone, reminder default
  state/active.md                 the running timer
  state/reminders.md              reminders already sent
  state/push.md                   VAPID key and subscriptions — mode 0600, secret
```

`state/` holds machine state and one private key. Gitignore it. Everything else
is meant to be read, diffed and committed.

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
name: timemd
color: '#4f46e5'
status: active
created: 2026-08-01
---

# timemd

Free-form project notes.
```

The filename stem is the canonical identity; `name` is only for display. `status`
is `active` or `archived`. Any value that is missing or unreadable falls back to a
default rather than failing the file.

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

## Durations across a clock change

Session length is wall-clock arithmetic on the stored times. On the two days a
year the clocks change, a session spanning the change is off by an hour. That is
the price of keeping offsets out of the files, and it is tested rather than
accidental.
