// Mock-only copy of the backend's default messages (src-tauri/src/api/error.rs).
// `Record<ErrorCode, …>` makes TypeScript fail if SPEC adds a code we lack.
// The UI itself always displays `error.message` from the backend.
import type { AppError, ErrorCode, FieldError } from "../bindings";

export const ERROR_MESSAGES: Record<ErrorCode, string> = {
  NOT_IMPLEMENTED: "This feature is not implemented yet.",
  INTERNAL: "An unexpected internal error occurred. Check the logs for details.",
  INVALID_ARGUMENT: "The request contained an invalid value.",
  BUSY: "Another operation is in progress. Try again in a moment.",
  UNSUPPORTED_PLATFORM: "This feature is not supported on this operating system.",

  NODE_NOT_FOUND: "The node no longer exists.",
  NO_NODE_SELECTED: "Select a node first.",
  NO_NODES: "Add a node before connecting.",
  NODE_DUPLICATE: "This node is already in your list.",
  REORDER_MISMATCH: "The node order does not match the current node list.",
  VALIDATION_FAILED: "Some fields are invalid. Fix the highlighted fields and try again.",
  LINK_UNSUPPORTED_SCHEME:
    "Unsupported link type. Only hy2://, hysteria2:// and vless:// links are supported.",
  LINK_MALFORMED: "The link is malformed and could not be read.",
  LINK_UNSUPPORTED_PARAM: "The link uses a parameter or value this app does not support.",
  LINK_MISSING_PARAM: "The link is missing a required parameter.",
  LINK_INVALID_VALUE: "The link contains an invalid value.",
  IMPORT_EMPTY: "No links were found in the input.",
  CLIPBOARD_EMPTY: "The clipboard does not contain any text.",
  CLIPBOARD_UNAVAILABLE: "The clipboard could not be read.",
  QR_NOT_FOUND: "No QR code was found in the image.",
  QR_IMAGE_UNREADABLE: "The image could not be opened. Use a PNG or JPEG file.",
  QR_TOO_MANY_CODES: "The image contains more than one QR code. Crop it to a single code.",

  FIELD_REQUIRED: "This field is required.",
  FIELD_INVALID: "This value is invalid.",
  FIELD_OUT_OF_RANGE: "This value is out of the allowed range.",
  FIELD_INVALID_HOST: "Enter a valid hostname or IPv4 address.",
  FIELD_INVALID_PORT: "Enter a port between 1 and 65535.",
  FIELD_INVALID_PORT_RANGE: "Enter port ranges like 20000-30000.",
  FIELD_INVALID_UUID: "Enter a valid UUID.",
  FIELD_INVALID_PUBLIC_KEY: "Enter a valid REALITY public key (base64url, 32 bytes).",
  FIELD_INVALID_SHORT_ID: "Enter a valid short ID (0–16 hex characters, even length).",
  FIELD_INVALID_FINGERPRINT: "Choose a supported TLS fingerprint.",
  FIELD_INVALID_PIN: "Enter a SHA-256 certificate pin (64 hex characters).",
  FIELD_INVALID_SNI: "Enter a valid server name (SNI).",
  FIELD_INVALID_BANDWIDTH: "Enter a bandwidth between 1 and 100000 Mbps.",
  FIELD_TOO_LONG: "This value is too long.",
  FIELD_CANNOT_CLEAR: "This secret is required and cannot be cleared.",

  KEYCHAIN_UNAVAILABLE: "The system keychain is unavailable.",
  KEYCHAIN_ACCESS_DENIED: "Access to the system keychain was denied.",
  SECRET_MISSING: "A stored credential for this node is missing. Edit the node and re-enter it.",

  CORE_BINARY_MISSING: "The proxy core is missing from the app bundle. Reinstall the app.",
  CORE_CHECKSUM_MISMATCH: "The proxy core failed its integrity check. Reinstall the app.",
  CORE_CONFIG_INVALID: "The generated proxy configuration was rejected by the core.",
  CORE_START_FAILED: "The proxy core failed to start.",
  CORE_CRASHED: "The proxy core stopped unexpectedly.",
  CORE_API_UNREACHABLE: "The proxy core is not responding.",
  PORT_IN_USE: "A local port needed by the proxy is already in use.",
  DNS_BOOTSTRAP_FAILED: "The proxy server's address could not be resolved.",
  NODE_UNREACHABLE: "The server could not be reached.",
  NODE_HANDSHAKE_FAILED: "The secure connection to the server failed.",
  NODE_AUTH_FAILED: "The server rejected the credentials.",
  PROBE_TIMEOUT: "The connectivity test timed out.",
  PROBE_FAILED: "The connectivity test failed.",
  ALL_NODES_FAILED: "All configured nodes are failing. Traffic is blocked to prevent leaks.",
  SYSTEM_PROXY_SET_FAILED: "The system proxy could not be set.",
  SYSTEM_PROXY_RESTORE_FAILED: "The previous system proxy settings could not be fully restored.",
  NETWORK_CHANGED: "The network changed. Reconnecting.",

  HELPER_NOT_INSTALLED: "TUN mode needs the privileged helper. Install it first.",
  HELPER_INSTALL_CANCELLED: "Helper installation was cancelled.",
  HELPER_INSTALL_FAILED: "The helper could not be installed.",
  HELPER_UNINSTALL_FAILED: "The helper could not be uninstalled.",
  HELPER_UNREACHABLE: "The privileged helper is not responding.",
  HELPER_VERSION_MISMATCH: "The installed helper is out of date. Reinstall it.",
  HELPER_AUTH_FAILED: "The privileged helper refused the request.",
  TUN_START_FAILED: "The TUN interface could not be created.",
  KILL_SWITCH_FAILED: "The kill switch could not be applied.",
  KILL_SWITCH_REMOVE_FAILED: "The kill switch could not be removed. Use Restore networking.",

  JOURNAL_CORRUPT: "The saved network state is damaged. Use Restore networking.",
  RESTORE_INCOMPLETE: "Some network settings could not be restored. See details.",

  RULES_DOWNLOAD_FAILED: "Rule update failed to download. The current rules are still in use.",
  RULES_VALIDATION_FAILED: "Downloaded rules were invalid. The current rules are still in use.",
  RULES_UPDATE_IN_PROGRESS: "A rule update is already running.",

  SETTINGS_INVALID: "One or more settings are invalid.",
  SETTINGS_SAVE_FAILED: "Settings could not be saved.",
  LAUNCH_AT_LOGIN_FAILED: "Launch at login could not be changed.",
  DIAGNOSTICS_EXPORT_FAILED: "Diagnostics could not be exported.",
};

export const ALL_ERROR_CODES = Object.keys(ERROR_MESSAGES) as ErrorCode[];

export function appError(
  code: ErrorCode,
  opts: { detail?: string | null; recoverable?: boolean; fields?: FieldError[] } = {},
): AppError {
  return {
    code,
    message: ERROR_MESSAGES[code],
    recoverable: opts.recoverable ?? true,
    fields: opts.fields ?? [],
    detail: opts.detail ?? null,
  };
}

export function fieldError(field: string, code: ErrorCode): FieldError {
  return { field, code, message: ERROR_MESSAGES[code] };
}
