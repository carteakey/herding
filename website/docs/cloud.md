---
title: Cloud machines
blurb: Create sandboxes and cloud VMs as devices.
status: shipped
platforms: macOS, Linux
---

# Cloud machines

Sheprd can create machines at a cloud provider, or containers on your own computer, and use each one as a device beside your SSH hosts. This page covers the providers, creating and deleting machines, sandbox images built from your project, setup scripts for new machines, and the `sheprd cloud` commands.

> **Not on Windows.** Every cloud provider, containers included, needs a macOS or Linux client.

## How a cloud machine works

A cloud machine is a Linux machine running its own Herdr daemon, saved as a device and reached the way an [SSH host](ssh-hosts.md) is. Its terminals and agents belong to that daemon, so closing a window or reconnecting never stops an agent there. Sheprd's part is the machine's lifecycle (create, start, stop, delete), getting Herdr onto it, and moving Git work in and out with [Teleport](teleport.md).

Accounts live in **Settings > Cloud devices**. Once a provider is set up, it appears under **Cloud** in **Add Device…**; until then its **Add provider** tile opens its Settings card. API keys and tokens are kept in your system credential store, not the config file, and stay on your computer.

Most providers' forms have an **Install Herdr** switch, on by default. It is the approval to run Herdr's published installer on the new machine and then to add the herdr-core (Herdr 2) every Sheprd release bundles, downloaded there and checked against its pinned checksum, into `~/.local/bin` unless the machine already has a herdr-core. A core that cannot be installed leaves Herdr working, and the machine's new tabs stay Herdr tabs. This is the only remote installation Sheprd performs, and reconnecting never installs anything.

## Providers

| Provider | What it makes | Status |
| --- | --- | --- |
| Azure | VMs (over SSH) and Container Apps sandboxes in one subscription | Shipped |
| Coder | Workspaces from your deployment's templates | Shipped |
| Containers | Docker or Podman containers on this computer | Shipped |
| Boat | Boat sandboxes | Experimental |
| boxd | Linux microVMs with a persistent disk | Experimental |
| Daytona | Daytona sandboxes | Experimental |
| Fly Sprites | Sprites that sleep when idle and keep their files | Experimental |
| OpenSandbox | Sandboxes on a self-hosted OpenSandbox server | Experimental |
| Railway | The temporary VM `ssh railway.new` gives out, no account needed | Experimental |

Experimental providers are tested against scripted API replies; Daytona, Boat, and Sprites have not yet been exercised against a live account. boxd has been tested against the live service, and OpenSandbox against a local server on Docker.

### Azure

The Azure card takes a subscription ID, resource group, location, VM size, and auto-stop time, and signs in with your active Azure CLI (`az`) account or a service principal's client secret. VMs are reached over SSH; sandboxes through the Azure data plane.

### Coder

The Coder card edits the `[coder]` keys and signs you in with Coder's OAuth2 provider. The **Coder** form then lists your templates, their presets, and existing workspaces, with a generated `prefix-adjective-noun` name ready, so creating takes one click. Choosing an existing workspace attaches it instead. Templates need not include Herdr. A stopped workspace is started before connecting. The `coder` CLI is required, found on `PATH`, in `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, or at `[coder] cli`.

Creating runs in the background: the dialog closes at once, the device picker lists the workspace under **Adding** with its latest status, and a toast says when it is ready.

### Containers (Docker or Podman)

**Settings > Cloud devices > Container** makes containers on this computer's Docker or Podman engine. Each is a private Linux machine with its own Herdr, so agents in it work on their own checkout without seeing your files or each other's. Agents run as an unprivileged `herdr` user, with CPU, memory, and process limits.

```toml
[container]
# docker or podman; the first one installed when unset.
engine = "docker"
# The image new containers start from, pinned to its digest when created. It
# needs curl to install Herdr and git for checkouts.
image = "buildpack-deps:bookworm"
cpus = 4
memory = "8g"
pids_limit = 4096
# Stop a running container once none of its agents has worked or waited for
# input for this long (at least 5m); its files stay and the next connection
# starts it. Default is never.
auto_stop = "30m"
# The most containers kept as devices at once, up to 64. Default is 8.
max_containers = 8
```

The default image has `curl` and `git` and common build tools. **Stop** in the card stops a container and keeps its files. Idle auto-stop counts only agents: a dev server in a plain shell does not keep a container running, so leave `auto_stop` unset where that matters. A container the app made but has no device for is listed under **Not saved here** with **Add**.

> **Containers are not a security boundary.** They share this computer's kernel: they keep honest agents apart, not hostile code. Use a VM provider such as Azure for that.

### Boat

