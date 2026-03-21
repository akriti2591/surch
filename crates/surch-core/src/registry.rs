use crate::channel::Channel;
use std::sync::Arc;

/// Manages registered channels and tracks which one is active.
pub struct ChannelRegistry {
    channels: Vec<Arc<dyn Channel>>,
    active_index: usize,
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
