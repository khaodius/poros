//! Where background tasks run. The app spawns them on Tauri's runtime, which exists before any
//! async context does, so managers can be built in the setup hook. The headless command line
//! builds its own Tokio runtime and spawns onto that; outside any runtime, as in synchronous
//! tests, a shared one is started on first use, as Tauri does.

use std::future::Future;

#[cfg(not(feature = "gui"))]
fn handle() -> tokio::runtime::Handle {
    use std::sync::OnceLock;
    static SHARED: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    tokio::runtime::Handle::try_current().unwrap_or_else(|_| {
        SHARED
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("a Tokio runtime for background tasks")
            })
            .handle()
            .clone()
    })
}

pub fn spawn<F>(future: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[cfg(feature = "gui")]
    tauri::async_runtime::spawn(future);
    #[cfg(not(feature = "gui"))]
    handle().spawn(future);
}

pub fn spawn_blocking<F, R>(work: F)
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    #[cfg(feature = "gui")]
    tauri::async_runtime::spawn_blocking(work);
    #[cfg(not(feature = "gui"))]
    handle().spawn_blocking(work);
}
