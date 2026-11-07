//! Nonblocking, project-owned cache. The guest receives symbols, never archive access.
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

const TTL: Duration = Duration::from_secs(2);
#[derive(Default)]
pub(crate) struct Cache {
    entries: HashMap<(PathBuf, String), Entry>,
}
struct Entry {
    at: Instant,
    value: Option<String>,
    locations: HashSet<String>,
    pending: bool,
    cancel: CancellationToken,
}
impl Cache {
    pub fn clear(&mut self) {
        for entry in self.entries.values() {
            entry.cancel.cancel();
        }
        self.entries.clear();
    }
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.clear();
    }
}

impl LspManager {
    /// Only exact virtual targets already returned by this project's JDT model.
    pub fn spring_beans_location_allowed(&self, uri: &str) -> bool {
        self.execution_allowed.load(Ordering::Acquire)
            && !self.shutting_down.load(Ordering::Acquire)
            && uri.starts_with("jdt:")
            && self.spring_beans_cache.lock().entries.values().any(|e| e.locations.contains(uri))
    }
    /// Returns immediately. JDT resolves the selected module's classpath off the provider thread.
    pub fn spring_beans_snapshot(&self, path: &Path, context: &str) -> Option<String> {
        if self.shutting_down.load(Ordering::Acquire) || !self.execution_allowed.load(Ordering::Acquire) {
            return None;
        }
        let key = (path.to_path_buf(), context.to_owned());
        let mut cache = self.spring_beans_cache.lock();
        if let Some(entry) = cache.entries.get(&key) {
            if entry.at.elapsed() < TTL {
                return entry.value.clone();
            }
            if entry.pending {
                return entry.value.clone();
            }
        }
        if cache.entries.len() >= 8 && !cache.entries.contains_key(&key) {
            let oldest = cache.entries.iter().min_by_key(|(_, e)| e.at).map(|(k, _)| k.clone());
            if let Some(oldest) = oldest
                && let Some(entry) = cache.entries.remove(&oldest)
            {
                entry.cancel.cancel();
            }
        }
        let previous = cache.entries.remove(&key);
        let previous_value = previous.as_ref().and_then(|e| e.value.clone());
        let previous_locations = previous.map(|e| e.locations).unwrap_or_default();
        let cancel = CancellationToken::new();
        let started = Instant::now();
        cache.entries.insert(
            key.clone(),
            Entry {
                at: started,
                value: previous_value.clone(),
                locations: previous_locations,
                pending: true,
                cancel: cancel.clone(),
            },
        );
        let weak = self.me.clone();
        self.runtime.spawn(async move {
            let Some(manager) = weak.upgrade() else { return };
            let result = tokio::time::timeout(
                Duration::from_secs(90),
                manager.jdtls_project_command(
                    &key.0,
                    "openidea.java.spring.beans",
                    vec![json!(crate::path_to_uri_string(&key.0)), json!(key.1)],
                    &cancel,
                ),
            )
            .await;
            let value = match result {
                Ok(Ok(mut model)) => {
                    // Use the same virtual URI transport as ordinary JDT navigation.
                    for group in ["beans", "injections"] {
                        if let Some(records) = model[group].as_array_mut() {
                            for record in records {
                                if let Some(uri) = record["location"]["uri"].as_str().map(str::to_owned) {
                                    let (path, _) = crate::uri_target(&uri);
                                    record["location"]["path"] = json!(ide_core::ipc_path::to_ipc(&path));
                                }
                            }
                        }
                    }
                    serde_json::to_string(&model).ok().filter(|s| s.len() <= 8 * 1024 * 1024)
                }
                _ => None,
            };
            cancel.cancel();
            let locations = value
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .and_then(|v| v["beans"].as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|b| b["location"]["path"].as_str())
                .filter(|s| s.starts_with("jdt:"))
                .map(str::to_owned)
                .collect();
            let mut changed = false;
            let mut cache = manager.spring_beans_cache.lock();
            if let Some(entry) = cache.entries.get_mut(&key)
                && entry.pending
                && entry.at == started
                && !manager.shutting_down.load(Ordering::Acquire)
            {
                changed = entry.value != value;
                entry.locations = locations;
                entry.value = value;
                entry.pending = false;
                entry.at = Instant::now();
            }
            drop(cache);
            if changed && let Some(server) = manager.servers.lock().iter().find(|s| s.name == "jdtls") {
                manager.sink.emit(ide_core::IdeEvent::LspRefresh {
                    server: server.id,
                    kind: ide_core::event::LspRefreshKind::CodeLens,
                });
            }
        });
        previous_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_the_cache_cancels_background_work_and_releases_navigation_authority() {
        let cancel = CancellationToken::new();
        let mut cache = Cache::default();
        cache.entries.insert(
            (PathBuf::from("/project/App.java"), String::new()),
            Entry {
                at: Instant::now(),
                value: Some("{}".into()),
                pending: true,
                cancel: cancel.clone(),
                locations: HashSet::from(["jdt://contents/authorized.jar/p/Bean.class?exact".into()]),
            },
        );
        cache.clear();
        assert!(cancel.is_cancelled());
        assert!(cache.entries.is_empty());
    }
}
