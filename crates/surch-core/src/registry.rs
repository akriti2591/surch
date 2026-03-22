use crate::channel::Channel;
use std::sync::Arc;

/// Manages registered channels and tracks which one is active.
pub struct ChannelRegistry {
    channels: Vec<Arc<dyn Channel>>,
    active_index: usize,
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            active_index: 0,
        }
    }

    pub fn register(&mut self, channel: Arc<dyn Channel>) {
        self.channels.push(channel);
    }

    pub fn active(&self) -> Option<&Arc<dyn Channel>> {
        self.channels.get(self.active_index)
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.channels.len() {
            self.active_index = index;
        }
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn channels(&self) -> &[Arc<dyn Channel>] {
        &self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::*;
    use crossbeam_channel::Sender;

    /// A mock channel for testing the registry.
    struct MockChannel {
        id: String,
    }

    impl MockChannel {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl Channel for MockChannel {
        fn metadata(&self) -> ChannelMetadata {
            ChannelMetadata {
                id: self.id.clone(),
                name: format!("Mock {}", self.id),
                icon: "search".to_string(),
                description: "A mock channel".to_string(),
            }
        }

        fn input_fields(&self) -> Vec<InputFieldSpec> {
            vec![]
        }

        fn search(&self, _query: ChannelQuery, _tx: Sender<SearchEvent>) {}

        fn cancel(&self) {}

        fn preview(&self, _entry: &ResultEntry) -> PreviewContent {
            PreviewContent::None
        }

        fn actions(&self, _entry: &ResultEntry) -> Vec<ChannelAction> {
            vec![]
        }

        fn execute_action(&self, _action_id: &str, _entry: &ResultEntry) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = ChannelRegistry::new();
        assert!(registry.channels().is_empty());
        assert_eq!(registry.active_index(), 0);
        assert!(registry.active().is_none());
    }

    #[test]
    fn test_register_single_channel() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("file_search")));

        assert_eq!(registry.channels().len(), 1);
        assert!(registry.active().is_some());
        assert_eq!(registry.active().unwrap().metadata().id, "file_search");
    }

    #[test]
    fn test_register_multiple_channels() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("files")));
        registry.register(Arc::new(MockChannel::new("git")));
        registry.register(Arc::new(MockChannel::new("k8s")));

        assert_eq!(registry.channels().len(), 3);
        // Default active is index 0
        assert_eq!(registry.active().unwrap().metadata().id, "files");
    }

    #[test]
    fn test_set_active_valid_index() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("files")));
        registry.register(Arc::new(MockChannel::new("git")));

        registry.set_active(1);
        assert_eq!(registry.active_index(), 1);
        assert_eq!(registry.active().unwrap().metadata().id, "git");
    }

    #[test]
    fn test_set_active_invalid_index_is_ignored() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("files")));

        registry.set_active(99);
        // Should stay at 0, not change
        assert_eq!(registry.active_index(), 0);
        assert_eq!(registry.active().unwrap().metadata().id, "files");
    }

    #[test]
    fn test_set_active_out_of_bounds_empty_registry() {
        let mut registry = ChannelRegistry::new();
        registry.set_active(0);
        assert!(registry.active().is_none());
    }

    #[test]
    fn test_active_index_default() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("a")));
        registry.register(Arc::new(MockChannel::new("b")));
        assert_eq!(registry.active_index(), 0);
    }

    #[test]
    fn test_switching_back_and_forth() {
        let mut registry = ChannelRegistry::new();
        registry.register(Arc::new(MockChannel::new("a")));
        registry.register(Arc::new(MockChannel::new("b")));
        registry.register(Arc::new(MockChannel::new("c")));

        registry.set_active(2);
        assert_eq!(registry.active().unwrap().metadata().id, "c");

        registry.set_active(0);
        assert_eq!(registry.active().unwrap().metadata().id, "a");

        registry.set_active(1);
        assert_eq!(registry.active().unwrap().metadata().id, "b");
    }
}
