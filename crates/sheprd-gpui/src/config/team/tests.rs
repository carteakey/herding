use super::*;

#[test]
fn the_stall_timeout_never_drops_below_the_noise_floor() {
    assert_eq!(
        TeamConfig {
            stall_timeout_seconds: 0,
            stall_pane_list_seconds: 0,
        }
        .stall_timeout(),
        std::time::Duration::from_secs(MIN_STALL_TIMEOUT_SECONDS)
    );
    assert_eq!(
        TeamConfig {
            stall_timeout_seconds: 240,
            stall_pane_list_seconds: 0,
        }
        .stall_timeout(),
        std::time::Duration::from_secs(240)
    );
    assert_eq!(
        TeamConfig::default().stall_timeout(),
        crate::team::bridge::prompts::stall::DEFAULT_STALL_TIMEOUT
    );
}

#[test]
fn the_pane_list_poll_never_drops_below_its_floor() {
    assert_eq!(
        TeamConfig {
            stall_timeout_seconds: 0,
            stall_pane_list_seconds: 0,
        }
        .pane_list_poll(),
        std::time::Duration::from_secs(5),
    );
    assert_eq!(
        TeamConfig {
            stall_timeout_seconds: 0,
            stall_pane_list_seconds: 90,
        }
        .pane_list_poll(),
        std::time::Duration::from_secs(90),
    );
    assert_eq!(
        TeamConfig::default().pane_list_poll(),
        std::time::Duration::from_secs(10)
    );
}
