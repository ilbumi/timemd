# Surfaces

timemd exposes one domain over four surfaces: the HTTP API, the MCP tools, the
`timemd` command, and the web UI. They are meant to stay level with each other —
`crates/server/src/lib.rs` puts it as "kept working in four places at once" — and
the only way to see that they have is to have it written down.

This table is the checklist. If you add an operation, add a row; if a row grows a
gap, that is a bug report, not a footnote.

## Addressing

Almost nothing in the tree has a synthetic id, so each thing is addressed by what
it actually has:

| Thing | Handle | Why |
|---|---|---|
| Project | `slug` | The filename. Not editable; renaming means a new file. |
| Milestone | its **title** | No id, and `docs/format.md` keeps it that way. An index is a position another writer invalidates between two calls; a title is what the agent, the file and the user all already see. |
| Session | index in the day | It has no name. |
| One-off block | index among that day's one-offs | Likewise. Not its position in the merged list. |
| Repeating block | `BlockId` | It has one, and skips already point at it. |
| Todo | `TodoId`, written on the line as `🆔 dcf64c` | The one synthetic id, and it earns it: a todo list has hundreds of entries in no order, the same words come round again next week, and `⛔ dependsOn` needs something to name. Every todo the app writes is given one. |

Sessions and one-off blocks are kept in start order, so **changing a start time
renumbers the day**. Every surface copes the same way: whoever writes one answers
with, or re-reads, the whole day. Never reuse an index across a write.

Milestone titles are unique per project on the way *in* — writes are strict — but
a hand-written duplicate still parses and lists, because reads are lenient. It
just cannot be addressed until one of them is renamed. The same bargain holds for
a todo carrying an id another one already has, and for a hand-written todo with
no id at all: it lists, and becomes addressable the moment anything writes
`todos.md`.

## What each surface can do

✓ available · — not applicable

| Operation | HTTP | MCP | CLI | Web |
|---|---|---|---|---|
| **Timer** |
| Start | `POST /api/timer/start` | `start_session` (focus) | `start` (focus) | ✓ |
| Stop and log | `POST /api/timer/stop` ⁷ | `stop_session` | `stop` | ✓ |
| Discard | `POST /api/timer/cancel` | `cancel_session` | `cancel` | ✓ |
| What is running | `GET /api/timer` | `current_session` | `status` | ✓ |
| **Logged time** |
| Read a day | `GET /api/days/{date}` | `day` | `today` | ✓ |
| Log by hand | `POST …/sessions` | `log_time` | `log` | ✓ |
| Amend | `PATCH …/sessions/{i}` | `edit_session` | `session edit` | ✓ |
| Delete | `DELETE …/sessions/{i}` | `delete_session` | `session rm` | ✓ |
| **Projects** |
| List | `GET /api/projects` | `list_projects` | `projects` | ✓ |
| Read one | `GET /api/projects/{slug}` | `project` | `project show` | ✓ |
| Create | `POST /api/projects` | `upsert_project` | `project new` | ✓ |
| Change | `PATCH /api/projects/{slug}` | `upsert_project` | `project set` | ✓ |
| Delete | `DELETE /api/projects/{slug}` | `delete_project` | `project rm --force` | ✓ |
| **Milestones** |
| Add | whole-list `PATCH` | `add_milestone` | `milestone add` | ✓ |
| Tick / untick | whole-list `PATCH` | `update_milestone` | `milestone set --done` | ✓ |
| Retitle | whole-list `PATCH` | `update_milestone` | `milestone set --rename` | ✓ |
| Reorder | whole-list `PATCH` | `update_milestone` | `milestone set --position` | ✓ |
| Remove | whole-list `PATCH` | `remove_milestone` | `milestone rm` | ✓ |
| **Todos** |
| List | `GET /api/todos` | `list_todos` | `todos` | ✓ |
| Read one | `GET /api/todos/{id}` | `list_todos` | `todos` | ✓ |
| Add | `POST /api/todos` | `add_todo` | `todo add` | ✓ |
| Tick / untick | `PATCH /api/todos/{id}` | `update_todo` | `todo set --done` | ✓ |
| Rewrite | `PATCH /api/todos/{id}` | `update_todo` | `todo set --rename` | ✓ |
| Date or re-prioritise | `PATCH /api/todos/{id}` | `update_todo` | `todo set --due …` | ✓ |
| Remove | `DELETE /api/todos/{id}` | `remove_todo` | `todo rm` | ✓ |
| Start a session on one | `POST /api/timer/start` | `start_session` | `start --todo` | ✓ |
| See today's | `GET /api/days/{date}` | `list_todos` | `todos --scheduled-on` | ✓ ⁶ |
| **Schedule** |
| Read a range | `GET /api/schedule` | `schedule` | `schedule` | ✓ |
| Read the pattern | `GET …/recurring` | `recurring` | `repeat list` | ✓ |
| Write the pattern | `PUT …/recurring` | `set_recurring_block` | `repeat set` | ✓ |
| Delete a repeat | `PUT …/recurring` | `remove_recurring_block` | `repeat rm` | ✓ |
| Plan a one-off | `POST …/blocks` | `add_block` | `block add` | ✓ |
| Amend a one-off | `PATCH …/blocks/{i}` | `edit_block` | `block edit` | ✓ |
| Remove a one-off | `DELETE …/blocks/{i}` | `remove_block` | `block rm` | ✓ |
| Skip a repeat | `POST …/skips` | `skip_block` | `repeat skip` | ✓ |
| Restore a skip | `DELETE …/skips/{id}` | `unskip_block` | `repeat restore` | ✓ |
| **Everything else** |
| Report | `GET /api/reports` | `report` | `report` | partial ¹ |
| Read settings | `GET /api/settings` | `settings` | `settings` | ✓ |
| Write settings | `PUT /api/settings` | `settings` | `settings --focus …` | partial ² |
| Push subscription | `/api/push/*` | — ³ | — ³ | ✓ |
| Read ntfy config ⁴ | `GET /api/ntfy` | `ntfy` | `ntfy` | ✓ |
| Write ntfy config | `PUT /api/ntfy` ⁵ | `ntfy` | `ntfy --topic …` | ✓ |

