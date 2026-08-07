# pxxxstore

A minimal CLI password manager written in Rust. Credentials are stored in a single encrypted vault protected by a passphrase.

## Features

- Encrypted vault file stored in your OS config directory
- Argon2id key derivation (KDF)
- ChaCha20-Poly1305 authenticated encryption
- Passphrase is never written to disk, prompted via stdin
- Subcommands: `init`, `add`, `list`, `get`, `update`, `delete`

## Build

```sh
cargo build --release
```

The binary will be at `target/release/pxxxstore`.

## Usage

Initialize the vault (creates the config directory and vault file):

```sh
pxxxstore init
```

Add an entry:

```sh
pxxxstore add --src github.com --id username --pass "secret"
```

List entries (ids only, no passwords):

```sh
pxxxstore list
```

Get a single entry (id, password, timestamps):

```sh
pxxxstore get github.com
```

Update an entry:

```sh
pxxxstore update --src github.com --id newuser --pass "newpass"
```

Delete an entry:

```sh
pxxxstore delete github.com
```

## How it works

The vault is stored at:

- Linux: `~/.config/pxxxstore/x.bin`
- macOS: `~/Library/Application Support/pxxxstore/x.bin`
- Windows: `%APPDATA%\pxxxstore\x.bin`

File format: `[salt 16 bytes] + [nonce 12 bytes] + [ciphertext]`

- A random salt is generated per encryption
- A key is derived from your passphrase with Argon2id
- The vault JSON is encrypted with ChaCha20-Poly1305
- Writes go to a temp file first, then rename for atomic replacement

## Tech stack

- Rust 2024 edition
- clap (CLI parsing)
- argon2, chacha20poly1305 (cryptography)
- serde / serde_json (serialization)
- chrono (timestamps)
- rpassword (hidden passphrase input)

## Security notes

- The master passphrase is the only key; losing it means the vault cannot be recovered
- The passphrase is never stored, only used to derive the encryption key
- Use a strong, unique passphrase
