//! Archive leases are bounded, explicit and reject unavailable resources.
use super::*;

#[test]
fn lease_limit_releases_capacity_and_open_faults_hide_paths() -> tinybus::Result<()> {
    let dir = super::super::test::TempDir::new("lease-capacity");
    let archives = Archives::default();
    let path = dir.0.to_string_lossy();
    let mut handles = Vec::new();
    for _ in 0..MAX_ARCHIVES {
        handles.push(archives.open(&path)?);
    }
    assert!(archives.open(&path).is_err());
    archives.close(&handles[0])?;
    assert!(archives.open(&path).is_ok());
    let unavailable = Archives::default();
    let file = dir.0.join("private-file");
    std::fs::write(&file, "fixture").map_err(|_| tinybus::Error::failed("fixture write"))?;
    let error = unavailable
        .open(&file.to_string_lossy())
        .err()
        .ok_or_else(|| tinybus::Error::failed("expected fault"))?;
    assert!(!error.to_string().contains("private-file"));
    Ok(())
}

#[test]
fn poisoned_archive_state_fails_all_operations_without_panicking() {
    let archives = Archives::default();
    let _ = std::panic::catch_unwind(|| {
        let _guard = archives.0.lock();
        // A caught fixture panic models a failed previous owner; production
        // refuses the poisoned state instead of recovering corrupted leases.
        std::panic::resume_unwind(Box::new("fixture poison"));
    });
    let handle = ArchiveHandle("unknown".into());
    assert!(archives.open("unused").is_err());
    assert!(archives.get(&handle).is_err());
    assert!(archives.close(&handle).is_err());
}