¹ The web app spends reports on the weekly target bars; there is no report screen,
so `groupBy=day` has no caller there.

² The web app steps the three durations. `remindBefore` is shown as prose and set
elsewhere; `timezone` and `longBreakEvery` are read-only on every surface, because
the timezone is what turns every bare wall-clock time in the tree into an instant.
Both are changed by editing `settings.md`.

³ Push belongs to a browser that has a service worker. An agent and a shell have
nothing to subscribe. ntfy is the opposite case: a topic is a value anyone can
type, so it is on all four. Where the file sits and which surfaces write it are
different questions — everything goes through `Store`.

⁴ The token is never read back. Every surface answers with whether one is set,
not with what it is; the file at mode 0600 is the only copy.

⁵ Writing over HTTP also sends one test notification, and only when the write
actually moved the topic, the server or the app URL — compared before and after,
not read off the request, because the settings screen sends all three every time
it saves. ntfy answers 200 for any topic name, so a typo is indistinguishable
from success at the transport, and the write is the one moment somebody is
looking at a screen and can be told. It catches a wrong server or a wrong token,
never a wrong topic — the message says so. The CLI and MCP write without one:
both are synchronous by design and hold no HTTP client.

⁶ Read-only on the day screen: the plan and the list are one picture there, but a
todo is edited where it lives.

⁷ Whole minutes only: a focus session that rounds to zero is not written. CLI and
MCP say so in the result; HTTP names it `stopped: "tooShort"`; the web shows the
same rather than returning to idle as if 0m had been logged.

## Two deliberate asymmetries

**HTTP replaces a list where MCP and the CLI address one element.** Milestones and
the repeating pattern both work this way. The reason is that the web app is the
only client that already holds the whole list, so for it one field beats three
endpoints; an agent does not hold it, and a read-then-write across two calls
silently clobbers whatever changed in between. Same domain rule, two shapes,
because the callers genuinely differ.

Todos are the exception, and they show what the rule is really about: a todo has
an id, so the web app can name the one row it changed. Nothing has to hold the
whole list, and nothing clobbers a neighbour — so all four surfaces address one
element and the asymmetry disappears.

**Archived projects are read-only in the web app only.** The API, MCP and the CLI
will all still change one. The web app hides the controls because archiving is
meant to get a project out of the way, not to lock it.
