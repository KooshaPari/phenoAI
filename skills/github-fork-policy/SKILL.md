# GitHub fork policy (Phenotype MCP fleet)

## Rule

**Always** create phenotype forks with:

```bash
gh repo fork <upstream>/<repo> --fork-name <PhenoName>
```

## Never

- Create empty repo + `git push --mirror` (produces `fork: false`, breaks GitHub UI)
- Re-parent by deleting without updating catalog + ADR-017

## Verify

```bash
gh api repos/KooshaPari/<Repo> --jq '{fork, parent: .parent.full_name}'
```

Must show `fork: true` and expected parent.

## Re-parent procedure (rare)

1. Document in ADR + FORK-NOTES
2. `gh repo delete KooshaPari/<Repo> --yes`
3. `gh repo fork <new-upstream> --fork-name <Repo>`
4. Update PhenoMCPServers catalog `fork_parent`
5. Enable issues: `gh api -X PATCH repos/KooshaPari/<Repo> -f has_issues=true`

Reference: session 40d15363 (PhenoFastMCP-rust rmcp → fastmcp_rust re-parent).
