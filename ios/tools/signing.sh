# Sourced by testflight.sh: the Apple Development certificate an archive
# signs with, in a keychain of its own. A CI runner's keychain starts empty,
# and Xcode's automatic signing then creates a new certificate on every
# upload until the team reaches Apple's limit and every archive fails.
#
# signing_setup WORK adds IOS_SIGNING_P12_BASE64 (base64 of a .p12, opened
# with IOS_SIGNING_P12_PASSWORD) to a keychain under WORK and searches it
# first. Without them it does nothing locally, where the login keychain
# signs, and refuses in CI. signing_cleanup puts the user's search list back
# as it was and deletes the keychain; call it on exit, failed or not.

signing_keychain=""
signing_saved=""

# The user's keychain search list, one path per line. `security` prints
# each path quoted and indented; the paths may hold spaces.
signing_list() {
    security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//'
}

# Sets the user's search list to the arguments, then the paths in file $1.
signing_search() {
    signing_file=$1
    shift
    while IFS= read -r signing_path; do
        if [ -n "$signing_path" ]; then set -- "$@" "$signing_path"; fi
    done < "$signing_file"
    security list-keychains -d user -s "$@"
}

signing_setup() {
    signing_work=$1
    if [ -z "${IOS_SIGNING_P12_BASE64:-}" ]; then
        if [ -n "${CI:-}" ]; then
            echo "TestFlight upload refused: set IOS_SIGNING_P12_BASE64 and IOS_SIGNING_P12_PASSWORD." >&2
            echo "Without them Xcode makes a new certificate on every run (see tools/signing.sh)." >&2
            return 1
        fi
        return 0
    fi
    if [ -z "${IOS_SIGNING_P12_PASSWORD:-}" ]; then
        echo "IOS_SIGNING_P12_BASE64 is set without IOS_SIGNING_P12_PASSWORD." >&2
        return 1
    fi
    echo "Adding the development certificate to a keychain of its own…"
    signing_saved="$signing_work/keychains.txt"
    signing_list > "$signing_saved" || return 1
    signing_keychain="$signing_work/signing.keychain-db"
    signing_password=$(openssl rand -hex 24) || return 1
    security create-keychain -p "$signing_password" "$signing_keychain" || return 1
    # A locked keychain stops codesign mid-archive; six hours outlasts the job.
    security set-keychain-settings -lut 21600 "$signing_keychain" || return 1
    security unlock-keychain -p "$signing_password" "$signing_keychain" || return 1
    printf '%s' "$IOS_SIGNING_P12_BASE64" | base64 --decode > "$signing_work/signing.p12" || return 1
    if ! security import "$signing_work/signing.p12" -k "$signing_keychain" \
        -P "$IOS_SIGNING_P12_PASSWORD" -T /usr/bin/codesign -T /usr/bin/security >/dev/null; then
        rm -f "$signing_work/signing.p12"
        echo "IOS_SIGNING_P12_BASE64 could not be imported with IOS_SIGNING_P12_PASSWORD." >&2
        return 1
    fi
    rm -f "$signing_work/signing.p12"
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
        -k "$signing_password" "$signing_keychain" >/dev/null || return 1
    signing_search "$signing_saved" "$signing_keychain" || return 1
    if ! security find-identity -v -p codesigning "$signing_keychain" | grep -q "Apple Development"; then
        echo "IOS_SIGNING_P12_BASE64 holds no Apple Development identity with its private key." >&2
        return 1
    fi
}

signing_cleanup() {
    if [ -n "$signing_saved" ] && [ -f "$signing_saved" ]; then
        signing_search "$signing_saved" || true
    fi
    if [ -n "$signing_keychain" ]; then
        security delete-keychain "$signing_keychain" 2>/dev/null || true
    fi
    signing_keychain=""
    signing_saved=""
}
