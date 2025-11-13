# Release Process

This document describes how to create a new release of Oxwatcher.

## Prerequisites

- Push access to the repository
- Git tag creation permissions

## Creating a Release

1. Update the version in `Cargo.toml`:
```toml
[package]
name = "Oxwatcher"
version = "0.2.0"  # Update this
```

2. Commit the version change:
```bash
git add Cargo.toml
git commit -m "chore: bump version to 0.2.0"
git push origin main
```

3. Create and push a git tag:
```bash
git tag v0.2.0
git push origin v0.2.0
```

4. GitHub Actions will automatically:
   - Build binaries for all platforms
   - Create a GitHub release
   - Upload all artifacts to the release

5. The release will be available at:
```
https://github.com/YOUR_USERNAME/oxwatcher/releases/latest
```

## Manual Release (if needed)

If GitHub Actions fails, you can build and upload manually:

```bash
# Build for current platform
cargo build --release

# Create tarball
cd target/release
tar czf oxwatcher-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz oxwatcher

# Upload to GitHub release manually
```

## Supported Platforms

- Linux x86_64
- Linux ARM64 (aarch64)
- macOS x86_64 (Intel)
- macOS ARM64 (Apple Silicon)

## Testing Releases

After creating a release, test the installation:

```bash
# Download and test
curl -L https://github.com/eddort/0xwatcher/releases/latest/download/oxwatcher-linux-x86_64.tar.gz | tar xz
./oxwatcher --version
```
