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
- **Simple UX** — `init / add / list / get / update / delete`

## Quick start

```sh
cargo build --release
./target/release/pxxxstore init
pxxxstore add --src github.com --id matkumatmat --pass "s3cr3t"
pxxxstore list
pxxxstore get github.com
```

## Build

```sh
cargo build --release
# binary at target/release/pxxxstore
```

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
