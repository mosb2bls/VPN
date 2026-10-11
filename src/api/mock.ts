// Mock backend entry point. Implements the whole SPEC in memory; see
// ./mock/engine.ts. `mockEngine` is for the dev page only — components use
// `src/api/client.ts`.
import { MockEngine } from "./mock/engine";

export const mockEngine = new MockEngine();
export const mockBackend = mockEngine.asBackend();

export { SCENARIOS } from "./mock/engine";
export type { DevConfig, NodeHealth, ScenarioId } from "./mock/engine";
export { ALL_ERROR_CODES } from "./mock/errors";
export { SAMPLE_LINKS } from "./mock/links";
