//! Looking a provider up by toolkit slug.

use std::collections::BTreeMap;
use std::sync::Arc;

use tinyconnectors_bus::{ComposioCapabilitiesResponse, ComposioCapability};

use super::traits::ConnectorProvider;

/// The providers a build knows about, keyed by toolkit slug.
///
/// An owned map rather than a process-wide static. The upstream used a global
/// `OnceLock` initialized during startup wiring, which meant a test could not
/// register a provider without leaking it into every other test in the binary,
/// and two hosts in one process could not differ. A registry that is passed
/// where it is needed costs one field and removes both problems.
#[derive(Debug, Default, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<&'static str, Arc<dyn ConnectorProvider>>,
}

impl ProviderRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider, replacing any previous one for its toolkit.
    ///
    /// Replacing rather than refusing: a host that deliberately overrides a
    /// built-in provider should not have to remove it first, and a duplicate
    /// registration is far more often an override than a mistake.
    pub fn register(&mut self, provider: Arc<dyn ConnectorProvider>) {
        self.providers.insert(provider.toolkit_slug(), provider);
    }

    /// Builder form of [`Self::register`].
    #[must_use]
    pub fn with(mut self, provider: Arc<dyn ConnectorProvider>) -> Self {
        self.register(provider);
        self
    }

    /// The provider for `toolkit`, if this build has one.
    ///
    /// Matched on the trimmed, lowercased slug, because the toolkit reaches
    /// here from a config file, a UI field, and a backend envelope, and only
    /// one of those three is reliably normalized.
    #[must_use]
    pub fn get(&self, toolkit: &str) -> Option<Arc<dyn ConnectorProvider>> {
        let key = toolkit.trim().to_ascii_lowercase();
        self.providers.get(key.as_str()).cloned()
    }

    /// Every provider, ordered by toolkit slug.
    ///
    /// Ordered because the capability matrix and the agent-ready listing are
    /// both rendered from it, and a set that reshuffles between runs makes a
    /// UI list jump for no reason.
    #[must_use]
    pub fn all(&self) -> Vec<Arc<dyn ConnectorProvider>> {
        self.providers.values().cloned().collect()
    }

    /// The toolkit slugs that ship a curated catalog, sorted.
    ///
    /// What a UI needs to label a connected toolkit as "preview — the agent
    /// cannot act through this yet" rather than presenting it as ready.
    #[must_use]
    pub fn agent_ready_toolkits(&self) -> Vec<String> {
        self.providers
            .values()
            .filter(|provider| {
                provider
                    .curated_tools()
                    .is_some_and(|catalog| !catalog.is_empty())
            })
            .map(|provider| provider.toolkit_slug().to_string())
            .collect()
    }

    /// What this build can do for each toolkit it knows.
    ///
    /// Describes the compiled binary, not the user: it needs no session and no
    /// connection. That distinction is the point of it — a UI can tell "you
    /// cannot connect this" apart from "you can connect it, but nothing will
    /// read it yet", which are the same blank row otherwise.
    #[must_use]
    pub fn capabilities(&self) -> ComposioCapabilitiesResponse {
        ComposioCapabilitiesResponse {
            capabilities: self
                .providers
                .values()
                .map(|provider| {
                    let curated = provider.curated_tools().unwrap_or_default();
                    let interval = provider.sync_interval_secs();
                    ComposioCapability {
                        toolkit: provider.toolkit_slug().to_string(),
                        description: provider.description().to_string(),
                        native_provider: true,
                        curated_tools: !curated.is_empty(),
                        curated_tool_count: curated.len(),
                        // Every toolkit can execute: the catalog narrows what
                        // is offered, it does not gate the mechanism.
                        tool_execution: true,
                        user_profile: true,
                        initial_sync: provider.can_sync(),
                        // Periodic sync needs both an interval and something to
                        // read. A write-only toolkit has neither.
                        periodic_sync: provider.can_sync() && interval.is_some(),
                        sync_interval_secs: interval,
                        // Triggers are a backend concern, not a provider one —
                        // the module has no endpoint for a webhook to reach.
                        trigger_webhooks: false,
                        memory_ingest: provider.can_sync(),
                    }
                })
                .collect(),
        }
    }

    /// Whether the registry holds no providers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// How many providers the registry holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }
}
