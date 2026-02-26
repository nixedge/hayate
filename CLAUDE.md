# Claude Code Guidelines for Hayate

## Commit Message Requirements

### Conventional Commits
All commits **MUST** follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Allowed types:**
- `feat:` - A new feature
- `fix:` - A bug fix
- `docs:` - Documentation only changes
- `style:` - Changes that do not affect the meaning of the code (white-space, formatting, etc)
- `refactor:` - A code change that neither fixes a bug nor adds a feature
- `perf:` - A code change that improves performance
- `test:` - Adding missing tests or correcting existing tests
- `chore:` - Changes to the build process or auxiliary tools and libraries
- `ci:` - Changes to CI configuration files and scripts
- `build:` - Changes that affect the build system or external dependencies

**Examples:**
```
feat: add snapshot-based durability to LSM storage
fix: correct epoch boundary calculation for SanchoNet
docs: update README with new snapshot configuration
refactor: simplify chain sync state machine
```

### NO Attribution in Commits

**NEVER** include Claude Code attribution in commit messages. The following are **FORBIDDEN**:

❌ `🤖 Generated with [Claude Code](https://claude.com/claude-code)`
❌ `Co-Authored-By: Claude <noreply@anthropic.com>`
❌ Any similar attribution lines

Commits with attribution will be **REJECTED** by pre-push hooks.

### Commit Message Format

- Use imperative mood in the subject line ("add feature" not "added feature")
- Keep subject line under 72 characters
- Separate subject from body with a blank line
- Use the body to explain **what** and **why**, not **how**
- Use Markdown formatting in the body if needed
- Reference issues/PRs in the footer when applicable

**Good commit example:**
```
feat: implement adaptive snapshot strategy for LSM trees

Add SnapshotManager with dual-mode snapshot logic:
- Epoch-based snapshots during bulk sync (every 86,400 slots)
- Time-based snapshots near chain tip (every 5 minutes)

This provides durability while minimizing snapshot overhead during
initial sync when data loss is less critical.

Closes #123
```

**Bad commit example:**
```
Fixed some bugs and added a thing

Made changes to the code.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
Co-Authored-By: Claude <noreply@anthropic.com>
```

## Code Style

- Follow existing code style and conventions
- Run formatters before committing
- Ensure all tests pass
- Add tests for new functionality

## Pull Request Guidelines

- Keep PRs focused on a single concern
- Write clear PR descriptions explaining the motivation and approach
- Reference related issues
- Ensure CI passes before requesting review
