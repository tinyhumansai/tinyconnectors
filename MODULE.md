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

The module requires a JSON configuration blob at load time, tagged by the route
it should use:

```json
{ "route": "proxy",  "base_url": "https://api.example.com", "auth_token": "<user session token>" }
{ "route": "direct", "api_key": "<user Composio key>", "entity_id": "default" }
```

**proxy** goes through the TinyHumans backend, which owns the Composio API key,
the billing margin, the toolkit allowlist, and the HMAC verification of inbound
webhooks. **direct** goes straight to `backend.composio.dev/api/v3` with the
user's own key.

The module implements both routes and selects neither — which one to use depends
on whether the user is signed in and whether they supplied a key, and those are
the host's decisions. Change route by reloading the module with a different
blob.

The credential is the host's to supply either way. The module never reads one
from the environment, never logs it, and never returns it through a member. It
also refuses a `base_url` that is not HTTPS or a genuine loopback address, so a
misconfiguration cannot send the credential somewhere it should not go. Loading
without the credential its route needs fails, rather than producing a module
that answers every call with a 401.

### The routes are not equivalent

Direct mode cannot answer `ListToolkits` — there is no per-user allowlist when
you talk to Composio directly — or `DeleteConnection`, whose proxy version also
clears memory sourced from the connection. Both return a named refusal rather
than an empty result that would read like an answer.

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
