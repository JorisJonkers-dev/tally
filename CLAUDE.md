# Agent contract

The estate-wide conventions live in one place and are **not duplicated here**:

**https://github.com/JorisJonkers-dev/workspace/blob/main/CLAUDE.md**

Read it before doing anything non-trivial in this repository. It covers the
things that most often go wrong, including:

- **Pull request labels.** The estate uses a prefixed taxonomy — `type:`,
  `area:`, `component:`, `priority:`, `status:`. Plain `bug` / `enhancement` /
  `documentation` do **not** exist, and `gh pr create` fails with
  `'bug' not found`. Run `gh label list --repo <owner>/<repo>` once before
  passing `--label`.
- **Verify the value, not the command.** An exit code, a `Ready` condition or
  an accepted object is not evidence that a consumer sees what you intended.
- **Tests.** Each test checks one behaviour and is named as that behaviour; tests run with
  no retries, and a flaky test is fixed or quarantined with an issue for at most 14 days.
  The standard is
  [docs/testing-standards.md](https://github.com/JorisJonkers-dev/workspace/blob/main/docs/testing-standards.md).
- Traps around workflow runs, `zsh` word-splitting, and detached submodule
  HEADs.

Duplicating that content into every repository guarantees the copies drift, so
this file stays a pointer. Add repo-specific guidance below.

---

## This repository

`tally` is the feature catalogue tool of the Raul repositories: it reads a repository's `catalogue/`, scans its
tests for `covers(&[ids::...])` calls, reads nextest JUnit results and reports what is implemented, tagged, passing
and covered on Linux, Windows and macOS. Run `task check`. Commits and pull request titles use Conventional
Commits (`scripts/install-git-hooks.sh` installs the check). Dependencies stay toml, serde and serde_json;
`src/idsgen.rs` stays standard library only, because repositories copy it into their build.rs. The catalogue
format is `docs/FORMAT.md`; the ids decision is `docs/adr/0001-typed-ids-without-a-crate.md`. tally checks its
own catalogue (`catalogue/`), and every test tags what it proves.
