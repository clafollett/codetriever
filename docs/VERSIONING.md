# Versioning Strategy

## Pre-1.0 Development (Current Phase)

While Codetriever is in pre-1.0 development:

- **All changes default to patch version bumps (0.1.X)**
  - Features, fixes, refactors, etc. all increment patch version
  - Breaking changes are expected and don't affect versioning

- **Minor version bumps (0.X.0) are reserved for milestones**
  - Use `[milestone]` tag in commit message
  - Examples:
    - Completion of a major architectural phase
    - First working end-to-end demo
    - Beta release readiness

- **Version 1.0.0 criteria** (future):
  - Stable API contracts
  - Production-ready orchestration
  - Comprehensive test coverage
  - Performance benchmarks met
  - Security audit completed

## Commit Message Guidelines

For pre-1.0 development, use conventional commits for clarity:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Code style
- `refactor`: Code refactoring
- `test`: Testing
- `chore`: Maintenance

**Special Tags**:
- `[milestone]`: Triggers minor version bump
- `[skip-release]`: Prevents automatic release

Examples:
```
feat(orchestrator): add phase planning logic

fix(acp): handle connection timeouts

feat(core): complete phase 1 implementation [milestone]
```

## Release Process

1. Every merge to `main` triggers the release workflow
2. Version is automatically bumped based on commit message
3. Git tag is created and pushed
4. GitHub release is created with generated notes
5. All releases are marked as "pre-release" until v1.0.0

## Version History

- `0.1.0` - Initial project setup and architecture
- _Future versions will be automatically tracked_