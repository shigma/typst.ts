//! Task are stateless actors that staring computing tasks.
//! `SyncTaskFactory` can hold *mutable* configuration but the mutations don't
//! blocking the computation, i.e. the mutations are non-blocking.

pub use tinymist_task::*;

#[cfg(feature = "system-watch")]
mod cache;
#[cfg(feature = "system-watch")]
pub use cache::*;
#[cfg(feature = "system-watch")]
mod base {

    use std::{ops::DerefMut, pin::Pin, sync::Arc};

    use futures::Future;
    use parking_lot::Mutex;
    use rayon::Scope;
    use reflexo::error::prelude::*;

    /// Please uses this if you believe all mutations are fast
    #[derive(Clone, Default)]
    pub(crate) struct SyncTaskFactory<T>(Arc<std::sync::RwLock<Arc<T>>>);

    impl<T> SyncTaskFactory<T> {
        pub fn new(config: T) -> Self {
            Self(Arc::new(std::sync::RwLock::new(Arc::new(config))))
        }
    }

    impl<T: Clone> SyncTaskFactory<T> {
        pub fn task(&self) -> Arc<T> {
            self.0.read().unwrap().clone()
        }
    }

    type FoldFuture = Pin<Box<dyn Future<Output = Option<()>> + Send + Sync>>;

    #[derive(Default)]
    pub(crate) struct FoldingState {
        running: bool,
        task: Option<(usize, FoldFuture)>,
    }

    #[derive(Clone, Default)]
    pub(crate) struct FutureFolder {
        state: Arc<Mutex<FoldingState>>,
    }

    impl FutureFolder {
        pub async fn compute<'scope, OP, R: Send + 'static>(op: OP) -> Result<R>
        where
            OP: FnOnce(&Scope<'scope>) -> R + Send + 'static,
        {
            tokio::task::spawn_blocking(move || -> R { rayon::in_place_scope(op) })
                .await
                .map_err(|e| error_once!("compute error", err: e))
        }

        pub fn spawn(&self, revision: usize, fut: impl FnOnce() -> FoldFuture) {
            let mut state = self.state.lock();
            let state = state.deref_mut();

            match &mut state.task {
                Some((prev_revision, prev)) => {
                    if *prev_revision < revision {
                        *prev = fut();
                        *prev_revision = revision;
                    }

                    return;
                }
                next_update => {
                    *next_update = Some((revision, fut()));
                }
            }

            if state.running {
                return;
            }

            state.running = true;

            let state = self.state.clone();
            tokio::spawn(async move {
                loop {
                    let fut = {
                        let mut state = state.lock();
                        let Some((_, fut)) = state.task.take() else {
                            state.running = false;
                            return;
                        };
                        fut
                    };
                    fut.await;
                }
            });
        }
    }
}

#[cfg(feature = "system-watch")]
pub(crate) use base::*;
