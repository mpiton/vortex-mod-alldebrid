# vortex-mod-alldebrid

AllDebrid debrid WASM plugin for [Vortex](https://github.com/mpiton/vortex).

Hands a covered hoster link to AllDebrid and returns the direct CDN URL
Vortex should download from, never the original hoster page. The API key
lives in the host keyring and is read through `get_credential`; it is never
logged, persisted, or echoed into an error message.

A debrid has no anonymous mode — every export needs the key, so a missing
credential is a typed failure rather than a silent free-tier attempt.

## Features

- Coverage check against a static list of ~35 file hosters (`can_handle`),
  deliberately excluding streaming sites so a debrid never diverts a URL
  that Vortex's own crawler plugins own
- `GET /v4/link/unlock` → direct CDN URL, filename, and size
- Account validation through `GET /v4/user`, rejecting non-premium accounts
  and reporting `premiumUntil` to the Accounts view
- AllDebrid answers **HTTP 200 even for most failures**, so the envelope is
  parsed before anything is treated as success. Its string error codes map
  to stable machine codes so the host's resolution cascade knows *why* a
  tier declined:

  | AllDebrid code | Plugin code | Cascade meaning |
  |---|---|---|
  | `AUTH_MISSING_APIKEY`, `AUTH_BAD_APIKEY`, `AUTH_BLOCKED`, `AUTH_USER_BANNED` | `ACCOUNT_INVALID_CREDENTIALS` | key is bad, stop using it |
  | `MUST_BE_PREMIUM` | `ACCOUNT_EXPIRED` | premium lapsed |
  | `LINK_TEMPORARY_UNAVAILABLE`, `LINK_HOST_FULL`, `LINK_TOO_MANY_DOWNLOADS`, `NO_SERVER` | `ACCOUNT_COOLDOWN` | back off, retry later |
  | `FREE_TRIAL_LIMIT_REACHED` | `ACCOUNT_QUOTA_EXCEEDED` | quota exhausted |
  | `LINK_IS_MISSING`, `BAD_LINK`, `LINK_DOWN`, `LINK_HOST_NOT_SUPPORTED`, `LINK_HOST_UNAVAILABLE` | `HOSTER_NO_FILE` | fall through to the next tier |
  | `LINK_PASS_PROTECTED` | `HOSTER_AUTHENTICATION_REQUIRED` | link needs a password |
  | anything else | `PLUGIN_ERROR` | unknown, treated as a hard failure |

- No path can produce a file entry without a direct URL. A `delayed` unlock
  (AllDebrid is still preparing the link) returns `ACCOUNT_COOLDOWN` rather
  than a file with an empty URL.
- An `ACCOUNT_*` code makes the host stop using the account for *every*
  hoster, so a failure that is really the hoster's or AllDebrid's is never
  classified as one — a third-party outage must not cost a paid account.

Traffic is not reported: AllDebrid's `limitedHostersQuotas` are per-hoster
daily *download counts*, not bytes, so there is no honest scalar to show.
Only the expiry is surfaced.

## Build

```bash
# Lint
cargo clippy --all-targets -- -D warnings

# WASM artefact (required by the smoke test)
rustup target add wasm32-wasip1   # one-time
cargo build --target wasm32-wasip1 --release
# target/wasm32-wasip1/release/vortex_mod_alldebrid.wasm

# Native, fixture, and mandatory WASM smoke tests
cargo test
```

## Install (development)

```bash
PLUGIN_NAME="vortex-mod-alldebrid"
PLUGIN_DIR="$HOME/.local/share/dev.vortex.app/plugins/$PLUGIN_NAME"

mkdir -p "$PLUGIN_DIR"
cp target/wasm32-wasip1/release/vortex_mod_alldebrid.wasm "$PLUGIN_DIR/plugin.wasm"
cp plugin.toml "$PLUGIN_DIR/plugin.toml"
```

Vortex hot-reloads the plugin via the file watcher.

## Configure

Get an API key from <https://alldebrid.com/apikeys> and store it under the
plugin's own service name:

- **Service**: `vortex-mod-alldebrid`
- **Username**: anything (unused)
- **Password**: your API key

The plugin reads it through the host's `get_credential` host function
(scoped — only `vortex-mod-alldebrid` can read this slot).

## License

GPL-3.0 — see [`LICENSE`](./LICENSE).
