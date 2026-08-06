# PSEUDO-JARVIS

**Ambient Playbooks — a local-first assistant that learns how you work, then runs it for you.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Status: Design](https://img.shields.io/badge/Status-Design-yellow.svg)]()

> *"Do the Friday thing."*  
> One sentence. Your whole workflow runs — locally, audibly, reversibly.

---

## The problem

Every AI assistant today is **reactive**. You open a chat, explain context, paste files, and hope it remembers next week. Meanwhile, you repeat the same workflows dozens of times:

- Export a report → rename files → update a spreadsheet → email your accountant  
- Pull git changes → run tests → bump version → draft release notes  
- Download invoices → categorize → upload to your tracker  

You are your own intern. The tools exist to *talk* and to *run code on demand*, but nothing **learns your rituals** and **executes them with trust and proof**.

## The idea

**PSEUDO-JARVIS** is an open-source, local-first **Ambient Chief-of-Staff**. With your explicit consent, it:

1. **Observes** repetitive patterns across your terminal, files, calendar, and (optionally) screen  
2. **Extracts** them into named **Playbooks** — reproducible workflows you own  
3. **Executes** them on voice, CLI, or messaging — with a **Trust Ladder** that starts read-only  
4. **Proves** every action in **Replay Theater** — a git-style audit trail + visual replay  

No cloud. No telemetry. No black box. Your machine, your rules, your playbooks.

## The 30-second demo

```
Week 1–3: You do "monthly close" manually — same 14 steps, every time.

Week 4:   "Hey JARVIS, do the monthly close."

          → Playbook runs locally
          → Replay Theater shows every file touched, command run, message drafted
          → You approve the final email, or let trusted steps auto-run

That's it. That's the product.
```

## How this is different

| Tool | Talks | Acts | Learns your workflows | Local & auditable |
|------|:-----:|:----:|:---------------------:|:-----------------:|
| ChatGPT / Open WebUI | ✅ | ❌ | ❌ | Partial |
| Open Interpreter | ✅ | ✅ | ❌ | ✅ |
| OpenClaw / Thoth | ✅ | ✅ | ❌ | ✅ |
| Rewind / Recall-style tools | ❌ | ❌ | Records only | Partial |
| **PSEUDO-JARVIS** | ✅ | ✅ | ✅ | ✅ |

We are not building another chat UI. We are building **observation → playbook → execution → proof**.

## Core concepts

### Observer

An opt-in, local-only watcher that captures *signals*, not surveillance:

- Terminal command history (shell hooks)
- File system events in allowed directories
- Calendar triggers (local calendar APIs)
- Optional: screen OCR regions, browser activity (explicit per-app consent)

Everything is stored in **your** SQLite vault on disk. Kill switch always visible. No upload path exists in the architecture.

### Playbooks

A Playbook is a versioned workflow — like a Makefile for your life:

```yaml
name: monthly-close
version: 3
trigger:
  phrases: ["monthly close", "do the friday thing"]
  schedule: "0 17 * * 5"  # optional cron
steps:
  - id: export-report
    type: shell
    command: "./scripts/export-q4.sh"
  - id: categorize
    type: file-transform
    action: rename-by-pattern
  - id: notify-accountant
    type: draft-email
    template: playbooks/monthly-close/email.md
    trust: confirm  # requires human approval
```

Playbooks can be **auto-suggested** from observed patterns or **authored manually**. Community playbooks ship as shareable templates (no personal data).

### Trust Ladder

Every step has a trust level. The agent earns autonomy over time:

| Level | Behavior | Example |
|-------|----------|---------|
| `observe` | Suggest only, no execution | "You usually run tests here" |
| `confirm` | Show diff, wait for approval | Send email, delete files |
| `auto` | Run without asking | Rename files in `~/Downloads/invoices/` |
| `deny` | Never run | `rm -rf`, wire transfers |

Trust is **per-step, per-playbook, revocable**. Default is paranoid.

### Replay Theater

After every run, you get:

- **Action log** — structured JSON of every step (command, path, timestamp, outcome)  
- **Diff view** — what changed on disk vs. before the run  
- **Optional screen capture** — short replay clip of UI steps (local WebM, never uploaded)  
- **Rollback hints** — git stash, file backups, undo commands where possible  

If you can't explain what it did, it failed as a product.

## Architecture (summary)

```
┌─────────────────────────────────────────────────────────────┐
│                     You (voice / CLI / Telegram)            │
└────────────────────────────┬────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────┐
│  JARVIS Core (Rust) — orchestration, trust, scheduling        │
│  ┌──────────┐  ┌────────────┐  ┌──────────┐  ┌─────────────┐  │
│  │ Observer │  │ Playbook   │  │ Executor │  │ Replay      │  │
│  │ Daemon   │→ │ Engine     │→ │ Runtime  │→ │ Theater     │  │
│  └──────────┘  └────────────┘  └──────────┘  └─────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
   SQLite Vault        Local LLM            Plugin Host
   (events, playbooks) (Ollama)             (Gmail, Jira, …)
```

Full design: **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**

## Tech stack

| Layer | Choice | Why |
|-------|--------|-----|
| Core runtime | **Rust** | Performance, safety, single static binary |
| Local DB | **SQLite** | Portable, auditable, one file to backup |
| LLM | **Ollama** (default) | Local inference, model-agnostic via OpenAI-compatible API |
| Voice | **Whisper** (STT) + **Piper/Kokoro** (TTS) | Fully offline voice trigger |
| Desktop UI | **Tauri 2** | Lightweight tray app, native permissions |
| CLI | `jarvis` | Headless mode, CI-friendly, scriptable |
| Messaging | Telegram / Discord (optional) | Remote trigger without cloud brain |
| Playbook format | **YAML** | Human-readable, git-friendly, diffable |

## Project status

**Phase 0 — Design** (current)

We are defining architecture, trust model, and MVP scope before writing application code. Follow this repo for updates.

### Roadmap

| Phase | Milestone | Target outcome |
|-------|-----------|----------------|
| **0** | Design docs + community feedback | Architecture signed off |
| **1** | MVP — `observe` + manual playbooks | Run 1 playbook from CLI with confirm gate |
| **2** | Pattern suggester | "This looks like a playbook — save it?" |
| **3** | Voice + tray app | "Hey JARVIS, do the Friday thing" |
| **4** | Community playbook registry | Share templates, import safely |
| **5** | Plugins | Gmail, Notion, Jira, Home Assistant |

### MVP scope (Phase 1)

Ship the smallest thing that proves the magic:

- [ ] `jarvis observe` — terminal + file watcher in allowed dirs  
- [ ] `jarvis playbook run <name>` — execute YAML playbooks  
- [ ] Trust ladder with `confirm` default on destructive actions  
- [ ] Replay log written to `~/.jarvis/replays/`  
- [ ] Ollama integration for natural-language trigger → playbook mapping  
- [ ] Zero network calls by default (enforced in CI)

**Explicitly out of MVP:** screen recording, auto-extraction, Telegram, plugins.

## Getting started (coming soon)

```bash
# Future install (not yet available)
curl -fsSL https://pseudo-jarvis.dev/install.sh | sh
jarvis init
jarvis observe --allow ~/Projects/acme
jarvis playbook run monthly-close
```

Until Phase 1 ships, read the architecture doc and open a Discussion if you want to help shape the design.

## Use cases

- **Freelancers** — client invoicing, follow-ups, weekly reporting  
- **Developers** — release rituals, dependency audits, standup prep  
- **Students** — assignment submission pipelines, citation formatting  
- **Small teams** — shared playbook templates (no shared surveillance)  

## Privacy principles

1. **Local by default** — no server in the data path  
2. **Opt-in observation** — per-directory, per-app, revocable  
3. **Inspectable** — open source; read the Observer code yourself  
4. **Provable** — every run produces an audit log  
5. **No telemetry** — CI will fail builds that add analytics SDKs  

## Contributing

We are in design phase. The highest-value contributions right now:

- Feedback on the [architecture](docs/ARCHITECTURE.md)  
- Playbook template ideas (YAML examples for real workflows)  
- Security review of the trust model  
- Platform expertise (macOS ScreenCaptureKit, Linux Wayland, Windows UI Automation)

Open an Issue or Discussion. Code contributions welcome once Phase 1 scaffolding lands.

## Community playbooks (planned)

Share workflows without sharing data:

```
playbooks/
  community/
    dev-release/
    monthly-accounting/
    job-application-pipeline/
```

Templates only — no credentials, no personal paths. Import → customize → run.

## FAQ

**Is this spyware?**  
Only if you opt in to observation, only in paths you allow, only stored locally, and you can delete the vault in one command. The architecture doc explains every signal collected.

**Why "PSEUDO" JARVIS?**  
Because real JARVIS is fiction. This is honest, local, inspectable automation — not magic, not cloud dependency.

**Why Rust?**  
Long-running daemon, file watchers, low memory, single binary distribution. Python plugins can still run in sandboxed subprocesses.

**Can I use GPT-4 instead of Ollama?**  
Optional cloud providers may come later, but **offline-first is non-negotiable** for the default path.

## License

MIT License — see [LICENSE](LICENSE).  
Copyright © 2023–2026 Bhuvan Prakash.

---

**Star this repo** if you want a JARVIS that learns your rituals instead of forgetting them every session.
