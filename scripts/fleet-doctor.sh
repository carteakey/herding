#!/usr/bin/env bash
# Report the ZCode + Herding plugin state of every fleet host.
#
# Checks, per host:
#   - whether `zcode` is installed, and whether it is the fork build
#     (`X+herding.N`) or upstream
#   - the `herdr-zcode` plugin's version and its skill/hook counts
#   - whether the `herdr-team` skill is present in ~/.agents and ~/.claude
#
# Usage:
#   scripts/fleet-doctor.sh [--json] [HOST...]
#
# Hosts come from the arguments, else HERDR_FLEET_HOSTS, else just this
# machine. Use `local` for this machine; anything else is an ssh target.
#
# Exit status: 0 when every host is healthy, 1 when any is not.
set -uo pipefail

json=0
hosts=()
for arg in "$@"; do
    case "$arg" in
        --json) json=1 ;;
        -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) hosts+=("$arg") ;;
    esac
done
if [[ ${#hosts[@]} -eq 0 ]]; then
    # shellcheck disable=SC2206
    hosts=(${HERDR_FLEET_HOSTS:-local})
fi

# The probe runs on each host with POSIX sh; it prints one tab-separated row:
# host, zcode, plugin, skills.
probe='
set -u
# A non-interactive ssh shell has a minimal PATH: homebrew, /usr/local, and
# the per-user bin directories are missing, so zcode looks absent when it is
# installed. Look where installs actually land.
PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$HOME/.cargo/bin:$PATH"
export PATH
if command -v zcode >/dev/null 2>&1; then
    ver=$(zcode --version 2>/dev/null | head -1 | awk "{print \$2}")
    [ -n "$ver" ] || ver=unknown
    case "$ver" in
        *+herding*) kind=fork ;;
        *) kind=upstream ;;
    esac
    zcode="$kind $ver"
    plugin=$(zcode plugins list 2>/dev/null | awk "
        /^- herdr-zcode@/ { found=1; next }
        found && /cache\// { n=split(\$0, a, \"/\"); version=a[n]; next }
        found && /skills:/ {
            gsub(/,/, \"\", \$0);
            print version \" skills=\" \$2 \" hooks=\" \$6;
            exit
        }
    ")
    [ -n "$plugin" ] || plugin="not installed"
else
    zcode="not installed"
    plugin="-"
fi
roots=""
[ -f "$HOME/.agents/skills/herdr-team/SKILL.md" ] && roots="$roots agents"
[ -f "$HOME/.claude/skills/herdr-team/SKILL.md" ] && roots="$roots claude"
roots=${roots# }
[ -n "$roots" ] || roots="missing"
printf "%s\t%s\t%s\t%s\n" "$1" "$zcode" "$plugin" "$roots"
'

rows=()
unhealthy=0
for host in "${hosts[@]}"; do
    if [[ "$host" == "local" ]]; then
        row=$(printf '%s' "$probe" | sh -s -- "$(hostname -s 2>/dev/null || echo local)" 2>/dev/null)
    else
        # The probe goes over stdin: quoting it as an ssh argument mangles it.
        row=$(printf '%s' "$probe" | ssh -o BatchMode=yes -o ConnectTimeout=8 "$host" "sh -s -- $host" 2>/dev/null)
    fi
    if [[ -z "$row" ]]; then
        row=$(printf "%s\tunreachable\t-\t-" "$host")
    fi
    rows+=("$row")
    # Healthy means: the fork build, the plugin installed with no bundled
    # skill, and the team skill present in at least one agent root.
    if ! printf '%s' "$row" | awk -F'\t' '
        $2 ~ /^fork / && $3 ~ /^[0-9]/ && $3 ~ /skills=0/ && $4 != "missing" { ok=1 }
        END { exit ok ? 0 : 1 }'; then
        unhealthy=1
    fi
done

if [[ "$json" == 1 ]]; then
    printf '{"hosts":['
    sep=""
    for row in "${rows[@]}"; do
        IFS=$'\t' read -r host zcode plugin skills <<<"$row"
        printf '%s{"host":"%s","zcode":"%s","plugin":"%s","skills":"%s"}' \
            "$sep" "$host" "$zcode" "$plugin" "$skills"
        sep=","
    done
    printf ']}\n'
    exit "$unhealthy"
fi

{
    printf 'HOST\tZCODE\tHERDR-ZCODE\tHERDR-TEAM\n'
    printf '%s\n' "${rows[@]}"
} | awk -F'\t' '
    { for (i = 1; i <= 4; i++) if (length($i) > w[i]) w[i] = length($i) }
    { line[NR] = $0 }
    END {
        for (n = 1; n <= NR; n++) {
            split(line[n], f, "\t")
            printf "%-*s  %-*s  %-*s  %s\n", w[1], f[1], w[2], f[2], w[3], f[3], f[4]
        }
    }'

exit "$unhealthy"
