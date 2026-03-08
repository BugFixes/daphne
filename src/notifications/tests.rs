use super::NotificationRegistry;
use crate::domain::NotificationProvider;

#[test]
fn registry_returns_expected_notification_provider() {
    let registry = NotificationRegistry::default();
    let provider = registry.get(NotificationProvider::Slack).expect("slack");

    assert_eq!(provider.kind(), NotificationProvider::Slack);
}
