# Windows installation

`packaging/windows/package.ps1` builds `boardcraft-<version>-windows-<arch>.msi` (WiX 5, per-user or
per-machine) and a portable ZIP. The MSI installs `boardcraft.exe` and `boardcraft-cli.exe`, a Start
menu shortcut, the licences, and registers `.boardcraft` boards (and offers BoardCraft for
`.fountain` and `.fdx` screenplays in Open With). Its UpgradeCode is BoardCraft's own and must never
change. Signing (`sign.ps1`) runs only with the release workflow's signing secrets.

Not yet: an install-policy check like DeckCraft's (`check-install-policy.ps1`), and an icon
embedded in `boardcraft.exe` (the window icon is set at run time).
