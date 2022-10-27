use crate::jira_api::{BoardConfiguration, BoardIssues, DevelopmentInfo, Issue, JiraApi};
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::future::Future;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;

#[derive(Debug)]
pub struct LocalJiraCache {
    api: JiraApi,
    data: Mutex<HashMap<CacheKey, CacheEntry>>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum CacheKey {
    BoardConfiguration {
        id: String,
    },
    BoardIssues {
        id: String,
        fields: String,
        jql: String,
    },
    Issue {
        key: String,
    },
    DevelopmentInfo {
        issue_id: String,
    },
}

#[derive(Debug, Clone)]
struct CacheEntry(Arc<Mutex<CacheEntryInner>>);

#[derive(Debug)]
enum CacheEntryInner {
    Loading(Arc<Notify>),
    Loaded(CachedBox),
}

#[derive(Debug)]
struct CachedBox {
    live_until: Instant,
    value: Result<Box<dyn Any + Send + Sync + 'static>>,
}

#[derive(Debug)]
enum CacheEntryState<T> {
    Loading(Arc<Notify>),
    Dead,
    Loaded(Result<T>),
}

impl LocalJiraCache {
    pub fn new(api: JiraApi) -> Self {
        LocalJiraCache {
            api,
            data: Default::default(),
        }
    }

    pub async fn board_configuration(&self, id: &str) -> Result<BoardConfiguration> {
        self.get(
            CacheKey::BoardConfiguration { id: id.to_owned() },
            Duration::from_secs(3600),
            || self.api.board_configuration(id),
        )
        .await
    }

    pub async fn board_issues(&self, id: &str, fields: &str, jql: &str) -> Result<BoardIssues> {
        self.get(
            CacheKey::BoardIssues {
                id: id.to_owned(),
                fields: fields.to_owned(),
                jql: jql.to_owned(),
            },
            Duration::from_secs(10),
            || self.api.board_issues(id, fields, jql),
        )
        .await
    }

    pub async fn issue(&self, key: &str) -> Result<Issue> {
        self.get(
            CacheKey::Issue {
                key: key.to_owned(),
            },
            Duration::from_secs(10),
            || self.api.issue(key),
        )
        .await
    }

    pub async fn epic(&self, key: &str) -> Result<Issue> {
        self.get(
            CacheKey::Issue {
                key: key.to_owned(),
            },
            Duration::from_secs(60),
            || self.api.issue(key),
        )
        .await
    }

    pub async fn development_info(&self, issue_id: &str) -> Result<DevelopmentInfo> {
        self.get(
            CacheKey::DevelopmentInfo {
                issue_id: issue_id.to_owned(),
            },
            Duration::from_secs(60),
            || self.api.development_info(issue_id),
        )
        .await
    }

    async fn get<T, G, F>(&self, key: CacheKey, time_to_live: Duration, generate: G) -> Result<T>
    where
        T: Clone + Send + Sync + 'static,
        G: FnOnce() -> F,
        F: Future<Output = Result<T>>,
    {
        let (was_vacant, entry) = self.cache_entry(key);

        if was_vacant {
            entry.settle(time_to_live, generate().await)
        } else {
            loop {
                match entry.state() {
                    CacheEntryState::Loading(notify) => {
                        notify.notified().await;
                        notify.notify_one();
                    }
                    CacheEntryState::Dead => {
                        return entry.settle(time_to_live, generate().await);
                    }
                    CacheEntryState::Loaded(value) => {
                        return value;
                    }
                }
            }
        }
    }

    fn cache_entry(&self, key: CacheKey) -> (bool, CacheEntry) {
        let mut data = self.data.lock();
        match data.entry(key) {
            Entry::Vacant(vacant) => {
                let entry = CacheEntry::loading();
                vacant.insert(entry.clone());
                (true, entry)
            }
            Entry::Occupied(occupied) => (false, occupied.get().clone()),
        }
    }
}

impl CacheEntry {
    fn loading() -> Self {
        let inner = CacheEntryInner::Loading(Arc::new(Notify::new()));
        CacheEntry(Arc::new(Mutex::new(inner)))
    }

    fn settle<T: Send + Sync + Clone + 'static>(
        &self,
        time_to_live: Duration,
        cached: Result<T>,
    ) -> Result<T> {
        let cached_box = CachedBox::new(time_to_live, cached);
        let cached = cached_box.get();
        let new_inner = CacheEntryInner::Loaded(cached_box);
        let old_inner = mem::replace(&mut *self.0.lock(), new_inner);

        if let CacheEntryInner::Loading(notify) = old_inner {
            notify.notify_waiters();
            notify.notify_one();
        }

        cached
    }

    fn state<T: Clone + Send + Sync + 'static>(&self) -> CacheEntryState<T> {
        let mut inner = self.0.lock();
        match &*inner {
            CacheEntryInner::Loading(notify) => CacheEntryState::Loading(notify.clone()),
            CacheEntryInner::Loaded(cached) => {
                if cached.live_until < Instant::now() {
                    *inner = CacheEntryInner::Loading(Arc::new(Notify::new()));
                    CacheEntryState::Dead
                } else {
                    CacheEntryState::Loaded(cached.get())
                }
            }
        }
    }
}

impl CachedBox {
    fn new<T: Send + Sync + 'static>(time_to_live: Duration, cached: Result<T>) -> CachedBox {
        CachedBox {
            live_until: Instant::now() + time_to_live,
            value: cached.map(|value| Box::new(value) as Box<dyn Any + Send + Sync>),
        }
    }

    fn get<T: Clone + 'static>(&self) -> Result<T> {
        let value = match &self.value {
            Err(error) => Err(anyhow!("{:?}", error)),
            Ok(boxed_value) => boxed_value
                .downcast_ref::<T>()
                .context("failed to downcast to desired type")
                .cloned(),
        };

        value
    }
}
