use crate::datatypes::LabelType;
use crate::AppError;
use proton_core_common::paginator::{DataSource, Paginator};
use stash::params;
use stash::stash::StashError;
use stash::{orm::Model, stash::Tether};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::models::Label;

/// Gets a watcher for move_to actions. This works for both messages and conversations.
pub async fn watch_available_move_to_actions(
    sender: flume::Sender<()>,
    tether: &Tether,
) -> Result<(), AppError> {
    let (tx, rx) = flume::unbounded();
    _ = Label::find(
        "WHERE label_type = ?",
        params![LabelType::Folder],
        tether,
        Some(tx),
    )
    .await?;

    tokio::spawn(async move {
        while rx.recv_async().await.is_ok() {
            if sender.send(()).is_err() {
                return;
            };
        }
    });

    Ok(())
}

/// Compatibility layer to map new behavior over old paginator code.
///
/// The new behavior expects all the pages to be loaded via `next_page()`
/// but in the older versions this does not happen in the first page.
///
// TODO: Remove when caching is completely implemented.
pub struct PaginatorCompat<T: Model, R: DataSource<Item = T> + 'static> {
    is_first_page: AtomicBool,
    paginator: Paginator<T, R>,
}

impl<T: Model, R: DataSource<Item = T> + 'static> PaginatorCompat<T, R> {
    pub fn new(paginator: Paginator<T, R>) -> Self {
        Self {
            paginator,
            is_first_page: AtomicBool::new(true),
        }
    }

    /// See [`Paginate::next_page`] for more details.
    pub async fn next_page(&self) -> Result<Vec<T>, R::Error> {
        // If it's the first time we are calling this we want the
        // current page. Otherwise we call `next_page`.
        if self.is_first_page.load(Ordering::Acquire) {
            let items = self.paginator.current_page().await?;
            self.is_first_page.store(false, Ordering::Release);
            Ok(items)
        } else {
            self.paginator.next_page().await
        }
    }

    /// See [`Paginate::result_count`] for more details.
    #[inline]
    pub async fn result_count(&self) -> u32 {
        self.paginator.result_count().await
    }

    /// See [`Paginate::has_next_page`] for more details.
    #[inline]
    pub async fn has_next_page(&self) -> bool {
        self.paginator.has_next_page().await
    }

    /// See [`Paginate::reload`] for more details.
    #[inline]
    pub async fn reload(&self) -> Result<Vec<T>, StashError> {
        self.paginator.reload().await
    }
}

/// Filter options for pagination
#[derive(Clone, Debug, Default)]
pub struct PaginatorFilter {
    /// If true, only return unread conversations/messages
    pub unread: Option<bool>,
}

/// Search options for pagination
#[derive(Clone, Debug, Default)]
pub struct PaginatorSearchOptions {
    /// Keywords to use in search.
    pub keywords: Option<String>,
}
