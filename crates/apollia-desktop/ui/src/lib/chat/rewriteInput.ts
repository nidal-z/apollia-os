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
 *   const workContext = await fetchConversationWorkContext(sessionId);
 *   const outcome = await rewriter.rewrite(currentText, workContext);
 *   const original = rewriter.restoreOriginal();
 *   rewriter.reset(); // when field cleared
 */

import { invoke } from '@tauri-apps/api/core';
import { getChatSession } from '$lib/ipc/chat';
import { DEFAULT_PROFILE, getProfileById, type ProfileDetailView } from '$lib/ipc/profile';

export type RewriteState = 'idle' | 'rewriting' | 'error';

/**
 * Work context read from the profile of the conversation: its role, its
 * function and its tools.
 * Mirrors Rust `WorkContext` struct in `apollia-llm/src/meta/rewrite_input.rs`.
 */
export interface WorkContext {
	role?: string | null;
	function?: string | null;
	tools?: string | null;
}

/** The profile fields the rewrite takes, by the key the schema gives them. */
const WORK_CONTEXT_KEYS = ['role', 'function', 'tools'] as const;

interface RewriteInputRequest {
	originalText: string;
	workContext?: WorkContext | null;
}

/**
 * Why no rewrite happened.
 *
 * Mirrors `RewriteFallbackReason` in
 * `crates/apollia-runtime/src/chat_assists.rs`. Only the first two
 * mean the operator has no engine to configure; the last two mean the engine
 * answered, and telling them to go set one up would be false.
 */
export type RewriteFallback = 'noRouter' | 'noBackend' | 'callFailed' | 'emptyAnswer';

interface RewriteInputResponse {
	rewrittenText: string;
	fallback: RewriteFallback | null;
}

/**
 * Outcome of a rewrite call.
 *
 * `fallback` is `null` when the model produced the rewrite. Otherwise `text` is
 * the original text returned unchanged, and `fallback` says which of the four
 * situations stopped the rewrite, so the caller renders the message that names
 * the real cause instead of one message for all four.
 */
export interface RewriteOutcome {
	text: string;
	fallback: RewriteFallback | null;
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
	 * @returns The rewritten text together with the fallback that stopped it, if any
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

			// THEN: return the text and the reason no rewrite happened, if any
			return { text: response.rewrittenText, fallback: response.fallback ?? null };
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
 * Fetch the work context of a conversation from its profile.
 *
 * Reads the profile the conversation designates, or the default profile when
 * it designates none or when the designated profile no longer exists, and
 * keeps its role, its function and its tools.
 *
 * @param profileId profile the conversation designates, if any
 * @returns WorkContext with the filled fields, or null when none is filled or
 *          no profile can be read
 */
export async function fetchWorkContext(profileId?: string | null): Promise<WorkContext | null> {
	let profile: ProfileDetailView;
	try {
		profile = await getProfileById(profileId || DEFAULT_PROFILE);
	} catch {
		if (!profileId) return null;
		try {
			profile = await getProfileById(DEFAULT_PROFILE);
		} catch {
			return null;
		}
	}

	const workContext: WorkContext = {};
	for (const key of WORK_CONTEXT_KEYS) {
		const value = profile.fields.find((field) => field.key === key)?.value.trim();
		if (value) workContext[key] = value;
	}
	return Object.keys(workContext).length > 0 ? workContext : null;
}

/**
 * Fetch the work context of the conversation `sessionId`, read from the
 * profile that conversation designates at the moment of the rewrite. A
 * conversation that cannot be read falls back to the default profile.
 */
export async function fetchConversationWorkContext(sessionId: string): Promise<WorkContext | null> {
	let profileId: string | null = null;
	try {
		profileId = (await getChatSession(sessionId)).profile_id ?? null;
	} catch {
		profileId = null;
	}
	return fetchWorkContext(profileId);
}
