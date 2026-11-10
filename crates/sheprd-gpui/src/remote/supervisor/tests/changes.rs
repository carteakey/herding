use super::*;
use crate::remote::supervisor::changes::{QUEUED, RUNNING};
use sheprd_remote_control::control::{
    changes::{Answer, Ask, AskId, Failure, Wanted},
    layout,
};
use std::sync::Mutex;

static NO_PHONE_WAITS: AtomicUsize = AtomicUsize::new(0);

fn daemon_without_phones(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(0))),
        Request::ChangeAsks => {
            NO_PHONE_WAITS.fetch_add(1, Ordering::SeqCst);
            Ok(Response::ChangeAsks {
                asks: Vec::new(),
                layout: Vec::new(),
            })
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn with_no_phone_connected_nothing_waits_on_the_daemon(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_without_phones));
    for _ in 0..2 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(NO_PHONE_WAITS.load(Ordering::SeqCst), 0);
}

static OLD_DAEMON_WAITS: AtomicUsize = AtomicUsize::new(0);

fn old_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks => {
            OLD_DAEMON_WAITS.fetch_add(1, Ordering::SeqCst);
            Err(control::Error::NoAnswer)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn a_daemon_that_refuses_is_not_asked_again_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), old_daemon));
    // The first tick learns a phone is connected; the next two would wait.
    for _ in 0..3 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(OLD_DAEMON_WAITS.load(Ordering::SeqCst), 1);
}

static ANSWERED: Mutex<Vec<(AskId, Answer)>> = Mutex::new(Vec::new());
static HANDED_OVER: AtomicUsize = AtomicUsize::new(0);

fn daemon_with_a_question(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks if HANDED_OVER.fetch_add(1, Ordering::SeqCst) == 0 => {
            Ok(Response::ChangeAsks {
                asks: vec![Ask {
                    id: AskId(5),
                    boot_id: "another session".into(),
                    workspace_id: "w".into(),
                    wanted: Wanted::Checks,
                }],
                layout: Vec::new(),
            })
        }
        Request::ChangeAsks => Err(control::Error::NoAnswer),
        Request::ChangeAnswer { id, answer } => {
            ANSWERED.lock().unwrap().push((*id, answer.clone()));
            Ok(Response::Done)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn a_question_taken_from_the_daemon_is_answered_by_its_id(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_with_a_question));
    for _ in 0..2 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(
        *ANSWERED.lock().unwrap(),
        [(AskId(5), Answer::Failed(Failure::Gone))]
    );
}

fn ask(id: u64) -> Ask {
    Ask {
        id: AskId(id),
        boot_id: "another session".into(),
        workspace_id: "w".into(),
        wanted: Wanted::Checks,
    }
}

static LIMITED_ANSWERS: Mutex<Vec<AskId>> = Mutex::new(Vec::new());
static LIMITED_WAITS: AtomicUsize = AtomicUsize::new(0);

fn daemon_with_many_questions(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks => {
            LIMITED_WAITS.fetch_add(1, Ordering::SeqCst);
            Err(control::Error::NoAnswer)
        }
        Request::ChangeAnswer { id, .. } => {
            LIMITED_ANSWERS.lock().unwrap().push(*id);
            Ok(Response::Done)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn only_a_few_answers_run_at_once_and_taking_goes_on_meanwhile(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_with_many_questions));
    // Learns a phone is connected, and that the daemon has nothing yet.
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    let waits = LIMITED_WAITS.load(Ordering::SeqCst);
    let asks = (0..RUNNING as u64 + 2).map(ask).collect();
    cx.update(|cx| {
        Supervisor::take_asks(
            cx,
            Ok(Response::ChangeAsks {
                asks,
                layout: Vec::new(),
            }),
        )
    });
    cx.update(|cx| {
        let changes = &cx.global::<Supervisor>().changes;
        assert_eq!(changes.running, RUNNING);
        assert_eq!(changes.queued.len(), 2);
        Supervisor::poll_changes(cx);
    });
    // A split or zoom must not wait behind slow workspace answers.
    cx.run_until_parked();
    assert!(LIMITED_WAITS.load(Ordering::SeqCst) > waits);

    cx.run_until_parked();
    let mut answered = LIMITED_ANSWERS.lock().unwrap().clone();
    answered.sort_by_key(|id| id.0);
    assert_eq!(
        answered,
        (0..RUNNING as u64 + 2).map(AskId).collect::<Vec<_>>()
    );
    cx.update(|cx| {
        let changes = &cx.global::<Supervisor>().changes;
        assert_eq!((changes.running, changes.queued.len()), (0, 0));
    });
}

static LAYOUT_ANSWERS: Mutex<Vec<(AskId, layout::Answer)>> = Mutex::new(Vec::new());

fn daemon_with_a_layout_change(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::LayoutAnswer { id, answer } => {
            LAYOUT_ANSWERS.lock().unwrap().push((*id, answer.clone()));
            Ok(Response::Done)
        }
        _ => Err(control::Error::NoAnswer),
    }
}

#[gpui::test]
fn a_layout_change_no_window_holds_is_answered_as_gone(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_with_a_layout_change));
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    let change = |id, terminal: &str| layout::Ask {
        id: AskId(id),
        tab: 4,
        terminal: terminal.into(),
        op: layout::Op::SplitRight,
    };
    let layout = vec![change(7, "t1"), change(8, "")];
    cx.update(|cx| {
        Supervisor::take_asks(
            cx,
            Ok(Response::ChangeAsks {
                asks: Vec::new(),
                layout,
            }),
        )
    });
    cx.run_until_parked();
    let mut answers = LAYOUT_ANSWERS.lock().unwrap().clone();
    answers.sort_by_key(|(id, _)| id.0);
    assert_eq!(
        answers,
        [
            (AskId(7), layout::Answer::Failed(layout::Failure::Gone)),
            // Out of bounds, it reaches no window.
            (AskId(8), layout::Answer::Failed(layout::Failure::Refused)),
        ]
    );
}

static FULL_WAITS: AtomicUsize = AtomicUsize::new(0);

fn daemon_counting_waits(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks => {
            FULL_WAITS.fetch_add(1, Ordering::SeqCst);
            Err(control::Error::NoAnswer)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn taking_stops_once_the_queue_is_full(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_counting_waits));
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    cx.update(|cx| {
        let changes = &mut cx.global_mut::<Supervisor>().changes;
        changes.running = RUNNING;
        changes.queued = (0..QUEUED as u64).map(ask).collect();
    });
    let waits = FULL_WAITS.load(Ordering::SeqCst);
    cx.update(Supervisor::poll_changes);
    cx.run_until_parked();
    assert_eq!(FULL_WAITS.load(Ordering::SeqCst), waits);
}
