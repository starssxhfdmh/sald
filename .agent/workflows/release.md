---
description: Release new version to GitHub with auto-build for Linux and Windows
---

// turbo-all

# Release Workflow

Workflow to create a new release with binaries for Linux and Windows.

## Prerequisites

- Ensure all changes are committed
- Ensure version in `Cargo.toml` is updated

## Steps

1. Update version in `Cargo.toml` following semantic versioning:
   ```toml
   [package]
   version = "X.Y.Z"
   ```

2. Commit version changes:
   ```bash
   git add Cargo.toml
   git commit -m "chore: bump version to vX.Y.Z"
   ```

3. Push commits to remote:
   ```bash
   git push origin main
   ```

4. Create and push tag with `v` prefix:
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. GitHub Actions will automatically:
   - Build binaries for Linux (glibc and musl/static)
   - Build binaries for Windows
   - Generate changelog from commit messages
   - Create GitHub Release with all binaries

## Release Naming Convention

- Tags must start with `v`, e.g.: `v0.1.0`, `v1.0.0`
- Pre-release: `v1.0.0-alpha`, `v1.0.0-beta`, `v1.0.0-rc.1`

## Build Targets

| Platform | Target Triple | Binary Name |
|----------|---------------|-------------|
| Linux (glibc) | x86_64-unknown-linux-gnu | `sald-linux-x86_64` |
| Linux (musl) | x86_64-unknown-linux-musl | `sald-linux-x86_64-musl` |
| Windows | x86_64-pc-windows-msvc | `sald-windows-x86_64.exe` |
| VSCode Extension | N/A | `sald-vscode-extension.vsix` |

## Troubleshooting

### Build failed?
- Check errors in the Actions tab on GitHub repository
- Ensure Rust dependencies are correct

### Binary not appearing in Release?
- Ensure tag is pushed: `git push origin --tags`
- Check workflow status in GitHub Actions
