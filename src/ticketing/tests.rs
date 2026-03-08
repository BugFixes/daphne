use super::TicketingRegistry;
use crate::domain::TicketProvider;

#[test]
fn registry_returns_expected_ticketing_providers() {
    let registry = TicketingRegistry::default();

    assert_eq!(
        registry.get(TicketProvider::Jira).expect("jira").kind(),
        TicketProvider::Jira
    );
    assert_eq!(
        registry.get(TicketProvider::Github).expect("github").kind(),
        TicketProvider::Github
    );
}
