use std::sync::OnceLock;
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinError;

#[cfg(target_os = "android")]
mod jni;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get the async runtime.
///
/// # Using both [`async_runtime`] and [`async_runtime_slim`]
///
/// Both functions are competing for initializing common runtime. It means, that whichever function
/// is called first, it's configuring the runtime, and all other following calls are only
/// returning already initialized runtime.
///
/// ## Examples
///
/// ```ignore
/// let runtime1 = async_runtime();
/// let runtime2 = async_runtime_slim();
/// // This runtime2 is a static reference to FULL runtime using all possible cores,
/// // because `async_runtime()` was called first
/// ```
///
/// ```ignore
/// let runtime1 = async_runtime_slim();
/// let runtime2 = async_runtime();
/// // This runtime2 is a static reference to SLIM runtime using limited number of cores,
/// // because `async_runtime_slim()` was called first
/// ```
///
#[must_use]
pub fn async_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .on_thread_start(on_thread_start)
            .build()
            .expect("build should succeed")
    })
}

/// Get slimmer version of the async runtime.
///
/// Comparing to [`async_runtime`] this takes very limited number of threads.
/// It is to enable Rust SDK in apps with limited amount of memory.
///
/// # Using both [`async_runtime`] and [`async_runtime_slim`]
///
/// Both functions are competing for initializing common runtime. It means, that whichever function
/// is called first, it's configuring the runtime, and all other following calls are only
/// returning already initialized runtime.
///
/// ## Examples
///
/// ```ignore
/// let runtime1 = async_runtime();
/// let runtime2 = async_runtime_slim();
/// // This runtime2 is a static reference to FULL runtime using all possible cores,
/// // because `async_runtime()` was called first
/// ```
///
/// ```ignore
/// let runtime1 = async_runtime_slim();
/// let runtime2 = async_runtime();
/// // This runtime2 is a static reference to SLIM runtime using limited number of cores,
/// // because `async_runtime_slim()` was called first
/// ```
///
#[must_use]
pub fn async_runtime_slim() -> &'static Runtime {
    // Those numbers are arbitrary
    //
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(4)
            .max_blocking_threads(4)
            .enable_io()
            .enable_time()
            .on_thread_start(on_thread_start)
            .build()
            .expect("build should succeed")
    })
}

/// Run an async function on the Tokio runtime.
pub async fn uniffi_async<T, E, F>(future: F) -> Result<T, E>
where
    E: Send + From<JoinError> + 'static,
    T: Send + 'static,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    let handle = async_runtime().spawn(future);
    handle.await?
}

/// Tasks to perform when a runtime thread first starts.
fn on_thread_start() {
    #[cfg(target_os = "android")]
    jni::register_thread_with_vm();
}
