# Storage-needs consumer proof

This independent workspace models a host that links only
`tinyconnectors-bus`. It imports and inspects the actual exported
`STORAGE_NEEDS`, including the full `CollectionSpec` and all resource-prefix
fields. The test script inspects the resolved normal/build dependency graph so
workspace feature unification cannot hide a TinyBus or implementation
dependency. The lockfile pins the consumer graph; refresh it with
`cargo update --manifest-path proof-workspaces/storage-needs/Cargo.toml --workspace`
when the root workspace package version changes.
