# TUI DAW - Claude Code Instructions

## Package Manager

This project uses **bun** as the package manager and runtime. Always use `bun` instead of `npm` or `yarn`.

```bash
# Install dependencies
bun install

# Run the app
bun start

# Type check
bun run tsc --noEmit
```

## Project Overview

A terminal-based Digital Audio Workstation (DAW) built with Ink (React for terminal). Combines FL Studio and Maschine workflow philosophies with terminal aesthetics.

## Key Technologies

- **Ink** - React renderer for terminal UIs
- **TypeScript** - Type safety
- **Bun** - Fast JavaScript runtime and package manager
