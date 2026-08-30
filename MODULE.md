# TinyConnectors TinyBus Module

This package contains the native `tinyconnectors` module for TinyBus module ABI
v1. Install only the archive matching the host operating system and
architecture.

The module claims `ai.tinyhumans.connectors.Composio`, serves the object at
`/ai/tinyhumans/connectors/Composio`, and provides `ListToolkits`,
`ListConnections`, `Authorize`, and `DeleteConnection`. Every payload type, the
interface name, the object path, and the member names are published as the
`tinyconnectors-bus` crate, so a host names them from a library rather than by
string literal.

## Configuration

The module requires a JSON configuration blob at load time:

```json
{ "base_url": "https://api.example.com", "auth_token": "<user session token>" }
```

It holds no Composio API key and calls Composio nowhere. Every request goes to
`base_url`, which owns the key, the billing margin, the toolkit allowlist, and
the HMAC verification of inbound webhooks. `auth_token` authenticates the
signed-in user to that backend and is the host's to supply — the module never
reads a credential from the environment or anywhere else, never logs it, and
never returns it through a member. Loading without both fields fails, rather
than producing a module that answers every call with a 401.

## Installing

The archive contains one `.so`, `.dylib`, or `.dll` plus `modules.toml`. Keep
those files together when copying them into a TinyBus module directory. The
allowlist binds the native library filename to its SHA-256 digest so TinyBus can
reject a missing, renamed, or modified artifact before initialization.

The GitHub release also publishes `checksum.toml` as a separate asset. TinyBus
checks that manifest before downloading and extracting the selected platform
archive. Install directly from a tagged release with:

```sh
tinybus modules load-github \
  https://github.com/tinyhumansai/tinyconnectors/releases/tag/v0.1.5 \
  tinyconnectors-0.1.5-ubuntu-24.04-x86_64.tar.gz \
  <archive-sha256>
```

TinyBus modules are trusted in-process code. Install release artifacts only
from a trusted source and restart the host after replacing a loaded module.
