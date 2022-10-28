use crate::config::CacheConfig;
use crate::jira_api::{BoardConfiguration, BoardIssues, DevelopmentInfo, Issue, JiraApi};
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, Semaphore};

#[derive(Debug)]
pub struct LocalJiraCache(Arc<LocalJiraCacheInner>);

#[derive(Debug)]
struct LocalJiraCacheInner {
    api: Arc<JiraApi>,
    data: Mutex<HashMap<CacheKey, CacheEntry>>,
    semaphore: Semaphore,
    config: CacheConfig,
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

#[derive(Debug)]
enum CacheEntry {
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
    Miss,
    Loading(Arc<Notify>),
    Hit(Result<T>),
}

impl LocalJiraCache {
    pub fn new(api: JiraApi, parallelism: usize, config: CacheConfig) -> Self {
        let inner = LocalJiraCacheInner {
            api: Arc::new(api),
            semaphore: Semaphore::new(parallelism),
            data: Default::default(),
            config,
        };

        LocalJiraCache(Arc::new(inner))
    }

    pub async fn board_configuration(&self, id: String) -> Result<BoardConfiguration> {
        self.get(
            CacheKey::BoardConfiguration { id: id.clone() },
            Duration::from_secs(self.0.config.ttl_board_configuration_seconds),
            |api| async move { api.board_configuration(&id).await },
        )
        .await
    }

    pub async fn board_issues(
        &self,
        id: String,
        fields: String,
        jql: String,
    ) -> Result<BoardIssues> {
        self.get(
            CacheKey::BoardIssues {
                id: id.to_owned(),
                fields: fields.to_owned(),
                jql: jql.to_owned(),
            },
            Duration::from_secs(self.0.config.ttl_board_issues_seconds),
            |api| async move { api.board_issues(&id, &fields, &jql).await },
        )
        .await
    }

    pub async fn issue(&self, key: String) -> Result<Issue> {
        self.get(
            CacheKey::Issue {
                key: key.to_owned(),
            },
            Duration::from_secs(self.0.config.ttl_issue_seconds),
            |api| async move { api.issue(&key).await },
        )
        .await
    }

    pub async fn epic(&self, key: String) -> Result<Issue> {
        self.get(
            CacheKey::Issue {
                key: key.to_owned(),
            },
            Duration::from_secs(self.0.config.ttl_epic_seconds),
            |api| async move { api.issue(&key).await },
        )
        .await
    }

    pub async fn development_info(&self, issue_id: String) -> Result<DevelopmentInfo> {
        self.get(
            CacheKey::DevelopmentInfo {
                issue_id: issue_id.to_owned(),
            },
            Duration::from_secs(self.0.config.ttl_development_info_seconds),
            |api| async move { api.development_info(&issue_id).await },
        )
        .await
    }

    async fn get<T, G, F>(&self, key: CacheKey, time_to_live: Duration, generate: G) -> Result<T>
    where
        T: Clone + Send + Sync + 'static,
        G: Send + 'static + FnOnce(Arc<JiraApi>) -> F,
        F: Send + Future<Output = Result<T>>,
    {
        loop {
            match self.cache_entry_state::<T>(key.clone()) {
                CacheEntryState::Miss => {
                    let inner = self.0.clone();
                    let task = tokio::spawn(async move {
                        let permit = inner.semaphore.acquire().await.unwrap();
                        let value = generate(inner.api.clone()).await;
                        drop(permit);

                        let boxed_value = CachedBox::new(time_to_live, value);
                        let value = boxed_value.get();

                        let old_entry = inner
                            .data
                            .lock()
                            .insert(key, CacheEntry::Loaded(boxed_value));
                        if let Some(CacheEntry::Loading(notify)) = old_entry {
                            notify.notify_waiters();
                            notify.notify_one();
                        }

                        value
                    });

                    return task.await.unwrap();
                }
                CacheEntryState::Loading(notify) => {
                    notify.notified().await;
                    notify.notify_one();
                }
                CacheEntryState::Hit(value) => return value,
            }
        }
    }

    fn cache_entry_state<T: Clone + 'static>(&self, key: CacheKey) -> CacheEntryState<T> {
        let mut data = self.0.data.lock();
        match data.entry(key) {
            Entry::Vacant(vacant) => {
                let notify = Arc::new(Notify::new());
                vacant.insert(CacheEntry::Loading(notify));
                CacheEntryState::Miss
            }
            Entry::Occupied(mut occupied) => match occupied.get() {
                CacheEntry::Loading(notify) => CacheEntryState::Loading(notify.clone()),
                CacheEntry::Loaded(cached) => {
                    if cached.live_until < Instant::now() {
                        let notify = Arc::new(Notify::new());
                        occupied.insert(CacheEntry::Loading(notify));
                        CacheEntryState::Miss
                    } else {
                        CacheEntryState::Hit(cached.get())
                    }
                }
            },
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
