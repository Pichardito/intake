/* tslint:disable */
/* eslint-disable */

/**
 * A compiled engine held on the WASM side. Build once, call many times.
 */
export class WasmEngine {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    maxInputBytes(): number;
    /**
     * Redact with options given as JSON (`{"style":"numbered","reversible":true}`).
     * Returns the `Redaction` as JSON.
     */
    redact(text: string, options_json?: string | null): string;
    ruleCount(): number;
    /**
     * Rule summaries as JSON, for UIs that list what the engine detects.
     */
    rules(): string;
    /**
     * Spans as a JSON array (see the TypeScript `Span` type).
     */
    scan(text: string): string;
}

/**
 * Build an engine from pack names (`"pii"`, `"phi-us"`, `"secrets"`,
 * `"finance"`) and optional custom rules in TOML.
 */
export function createEngine(packs: string[], custom_toml?: string | null): WasmEngine;

export function packsVersion(): string;

export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_wasmengine_free: (a: number, b: number) => void;
    readonly createEngine: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly packsVersion: (a: number) => void;
    readonly version: (a: number) => void;
    readonly wasmengine_maxInputBytes: (a: number) => number;
    readonly wasmengine_redact: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly wasmengine_ruleCount: (a: number) => number;
    readonly wasmengine_rules: (a: number, b: number) => void;
    readonly wasmengine_scan: (a: number, b: number, c: number, d: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
