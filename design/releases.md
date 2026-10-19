# Releases

Code: `.github/`, `dist-workspace.toml`

- **How a release happens**: on a push to `main`, `.github/workflows/version.yml` runs `.github/version.sh release`, which tags `v<version>` if it is new and dispatches `.github/workflows/release.yml` (`dist`, `dispatch-releases`, because a tag pushed with `GITHUB_TOKEN` triggers no workflow). That builds the four targets, makes the GitHub Release (notes from `CHANGELOG.md`) with `cornercase-installer.sh`, and pushes the formula to `usecornercase/homebrew-tap` (secret `HOMEBREW_TAP_TOKEN`, a fine-grained token that expires). `.github/build-setup.yml` is the Zig step `dist generate` puts into `release.yml`. `site/public/install.sh` (served at `usecornercase.dev/install.sh`) runs the latest release's installer.
