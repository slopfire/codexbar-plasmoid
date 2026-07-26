# Mock CLI Data

Use the repo mock installer (preferred):

```sh
./scripts/setup-mock-cli.sh
# installs /tmp/codexbar-plasma-mock/codexbar
```

Put that directory first on `PATH` for deterministic helper/UI tests.

Required mock behavior:

- `codexbar usage ...` prints a JSON array with at least two providers.
- `codexbar cost ...` prints a JSON array with matching provider cost summaries.
- Include Codex data with nested `usage` (primary/secondary, credits, code review, dashboard daily breakdown), account email, source, and version.
- Include Claude data with primary/secondary/tertiary usage windows, status, account email, source, and version.
- Include `daily[]` cost data for chart fallback.

Smoke command after creating the mock:

```sh
PATH=/tmp/codexbar-plasma-mock:$PATH \
  node plasmoid/contents/code/codexbar-plasmoid-helper.mjs --provider all --timeout 5 |
  node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>{const j=JSON.parse(s); if(!j.ok||j.entries.length<2) process.exit(1); console.log(j.entries.map(e=>`${e.provider}:${e.rows.length}:${e.tokenUsage?"cost":"nocost"}`).join(", "))})'
```

Expected output shape:

```text
codex:2:cost, claude:3:cost
```
