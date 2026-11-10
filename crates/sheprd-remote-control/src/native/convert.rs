//! `POST /api/v2/term/{pane}/convert`: moves the agent in a terminal tab
//! into a chat in the same pane, as Herding's Move Agent into Chat does. The
//! phone offers it on the pane a tab names in `convert`, and sends the
//! agent that offer showed.

use crate::{
    hub,
    web::{ApiError, Device, NEW_CHAT_WAIT, Shared, parse_agent},
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

/// What the phone asks to move.
#[derive(Debug, Deserialize)]
pub(crate) struct Convert {
    /// The agent the tab's offer showed, such as `claude`; a pane holding
    /// another one now is refused.
    agent: String,
}

/// An agent that moved into a chat.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Converted {
    /// The agent's name, such as `claude`.
    pub(crate) agent: String,
    /// The chat's pane, the terminal's own, once it is listed and linked;
    /// `None` when it was not in time, and the phone finds it on home.
    pub(crate) pane: Option<String>,
}

/// `POST /api/v2/term/{pane}/convert`. Answers once the line continuing the
/// agent is typed and its chat is up, or the wait for it ran out. A refusal
/// is a `409` with a message for the person, and leaves the agent running.
pub(crate) async fn convert(
    State(state): State<Shared>,
    _device: Device,
    Path(pane): Path<String>,
    Json(request): Json<Convert>,
) -> Result<Json<Converted>, ApiError> {
    let agent = parse_agent(&request.agent)?;
    state
        .hub
        .convert_to_chat(&pane, agent)
        .await
        .map_err(hub::Error::from)?;
    let ready = state.hub.chat_ready(&pane, NEW_CHAT_WAIT).await;
    Ok(Json(Converted {
        agent: agent.name().to_owned(),
        pane: ready.then_some(pane),
    }))
}
