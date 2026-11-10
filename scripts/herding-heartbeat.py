#!/usr/bin/env python3
"""Herding Supervisor Heartbeat & Liveliness Ping Tool.

Inspects active team members, checks recent pane terminal output for rate limits,
provider usage exhaustion (PTO), or crashes, optionally sends lightweight pings,
and classifies worker health for autonomous supervisor failovers.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from typing import Dict, List, Optional, Tuple

PTO_PATTERNS = [
    (re.compile(r"429\s+Too\s+Many\s+Requests", re.IGNORECASE), "HTTP 429 Too Many Requests"),
    (re.compile(r"RESOURCE_EXHAUSTED", re.IGNORECASE), "API Resource Exhausted (429)"),
    (re.compile(r"quota\s+exceeded", re.IGNORECASE), "API Quota Exceeded"),
    (re.compile(r"usage limit reached", re.IGNORECASE), "Anthropic / Provider usage limit reached"),
    (re.compile(r"you've reached your (?:daily |weekly |5-hour )?usage limit", re.IGNORECASE), "Usage quota reached"),
    (re.compile(r"rate limit (?:exceeded|reached)", re.IGNORECASE), "Rate limit exceeded"),
    (re.compile(r"credit\s+balance\s+is\s+too\s+low", re.IGNORECASE), "Account credit balance exhausted"),
    (re.compile(r"insufficient_quota", re.IGNORECASE), "Insufficient quota"),
    (re.compile(r"out\s+of\s+usage", re.IGNORECASE), "Worker out of usage"),
    (re.compile(r"context\s+(?:window\s+)?exhausted", re.IGNORECASE), "Context window exhausted"),
]

CRASH_PATTERNS = [
    (re.compile(r"connection\s+closed", re.IGNORECASE), "Session connection closed"),
    (re.compile(r"process\s+exited\s+with\s+status\s+[1-9]\d*", re.IGNORECASE), "Process exited with error"),
    (re.compile(r"fatal:\s+out\s+of\s+memory", re.IGNORECASE), "Out of memory crash"),
    (re.compile(r"killed\s+by\s+signal", re.IGNORECASE), "Process terminated by signal"),
]


@dataclass
class MemberHealth:
    member_id: str
    name: str
    role: str
    agent: str
    pane_id: Optional[str]
    agent_status: str
    health: str  # RESPONSIVE, BUSY, PTO, STALLED, DISCONNECTED, UNKNOWN
    assigned_tasks: List[str]
    details: str
    rtt_ms: Optional[float] = None
    suggested_action: Optional[str] = None


def run_command(cmd: List[str], env: Optional[Dict[str, str]] = None) -> Tuple[int, str, str]:
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=full_env,
            timeout=15,
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as e:
        return 1, "", str(e)


def classify_pane_output(output: str) -> Tuple[str, str]:
    """Scan recent pane terminal buffer for PTO or crash signatures."""
    if not output:
        return "HEALTHY", ""

    for pattern, desc in PTO_PATTERNS:
        if pattern.search(output):
            return "PTO", desc

    for pattern, desc in CRASH_PATTERNS:
        if pattern.search(output):
            return "CRASHED", desc

    return "HEALTHY", ""


def get_team_info(gpui_bin: str, team_id: str) -> Tuple[List[dict], List[dict]]:
    """Retrieve team members and active tasks."""
    code, out, _ = run_command([gpui_bin, "team", "roster", team_id, "--json"])
    members = []
    if code == 0:
        try:
            data = json.loads(out)
            members = data.get("members", [])
        except Exception:
            pass

    code, out, _ = run_command([gpui_bin, "team", "task", "list", team_id, "--json"])
    tasks = []
    if code == 0:
        try:
            data = json.loads(out)
            tasks = data.get("tasks", [])
        except Exception:
            pass

    return members, tasks


def get_panes(herdr_bin: str) -> List[dict]:
    """Retrieve running panes from herdr server."""
    code, out, _ = run_command([herdr_bin, "pane", "list"])
    if code == 0:
        try:
            data = json.loads(out)
            return data.get("result", {}).get("panes", [])
        except Exception:
            pass
    return []


def read_pane_output(herdr_bin: str, pane_id: str, lines: int = 30) -> str:
    code, out, _ = run_command([herdr_bin, "pane", "read", pane_id, "--lines", str(lines)])
    return out if code == 0 else ""


def ping_member(gpui_bin: str, team_id: str, member_id: str, timeout_sec: float = 3.0) -> Optional[float]:
    """Send a lightweight ping and check for acknowledgment."""
    token = f"ping-{int(time.time() * 1000)}"
    start = time.time()
    code, out, _ = run_command([
        gpui_bin,
        "team",
        "bulletin",
        "post",
        team_id,
        "--mention",
        member_id,
        f"HEARTBEAT {token}",
    ])
    if code != 0:
        return None

    msg_id = out.replace("Posted", "").strip()
    deadline = start + timeout_sec
    while time.time() < deadline:
        time.sleep(0.5)
        code, out, _ = run_command([gpui_bin, "team", "bulletin", "list", team_id, "--after", "100", "--json"])
        if code == 0:
            try:
                entries = json.loads(out).get("entries", [])
                for entry in entries:
                    if entry.get("id") == msg_id:
                        if entry.get("delivery_state") == "acknowledged":
                            return round((time.time() * 1000) - (start * 1000), 2)
            except Exception:
                pass

    return None


def inspect_team_health(
    gpui_bin: str,
    herdr_bin: str,
    team_id: str,
    do_ping: bool = False,
    ping_timeout: float = 3.0,
) -> List[MemberHealth]:
    members, tasks = get_team_info(gpui_bin, team_id)
    panes = get_panes(herdr_bin)

    pane_by_agent_or_title = {}
    for p in panes:
        pane_id = p.get("pane_id")
        title = p.get("terminal_title", "")
        agent = p.get("agent", "")
        pane_by_agent_or_title[pane_id] = p
        if title:
            pane_by_agent_or_title[title] = p

    # Map in-progress tasks to members
    member_tasks: Dict[str, List[str]] = {}
    for t in tasks:
        if t.get("status") in ("in_progress", "blocked"):
            aid = t.get("assignee_id") or t.get("assignee")
            if aid:
                member_tasks.setdefault(aid, []).append(t.get("id"))

    health_report = []

    for m in members:
        mid = m.get("id")
        name = m.get("name", "unknown")
        role = "supervisor" if m.get("supervisor") else "worker"
        agent = m.get("agent", "unknown")

        # Try to find matching pane
        matched_pane = None
        for p in panes:
            # Check terminal title or agent matches
            title = p.get("terminal_title", "")
            if name in title or mid in title:
                matched_pane = p
                break
        if not matched_pane and panes:
            # Fallback by agent harness match if single
            matching_agent_panes = [p for p in panes if p.get("agent") == agent]
            if len(matching_agent_panes) == 1:
                matched_pane = matching_agent_panes[0]

        pane_id = matched_pane.get("pane_id") if matched_pane else None
        agent_status = matched_pane.get("agent_status", "unknown") if matched_pane else "disconnected"

        assigned = member_tasks.get(mid, []) + member_tasks.get(name, [])

        pane_text = ""
        if pane_id:
            pane_text = read_pane_output(herdr_bin, pane_id, lines=30)

        sig, details = classify_pane_output(pane_text)

        rtt: Optional[float] = None
        suggested: Optional[str] = None

        if sig == "PTO":
            health = "PTO"
            details = f"Provider rate limit / quota exhausted: {details}"
            if assigned:
                suggested = f"Reassign task {assigned[0]} to standby worker or reset to open"
        elif sig == "CRASHED":
            health = "CRASHED"
            details = f"Process failure in pane: {details}"
            if assigned:
                suggested = f"Reset task {assigned[0]} to open and respawn pane"
        elif not matched_pane:
            health = "DISCONNECTED"
            details = "No live Herdr pane detected for this member session"
            if assigned:
                suggested = f"Task {assigned[0]} stalled on disconnected worker; reassign"
        elif agent_status == "working":
            health = "BUSY"
            details = "Actively generating code or executing commands"
        else:
            health = "RESPONSIVE"
            details = "Idle and ready for task assignments"
            if do_ping and role == "worker":
                rtt = ping_member(gpui_bin, team_id, mid, timeout_sec=ping_timeout)
                if rtt is not None:
                    details = f"Responsive (ping RTT: {rtt} ms)"
                else:
                    details = "Responsive (ping timed out, buffer healthy)"

        health_report.append(
            MemberHealth(
                member_id=mid,
                name=name,
                role=role,
                agent=agent,
                pane_id=pane_id,
                agent_status=agent_status,
                health=health,
                assigned_tasks=assigned,
                details=details,
                rtt_ms=rtt,
                suggested_action=suggested,
            )
        )

    return health_report


def print_table(reports: List[MemberHealth]) -> None:
    headers = ["Member", "Role", "Agent", "Pane", "Health", "Tasks", "Details"]
    print(f"{headers[0]:<14} {headers[1]:<11} {headers[2]:<8} {headers[3]:<8} {headers[4]:<12} {headers[5]:<6} {headers[6]}")
    print("-" * 90)
    for r in reports:
        task_str = str(len(r.assigned_tasks))
        print(f"{r.name:<14} {r.role:<11} {r.agent:<8} {str(r.pane_id):<8} {r.health:<12} {task_str:<6} {r.details}")
        if r.suggested_action:
            print(f"  └─> Suggested: {r.suggested_action}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Supervisor liveliness ping & PTO heartbeat detector")
    parser.add_argument("--team", help="Team ID (defaults to HERDR_TEAM_ID or discovers from team status)")
    parser.add_argument("--gpui-bin", default="/home/kchauhan/repos/forks/herding/target/debug/herding", help="Path to the herding binary")
    parser.add_argument("--herdr-bin", default="herdr", help="Path to herdr CLI binary")
    parser.add_argument("--ping", action="store_true", help="Send a lightweight heartbeat ping to idle workers")
    parser.add_argument("--json", action="store_true", help="Output health report as JSON")
    args = parser.parse_args()

    team_id = args.team or os.environ.get("HERDR_TEAM_ID")
    if not team_id:
        # Try discovering from team status
        code, out, _ = run_command([args.gpui_bin, "team", "status", "--json"])
        if code == 0:
            try:
                data = json.loads(out)
                team_id = data.get("team", {}).get("id")
            except Exception:
                pass

    if not team_id:
        print("Error: Could not determine active team ID. Pass --team <ID>.", file=sys.stderr)
        return 1

    reports = inspect_team_health(args.gpui_bin, args.herdr_bin, team_id, do_ping=args.ping)

    if args.json:
        print(json.dumps([asdict(r) for r in reports], indent=2))
    else:
        print(f"Team Heartbeat Report for {team_id}:")
        print_table(reports)

    return 0


if __name__ == "__main__":
    sys.exit(main())
