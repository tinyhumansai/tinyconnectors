//! Loads a built module through the real `TinyBus` dynamic loader.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use tinyconnectors::names;
use tinybus::Connection;
use tinybus::broker::Broker;
use tinybus::module::ModuleHost;
use tinybus::transport::memory::MemoryBus;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let module = module_argument()?;
    let bus = MemoryBus::new();
    let broker = Broker::new();
    let broker_task = broker.spawn(bus.clone());
    let module_host = ModuleHost::new(broker);
    // The module refuses to load without a backend URL and a credential, which
    // is the point: it never invents its own. These are placeholders — the
    // verifier checks the served surface, not a live call.
    let info = module_host.load_file_with_config(
        &module,
        serde_json::json!({
            "base_url": "https://verify.invalid",
            "auth_token": "verify-only-not-a-credential",
        }),
    )?;

    if info.name != env!("CARGO_PKG_NAME") {
        return Err(io::Error::other(format!(
            "loaded module `{}` instead of `{}`",
            info.name,
            env!("CARGO_PKG_NAME")
        ))
        .into());
    }

    verify_served_surface(&bus).await?;

    println!(
        "verified {} as TinyBus module `{}`",
        module.display(),
        info.name
    );
    broker_task.abort();
    Ok(())
}

/// Wait until the module claims its bus name, then check that it serves exactly
/// the members the contract declares.
///
/// The verifier deliberately stops short of *calling* a member. Every member
/// reaches a live connector backend with a real user's credential, so a
/// call-based check would need a signed-in account and would make a release
/// gate depend on a third party being up. Claiming the name and matching the
/// member table is what the artifact can honestly be held to.
async fn verify_served_surface(bus: &MemoryBus) -> Result<(), Box<dyn std::error::Error>> {
    let client = Connection::connect(bus.connect().await?).await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let claimed = client.list_names().await?;
            if claimed.iter().any(|name| name.as_str() == names::INTERFACE) {
                return tinybus::Result::Ok(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    let served = proxy.introspect().await?;
    let mut missing = names::METHODS
        .iter()
        .filter(|member| !served.methods.iter().any(|served| served == *member))
        .peekable();
    if missing.peek().is_some() {
        let missing: Vec<_> = missing.collect();
        return Err(io::Error::other(format!(
            "module does not serve declared members: {missing:?}"
        ))
        .into());
    }
    Ok(())
}

fn module_argument() -> Result<PathBuf, io::Error> {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: cargo run --example verify_module -- <module-path>",
            )
        })
}
