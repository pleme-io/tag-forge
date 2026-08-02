# AGENTS.md

Repo guidance for AI agents. See `CLAUDE.md` for the full design rationale.

## Quick rules

- This is a Rust CLI. The full design + invariants are in `CLAUDE.md`. Read it
  before non-trivial changes.
- Tests live alongside their module via `#[cfg(test)] mod tests`. Add tests
  for every new validator type, every new merge case, every new subcommand.
- Don't add dependencies casually. v0.1 is lean: clap, serde, serde_yaml,
  serde_json, anyhow, tracing, regex, walkdir. Adding cloud SDKs is a v0.2
  decision, not a stealth dependency bump.
- Edition 2024, Rust 1.89+. Let-chains are fair game.
