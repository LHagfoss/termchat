# Winget Manifest — termchat

This directory contains a winget manifest for Windows users to install termchat via:

```powershell
winget install LHagfoss.termchat
```

## How to make it work

1. After publishing a GitHub release (tag `v0.1.0`), download the `termchat-x86_64-windows.exe` asset
2. Compute its SHA256: `sha256sum termchat-x86_64-windows.exe`
3. Replace `PLACEHOLDER_SHA256` in `LHagfoss.termchat.0.1.0.yaml` with the real hash
4. Commit and push
5. Users can then install via `winget install LHagfoss.termchat`

For **automatic** winget support, open a PR to the community [winget-pkgs](https://github.com/microsoft/winget-pkgs) repo with this manifest.

## Version bumps

After each release:
1. Copy `LHagfoss.termchat.0.1.0.yaml` → `LHagfoss.termchat.0.2.0.yaml` (update version number)
2. Update URLs and SHA256 in the new file
3. Delete the old manifest
