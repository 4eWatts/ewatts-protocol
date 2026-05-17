# Contributing to eWatts

## Code of Conduct

Be respectful. Disagree constructively. This is a technical project — arguments are settled by code and testing, not by authority.

## How to Contribute

### Reporting Bugs

Open a [GitHub Issue](https://github.com/4Ewatts/ewatts-protocol/issues) with:

- A clear description of the bug
- Steps to reproduce
- Expected vs actual behavior
- Your environment (OS, Rust version, commit hash)

For security vulnerabilities, see [SECURITY.md](SECURITY.md).

### Suggesting Features

Open an issue with the label `enhancement`. Be specific about:

- What problem it solves
- How it fits the protocol's design (privacy, decentralization, memory-boundedness)
- Rough implementation approach (if you have one)

### Submitting Code

1. Fork the repository
2. Create a branch: `git checkout -b fix/description`
3. Make your changes
4. Ensure all tests pass: `cargo test --release`
5. Ensure no warnings: `cargo build --release 2>&1 | grep -i warning`
6. Commit with a clear message
7. Open a Pull Request

### Style Guide

- Run `cargo fmt` before committing
- Follow existing patterns in the codebase
- Prefer clarity over cleverness
- Add tests for new functionality
- Document public API with doc comments

## What Needs Help

Check [open issues](https://github.com/4Ewatts/ewatts-protocol/issues) for:

- Bugs tagged `bug`
- Features tagged `enhancement`
- Documentation tagged `docs`
- Good first issues tagged `good first issue`

## Architecture

See [App/ARCHITECTURE.md](App/ARCHITECTURE.md) for the full software architecture document.
