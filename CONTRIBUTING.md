# Contributing to Skylock

Thank you for your interest in contributing to Skylock!

## How to Contribute

### Pull Requests

1. Fork the repository
2. Create a new branch for your feature or bugfix
3. Make your changes with clear, descriptive commit messages
4. Ensure all tests pass: `cargo test --workspace`
5. Run code formatting: `cargo fmt --all`
6. Run linter: `cargo clippy --workspace --all-targets`
7. Submit a pull request with a clear description of your changes

### Bug Reports

Found a bug? Please open an issue with:

- Clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- System information (OS, Rust version)
- Relevant logs or error messages

### Feature Requests

Have an idea? Open an issue describing:

- The problem you're trying to solve
- Your proposed solution
- Any alternative solutions you've considered
- How this benefits the project

## Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/Skylock.git
cd Skylock

# Build the project
cargo build --workspace

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Check for issues
cargo clippy --workspace --all-targets
```

## Code Standards

- Follow Rust idioms and best practices
- Write clear, self-documenting code
- Add tests for new functionality
- Update documentation as needed
- Keep commits focused and atomic

## Questions?

Contact the maintainer at null@nullme.lol
