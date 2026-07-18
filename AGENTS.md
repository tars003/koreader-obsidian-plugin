# AGENTS.md — instructions for AI coding agents working on this project

## Workflow

- **After every fix, commit + push to `origin main`.** No exceptions. The user reviews the remote, not the local working tree. Use descriptive commit messages that explain *why*, not just *what*.

## Response format

- **Bullet-only, no prose paragraphs.** Use bullets, sub-bullets, and code blocks. One-line per bullet where possible. The user is skimming, not reading prose.
- **Code in code blocks** with `file:line` references. No inline code in prose.
- **Order:** TL;DR answer first, then structure (where / bug / effect / fix / next step).

## Vocabulary

- **Use glossary terms exactly.** See [`docs/glossary.md`](docs/glossary.md) for the locked vocabulary. No synonyms. No alternate spellings. If a new term shows up, add it to the glossary with a `NEW:` marker on first introduction, drop the marker once reused.
- **Examples:**
  - ✅ "vault browser" — not "file tree widget" or "tree view"
  - ✅ "back_stack" — not "back history" or "navigation stack"
  - ✅ "KOReader" — not "the reader" or "the app"
  - ✅ "Kindle" — not "device" or "PW7"
  - ✅ "md2kindle" — not "the converter" or "the Python side"
  - ✅ "obsidian.koplugin" — not "the plugin" or "the Lua side"
