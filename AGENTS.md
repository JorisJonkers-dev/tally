# Agents

Guidance for any coding agent working in this repository.

- Agent contract: [CLAUDE.md](CLAUDE.md)
- Estate conventions: https://github.com/JorisJonkers-dev/workspace/blob/main/CLAUDE.md

## Build and test

- Everything CI runs: `task check` (or `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; coverage: `task coverage`)
- Install the git hooks (Conventional Commits check, no direct push to `main`): `scripts/install-git-hooks.sh`
