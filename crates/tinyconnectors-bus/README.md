# tinyconnectors-bus

Every type that crosses the TinyConnectors module's `TinyBus` boundary, and the
names of the members that carry them.

TinyConnectors ships as a loadable module so a host does not compile the
implementation: `crates/tinyconnectors` is built as a `cdylib` and exports one
object. A host can load that binary but cannot `use` anything out of it, so the
payload vocabulary has to be published as an ordinary library. This is it.

| module     | what it holds                                                |
| ---------- | ------------------------------------------------------------ |
| `names`    | interface name, object path, one constant per member          |
| `composio` | the Composio backend's value vocabulary, by payload family    |
| `records`  | what a connector sync emits, on its way to memory             |
| `version`  | `CONTRACT_VERSION` and the bind rule a host applies to it     |

`composio` holds six families — `toolkits`, `connections`, `tools`, `execute`,
`triggers`, `github` — all re-exported at the crate root. Composio is one OAuth
connector backend, not the only one this contract expects to carry, so it is
namespaced: a second backend arrives as a sibling module with its own interface
and object path rather than as a rename of every type here.

`records` goes the other direction. Everything in `composio` is an answer to a
question a host asked; `ConnectorRecordBatch` is what a *sync* emits — the items
pulled out of a connected account, handed to the host, and written into memory
over memory's own bus API. `ConnectorRecord`'s field names are memory's
ingestion vocabulary exactly, asserted against a literal key list in
`records/test.rs` rather than imported from the memory contract: importing it
would reintroduce the coupling this crate exists to remove, and a near-miss
shape means a translation step where fields quietly stop arriving.

Two dependencies, both pure Rust: `serde` and `serde_json`.

## This crate sits underneath `tinyconnectors`

`tinyconnectors` **depends on this crate and re-exports all of it**. That direction
matters, and it is the opposite of the obvious one.

A *host* needs the payload types and needs nothing else: it loads the module and
makes calls, so it names `ComposioConnection` and `ComposioAuthorizeResponse` but
implements no behavior and links no transport. Making it depend on the whole module crate — and
through it on `tinybus`, `tokio`, and the module SDK — to spell a payload type
would be the wrong shape.

The alternative, a parallel set of payload types for hosts, is worse: a
`ComposioConnection` defined twice is two distinct types, with a conversion at
every call site that nothing checks. One definition, here, at the bottom. That
is not hypothetical — these types already lived in `tinymemory-api` because two
crates needed to name them and there was nowhere else both could.

Because the re-export is by module as well as by item,
`tinyconnectors::ComposioConnection`, `tinyconnectors::names::OBJECT_PATH`, and
`tinyconnectors_bus::composio::connections::ComposioConnection` all resolve to
the same items, not twins.

So: a module author depends on `tinyconnectors` and gets behavior and vocabulary. A
host depends on `tinyconnectors-bus` and gets vocabulary alone.

## What is deliberately absent

**No behavior.** The client, the OAuth handoff, and the transport live in
`crates/tinyconnectors`. A payload type describes what a frame carries, not what
the module does with it. The split is readable off the path: a name here is
data, a name there is an obligation.

**No credentials.** Nothing here holds an API key, a token, or a refresh secret.
The OAuth handoff crosses this boundary as a URL the user opens and an id to
poll — never as a token — so a host that links this crate has nothing worth
leaking.

**No transport.** This crate does not depend on `tinybus` and holds no
connection, client, or codec. A host already owns its connection — its reconnect
policy, its timeouts, its tracing — and the useful part is the vocabulary.

That is also structural, not just preference: `tinybus` is vendored as a
submodule whose manifest inherits fields from its own nested
`[workspace.package]`. Keeping the contract crate transport-free is what keeps
it down to two dependencies and what lets anything in the workspace — or outside
it — depend on it freely. CI asserts the dependency tree stays that way.

## Making a call

Arguments travel as a positional JSON array — `#[tinybus::interface]` decodes
them into a tuple — and the member name comes from `names`:

```rust,ignore
use tinyconnectors_bus::{names, ComposioAuthorizeRequest, ComposioAuthorizeResponse};

let proxy = connection.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
let reply: ComposioAuthorizeResponse = proxy
    .call(
        names::methods::AUTHORIZE,
        (ComposioAuthorizeRequest { toolkit: "gmail".into(), extra_params: None },),
    )
    .await?;
// The handoff is not finished here: the user opens this URL in a browser, and
// the connection it names stays inactive until they do.
println!("{}", reply.connect_url);
```

Nothing above is a string literal at a call site. Renaming the interface, the
path, or a member is therefore a compile error in every consumer rather than an
`UnknownMethod` discovered at runtime.

## Staying in step with the module

`names::METHODS` lists every member in dispatch order. `crates/tinyconnectors`
asserts its served members against that list, so a method added to the interface
without an entry here fails that crate's tests rather than surfacing in a host.

The table describes what the module serves, never what is planned. A constant
for a member nothing answers is discovered by a host as a runtime "unknown
method", which is strictly worse than the member not existing — so the remaining
Composio operations arrive as additive minor bumps rather than sitting here
unanswered.

## Versioning

`CONTRACT_VERSION` describes *this vocabulary*, not the package. Bump its major
component when a payload's wire form changes incompatibly or a member is removed
or renamed, and its minor component when a member or an optional field is added.
It is deliberately independent of the package version the release workflow owns,
which tracks the shipped artifact.

The payload tests pin the serde representation, because that representation is
the wire form: a host and a module that disagree about a field name fail at
runtime with a decode error, so the shape is asserted rather than assumed.

## Adding a payload family

One directory per family, with `mod.rs` explaining what the family is for,
`types.rs` holding the definitions, and `test.rs` pinning the serde form. Put a
Composio envelope under `composio/`; put something genuinely backend-neutral at
the root. Keep the crate dependency-light: the moment it links a transport or a
runtime, the reason it exists is gone.
