/**
 * Tests for InputRewriter class.
 *
 * Volet 3: No iteration drift (successive rewrites use first original, never prior rewrite)
 * Volet 4: Work context passed when filled, success when empty
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { InputRewriter, type WorkContext } from './rewriteInput';

// Mock Tauri invoke - must be hoisted, so use vi.fn() inside factory
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn()
}));

import { invoke } from '@tauri-apps/api/core';

// `invoke` is typed as the real Tauri function, so the mock helpers only exist
// once it is narrowed to a Mock. Without this, every `mockResolvedValueOnce`
// below is a type error.
const mockInvoke = vi.mocked(invoke);

describe('InputRewriter', () => {
	beforeEach(() => {
		mockInvoke.mockClear();
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	describe('Volet 3: No iteration drift', () => {
		it('should pass original text on successive rewrites, never the previous rewrite', async () => {
			// GIVEN: InputRewriter instance
			const rewriter = new InputRewriter();

			// Mock responses for three successive rewrites
			mockInvoke
				.mockResolvedValueOnce({ rewrittenText: 'First rewrite result', fallback: null })
				.mockResolvedValueOnce({ rewrittenText: 'Second rewrite result', fallback: null })
				.mockResolvedValueOnce({ rewrittenText: 'Third rewrite result', fallback: null });

			// WHEN: first rewrite with "original input"
			const result1 = await rewriter.rewrite('original input', null);

			// THEN: first call should pass "original input"
			expect(mockInvoke).toHaveBeenCalledTimes(1);
			expect(mockInvoke).toHaveBeenNthCalledWith(1, 'meta_rewrite_input', {
				request: {
					originalText: 'original input',
					workContext: null
				}
			});
			expect(result1.text).toBe('First rewrite result');

			// WHEN: second rewrite with the first rewrite result (simulating user kept it)
			const result2 = await rewriter.rewrite('First rewrite result', null);

			// THEN: second call should STILL pass "original input", NOT "First rewrite result"
			expect(mockInvoke).toHaveBeenCalledTimes(2);
			expect(mockInvoke).toHaveBeenNthCalledWith(2, 'meta_rewrite_input', {
				request: {
					originalText: 'original input', // NOT 'First rewrite result'
					workContext: null
				}
			});
			expect(result2.text).toBe('Second rewrite result');

			// WHEN: third rewrite with the second rewrite result
			const result3 = await rewriter.rewrite('Second rewrite result', null);

			// THEN: third call should STILL pass "original input"
			expect(mockInvoke).toHaveBeenCalledTimes(3);
			expect(mockInvoke).toHaveBeenNthCalledWith(3, 'meta_rewrite_input', {
				request: {
					originalText: 'original input', // Still the first input
					workContext: null
				}
			});
			expect(result3.text).toBe('Third rewrite result');
		});
	});

	describe('Volet 4: Work context feeding the rewrite', () => {
		it('should pass work context when filled', async () => {
			// GIVEN: InputRewriter instance and filled work context
			const rewriter = new InputRewriter();
			const workContext: WorkContext = {
				sector: 'fintech',
				teamSize: 'small team',
				techStack: 'Rust, Svelte, SQLite',
				dailyTools: 'Git, Notion',
				integrations: 'GitHub, Gmail'
			};

			mockInvoke.mockResolvedValueOnce({
				rewrittenText: 'Rewritten with context',
				fallback: null
			});

			// WHEN: rewrite with work context
			const result = await rewriter.rewrite('fix bug', workContext);

			// THEN: Tauri invoke called with work context
			expect(mockInvoke).toHaveBeenCalledTimes(1);
			expect(mockInvoke).toHaveBeenCalledWith('meta_rewrite_input', {
				request: {
					originalText: 'fix bug',
					workContext: {
						sector: 'fintech',
						teamSize: 'small team',
						techStack: 'Rust, Svelte, SQLite',
						dailyTools: 'Git, Notion',
						integrations: 'GitHub, Gmail'
					}
				}
			});
			expect(result.text).toBe('Rewritten with context');
		});

		it('should succeed when work context is null', async () => {
			// GIVEN: InputRewriter instance and null work context
			const rewriter = new InputRewriter();

			mockInvoke.mockResolvedValueOnce({
				rewrittenText: 'Rewritten without context',
				fallback: null
			});

			// WHEN: rewrite with null context
			const result = await rewriter.rewrite('debug api', null);

			// THEN: Tauri invoke called with null workContext
			expect(mockInvoke).toHaveBeenCalledTimes(1);
			expect(mockInvoke).toHaveBeenCalledWith('meta_rewrite_input', {
				request: {
					originalText: 'debug api',
					workContext: null
				}
			});
			expect(result.text).toBe('Rewritten without context');
		});

		it('should succeed when work context is partially filled', async () => {
			// GIVEN: InputRewriter instance and partial work context
			const rewriter = new InputRewriter();
			const partialContext: WorkContext = {
				sector: 'healthcare',
				dailyTools: 'Slack, Jira'
				// other fields undefined
			};

			mockInvoke.mockResolvedValueOnce({
				rewrittenText: 'Rewritten with partial context',
				fallback: null
			});

			// WHEN: rewrite with partial context
			const result = await rewriter.rewrite('help', partialContext);

			// THEN: Tauri invoke called with partial context
			expect(mockInvoke).toHaveBeenCalledTimes(1);
			expect(mockInvoke).toHaveBeenCalledWith('meta_rewrite_input', {
				request: {
					originalText: 'help',
					workContext: {
						sector: 'healthcare',
						dailyTools: 'Slack, Jira'
					}
				}
			});
			expect(result.text).toBe('Rewritten with partial context');
		});
	});

	describe('Volet 5: no rewrite happened, and the reason survives the IPC hop', () => {
		it('should hand back the untouched text with the reason that stopped it', async () => {
			// GIVEN: a runtime with no LLM, which echoes the input back
			const rewriter = new InputRewriter();
			mockInvoke.mockResolvedValueOnce({ rewrittenText: 'fix bug', fallback: 'noBackend' });

			// WHEN: rewrite called
			const result = await rewriter.rewrite('fix bug', null);

			// THEN: the caller can tell no rewrite happened, and why
			expect(result.fallback).toBe('noBackend');
			expect(result.text).toBe('fix bug');
		});

		it('should report a null fallback when the model produced the rewrite', async () => {
			// GIVEN: a runtime with an LLM
			const rewriter = new InputRewriter();
			mockInvoke.mockResolvedValueOnce({ rewrittenText: 'Fix the bug', fallback: null });

			// WHEN: rewrite called
			const result = await rewriter.rewrite('fix bug', null);

			// THEN: the caller sees a genuine rewrite
			expect(result.fallback).toBeNull();
			expect(result.text).toBe('Fix the bug');
		});

		it('should carry each of the four reasons through unchanged', async () => {
			// GIVEN: the four situations the backend can report
			const reasons = ['noRouter', 'noBackend', 'callFailed', 'emptyAnswer'] as const;
			const seen: (string | null)[] = [];

			// WHEN: each one comes back from the command
			for (const reason of reasons) {
				const rewriter = new InputRewriter();
				mockInvoke.mockResolvedValueOnce({ rewrittenText: 'fix bug', fallback: reason });
				const result = await rewriter.rewrite('fix bug', null);
				seen.push(result.fallback);
			}

			// THEN: the composer receives four distinct reasons, so no single
			// message can stand for all four
			expect(seen).toEqual(['noRouter', 'noBackend', 'callFailed', 'emptyAnswer']);
			expect(new Set(seen).size).toBe(4);
		});
	});

	describe('restoreOriginal', () => {
		it('should return original text after rewrites', async () => {
			// GIVEN: InputRewriter with completed rewrites
			const rewriter = new InputRewriter();
			mockInvoke.mockResolvedValue({ rewrittenText: 'Rewritten', fallback: null });

			await rewriter.rewrite('original', null);
			await rewriter.rewrite('something else', null);

			// WHEN: restoreOriginal called
			const original = rewriter.restoreOriginal();

			// THEN: returns exact original text
			expect(original).toBe('original');
		});

		it('should return null if no rewrite has been performed', () => {
			// GIVEN: fresh InputRewriter
			const rewriter = new InputRewriter();

			// WHEN: restoreOriginal called before any rewrite
			const original = rewriter.restoreOriginal();

			// THEN: returns null
			expect(original).toBeNull();
		});
	});

	describe('reset', () => {
		it('should clear all state', async () => {
			// GIVEN: InputRewriter with rewrite history
			const rewriter = new InputRewriter();
			mockInvoke.mockResolvedValue({ rewrittenText: 'Rewritten', fallback: null });

			await rewriter.rewrite('original', null);
			expect(rewriter.restoreOriginal()).toBe('original');

			// WHEN: reset called
			rewriter.reset();

			// THEN: state cleared
			expect(rewriter.restoreOriginal()).toBeNull();
			expect(rewriter.getState()).toBe('idle');
			expect(rewriter.getLastError()).toBeNull();
		});
	});

	describe('error handling', () => {
		it('should set error state and throw on invoke failure', async () => {
			// GIVEN: InputRewriter and failing invoke
			const rewriter = new InputRewriter();
			const error = new Error('LLM backend not configured');
			mockInvoke.mockRejectedValueOnce(error);

			// WHEN: rewrite called
			// THEN: throws error
			await expect(rewriter.rewrite('test', null)).rejects.toThrow('LLM backend not configured');

			// AND: state is error
			expect(rewriter.getState()).toBe('error');
			expect(rewriter.getLastError()).toBe('LLM backend not configured');
		});
	});
});
