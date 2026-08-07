//! 事件总线实现，基于 tokio::sync::broadcast

use crate::event_bus::event::CoreEvent;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::broadcast;

/// 事件总线，支持多消费者订阅和发布
pub struct EventBus {
    sender: broadcast::Sender<CoreEvent>,
    subscriber_count: Arc<RwLock<usize>>,
}

impl EventBus {
    /// 创建新的事件总线，指定缓冲区大小
    pub fn new(buffer_size: usize) -> Self {
        let (sender, _) = broadcast::channel(buffer_size);
        Self {
            sender,
            subscriber_count: Arc::new(RwLock::new(0)),
        }
    }

    /// 创建默认缓冲区大小（1024）的事件总线
    pub fn default() -> Self {
        Self::new(1024)
    }

    /// 发布事件到所有订阅者
    pub fn publish(&self, event: CoreEvent) {
        // 忽略发送错误（无订阅者时）
        let _ = self.sender.send(event);
    }

    /// 订阅事件总线，返回一个接收器
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        {
            let mut count = self.subscriber_count.write();
            *count += 1;
        }
        self.sender.subscribe()
    }

    /// 获取当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        *self.subscriber_count.read()
    }

    /// 订阅特定类型的事件
    /// 返回一个 tokio 任务，处理匹配的事件
    pub fn subscribe_filtered<F>(&self, filter: F, handler: Box<dyn Fn(CoreEvent) + Send + Sync>)
    where
        F: Fn(&CoreEvent) -> bool + Send + Sync + 'static,
    {
        let mut rx = self.subscribe();
        let filter = Arc::new(filter);
        let handler = Arc::new(handler);

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if filter(&event) {
                    handler(event);
                }
            }
        });
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            subscriber_count: Arc::clone(&self.subscriber_count),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(CoreEvent::PluginLoaded {
            plugin_id: "test-plugin".to_string(),
        });

        let event = rx.recv().await.unwrap();
        match event {
            CoreEvent::PluginLoaded { plugin_id } => {
                assert_eq!(plugin_id, "test-plugin");
            }
            _ => panic!("Unexpected event"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(CoreEvent::TaskCreated {
            task_id: "task-1".to_string(),
            title: "Test Task".to_string(),
        });

        let event1 = rx1.recv().await.unwrap();
        let event2 = rx2.recv().await.unwrap();

        assert!(matches!(event1, CoreEvent::TaskCreated { .. }));
        assert!(matches!(event2, CoreEvent::TaskCreated { .. }));
    }

    #[tokio::test]
    async fn test_filtered_subscription() {
        let bus = EventBus::new(16);
        let received = Arc::new(RwLock::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        bus.subscribe_filtered(
            |event| matches!(event, CoreEvent::DocumentChanged { .. }),
            Box::new(move |event| {
                let mut buf = received_clone.write();
                buf.push(format!("{:?}", event));
            }),
        );

        bus.publish(CoreEvent::PluginLoaded {
            plugin_id: "test".to_string(),
        });
        bus.publish(CoreEvent::DocumentChanged {
            doc_id: "doc-1".to_string(),
            change_summary: crate::event_bus::event::DocumentChangeSummary {
                doc_id: "doc-1".to_string(),
                changed_blocks: vec![],
            },
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let received = received.read();
        assert_eq!(received.len(), 1);
    }
}
