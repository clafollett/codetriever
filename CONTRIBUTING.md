# Contributing to MAOS

Welcome to the Multi-Agent Orchestration System (MAOS) project! This guide will help you set up your development environment and contribute effectively.

## 🚀 Quick Start

### 1. Development Environment Setup

```bash
# Clone the repository
git clone https://github.com/clafollett/maos.git
cd maos

# Source the development stack environment
source stack.env

# Set up development environment (installs git hooks)
just dev-setup
```

### 2. Install Just Command Runner

MAOS uses [`just`](https://github.com/casey/just) for development task automation:

```bash
# macOS
brew install just

# Linux
curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to ~/bin

# Windows (PowerShell)
winget install --id Casey.Just --exact

# Or via Cargo (all platforms)
cargo install just
```

### 3. Essential Commands

```bash
# See all available commands
just

# Run pre-commit checks
just pre-commit

# Development workflow
just format    # Format code
just lint      # Run clippy
just test      # Run tests
just build     # Build project
```

## 📋 Development Stack

Our development environment is defined in `stack.env` to ensure consistency across all contributors and CI environments. This prevents "works on my machine" issues.

### Required Tools

- **Rust**: Stable toolchain (defined in `rust-toolchain.toml`)
- **Just**: Task runner for development workflows
- **cargo-audit**: Security vulnerability scanner (auto-installed via `just dev-setup`)
- **Git 2.5+**: For worktree support in multi-agent orchestration

### Rust Toolchain Setup

If you don't have Rust installed:

```bash
# Install Rust (stable toolchain)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

**Important**: Users don't need Rust - only contributors. The CLI distributes as compiled binaries via NPX, Homebrew, etc.

### Stack Validation

```bash
# Load environment and validate versions
source stack.env
just validate-stack
```

## 🛠️ Development Workflow

### 1. Before You Start

1. Read the [Development Workflow](docs/DEVELOPMENT_WORKFLOW.md) document
2. Check the [GitHub Project Board](https://github.com/clafollett/maos/projects/1) for available issues
3. Pick an issue labeled `status:ready`

### 2. Branch Naming Convention

```
<type>/issue-<number>/<brief-description>

Examples:
- feature/issue-15/agent-registration
- fix/issue-23/session-cleanup  
- docs/issue-45/api-documentation
```

### 3. Development Process

```bash
# 1. Create and switch to feature branch
git checkout -b feature/issue-X/description

# 2. Make your changes following TDD principles
just test      # Write failing tests first
# ... implement code ...
just test      # Make tests pass
just format    # Format code
just lint      # Fix any linting issues

# 3. Run full pre-commit checks
just pre-commit

# 4. Commit your changes
git add .
git commit -m "feat: implement feature X (#issue-number)"

# 5. Push and create PR
git push -u origin feature/issue-X/description
gh pr create --title "Feature: Description" --body "Closes #X"
```

### 4. Code Quality Standards

All code must pass these quality gates:

- ✅ **Formatting**: `just format-check` (rustfmt with project config)
- ✅ **Linting**: `just lint` (clippy with `-D warnings`, zero warnings policy)
- ✅ **Tests**: `just test` (all tests pass, >90% coverage goal)
- ✅ **Security**: `just audit` (cargo-audit, no known vulnerabilities)
- ✅ **Compilation**: `just build` (clean release build)
- ✅ **Performance**: Commands execute in <10ms
- ✅ **Binary Size**: Single binary under reasonable size limits

### 5. Git Hooks

MAOS automatically sets up git hooks during `just dev-setup` to catch issues before committing:

```bash
# Git hooks are installed automatically with dev-setup
just dev-setup

# Or install hooks manually
just setup-git-hooks

# Test quality checks manually
just pre-commit
```

The git hooks will automatically run `just pre-commit` before each commit, ensuring all quality gates pass.

## 🏗️ Architecture Guidelines

MAOS follows a modular multi-crate workspace architecture with security-first design:

### Multi-Crate Workspace Structure

```
crates/
├── maos-cli/       # Main CLI application (entry point)
├── maos-core/      # Core orchestration logic (sessions, worktrees)
├── maos-security/  # Security features (rm -rf blocking, .env protection)
├── maos-tts/       # Text-to-speech provider integration
└── maos-config/    # Configuration management (JSON, env vars)
```

### Benefits of Multi-Crate Architecture

- **Parallel Compilation**: Crates build independently
- **Clear Separation**: Each crate has focused responsibility
- **Modular Testing**: Test individual components in isolation
- **Type Safety**: Strong compile-time guarantees across boundaries

### Coding Standards

1. **Test-First Development**: Write failing tests before implementation (TDD)
2. **Security-First Design**: All inputs validated, dangerous operations blocked
3. **Performance-First**: Target <10ms startup time for any command
4. **Modern Rust**: Use latest stable features and idioms
5. **Documentation**: Document public APIs with examples and usage patterns
6. **Single Binary**: Compile to standalone binary with zero runtime dependencies

### String Formatting (Rust 1.88.0+)

Use inline format strings (enforced by clippy):

```rust
// ✅ Good - Modern Rust style
let agent_id = 42;
let endpoint = "http://example.com";

println!("Processing agent: {agent_id}");
error!("Failed to connect to {endpoint}");

// ❌ Bad - Legacy style
println!("Processing agent: {}", agent_id);
error!("Failed to connect to {}", endpoint);
```

## 🧪 Testing Strategy

For comprehensive testing guidelines and performance targets, see [Testing Strategy](docs/testing-strategy.md).

### Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_security_validation() {
        // Test dangerous command detection
        let result = validate_command("rm -rf /");
        assert!(result.is_err());
    }

    #[test]
    fn test_worktree_creation() {
        // Test git worktree operations
        let temp_dir = TempDir::new().unwrap();
        // Test implementation
    }

    #[test]
    fn test_config_parsing() {
        // Test configuration management
    }
}
```

### Testing Types

1. **Unit Tests**: Each crate has comprehensive unit tests
2. **Integration Tests**: Full CLI command execution tests
3. **Security Tests**: Validate all security features work correctly
4. **Performance Tests**: Benchmark startup time and command execution

### Running Tests

```bash
just test              # All tests across workspace
just test-crate maos-security  # Single crate tests
just test-coverage     # With coverage report (requires cargo-tarpaulin)
just test-integration  # Integration tests with real git repos
cargo test specific_test  # Single test by name
```

### Performance Testing

```bash
just bench            # Run benchmarks
just profile-startup  # Profile command startup time
just test-perf        # Performance regression tests
```

## 📝 Commit Standards

Follow Conventional Commits with GitHub issue linking:

```
<type>: <description> (#<issue_number>)

Types:
- feat: New features (minor version bump)
- fix: Bug fixes (patch version bump)  
- chore: Maintenance (no version bump)
- docs: Documentation (no version bump)
- refactor: Code refactoring (no version bump)
- test: Adding/updating tests (no version bump)

Examples:
- feat: implement agent registration system (#15)
- fix: resolve task assignment race condition (#23)
- docs: add API documentation for orchestration (#42)
```

## 🔄 Pull Request Process

### 1. PR Requirements

- [ ] All quality gates pass (CI will check)
- [ ] Tests added/updated for new functionality
- [ ] Documentation updated if needed
- [ ] Code follows DDD and Clean Architecture principles
- [ ] Commit messages follow conventional format

### 2. Review Process

1. **Automated Checks**: CI runs all quality gates
2. **Code Review**: At least one approving review required
3. **Merge**: Squash and merge to main after approval

### 3. PR Template

Your PR should include:

```markdown
## Summary
Brief description of changes.

## Related Issue
Closes #[issue number]

## Type of Change
- [ ] Bug fix
- [ ] New feature  
- [ ] Refactoring
- [ ] Documentation

## Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Coverage maintained >90%

## Checklist
- [ ] Code follows DDD principles
- [ ] Documentation updated
- [ ] All CI checks pass
```

## 🆘 Getting Help

- **Questions**: Create a discussion in the GitHub repository
- **Bugs**: Create an issue with reproduction steps
- **Features**: Create an issue with detailed requirements
- **Development**: Check [DEVELOPMENT_WORKFLOW.md](docs/DEVELOPMENT_WORKFLOW.md)

## 🔧 IDE Setup

### VS Code (Recommended)

MAOS includes VS Code configuration for optimal development experience:

```bash
# Install recommended extensions
code --install-extension rust-lang.rust-analyzer
code --install-extension skellock.just
code --install-extension tamasfe.even-better-toml
```

**Included configurations:**
- **settings.json**: Rust Analyzer with clippy integration, format on save
- **extensions.json**: Recommended extensions for Rust development
- **launch.json**: Debug configurations for MAOS binary and tests

### Other IDEs

**IntelliJ IDEA / CLion:**
- Install Rust plugin
- Configure rustfmt and clippy integration
- Set up run configurations for `just` commands

**Vim/Neovim:**
- Use rust-analyzer with your LSP client
- Configure rustfmt for format on save
- Set up just command integration

### Editor-agnostic Setup

All editors should be configured to:
1. Use rust-analyzer as the language server
2. Run rustfmt on save with our rustfmt.toml
3. Show clippy lints inline with `-D warnings`
4. Exclude target/ directory from file watching
5. Enable Rust 2024 edition features
6. Configure for workspace development (multiple crates)

## 🚀 CLI Development Guidelines

### Command Structure

All MAOS commands follow a consistent pattern:

```rust
// Example command implementation
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct PreToolUseArgs {
    #[arg(long)]
    tool_name: String,
    
    #[arg(long)]
    session_id: Option<String>,
    
    #[arg(long, short)]
    verbose: bool,
}

pub fn execute(args: PreToolUseArgs) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Validate input
    // 2. Apply security checks
    // 3. Execute core logic
    // 4. Return structured result
    Ok(())
}
```

### Error Handling Standards

```rust
// Use custom error types for clear error reporting
#[derive(Debug, thiserror::Error)]
pub enum MaosError {
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    
    #[error("Git operation failed: {0}")]
    GitError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

// Always provide helpful error messages
pub fn validate_session(session_id: &str) -> Result<Session, MaosError> {
    if session_id.is_empty() {
        return Err(MaosError::ConfigError(
            "Session ID cannot be empty. Run 'maos session-info' to see current session.".into()
        ));
    }
    // ...
}
```

### CLI Testing

```rust
// Use assert_cmd for CLI testing
#[cfg(test)]
mod cli_tests {
    use assert_cmd::Command;
    use predicates::prelude::*;

    #[test]
    fn test_pre_tool_use_command() {
        let mut cmd = Command::cargo_bin("maos").unwrap();
        cmd.arg("pre-tool-use")
           .arg("--tool-name")
           .arg("Task")
           .assert()
           .success()
           .stdout(predicate::str::contains("Tool validated"));
    }

    #[test]
    fn test_security_validation() {
        let mut cmd = Command::cargo_bin("maos").unwrap();
        cmd.arg("pre-tool-use")
           .arg("--tool-name")
           .arg("Bash")
           .arg("--command")
           .arg("rm -rf /")
           .assert()
           .failure()
           .stderr(predicate::str::contains("Security violation"));
    }
}
```

## 📦 Distribution Development

### Building Release Binaries

```bash
# Build optimized release binary
just build-release

# Build for all target platforms
just build-all-targets

# Test binary size and performance
just check-binary-size
just benchmark-startup
```

### NPX Package Development

For contributors working on NPX distribution:

```bash
# Setup NPX package structure
just setup-npm-package

# Test NPX installation locally
npm pack
npx ./maos-cli-1.0.0.tgz --help

# Validate package.json and binary paths
just validate-npm-package
```

### Testing Distribution

```bash
# Test different installation methods
just test-homebrew-install
just test-npm-install
just test-direct-download

# Verify binary works across platforms
just test-cross-platform
```

### Rust-Specific Performance Guidelines

```rust
// ✅ Good - Efficient string handling
fn format_session_path(session_id: &str, agent: &str) -> PathBuf {
    PathBuf::from(format!(".maos/sessions/{session_id}/{agent}"))
}

// ✅ Good - Zero-allocation where possible
fn is_dangerous_command(cmd: &str) -> bool {
    cmd.contains("rm -rf") || cmd.starts_with("sudo rm")
}

// ✅ Good - Use owned strings only when necessary
fn validate_path(path: &Path) -> Result<(), SecurityError> {
    // Path validation without allocation
}
```

### Security Implementation Guidelines

```rust
// Security validation must be comprehensive
pub fn validate_tool_execution(tool: &ToolInput) -> Result<(), SecurityError> {
    // 1. Block dangerous commands
    security::validate_command(&tool.command)?;
    
    // 2. Protect sensitive files
    security::validate_file_access(&tool.file_paths)?;
    
    // 3. Sanitize paths
    security::validate_path_traversal(&tool.working_dir)?;
    
    Ok(())
}
```

## 📚 Additional Resources

- [Development Workflow](docs/DEVELOPMENT_WORKFLOW.md) - Detailed development process
- [Architecture Overview](ARCHITECTURE.md) - Rust crate structure and design
- [Security Model](docs/security/) - Security features and threat model
- [CLI Reference](docs/cli/) - Command documentation and usage
- [Performance Guidelines](docs/performance/) - Optimization strategies
- [Distribution](docs/distribution/) - NPX, Homebrew, and binary release process

---

Thank you for contributing to MAOS! 🚀 Together we're building the future of multi-agent orchestration.