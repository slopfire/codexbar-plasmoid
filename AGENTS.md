# Agent instructions

## Context7

Use `bunx ctx7@latest` for all Context7 CLI requests. Do not use `npx` or `npm exec` for Context7.

Examples:

```sh
bunx ctx7@latest library <name> "<question>"
bunx ctx7@latest docs <library-id> "<question>"
```

## CodeGraph

When a `.codegraph/` directory exists at the repository root, use CodeGraph before text search or manually reading files when locating or understanding code.
