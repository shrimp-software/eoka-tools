use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eoka::Page;
use parking_lot::Mutex;

use crate::cloudflare::CloudflareWatcher;
use crate::InteractiveElement;

pub struct Tab {
    pub page: Page,
    elements: Mutex<Vec<InteractiveElement>>,
    console_injected: AtomicBool,
}

impl Tab {
    fn new(page: Page) -> Self {
        Self {
            page,
            elements: Mutex::new(Vec::new()),
            console_injected: AtomicBool::new(false),
        }
    }

    pub fn invalidate(&self) {
        self.elements.lock().clear();
    }

    pub fn elements(&self) -> Vec<InteractiveElement> {
        self.elements.lock().clone()
    }

    pub fn set_elements(&self, elements: Vec<InteractiveElement>) {
        *self.elements.lock() = elements;
    }

    pub fn console_injected(&self) -> bool {
        self.console_injected.load(Ordering::Acquire)
    }

    pub fn mark_console_injected(&self) {
        self.console_injected.store(true, Ordering::Release);
    }
}

#[derive(Clone, Default)]
pub struct Tabs {
    registry: Arc<Mutex<Registry>>,
}

#[derive(Default)]
struct Registry {
    tabs: HashMap<String, Arc<Tab>>,
    cloudflare: Option<CloudflareWatcher>,
}

impl Registry {
    fn publish(&self) {
        if let Some(watcher) = &self.cloudflare {
            watcher.set_pages(self.tabs.iter().map(|(id, tab)| (id, &tab.page)));
        }
    }
}

impl Tabs {
    pub fn insert(&self, id: impl Into<String>, page: Page) -> Arc<Tab> {
        crate::engine::blockers::watch(&page);
        let tab = Arc::new(Tab::new(page));
        let mut registry = self.registry.lock();
        registry.tabs.insert(id.into(), tab.clone());
        registry.publish();
        tab
    }

    pub fn insert_missing(&self, id: impl Into<String>, page: Page) -> Arc<Tab> {
        crate::engine::blockers::watch(&page);
        let mut registry = self.registry.lock();
        let tab = registry
            .tabs
            .entry(id.into())
            .or_insert_with(|| Arc::new(Tab::new(page)))
            .clone();
        registry.publish();
        tab
    }

    pub fn get(&self, id: &str) -> Option<Arc<Tab>> {
        self.registry.lock().tabs.get(id).cloned()
    }

    pub fn page(&self, id: &str) -> Option<Page> {
        self.get(id).map(|tab| tab.page.clone())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.registry.lock().tabs.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.registry.lock().tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn any_id(&self) -> Option<String> {
        self.registry.lock().tabs.keys().next().cloned()
    }

    pub fn entries(&self) -> Vec<(String, Page)> {
        self.registry
            .lock()
            .tabs
            .iter()
            .map(|(id, tab)| (id.clone(), tab.page.clone()))
            .collect()
    }

    pub fn pages(&self) -> Vec<Page> {
        self.registry
            .lock()
            .tabs
            .values()
            .map(|tab| tab.page.clone())
            .collect()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Tab>> {
        let mut registry = self.registry.lock();
        let tab = registry.tabs.remove(id);
        registry.publish();
        tab
    }

    pub fn clear(&self) {
        let mut registry = self.registry.lock();
        registry.tabs.clear();
        registry.publish();
    }

    pub fn watch_cloudflare(&self, interval: Duration, timeout: Duration) {
        let mut registry = self.registry.lock();
        registry.cloudflare = Some(CloudflareWatcher::start(interval, timeout));
        registry.publish();
    }

    pub fn take_cloudflare(&self) -> Option<CloudflareWatcher> {
        self.registry.lock().cloudflare.take()
    }
}
