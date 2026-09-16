# pxxxstore — encrypted vault for humans

<p>
  <img src="https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white" />
  <img src="https://img.shields.io/badge/edition-2024-black?style=flat" />
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat" />
  <img src="https://img.shields.io/badge/crypto-Argon2%20%7C%20ChaCha20--Poly1305-blue?style=flat" />
  <img src="https://img.shields.io/github/stars/matkumatmat/pxxxstore?style=flat&label=stars" />
</p>

> **No cloud. One file. Your passphrase is the key.**

A minimal **Rust CLI password manager** — single encrypted vault in your OS config directory, protected by a master passphrase. Built for Arch Linux, works everywhere.

---

## Features

- **One vault, one file** — `x.bin` in OS config dir
- **Argon2id** key derivation + **ChaCha20-Poly1305** AEAD
- **Zero disk leaks** — passphrase never written, read via `rpassword` hidden prompt
- **Atomic writes** — temp file + rename, never half-written
- **Simple UX** — `init / add / list / get / update / delete / tui`
- **Interactive TUI** — `ratatui` + `crossterm`, keyboard-first vault browser

## Quick start

```sh
cargo build --release
./target/release/pxxxstore init
pxxxstore add --src github.com --id matkumatmat --pass "s3cr3t"
pxxxstore list
pxxxstore get github.com
pxxxstore tui
```

## Build & Install

```sh
cargo build --release
# binary at target/release/pxxxstore
```

### Install to PATH (so `pxxxstore` works everywhere)

`cargo build` alone does NOT put binary in PATH — that's why `fish: Unknown command` happens.

```sh
# install to ~/.cargo/bin (add once)
cargo install --path . --force

# fish — add ~/.cargo/bin to PATH (run once, persisted)
fish_add_path ~/.cargo/bin
# or manually: set -Ux fish_user_paths ~/.cargo/bin $fish_user_paths

# verify
which pxxxstore
pxxxstore --help
```

Alternative without install (run from repo):

```sh
cargo run --bin pxxxstore -- --help
./target/release/pxxxstore --help
./target/debug/pxxxstore --help
```

### Fresh start after `rm ~/.config/pxxxstore/x.bin`

If you deleted the vault (like `rm ~/.config/pxxxstore/x.bin`), you must re-init:

```sh
# 1. re-create vault (will prompt "New passphrase:" twice, hidden input)
pxxxstore init
# or without global install:
cargo run --bin pxxxstore -- init
./target/release/pxxxstore init

# 2. verify
pxxxstore list
# should print: [ Error ]: No entries found.  (empty vault is ok)

# 3. add first entry + launch TUI
pxxxstore add --src github.com --id myuser --pass "secret123"
pxxxstore tui
```

> Tip: `init` must run in a real TTY (not piped). If you need non-interactive/CI, use env var:
> ```sh
> PXXXSTORE_PASSPHRASE="test123" pxxxstore init
> PXXXSTORE_PASSPHRASE="test123" pxxxstore tui
> ```

## Usage

**Init**

```sh
pxxxstore init
# creates ~/.config/pxxxstore/x.bin  (Linux)
# or ~/Library/Application Support/pxxxstore/x.bin (macOS)
# or %APPDATA%\pxxxstore\x.bin (Windows)
```

**Add / List / Get / Update / Delete**

```sh
pxxxstore add --src github.com --id username --pass "secret"
pxxxstore list
pxxxstore get github.com
pxxxstore update --src github.com --id newuser --pass "newpass"
pxxxstore delete github.com
```

**TUI**

```sh
pxxxstore tui
# or with env var for scripting/CI (avoids tty prompt)
PXXXSTORE_PASSPHRASE="your-pass" pxxxstore tui
```

TUI keybinds:

| Key | Action |
|-----|--------|
| `j`/`k` or `↑`/`↓` | navigate entries |
| `g` / `G` | top / bottom |
| `/` | search (live filter `src`/`id`), `Enter` confirm, `Esc` cancel |
| `y` | yank password → clipboard (30s, `arboard`; fallback shows pass) |
| `r` | reveal / hide pass (zeroized on hide) |
| `a` | add entry (Tab to switch fields, Enter to submit) |
| `e` / `u` | edit selected entry |
| `d` | delete (confirm `y`/`n`) |
| `?` | help overlay |
| `q` / `Esc` | quit (or clear filter) |

TUI layout (ratatui `Layout`):

```
┌─ HEADER (vault path • stats • quick) ─┐
│ Tabs: Vault  Audit  Crypto  Config      │
├─ LIST (36) ─┬─ DETAIL ─────┬─ INSPECTOR ─┤
│  ⌕ search   │  SRC/ID/PASS │  x.bin hex  │
│  entries    │  timestamps  │  growth spark│
│  j/k nav    │  y/r/e/d     │  logs        │
├─────────────┴──────────────┴─────────────┤
│ [q]quit [j/k]nav [/]search [a]add [y]yank│
└─────────────────────────────────────────┘
```

## How it works

```
vault path:
  Linux   → ~/.config/pxxxstore/x.bin
  macOS   → ~/Library/Application Support/pxxxstore/x.bin
  Windows → %APPDATA%\pxxxstore\x.bin

file layout: [salt:16][nonce:12][ciphertext]

salt (random) ──► Argon2id(passphrase, salt) ──► key
vault JSON ──► ChaCha20-Poly1305(key, nonce) ──► ciphertext
```

- Random 16-byte salt per encryption
- Argon2id KDF from your passphrase
- JSON encrypted with ChaCha20-Poly1305
- Write to temp file → `rename` for atomic replace

## Tech stack

- **Rust 2024**, `clap` (derive CLI), `dirs` (config dir)
- `ratatui` 0.29 + `crossterm` 0.28 (TUI), `arboard` (clipboard, optional `clipboard` feature)
- `argon2` + `chacha20poly1305` (crypto)
- `serde` / `serde_json` + `chrono` (timestamps)
- `rpassword` (hidden input), `zeroize` (mem wipe), `anyhow`

## Security notes

- Master passphrase = only key. Lose it = vault unrecoverable
- Passphrase never stored, only used to derive key
- Salt + nonce are stored, ciphertext is authenticated — tampering is detected
- Use a strong, unique passphrase

## License

MIT — see `LICENSE`
