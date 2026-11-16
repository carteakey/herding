---
title: Plan usage
blurb: Plan limits of 85+ AI services in the status bar.
status: shipped
platforms: macOS, Linux, Windows
---

# Plan usage

Sheprd shows how much of each AI service's plan you have used, read on the host you are looking at. This page covers the status bar items, the usage panel, how sign-ins are found, the settings, and what is not supported.

## In the status bar

The status bar shows as many signed-in AI services as fit, for the selected host. The service selected in the panel comes first, followed by those closest to their limits. Narrowing the window hides the segments that no longer fit; the panel still lists every service.

Each segment reads `N% used <time to reset>` for the service's two windows closest to their limits, such as a five-hour session and a weekly limit. A service with no limit windows shows its first balance instead, in money or credits.

`[status_bar] usage` picks how much each segment draws:

```toml
[status_bar]
# "detailed" draws a meter and a service's two tightest windows; "compact"
# shows each service's tightest limit as one share.
usage = "detailed"
```

Compact leaves room for more services. Turn the whole display on or off with **Usage** in **Settings > Status bar**, or `[usage] show`.

## The usage panel

Click a service in the status bar to open its panel. It lists every window the service reports, with how long until each resets, plus balances and any other details the service gives, such as the account's email and plan. The panel lists every available service, so you can switch between them there.

The activity bar's **Usage** section shows every host's plan limits, one card per signed-in agent. Only the selected host is read; another host shows what was read while it was selected. Open it from a script with `sheprd ui panel usage`, and print the selected host's usage with `sheprd usage`.

## How sign-ins are found

A service shows when its sign-in is found. There are two kinds.

**The agent's own sign-in on the host.** Codex, Claude, Kiro, Doubao, and Pi are found from their own login with nothing to set. For a remote host the sign-in is read over SSH and never copied off it. Usage follows the selected host, including SSH hosts and, on Windows, WSL distributions. SSH hosts and WSL distributions need `python3` or `jq` to read JSON sign-in files.

**A key or cookie you set on this machine.** Other services read an API key, token, or cookie from `config-gpui.local.toml`, or from the environment variables that service's CLI uses:

```toml
[usage.providers.openrouter]
api_key = "sk-or-…"
```

Apps launched from the Dock or a desktop launcher see few environment variables, so the config file is the dependable place.

### Cookies and browser sessions

Some services have no API for plan usage and are read with your web session. Set its `cookie` value yourself: sign in to the service's site, open Developer Tools > Application > Cookies, and copy the cookies the provider's help names as one `name=value; name2=value2` header. The panel shows that help when the service is listed but not signed in.

Sheprd can also read the cookie from Chrome (and Brave, Edge, Arc, Chromium) or Safari, but only for services you list in `show_providers`, and only after you press **Allow** in that service's usage panel, since Chrome makes macOS ask for Keychain access. Safari needs Full Disk Access.

> **Note.** A service is never read from another app's private data on your Mac without your say. Zed and Muse Code keep their sign-in in the macOS keychain, and are read only after you press **Allow** in their panel; until then they show a lock in the status bar. On the Mac running Herdr, JetBrains AI, Kiro, and Windsurf are read only when listed in `show_providers`.

## Settings

```toml
[usage]
# Also under Settings > Status bar.
show = true
# Daemon toolbar segments, including plugin usage percentages; independent of show.
topbar = true
# Honor the daemon's [ui.sidebar] rows and plugin tokens; false uses native layouts.
inline = true
# Show these even when no sign-in was found, with what to set up.
show_providers = ["cursor", "openrouter"]
# Never show these, even when signed in.
hide_providers = []
# Let providers listed in show_providers read their web session from a browser.
browser_cookies = true
```

- `show_providers` lists services by ID to show even without a sign-in. Their panel then works as a setup guide, naming each value to set and how to get it.
- `hide_providers` hides services you are signed in to but do not want to see.
- `topbar` and `inline` cover usage text that Herdr plugins report, such as [Usage Tracker](https://github.com/VHemanth45/herdr_agents_tracker); see [Appearance](appearance.md#herdr-plugins).

Every service's settings, with the help text for each key and the environment variables it reads, are in the generated `[usage.providers.*]` tables of [`config-gpui.example.toml`](https://github.com/sheprd-labs/sheprd/blob/main/crates/sheprd-gpui/config-gpui.example.toml).

## Which services

Sheprd knows 87 services, ported from [CodexBar](https://github.com/steipete/CodexBar). They include:

- **Coding agents and editors:** Claude, Codex, Copilot, Cursor, Gemini, Antigravity, Amp, Augment, Windsurf, Zed, Warp, Kiro, Kilo, Droid, Devin, JetBrains AI, OpenCode, OpenCode Go, Pi, Qoder, Kimi Code, and more.
- **Model APIs and clouds:** OpenAI, Azure OpenAI, AWS Bedrock, Vertex AI, xAI, Grok, Mistral, DeepSeek, Groq, Fireworks, DeepInfra, MiniMax, z.ai / GLM, Moonshot, Alibaba, and more.
- **Gateways and routers:** OpenRouter, Vercel AI Gateway, LiteLLM, Bifrost, Hugging Face, Ollama, and more.

The full list is the [providers folder](https://github.com/sheprd-labs/sheprd/tree/main/crates/sheprd-usage/src/providers). Each service needs its own sign-in or key.

### ChatGPT through OpenCode

A ChatGPT subscription used through OpenCode appears as **Codex**. Sheprd checks `~/.codex/auth.json` first, then OpenCode's `openai` sign-in in `~/.local/share/opencode/auth.json`. Use OpenCode's ChatGPT browser or device login: an OpenAI API key does not carry ChatGPT subscription limits. If both Codex and OpenCode are signed in, Codex wins.

### Grok prepaid balance

Sign in with `grok login` on the selected host. When Grok reports a prepaid balance without a usage percentage, the panel shows the balance in USD and the period end, and included usage stays **Not reported**.

## What is not supported

Usage sources that need a browser's local storage or IndexedDB, a local SQLite database, a WebView session, gRPC-web, an interactive login, or signed requests (AWS SigV4, Google service accounts) are left out.

The [CPU, memory, and GPU display](configuration.md) beside usage in the status bar is separate and has its own switches.