The Boat card takes the API URL (`https://boat.dev/api/v1` for Boat's cloud), an optional organization wallet, machine type, and environment, and an API key with the `sandbox.create`, `sandbox.read`, `sandbox.update`, `sandbox.resume`, and `ssh` actions. **Auto-stop after** defaults to `never`, so the sandbox keeps running and billing until you stop it in Boat:

```sh
sheprd config set boat.auto_stop never
sheprd config set boat.auto_stop '"7200"'  # seconds, stored as a string
```

This is a fixed deadline, not an idle timeout. Trial accounts need a deadline of at most 7200 seconds. If setup fails, **Finish setup** retries the same sandbox; if creation's reply is lost, **Recover creation** retries the original request for up to 23 hours.

### boxd, Daytona, Fly Sprites, and OpenSandbox

Each card takes the service's API URL and an API key or token, then **Create machine**, **Create sandbox**, or **Create sprite** makes one with a generated name and installs Herdr. A failed creation deletes the machine again (Daytona excepted: removing a Daytona device leaves its sandbox). Stopped, paused, or sleeping machines are started or woken when Sheprd connects.

- **boxd**: `https://boxd.sh:9443`, or a region such as `https://nyc.boxd.sh:9443`. Create a key in the boxd console or with `boxd auth keys create <name>`.
- **Daytona**: `https://app.daytona.io/api`, with an optional organization, region, and snapshot. Needs `ssh` on the `PATH`.
- **Fly Sprites**: `https://api.sprites.dev`. Herdr is registered as the sprite's service, so it runs again whenever the sprite wakes.
- **OpenSandbox**: your own server's URL (`https`, or `http` on this computer, such as one started with `uvx opensandbox-server`), plus the image, CPU, and memory new sandboxes get.

### Railway

**Railway** sets up the temporary Linux VM that `ssh railway.new` gives out without an account. The SSH key is its identity: pick an existing private key or generate one by name, then **Connect and set up**. Every reconnect uses the same key and never creates a VM. Railway limits the VM's lifetime and how many a network can create. Needs `ssh` and `ssh-keygen` on the `PATH`.

## Removing a machine

Right-click a cloud device's header in the sidebar for **Rename…**, **Cloud device settings…**, **Run setup script**, and **Remove device…**. For every provider except Railway, removing deletes the machine at the provider by default, with **Remove from Herdr only** beside it. A Railway VM is only forgotten and expires on its own.

Before deleting, the dialog searches the machine's home directory for Git checkouts with changed or untracked files, or commits on no remote. When it finds some, it names them and the button becomes **Delete anyway**. A machine it cannot reach is reported as unchecked, never as clean.

Port forwards and server keybindings need a saved SSH host, so cloud devices do not offer them, and a cloud machine's load columns stay empty in the Devices overview.

## Sandbox images

A project can describe the machine its agents need, so a new container or Daytona sandbox starts with its tools and Herdr already installed. Commit a Dockerfile as `.herdr/sandbox/Dockerfile`, with that folder as its build context:

```text
FROM rust:1.90-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends ripgrep
COPY setup.sh /usr/local/bin/project-setup
```

Without one, a dev container is used: `.devcontainer/devcontainer.json` (or `.devcontainer.json`), and the Dockerfile or `image` it names. Only the image is built; dev container features, lifecycle scripts, and Docker Compose are not.

Sheprd adds a last layer that installs Herdr and the pinned herdr-core (the image needs `curl`, and `sha256sum` or `shasum` for the core; an image without them still builds, without herdr-core). Clones and worktrees of one repository share an image, and any change to the Dockerfile or the files it copies builds a new one. A base image tag that moves upstream does not: pin a digest, or use **Reset**.

**Build sandbox images** in a workspace's menu builds ahead. The Container and Daytona cards list each project's image with **Log**, **Build now**, and **Reset**. Daytona images cannot `COPY` or `ADD` files from the project yet.

## Setup scripts for new machines

A new cloud machine can install coding agents and run your own setup the first time it connects. **Settings > Cloud devices > New device setup** has one switch per agent (Claude Code, Codex, Gemini CLI, OpenCode), and **Edit setup script in config file** opens your own lines:

```toml
[cloud]
agents = ["claude", "opencode"]
setup = """
git config --global pull.rebase true
"""

# Replaces either key for one provider; an empty list or script turns it off.
[cloud.coder]
agents = []
```

The script installs the agents first, each skipped when already there, then runs your lines with `sh -e`, so the first failing command stops it. Codex and Gemini CLI need `npm` on the machine and are skipped with a note when it is missing.

Each provider's form has a **Run the setup script when it connects** switch. When the machine is ready, the script waits up to 30 minutes for a window to show that device, then runs in a new, focused `setup` tab of its first workspace. Nothing runs in the background, and reconnecting never runs it again by itself. Open a new tab afterwards to pick up `PATH` changes.

## Command line

Agents and scripts can manage cloud machines through the app. Creating and destroying always ask you in the window first, once per batch.

| Command | What it does |
| --- | --- |
| `sheprd cloud` | Saved cloud machines |
| `sheprd cloud add` | Open the settings that add one |
| `sheprd cloud create PROVIDER [--count N] [--template T] [--preset P] [--sandbox] [--no-install] [--no-setup]` | Ask to create up to 16 machines |
| `sheprd cloud wait BATCH [--timeout S]` | Wait until every machine of a batch is settled |
| `sheprd cloud exec ID [--timeout S] [--cwd DIR] -- COMMAND...` | Run a command on a saved machine and print its output |
| `sheprd cloud setup ID` | Run the `[cloud]` setup script again |
| `sheprd cloud destroy ID... \| --batch BATCH` | Ask to delete machines and forget them |
| `sheprd cloud batches` | The batches the app keeps |
| `sheprd cloud images` | Sandbox images built from projects |
| `sheprd cloud images build [PROVIDER]` | Build the current repository's images |

Providers are named azure, boat, boxd, coder, container, daytona, opensandbox, railway, or sprites. Coder takes `--template` and `--preset`; Azure makes a Container Apps sandbox with `--sandbox`.

```sh
sheprd cloud create coder --count 10 --template dev --json
sheprd cloud wait 3 --timeout 600 --json
sheprd cloud exec herdr-tidy-lark-aa --cwd ~/code -- git clone https://github.com/me/app
sheprd cloud destroy herdr-tidy-lark-aa --json
```

`cloud exec` asks nothing: like an agent's own shell, it acts at once. Git pushes from a machine need credentials there, such as a `gh auth login` in the `[cloud]` setup script. See the [command line](cli.md) for exit codes.
