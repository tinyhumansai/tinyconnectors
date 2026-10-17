# TinyBus adapter

`ConnectorService` serves the connector contract with the module-owned route,
provider registry, preferences and daily trigger archive. `setup` registers its
object and claims the contract interface. Manifest methods, contract names and
dispatch order are tested together; no host implementation is needed to call it.

Contract 1.13 adds `PrepareArguments`, `FilterResponse`, `ClassifyError`,
`RecordTrigger`, `OpenArchive`, `ReadArchive`, and `CloseArchive`. Hosts choose
the time zone and recency boundary, authorize
external calls, and reconcile credentials through the existing `Configure`
operation. Argument normalization, calendar defaults, task filtering, provider
classification and archive appends execute inside the module. A structured
provider error carries the stable class identifier and existing product message;
it can contain provider detail and must never be sent to telemetry.

`OpenArchive` returns an opaque lease on the caller's trigger streams (its
`state_dir` is ignored; the host binds the scope). `RecordTrigger` and `ReadArchive` use that lease; `CloseArchive` releases
it without deleting data. Leases are bounded, unknown/closed handles fail, and
module shutdown drops the table. Hosts close the old user's lease when switching
identity. Existing load-time `ListTriggerHistory` remains compatible. Records
live in the host's tinystoragedrivers stream and document stores, reached over
the storage bus (`tinystoragedrivers-busproxy::connect_storage`); the grant the
host must give is generated from `tinyconnectors_bus::storage::storage_schema()` (everything
in it is `ScopeClass::Module`; `tinyconnectors_bus::storage::storage_needs()` lists
it: collection `connectors.user_scopes`, stream prefix `connectors.triggers/`).
The schema also carries the v1 to v2 migration steps the host runs with
`Registry::migrate`; existing
arities are unchanged.

The dynamic verifier exercises preparation and filtering with local payloads.
Hosts must wait for a published compatible artifact with a verified digest
before adopting new members. ABI exports remain owned by the TinyBus SDK.
