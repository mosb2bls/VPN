// Experimental-feature labels. Remove a label everywhere by flipping its flag.
import type { AppInfo } from "../api/bindings";

/** Windows TUN + helper are implemented but untested on a real PC (SPEC 1.4.2, owner decision). */
export const WINDOWS_TUN_EXPERIMENTAL = true;
export const WINDOWS_TUN_LABEL = "Experimental — untested on Windows";

/** The label to show next to TUN / helper controls on this platform, or null. */
export function tunExperimentalLabel(info: AppInfo | null): string | null {
  return WINDOWS_TUN_EXPERIMENTAL && info?.platform === "windows" ? WINDOWS_TUN_LABEL : null;
}
