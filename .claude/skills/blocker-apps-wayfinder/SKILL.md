---
name: blocker-apps-wayfinder
description: Use when a blocker-apps wayfinder map or research ticket.
category: wayfinder
---

Find the live map with `gh issue list --label wayfinder:map --state open`, never from a remembered number. Confirm any chart with the user before creating issues or pushing docs, because devinsba/blocker-apps is public. Where the repo's CLAUDE.md already fixes the placement (for example, which branch research lives on), follow it and report what you did; do not ask "want me to do that?" about it.

## Working with the user

- Ask each decision as a numbered question with lettered options and one stated recommendation. The user answers "Yes" or a letter, so a short reply settles it.
- When the session's ticket is resolved, end with the exact next step: run `/clear` (or open a new chat), then `/wayfinder #MAP`, naming the next ticket if known. Do not leave the user asking how to start a new session.

## Conventions

- Tickets are sub-issues of the map: `gh issue create --parent MAP --label wayfinder:research --title "..." --body "..."`. Put the question under `## Question` and link the plan doc.
- Create the `wayfinder:map`, `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling` and `wayfinder:task` labels first with `gh label create --force`.
- Docs a ticket links to (GLOSSARY.md, docs/adr/, docs/plans/) must be pushed to the branch the links name, or the links break on GitHub.
- Research subagents work in their own worktree, push findings to a `research/` branch with a short topic slug, open no pull request, comment the answer on their ticket, and close it. They never edit the map. The parent agent updates the map's Decisions so far.
- Research findings never sit in a `research/` folder on the working branch. Each topic gets its own unmerged `research/<topic>` branch holding one `research/<topic>.md`, and the repo's CLAUDE.md Research section lists them. When a finding contradicts an existing repo doc, correct that doc in the same run and repoint its links at the branch, rather than leaving the stale claim beside the new one.
- Hands-on tickets need a person on a real Mac with a signed build. Do not assign them to a subagent or close them from research.
- Before a hands-on result goes on a ticket or the map, the test starts from a clean state: private windows, no cookies or profile state left from earlier runs, and the conditions written next to the result. A result from a dirty state is retested. A comment already posted from it is corrected on the ticket and on every map line that cites it.
- Blocking edges: `gh issue edit TICKET --add-blocked-by BLOCKER`, then verify with `gh issue view TICKET --json blockedBy`. See the wayfinder skill's verification rule.
- Recording a research handback on the map (parent agent only): `gh issue view MAP --json body --jq .body > FILE`, insert one line under `## Decisions so far`, then `gh issue edit MAP --body-file FILE` and re-read that section to verify. Have the script fail when the anchor heading is missing; a silent `str.replace` no-op leaves the map unchanged with no error.
- A research gist keeps the subagent's caveats. Anything it could only read from docs and not test is written as "untested until <hands-on ticket>", naming that ticket. Never state an undocumented or untested behaviour as fact on the map.
- If a handback says a plan item needs revisiting, add that item to `## Not yet specified` (or open a ticket) in the same map update. Do not leave it only in chat.
- When a ticket resolves or a result changes, search the map and every open ticket for lines that repeat the superseded claim, and fix each one in the same run. Re-read the map's Decisions and plan-update list after the edit.
- Plan and decision docs for this effort live in docs/plans/ and docs/adr/, and the vocabulary in GLOSSARY.md.
- Do not commit a GLOSSARY.md or plan-doc edit as soon as one question is answered. Re-read it against every decision made in the session, because a later answer can make it wrong. Commit after the last related decision is in.