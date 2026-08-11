/**
 * InputRewriter: manages state for the "Improve prompt" feature in chat composers.
 *
 * Encapsulates:
 * - Original text preservation (first input, never a rewrite)
 * - Rewrite state tracking (idle, rewriting, error)
 * - Tauri IPC call to `meta_rewrite_input`
 * - Restoration to original text
 *
 * Usage:
 *   const rewriter = new InputRewriter();
 *   const workContext = await fetchWorkContext();
 *   const outcome = await rewriter.rewrite(currentText, workContext);
 *   const original = rewriter.restoreOriginal();
 *   rewriter.reset(); // when field cleared
 */

import { invoke } from '@tauri-apps/api/core';
import type { UserProfileView } from '$lib/types';

export type RewriteState = 'idle' | 'rewriting' | 'error';

/**
 * Work context extracted from user profile's Work section.
 * Mirrors Rust `WorkContext` struct in `apollia-llm/src/meta/rewrite_input.rs`.
 */
export interface WorkContext {
	sector?: string | null;
	teamSize?: string | null;
	techStack?: string | null;
	dailyTools?: string | null;
	integrations?: string | null;
}

interface RewriteInputRequest {
	originalText: string;
	workContext?: WorkContext | null;
}

interface RewriteInputResponse {
	rewrittenText: string;
	fromLlm: boolean;
}

/**
 * Outcome of a rewrite call.
 *
 * `fromLlm` is `false` when the runtime had no LLM available, or when the model
 * answered with nothing usable. In that case `text` is the original text
 * returned unchanged, and the caller must tell the operator that no rewrite
 * happened instead of silently pretending one did.
 */
export interface RewriteOutcome {
	text: string;
	fromLlm: boolean;
}

export class InputRewriter {
	private state: RewriteState = 'idle';
	private originalText: string | null = null;
	private lastError: string | null = null;

	/**
	 * Rewrite the current text via the LLM.
	 *
	 * On first call: stores `currentText` as `originalText`.
	 * On subsequent calls: always passes `originalText` to the LLM, never `currentText`.
	 * This prevents iteration drift (rewrites compounding on each other).
	 *
	 * @param currentText The text currently in the composer field
	 * @param workContext User's Work section from profile, if filled
	 * @returns The rewritten text together with whether the LLM produced it
	 * @throws Error if rewrite fails (caller should catch and display toast)
	 */
	async rewrite(currentText: string, workContext: WorkContext | null): Promise<RewriteOutcome> {
		// GIVEN: first rewrite or subsequent rewrite
		if (this.originalText === null) {
			// WHEN: first rewrite
			// THEN: store current text as original
			this.originalText = currentText;
		}

		// WHEN: rewrite called
		this.state = 'rewriting';
		this.lastError = null;

		try {
			// THEN: call Tauri command with original text (not current)
			const request: RewriteInputRequest = {
				originalText: this.originalText,
				workContext: workContext ?? null
			};

			const response = await invoke<RewriteInputResponse>('meta_rewrite_input', { request });

			// WHEN: response received
			this.state = 'idle';

			// THEN: return the text and whether the LLM actually produced it
			return { text: response.rewrittenText, fromLlm: response.fromLlm };
		} catch (error) {
			// WHEN: error occurred
			// THEN: set error state and throw
			this.state = 'error';
			this.lastError = error instanceof Error ? error.message : String(error);
			throw error;
		}
	}

	/**
	 * Restore the original text (first input before any rewrites).
	 *
	 * @returns The original text, or null if no rewrite has been performed yet
	 */
	restoreOriginal(): string | null {
		return this.originalText;
	}

	/**
	 * Reset the rewriter state.
	 * Call this when the user clears the input field.
	 */
	reset(): void {
		this.state = 'idle';
		this.originalText = null;
		this.lastError = null;
	}

	/**
	 * Get the current state.
	 */
	getState(): RewriteState {
		return this.state;
	}

	/**
	 * Get the last error message, if any.
	 */
	getLastError(): string | null {
		return this.lastError;
	}
}

/**
 * Fetch Work context from user profile.
 *
 * Reads the user's profile via get_profile and extracts the Work section fields
 * (domain.sector, domain.team_size, tech.stack, tools.daily, tech.integrations).
 *
 * @returns WorkContext with filled fields from profile, or null if profile fetch fails
 */
export async function fetchWorkContext(): Promise<WorkContext | null> {
	try {
		const profile = await invoke<UserProfileView>('get_profile');
		const workContext: WorkContext = {};

		// Extract Work section fields from profile entries
		for (const entry of profile.entries) {
			if (entry.key === 'domain.sector' && entry.value.trim()) {
				workContext.sector = entry.value;
			} else if (entry.key === 'domain.team_size' && entry.value.trim()) {
				workContext.teamSize = entry.value;
			} else if (entry.key === 'tech.stack' && entry.value.trim()) {
				workContext.techStack = entry.value;
			} else if (entry.key === 'tools.daily' && entry.value.trim()) {
				workContext.dailyTools = entry.value;
			} else if (entry.key === 'tech.integrations' && entry.value.trim()) {
				workContext.integrations = entry.value;
			}
		}

		// Return null if all fields are empty
		if (
			!workContext.sector &&
			!workContext.teamSize &&
			!workContext.techStack &&
			!workContext.dailyTools &&
			!workContext.integrations
		) {
			return null;
		}

		return workContext;
	} catch {
		// Profile fetch failed, return null
		return null;
	}
}
