//! Stable error codes shared by the app, the helper and the UI contract.
//! Codes are part of the SPEC: never rename or reuse one; add new ones
//! (minor SPEC bump). Messages are display-ready English and may be reworded
//! (patch bump).

use serde::{Deserialize, Serialize};

macro_rules! error_codes {
    ($($variant:ident => $msg:literal,)+) => {
        #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "specta", derive(specta::Type))]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum ErrorCode {
            $($variant,)+
        }

        impl ErrorCode {
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant,)+];

            /// Default human-readable message, suitable for direct display.
            pub fn message(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $msg,)+
                }
            }
        }
    };
}

error_codes! {
    // General
    NotImplemented => "This feature is not implemented yet.",
    Internal => "An unexpected internal error occurred. Check the logs for details.",
    InvalidArgument => "The request contained an invalid value.",
    Busy => "Another operation is in progress. Try again in a moment.",
    UnsupportedPlatform => "This feature is not supported on this operating system.",

    // Nodes and import
    NodeNotFound => "The node no longer exists.",
    NoNodeSelected => "Select a node first.",
    NoNodes => "Add a node before connecting.",
    NodeDuplicate => "This node is already in your list.",
    ReorderMismatch => "The node order does not match the current node list.",
    ValidationFailed => "Some fields are invalid. Fix the highlighted fields and try again.",
    LinkUnsupportedScheme => "Unsupported link type. Only hy2://, hysteria2:// and vless:// links are supported.",
    LinkMalformed => "The link is malformed and could not be read.",
    LinkUnsupportedParam => "The link uses a parameter or value this app does not support.",
    LinkMissingParam => "The link is missing a required parameter.",
    LinkInvalidValue => "The link contains an invalid value.",
    ImportEmpty => "No links were found in the input.",
    ClipboardEmpty => "The clipboard does not contain any text.",
    ClipboardUnavailable => "The clipboard could not be read.",
    QrNotFound => "No QR code was found in the image.",
    QrImageUnreadable => "The image could not be opened. Use a PNG or JPEG file.",
    QrTooManyCodes => "The image contains more than one QR code. Crop it to a single code.",

    // Field-level (used inside FieldError, also valid as top-level codes)
    FieldRequired => "This field is required.",
    FieldInvalid => "This value is invalid.",
    FieldOutOfRange => "This value is out of the allowed range.",
    FieldInvalidHost => "Enter a valid hostname or IPv4 address.",
    FieldInvalidPort => "Enter a port between 1 and 65535.",
    FieldInvalidPortRange => "Enter port ranges like 20000-30000.",
    FieldInvalidUuid => "Enter a valid UUID.",
    FieldInvalidPublicKey => "Enter a valid REALITY public key (base64url, 32 bytes).",
    FieldInvalidShortId => "Enter a valid short ID (0–16 hex characters, even length).",
    FieldInvalidFingerprint => "Choose a supported TLS fingerprint.",
    FieldInvalidPin => "Enter a SHA-256 certificate pin (64 hex characters).",
    FieldInvalidSni => "Enter a valid server name (SNI).",
    FieldInvalidBandwidth => "Enter a bandwidth between 1 and 100000 Mbps.",
    FieldTooLong => "This value is too long.",
    FieldCannotClear => "This secret is required and cannot be cleared.",

    // Secure storage
    KeychainUnavailable => "The system keychain is unavailable.",
    KeychainAccessDenied => "Access to the system keychain was denied.",
    SecretMissing => "A stored credential for this node is missing. Edit the node and re-enter it.",

    // Core / connection
    CoreBinaryMissing => "The proxy core is missing from the app bundle. Reinstall the app.",
    CoreChecksumMismatch => "The proxy core failed its integrity check. Reinstall the app.",
    CoreConfigInvalid => "The generated proxy configuration was rejected by the core.",
    CoreStartFailed => "The proxy core failed to start.",
    CoreCrashed => "The proxy core stopped unexpectedly.",
    CoreApiUnreachable => "The proxy core is not responding.",
    PortInUse => "A local port needed by the proxy is already in use.",
    DnsBootstrapFailed => "The proxy server's address could not be resolved.",
    NodeUnreachable => "The server could not be reached.",
    NodeHandshakeFailed => "The secure connection to the server failed.",
    NodeAuthFailed => "The server rejected the credentials.",
    ProbeTimeout => "The connectivity test timed out.",
    ProbeFailed => "The connectivity test failed.",
    AllNodesFailed => "All configured nodes are failing. Traffic is blocked to prevent leaks.",
    SystemProxySetFailed => "The system proxy could not be set.",
    SystemProxyRestoreFailed => "The previous system proxy settings could not be fully restored.",
    NetworkChanged => "The network changed. Reconnecting.",

    // Helper / TUN / kill switch
    HelperNotInstalled => "TUN mode needs the privileged helper. Install it first.",
    HelperInstallCancelled => "Helper installation was cancelled.",
    HelperInstallFailed => "The helper could not be installed.",
    HelperUninstallFailed => "The helper could not be uninstalled.",
    HelperUnreachable => "The privileged helper is not responding.",
    HelperVersionMismatch => "The installed helper is out of date. Reinstall it.",
    HelperAuthFailed => "The privileged helper refused the request.",
    TunStartFailed => "The TUN interface could not be created.",
    KillSwitchFailed => "The kill switch could not be applied.",
    KillSwitchRemoveFailed => "The kill switch could not be removed. Use Restore networking.",

    // Recovery
    TunStopFailed => "The TUN session could not be stopped.",
    RouteRestoreFailed => "Some network routes could not be restored.",
    DnsRestoreFailed => "The previous DNS settings could not be restored.",
    Ipv6RestoreFailed => "The previous IPv6 settings could not be restored.",
    JournalCorrupt => "The saved network state is damaged. Use Restore networking.",
    RestoreIncomplete => "Some network settings could not be restored. See details.",

    // Rules
    RulesDownloadFailed => "Rule update failed to download. The current rules are still in use.",
    RulesValidationFailed => "Downloaded rules were invalid. The current rules are still in use.",
    RulesUpdateInProgress => "A rule update is already running.",

    // App
    SettingsInvalid => "One or more settings are invalid.",
    SettingsSaveFailed => "Settings could not be saved.",
    LaunchAtLoginFailed => "Launch at login could not be changed.",
    DiagnosticsExportFailed => "Diagnostics could not be exported.",
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_message_ending_in_a_period() {
        for code in ErrorCode::ALL {
            assert!(code.message().ends_with('.'), "{code:?} message should be a sentence");
        }
    }

    #[test]
    fn codes_serialize_screaming_snake() {
        let s = serde_json::to_string(&ErrorCode::LinkUnsupportedScheme).unwrap();
        assert_eq!(s, "\"LINK_UNSUPPORTED_SCHEME\"");
    }
}
