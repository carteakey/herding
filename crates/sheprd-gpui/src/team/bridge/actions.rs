//! Turning view intents into worker operations: the TeamAction dispatcher,
//! the helpers that read and mutate the selected team behind it, and the
//! pending bookkeeping every queued write waits on. Nothing here blocks;
//! the store work runs on the worker threads behind [`Service`].
use super::*;

impl Bridge {
    pub(super) fn act(&mut self, action: TeamAction, candidates: &[PaneCandidate]) {
        // Whatever this action queues is the confirmation its prompts wait for;
        // an action that queues nothing announces nothing.
        self.action_request = None;
        self.action_refused = false;
        // A pane is only a member if the current daemon snapshot still proves it.
        let live = |pane: &PaneIdentity| {
            pane.verified()
                && candidates
                    .iter()
                    .any(|candidate| candidate.identity == *pane)
        };
        match action {
            TeamAction::SwitchTeamView => {
                self.selected = None;
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.clear_board_cursors();
                self.snapshot_in_flight = None;
                self.warning = None;
                self.dirty = true;
                self.submit(
                    Operation::ListTeams {
                        after: None,
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: false },
                );
            }
            TeamAction::SelectTeam { team_id } => {
                self.selected = Some(team_id.clone());
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.clear_board_cursors();
                self.snapshot_in_flight = None;
                self.warning = None;
                self.undeliverable = 0;
                self.refresh(&team_id);
            }
            TeamAction::LoadEarlierTasks => self.load_earlier_tasks(),
            TeamAction::LoadEarlierFindings => self.load_earlier_findings(),
            TeamAction::LoadMoreTeams { after } => {
                self.submit(
                    Operation::ListTeams {
                        after: Some(after),
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: true },
                );
            }
            TeamAction::CreateTeam { name, supervisor } => {
                let Some(target) = live(&supervisor).then(|| target(&supervisor)).flatten() else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                let team = Team {
                    team_id: uuid::Uuid::new_v4().to_string(),
                    name: name.to_owned(),
                    supervisor: member("Supervisor", target),
                    members: Vec::new(),
                };
                self.selected = Some(team.team_id.clone());
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.warning = None;
                self.undeliverable = 0;
                self.save(team);
            }
            TeamAction::SetGoal { team_id, goal } => {
                let goal = goal.trim().to_owned();
                if goal.is_empty() {
                    return;
                }
                self.warning = None;
                self.submit(
                    Operation::SetGoal {
                        team_id: team_id.clone(),
                        goal,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::RenameTeam { team_id, name } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                self.warning = None;
                team.name = name.to_owned();
                self.last_activity = Some(format!("Renamed team to \"{name}\"").into());
                self.save(team);
            }
            TeamAction::DeleteTeam { team_id } => {
                self.selected = None;
                self.team = None;
                self.goal = None;
                self.posts.clear();
                self.tasks.clear();
                self.findings.clear();
                self.snapshot_in_flight = None;
                self.supervisor_wakes.remove(&team_id);
                self.warning = None;
                self.dirty = true;
                self.submit(
                    Operation::DeleteTeam { team_id },
                    Pending::Teams { append: false },
                );
                self.submit(
                    Operation::ListTeams {
                        after: None,
                        limit: TEAM_PAGE,
                    },
                    Pending::Teams { append: false },
                );
            }
            TeamAction::RenameMember {
                team_id,
                member_id,
                name,
            } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                if roster(&team)
                    .any(|m| m.member_id != member_id && m.display_name.eq_ignore_ascii_case(name))
                {
                    self.warning = Some(fl!("team-warning-duplicate-name", name = name).into());
                    self.dirty = true;
                    return;
                }
                if team.supervisor.member_id == member_id {
                    team.supervisor.display_name = name.to_owned();
                } else if let Some(worker) =
                    team.members.iter_mut().find(|m| m.member_id == member_id)
                {
                    worker.display_name = name.to_owned();
                } else {
                    return;
                }
                self.warning = None;
                self.last_activity = Some(format!("Renamed agent to \"{name}\"").into());
                self.save(team);
            }
            TeamAction::RemoveMember { team_id, member_id } => {
                self.warning = None;
                self.last_activity = Some("Removed worker from team".into());
                self.submit(
                    Operation::RemoveMember {
                        team_id: team_id.clone(),
                        member_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::AddWorker {
                team_id,
                name,
                pane,
            } => {
                let (Some(mut team), Some(target)) = (self.current(&team_id), target(&pane)) else {
                    return;
                };
                let name = name.trim();
                if !live(&pane) || name.is_empty() || team.members.len() + 1 >= MAX_MEMBERS {
                    return;
                }
                if roster(&team).any(|m| m.display_name.eq_ignore_ascii_case(name)) {
                    self.warning = Some(fl!("team-warning-duplicate-name", name = name).into());
                    self.dirty = true;
                    return;
                }
                if roster(&team).any(|m| {
                    (target.pane.is_some() && m.target.pane == target.pane)
                        || (!target.session.is_empty() && m.target.session == target.session)
                }) {
                    self.warning = Some(fl!("team-warning-duplicate-target").into());
                    self.dirty = true;
                    return;
                }
                self.warning = None;
                team.members.push(member(name, target));
                self.save(team);
            }
            TeamAction::SelectSupervisor { team_id, member_id } => {
                let Some(mut team) = self.current(&team_id) else {
                    return;
                };
                let Some(index) = team.members.iter().position(|m| m.member_id == member_id) else {
                    return;
                };
                self.warning = None;
                std::mem::swap(&mut team.supervisor, &mut team.members[index]);
                self.save(team);
            }
            TeamAction::Post {
                team_id,
                author,
                body,
            } => self.post(&team_id, author, None, body),
            TeamAction::Reply {
                team_id,
                author,
                reply_to,
                body,
            } => self.post(&team_id, author, Some(reply_to), body),
            TeamAction::CreateTask {
                team_id,
                title,
                brief,
                assignee,
            } => {
                let Some(team) = self.current(&team_id) else {
                    return;
                };
                let title = title.trim().to_owned();
                let brief = brief.trim().to_owned();
                if title.is_empty() {
                    return;
                }
                let assignee_id = match assignee.as_deref().map(str::trim).filter(|s| !s.is_empty())
                {
                    Some(raw) => match team_store::resolve_member_ref(&team, raw) {
                        Ok(id) => Some(id),
                        Err(error) => {
                            self.warning = Some(error.to_string().into());
                            self.dirty = true;
                            return;
                        }
                    },
                    None => None,
                };
                self.warning = None;
                self.submit(
                    Operation::CreateTask {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        creator: human(),
                        title,
                        brief,
                        assignee: assignee_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::UpdateTask {
                team_id,
                task_id,
                title,
                brief,
                assignee,
            } => {
                let Some(team) = self.current(&team_id) else {
                    return;
                };
                let title = title.trim().to_owned();
                let brief = brief.trim().to_owned();
                if title.is_empty() {
                    return;
                }
                let assignee_id = match assignee.as_deref().map(str::trim).filter(|s| !s.is_empty())
                {
                    Some(raw) => match team_store::resolve_member_ref(&team, raw) {
                        Ok(id) => Some(id),
                        Err(error) => {
                            self.warning = Some(error.to_string().into());
                            self.dirty = true;
                            return;
                        }
                    },
                    None => None,
                };
                self.warning = None;
                self.last_activity = Some(format!("Updated task \"{title}\"").into());
                self.submit(
                    Operation::UpdateTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                        title,
                        brief,
                        assignee: assignee_id,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::CompleteTask {
                team_id,
                task_id,
                result,
            } => {
                let result = result.trim().to_owned();
                let result = if result.is_empty() {
                    fl!("team-task-completed-default")
                } else {
                    result
                };
                self.warning = None;
                self.submit(
                    Operation::CompleteTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                        result,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::CancelTask { team_id, task_id } => {
                self.warning = None;
                self.submit(
                    Operation::CancelTask {
                        task_id,
                        team_id: team_id.clone(),
                        actor: human(),
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::AddFinding {
                team_id,
                author,
                description,
                evidence,
                confidence,
            } => {
                let description = description.trim().to_owned();
                let evidence = evidence.trim().to_owned();
                if description.is_empty() || evidence.is_empty() {
                    return;
                }
                self.warning = None;
                self.submit(
                    Operation::AddFinding {
                        finding_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        author,
                        task_id: None,
                        description,
                        evidence,
                        confidence,
                        supersedes_finding_id: None,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::ToggleBulletinSidebar => {
                self.bulletin_closed = !self.bulletin_closed;
                self.dirty = true;
            }
            TeamAction::TogglePauseTeam => {
                if let Some(team_id) = self
                    .team
                    .as_ref()
                    .map(|t| t.team_id.clone())
                    .or_else(|| self.selected.clone())
                {
                    if self.selected.is_none() {
                        self.selected = Some(team_id.clone());
                    }
                    let paused = if self.paused_teams.contains(&team_id) {
                        self.paused_teams.remove(&team_id);
                        false
                    } else {
                        self.paused_teams.insert(team_id);
                        true
                    };
                    self.last_activity = Some(
                        if paused {
                            "Paused team prompt delivery"
                        } else {
                            "Resumed team prompt delivery"
                        }
                        .into(),
                    );
                    self.dirty = true;
                }
            }
            TeamAction::TogglePauseMember { member_id } => {
                let paused = if self.paused_members.remove(&member_id) {
                    false
                } else {
                    self.paused_members.insert(member_id.clone());
                    true
                };
                self.last_activity = Some(
                    if paused {
                        "Paused agent prompt delivery"
                    } else {
                        "Resumed agent prompt delivery"
                    }
                    .into(),
                );
                self.dirty = true;
            }
            TeamAction::ImportGithubIssue {
                team_id,
                issue_ref,
                as_goal,
            } => {
                let issue_ref = issue_ref.trim().to_owned();
                if issue_ref.is_empty() {
                    return;
                }
                self.warning = None;
                self.last_activity = Some(format!("Imported GitHub issue {issue_ref}").into());
                self.submit(
                    Operation::ImportGithubIssue {
                        task_id: uuid::Uuid::new_v4().to_string(),
                        team_id: team_id.clone(),
                        creator: human(),
                        issue_ref,
                        as_goal,
                    },
                    Pending::Write { team_id },
                );
            }
            TeamAction::DispatchTask { .. }
            | TeamAction::NudgeMember { .. }
            | TeamAction::SpawnAgentSession { .. }
            | TeamAction::SplitNewPane
            | TeamAction::FocusWorker { .. } => {}
        }
    }

    fn current(&self, team_id: &str) -> Option<Team> {
        self.team.clone().filter(|team| team.team_id == team_id)
    }

    fn post(&mut self, team_id: &str, author: Author, reply_to: Option<String>, body: String) {
        let Some(team) = self.current(team_id) else {
            return;
        };
        let body = body.trim().to_owned();
        if body.is_empty() {
            return;
        }
        let recipients = match team_store::resolve_recipients(&team, &author, &[], &body) {
            Ok(recipients) => recipients,
            Err(error) => {
                self.warning = Some(error.to_string().into());
                self.dirty = true;
                return;
            }
        };
        self.warning = None;
        self.submit(
            Operation::AddPost {
                post_id: uuid::Uuid::new_v4().to_string(),
                team_id: team_id.to_owned(),
                author,
                reply_to,
                body,
                recipients,
            },
            Pending::Write {
                team_id: team_id.to_owned(),
            },
        );
    }
}
