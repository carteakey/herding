//! The service worker opens what a notification links to with no network.

const SERVICE_WORKER: &str = include_str!("../assets/sw.js");

#[test]
fn every_page_a_notification_opens_is_served_from_the_shell_offline() {
    let prefixes = SERVICE_WORKER
        .lines()
        .find_map(|line| line.strip_prefix("const SHELL_PREFIXES = "))
        .expect("the service worker lists its shell prefixes");
    // Chat alerts open /chat/{pane}; terminal agents' alerts open /term/{pane}.
    for page in ["\"/chat/\"", "\"/term/\""] {
        assert!(prefixes.contains(page), "{page} missing from {prefixes}");
    }
}
