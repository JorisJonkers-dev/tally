# tally

**tally draws progress bars that nobody updates by hand.** Each Raul project keeps a list of the features it must
have (for a rebuilt program, everything the original does) and its tests say which feature they prove. tally counts
both and draws how far along the project is: built, tested, and passing on Linux, Windows and macOS.

<p align="center">
  <img src="docs/images/how-it-works.svg" alt="tally reads a feature list and the tests tagged with it, and draws progress bars" width="100%">
</p>

## What it looks like

The picture for [loot-rs](https://github.com/JorisJonkers-dev/loot-rs), redrawn on every change to its main branch:

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/loot-rs/main/docs/coverage/tally.svg" alt="loot-rs feature catalogue: items, tagged, passing and covered per area" width="100%">
</p>

## Why it helps

- 📋 **One list of what matters**: every feature, where it comes from, and its state.
- ✅ **Honest numbers**: a feature counts as covered only when a test for it passed on all three systems.
- 🔒 **No silent drops**: a removed feature or a falling count stops the change until someone says why.

## Learn more

- [Using tally](docs/USAGE.md): what it checks, its commands, adding it to a repository, development.
- [The catalogue format](docs/FORMAT.md) and [decisions](docs/adr).

Licence: GPL-3.0-only ([LICENSE](LICENSE)).
