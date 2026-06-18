# AUR (Arch User Repository) — termchat

These files are templates for packaging `termchat` on Arch Linux.

## How to use

1. Create a Git repo with just these files
2. Push to GitHub
3. Import into the AUR at https://aur.archlinux.org/account/
4. After each release, update the version numbers in `PKGBUILD` and `.SRCINFO`, then push

## Install (for users)

```bash
yay -S termchat    # or your preferred AUR helper
```

**Note:** This only builds a Linux binary. Windows/macOS packages are handled separately.
