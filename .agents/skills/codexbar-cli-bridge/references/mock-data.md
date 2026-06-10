# Mock CLI Data

Use a mock `codexbar` executable for deterministic plasmoid runtime tests. Put it before the real CLI on `PATH`.

Required mock behavior:

- `codexbar usage ...` prints a JSON array with at least two providers.
- `codexbar cost ...` prints a JSON array with matching provider cost summaries.
- Include Codex data with credits, code review percent, status, dashboard daily breakdown, account email, source, and version.
- Include Claude data with primary/secondary/tertiary usage windows, status, account email, source, and version.
- Include `daily[]` cost data for chart fallback.

Smoke command after creating the mock:

```sh
PATH=/tmp/codexbar-plasma-mock:$PATH \
  plasmoid/contents/code/codexbar-plasmoid-helper.mjs --provider all --timeout 5 |
  node -e 'const fs=require("fs"); const j=JSON.parse(fs.readFileSync(0,"utf8")); if(!j.ok||j.entries.length<2) process.exit(1); console.log(j.entries.map(e=>`${e.provider}:${e.rows.length}:${e.tokenUsage?"cost":"nocost"}`).join(", "))'
```

Expected output shape:

```text
codex:2:cost, claude:3:cost
```
