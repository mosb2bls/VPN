// Lets a section link to another section of the settings window.
import { createContext, useContext } from "react";
import type { SettingsSection } from "../api/bindings";

export const SettingsNav = createContext<(s: SettingsSection) => void>(() => {});
export const useGoTo = () => useContext(SettingsNav);
