#![allow(clippy::unwrap_used)]
use super::*;
use std::ffi::OsString;

fn parse(words: &[&str]) -> TeamCommand {
    let mut args = Args::new(words.iter().map(OsString::from), "team");
    let result = team(&mut args).unwrap();
    args.finish().unwrap();
    result
}

#[test]
fn parses_roster_list_post_and_ack() {
    assert_eq!(
        parse(&["roster", "team-1"]),
        TeamCommand::Roster {
            team_id: "team-1".into()
        }
    );
    assert_eq!(
        parse(&["bulletin", "list", "team-1", "--after", "8"]),
        TeamCommand::BulletinList {
            team_id: "team-1".into(),
            after: Some(8)
        }
    );
    assert_eq!(
        parse(&[
            "bulletin",
            "post",
            "team-1",
            "--reply-to",
            "msg-2",
            "--mention",
            "member-1",
            "please",
            "review"
        ]),
        TeamCommand::BulletinPost {
            team_id: "team-1".into(),
            reply_to: Some("msg-2".into()),
            mentions: vec!["member-1".into()],
            text: "please review".into(),
        }
    );
    assert_eq!(
        parse(&["bulletin", "ack", "team-1", "msg-4"]),
        TeamCommand::BulletinAck {
            team_id: "team-1".into(),
            message_id: "msg-4".into()
        }
    );
    assert_eq!(
        parse(&["register", "team-1", "--name", "builder"]),
        TeamCommand::Register {
            team_id: "team-1".into(),
            name: "builder".into(),
        }
    );
}

#[test]
fn rejects_invalid_bounds_and_missing_text() {
    for words in [
        vec!["roster", ""],
        vec!["bulletin", "list", "team", "--after", "-1"],
        vec!["bulletin", "post", "team", "--mention", "", "body"],
        vec!["bulletin", "post", "team"],
        vec!["register", "team"],
    ] {
        let mut args = Args::new(words.into_iter().map(OsString::from), "team");
        assert!(team(&mut args).is_err());
    }
    let overlong = "x".repeat(MAX_BULLETIN_TEXT_BYTES + 1);
    let mut args = Args::new(
        ["bulletin", "post", "team", &overlong].map(OsString::from),
        "team",
    );
    assert!(team(&mut args).is_err());
}
