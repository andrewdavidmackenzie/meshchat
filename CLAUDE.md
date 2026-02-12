# Project Context

meshchat is a GUI application for chatting with people over mesh radio networks such as meshtastic and
meshcore.

## About This Project

It is written entirely in rust.

Key dependencies are:

- Iced GUI framework
- meshtastic rust crate from the meshtastic project
- meshcore-rs crate of my own

I provide a set of consistent make targets across all of my projects. There are targets to build, run and test
locally. There are also targets to check that all dependencies are used (udep), check that all feature combinations
compile (cargo all-features), and that there are no clippy warnings.

It has extensive test coverage which I attempt to maintain high. These tests can be run locally but are run
in GH Actions CI using the workflows in the .github/workflows directory.

I attempt to avoid panics and unwraps in the code, there are directives to check for this and clippy will warn
if it finds any. This should be maintained.

In tests, I use `expect()` to not use panic directly and to provide an explanation of why the assertion failed.

Do not commit changes, I will do that manually.

# Instructions

When working with this codebase, prioritize readability over cleverness.
Ask clarifying questions before making architectural changes.

When completing a task, run `cargo fmt` to ensure code formatting is maintained, and to ensure that there are
no clippy warnings or test failures using "make clippy test"

## Key Directories

- `src/` — Source code
- `assets/` — Assets used in the application or docs or websites, mainly images and fonts

## Commands

```bash
make run      # Build and run the app locally
make clippy   # make sure there are not clippy warnings
make test     # Run tests locally