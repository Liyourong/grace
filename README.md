# Grace

## Commit Message Rules

### Recommended Format

In this course project, we recommend a lightweight [**Conventional Commits–style**](https://www.conventionalcommits.org/) format:

```text
<type>(optional scope)[!]: <short imperative summary>
```

### Guidelines

- Use **imperative mood** (e.g., “add”, “fix”, “remove”).
- Keep the entire subject line **≤ 50 characters**, including the type and scope.
- `scope` and `!` are optional — use them when helpful. Use `!` to indicate a breaking change.

In real projects, always follow the commit message convention used by your team or organization — even if it differs from this one.

### Common Types

- `feat`: new functionality
- `fix`: bug fixes
- `docs`: documentation changes
- `refactor`: code changes without behavior change
- `chore`: cleanup, configuration, maintenance
- `ci`: GitHub Actions / automation

### Examples

```text
feat: add WebSocket updates for task status
feat(storage): mount a postgres volume
fix(compose)!: rename db service to postgres
docs: add local dev setup instructions
refactor(api): simplify request validation
```
