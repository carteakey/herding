# Herding: projects to study

Collected 2026-11-03 from the projects' public documentation. These are
references for research, not dependencies or claims that the projects have
been installed, tested, or verified against this Herding branch. Recheck
versions, behavior, licenses, and attribution before reusing any code.

## Herdr-Synapse

- **Project:** [vitalysim/herdr-synapse](https://github.com/vitalysim/herdr-synapse)
- **Why it is relevant:** A Herdr plugin for teams of coding agents within one
  Herdr session. Its documentation describes a shared board, leader, mentions,
  task records, and delivery that waits for an agent to be ready.
- **Study first:** How it establishes a member's session identity; how its
  board distinguishes queued, submitted, and read messages; how it handles
  permission prompts and agent availability; and what it does when the daemon
  cannot atomically check readiness and submit input. Check the documented
  harness support and live-verification limits against our actual versions.
- **Current scope:** The documented team is inside one Herdr session. Do not
  infer cross-host team support from the local coordination features.
- **License:** The project identifies its license as Apache-2.0; review the
  exact license and NOTICE before copying or integrating anything.

## Horde

- **Project:** [gethamster/horde](https://github.com/gethamster/horde)
- **Why it is relevant:** Its documentation describes local-first task
  orchestration, agents connecting through CLI or MCP, and worker machines
  that retain their own provider credentials.
- **Study:** Worker enrollment, authentication, capability discovery, session
  ownership, scheduling, offline/reconnect behavior, and how code or artifacts
  move between machines. Determine which guarantees are implemented and tested
  before applying them to Herding's remote-team design.
- **Starting point:** [Connect worker machines](https://github.com/gethamster/horde#connect-worker-machines).

## Fusion

- **Project:** [Runfusion/Fusion](https://github.com/Runfusion/Fusion)
- **Why it is relevant:** An early-preview agent orchestration project that
  describes a shared board and a multi-node mesh, with task state, agents, logs,
  and diffs synchronized across machines.
- **Study:** Node enrollment and trust, state authority and synchronization,
  disconnected operation, conflict handling, per-node credentials, and how
  its workflow and user controls compare with Herding's real agent sessions.
- **Starting point:** [Multi-node overview](https://github.com/Runfusion/Fusion#multi-node-one-board-every-platform).

## Claude Code agent teams

- **Documentation:** [Orchestrate teams of Claude Code sessions](https://code.claude.com/docs/en/agent-teams)
- **Why it is relevant:** Documents a lead, teammates, shared tasks, and
  messaging for coordinating Claude Code sessions.
- **Study:** Team and task lifecycle, delegation and teammate messaging,
  visibility for the human, handoff behavior, and the documented limitations
  around resuming in-process teammates. Distinguish Claude-specific features
  from contracts that work across different agent harnesses.
- **Status:** The documentation labels agent teams experimental; check its
  current limitations rather than assuming future behavior.

## Research questions

1. Can any existing integration safely submit only when the same verified
   session is ready, and what happens if that capability is absent?
2. How are an agent's identity and permissions established without trusting a
   caller-supplied pane ID? How are they revoked and revalidated on reconnect?
3. How do messages and tasks survive a crash or disconnection without silent
   loss, duplicate execution, or pretending submission means acknowledgement?
4. Which coordination features can reuse an existing project without patching
   Herdr, adding an unnecessary service, or compromising the native UI?
5. How do remote workers locate and transfer the correct files and evidence
   while keeping credentials on the host where the agent runs?

Record conclusions with the exact version, source, test environment, and
observed limitations. Use isolated agents and test data; do not change personal
sessions, install plugins, or copy external code merely to answer these
questions.
