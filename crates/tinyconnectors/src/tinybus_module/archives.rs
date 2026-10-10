//! Module-owned trigger archive leases; closing never deletes persisted data.
use crate::triggers::TriggerArchive;
use std::collections::HashMap;
use std::sync::Mutex;
use tinyconnectors_bus::ArchiveHandle;

#[derive(Debug, Default)]
pub(super) struct Archives(Mutex<HashMap<ArchiveHandle, TriggerArchive>>);
const MAX_ARCHIVES: usize = 64;

impl Archives {
    pub(super) fn open(&self, state_dir: &str) -> tinybus::Result<ArchiveHandle> {
        let mut leases = self
            .0
            .lock()
            .map_err(|_| tinybus::Error::failed("trigger archive state unavailable"))?;
        if leases.len() >= MAX_ARCHIVES {
            return Err(tinybus::Error::failed(
                "trigger archive lease limit reached",
            ));
        }
        let mut entropy = [0_u8; 16];
        getrandom::fill(&mut entropy)
            .map_err(|_| tinybus::Error::failed("trigger archive entropy unavailable"))?;
        let handle = ArchiveHandle(format!("{:032x}", u128::from_le_bytes(entropy)));
        let archive = TriggerArchive::open(std::path::Path::new(state_dir))
            .map_err(|_| tinybus::Error::failed("trigger archive open failed"))?;
        leases.insert(handle.clone(), archive);
        Ok(handle)
    }
    pub(super) fn get(&self, handle: &ArchiveHandle) -> tinybus::Result<TriggerArchive> {
        self.0
            .lock()
            .map_err(|_| tinybus::Error::failed("trigger archive state unavailable"))?
            .get(handle)
            .cloned()
            .ok_or_else(|| tinybus::Error::failed("unknown trigger archive lease"))
    }
    pub(super) fn close(&self, handle: &ArchiveHandle) -> tinybus::Result<()> {
        self.0
            .lock()
            .map_err(|_| tinybus::Error::failed("trigger archive state unavailable"))?
            .remove(handle)
            .map(|_| ())
            .ok_or_else(|| tinybus::Error::failed("unknown trigger archive lease"))
    }
}

#[cfg(test)]
#[path = "archives_tests.rs"]
mod tests;
