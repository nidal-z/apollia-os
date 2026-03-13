"""code-reviewer-moe — Mixture-of-Experts code review agent.

Analyses each local Git commit using 3 sequential expert LLM passes
(Security, Architecture, Quality & Patterns) followed by a synthesis pass.
Falls back to pure-Python static analysis when no LLM backend is configured.

Deploy:
    apollia-os agent start agents/code-reviewer-moe.py
    apollia-os agent info code-reviewer-moe

Manual trigger:
    apollia-os run code-reviewer-moe /path/to/repo

Automatic trigger:
    Configured via [[triggers]] git-commit-review in apollia.toml.
    Fires on .git directory changes (file_watch source).

HITL:
    file_io is in tools_requiring_approval — the runtime suspends the task
    and emits a desktop notification before writing the review report.
    Approve or reject via:
        apollia-os task list --pending-approval
        apollia-os task resume <id> --approve
        apollia-os task resume <id> --reject

Graceful degradation:
    - ctx.llm   is None → static analysis only (no crash)
    - ctx.memory is None → no memory recording (no crash)
    - ctx.tools  is None → subprocess fallback for bash + stdlib for file write
"""

import asyncio
import os
import subprocess


class CodeReviewerMoe:
    """Mixture-of-Experts code review agent for local Git repositories.

    Manifest:
        name:                     code-reviewer-moe
        version:                  1.0.0
        tools_required:           bash_executor, file_io
        tools_requiring_approval: file_io  (HITL gate before report write)
        memory_namespace:         code-reviewer-moe
        execution_mode:           direct
    """

    def manifest(self):
        """Return the AIP agent manifest.

        Called by the runtime at `agent start` time to register metadata and
        validate duck-typing requirements. tools_requiring_approval triggers
        the HITL suspension mechanism before any file_io call proceeds.
        """
        return {
            "name": "code-reviewer-moe",
            "version": "1.0.0",
            "description": (
                "Code reviewer MoE — analyse chaque commit Git local "
                "et produit un rapport de qualité via 3 passes expertes LLM."
            ),
            "tools_required": ["bash_executor", "file_io"],
            "tools_requiring_approval": ["file_io"],
            "memory_namespace": "code-reviewer-moe",
            "execution_mode": "direct",
        }

    async def run(self, task, ctx):
        """Main entry point — called once per task by the Apollia runtime.

        Phases:
            1. Collecte   — git diff, log, stat via bash_executor
            2. Analyse    — 3 expert LLM passes (or static grep fallback)
            3. Synthèse   — 4th LLM pass aggregating expert findings
            4. Mémoire    — selective recording (Critical/High/cross-expert pattern)
            5. HITL+Write — file_io with approval gate → .apollia/reviews/
            6. Retour     — {"status": "completed", "output": [{"type": "text", ...}]}

        Args:
            task: AIP task dict — input parts[0].text is the absolute repo path.
            ctx:  RuntimeContext — exposes ctx.llm, ctx.tools, ctx.memory.
                  Any of these may be None; all phases degrade gracefully.
        """
        # ── Parse input ────────────────────────────────────────────────────
        parts = task["input"]["parts"]
        if not parts or not parts[0].get("text", "").strip():
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {
                    "code": "MISSING_INPUT",
                    "message": "Expected first part to be the absolute repo path.",
                },
            }

        repo_path = parts[0]["text"].strip()

        # ── Phase 1 — Collecte ─────────────────────────────────────────────
        print("[code-reviewer-moe] Phase 1 — Collecte...")

        diff = await self._git(ctx, repo_path, "diff HEAD~1")
        log_line = (await self._git(ctx, repo_path, "log -1 --pretty=format:%H %s")).strip()
        stat = await self._git(ctx, repo_path, "diff HEAD~1 --stat")

        # Handle first commit (no HEAD~1 parent)
        if not diff.strip():
            print("[code-reviewer-moe] Phase 1 — Premier commit détecté, git show HEAD utilisé.")
            diff = await self._git(ctx, repo_path, "show HEAD")
        if not stat.strip():
            stat = await self._git(ctx, repo_path, "show HEAD --stat")

        # Parse commit hash and message — format is "<40-char-hash> <subject>"
        if log_line and len(log_line) >= 40:
            commit_hash = log_line[:40]
            commit_message = log_line[41:].strip() if len(log_line) > 40 else "no message"
        else:
            commit_hash = "0000000000000000000000000000000000000000"
            commit_message = log_line or "no commit"

        commit_hash_short = commit_hash[:7]
        print(f"[code-reviewer-moe] Commit: {commit_hash_short} — {commit_message[:60]}")

        # ── Phase 2 — Analyse MoE ──────────────────────────────────────────
        print("[code-reviewer-moe] Phase 2 — Analyse MoE...")

        expert_findings = []

        if ctx.llm is not None:
            diff_body = diff[:8000]
            if len(diff) > 8000:
                diff_body += "\n\n... (diff tronqué à 8000 caractères)"

            # Expert 1 — Sécurité
            print("[code-reviewer-moe] Phase 2.1 — Expert Sécurité...")
            sec_content = await self._call_expert(
                ctx,
                system=(
                    "Tu es un expert en sécurité applicative Rust. "
                    "Analyse ce diff Git et liste uniquement les problèmes de sécurité "
                    "avec leur sévérité (Critical / High / Medium / Low). "
                    "Réponds en Markdown structuré, sois concis."
                ),
                user=f"Diff Git :\n{diff_body}",
                label="sécurité",
            )
            expert_findings.append(("security", sec_content))

            # Expert 2 — Architecture
            print("[code-reviewer-moe] Phase 2.2 — Expert Architecture...")
            arch_content = await self._call_expert(
                ctx,
                system=(
                    "Tu es un architecte logiciel senior spécialisé Rust. "
                    "Analyse ce diff et identifie uniquement les problèmes architecturaux. "
                    "Réponds en Markdown structuré, liste les violations avec leur impact."
                ),
                user=f"Diff Git :\n{diff_body}",
                label="architecture",
            )
            expert_findings.append(("architecture", arch_content))

            # Expert 3 — Qualité & patterns
            print("[code-reviewer-moe] Phase 2.3 — Expert Qualité...")
            quality_content = await self._call_expert(
                ctx,
                system=(
                    "Tu es un expert qualité code Rust. "
                    "Identifie uniquement les problèmes de qualité et mauvais patterns. "
                    "Réponds en Markdown structuré."
                ),
                user=f"Diff Git :\n{diff_body}",
                label="qualité",
            )
            expert_findings.append(("quality", quality_content))

        else:
            # Mode dégradé — analyse statique uniquement
            print("[code-reviewer-moe] Phase 2 — Mode dégradé: analyse statique (pas de LLM).")
            static_content = self._static_analysis(diff)
            expert_findings.append(("static", static_content))

        # ── Phase 3 — Synthèse ─────────────────────────────────────────────
        print("[code-reviewer-moe] Phase 3 — Synthèse...")

        synthesis = ""
        if ctx.llm is not None and len(expert_findings) >= 3:
            combined = "\n\n".join(
                f"### Analyse {label} :\n{content}"
                for label, content in expert_findings
            )
            synthesis = await self._call_expert(
                ctx,
                system=(
                    "Tu es un tech lead. Voici 3 analyses d'experts "
                    "sur le même commit Git. Produis une synthèse de 5 lignes maximum, "
                    "classe les problèmes par priorité décroissante, conclus avec une "
                    "recommandation d'action (merge / review requis / bloquer)."
                ),
                user=combined,
                label="synthèse",
            )
        elif ctx.llm is None:
            synthesis = "_Mode statique — synthèse LLM non disponible._"

        # ── Phase 4 — Mémoire sélective ────────────────────────────────────
        print("[code-reviewer-moe] Phase 4 — Mémoire sélective...")

        if ctx.memory is not None:
            if self._should_record_memory(expert_findings):
                memory_desc = f"review:{commit_hash_short} — {commit_message[:80]}"
                memory_desc = memory_desc[:120]
                try:
                    await ctx.memory.record(
                        memory_desc,
                        importance=0.7,
                        task_id=task["task_id"],
                    )
                    print(f"[code-reviewer-moe] Mémoire enregistrée: {memory_desc}")
                except Exception as mem_err:
                    print(f"[code-reviewer-moe] Mémoire: erreur ignorée: {mem_err}")
            else:
                print("[code-reviewer-moe] Mémoire: conditions non remplies, pas d'enregistrement.")

        # ── Assemblage du rapport ──────────────────────────────────────────
        report = self._build_report(
            commit_hash_short, commit_message, stat, expert_findings, synthesis
        )

        # ── Phase 5 — HITL + écriture rapport ──────────────────────────────
        print("[code-reviewer-moe] Phase 5 — HITL + écriture rapport...")

        review_relpath = os.path.join(".apollia", "reviews", f"review-{commit_hash_short}.md")
        await self._write_report_hitl(ctx, repo_path, review_relpath, report)

        # ── Phase 6 — Retour ───────────────────────────────────────────────
        print("[code-reviewer-moe] Phase 6 — Terminé.")
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": report}],
        }

    # ─────────────────────────────────────────────────────────── helpers ───

    async def _run_command(self, ctx, command, timeout_seconds=30):
        """Run a shell command via bash_executor (preferred) or subprocess (fallback).

        When ctx.tools is wired, the call goes through the tool proxy so it is
        audited and subject to sandbox policy. Falls back to asyncio subprocess
        when ctx.tools is None (onboarding / degraded mode).
        """
        if ctx.tools is not None:
            try:
                result = await ctx.tools.call("bash_executor", {
                    "command": command,
                    "timeout_seconds": timeout_seconds,
                })
                return result.get("stdout", "")
            except Exception:
                pass  # Fall through to subprocess fallback

        loop = asyncio.get_event_loop()
        try:
            proc = await loop.run_in_executor(
                None,
                lambda: subprocess.run(
                    command,
                    shell=True,
                    capture_output=True,
                    text=True,
                    timeout=timeout_seconds,
                ),
            )
            return proc.stdout
        except Exception:
            return ""

    async def _git(self, ctx, repo_path, args):
        """Run a git sub-command inside repo_path; return stdout (empty on error)."""
        return await self._run_command(ctx, f"git -C '{repo_path}' {args}")

    async def _call_expert(self, ctx, system, user, label):
        """Call ctx.llm.chat() and return the response content.

        On error, returns a human-readable error notice so the report remains
        valid even when one expert fails.
        """
        try:
            response = await ctx.llm.chat(system=system, user=user)
            return response.content
        except Exception as llm_err:
            return f"_Erreur expert {label}: {llm_err}_"

    def _static_analysis(self, diff):
        """Pure-Python static analysis — grep dangerous patterns in added lines.

        Used when ctx.llm is None. Checks for: unwrap(), todo!(), unsafe,
        TODO/FIXME comments, hardcoded string candidates.

        Returns a Markdown section.
        """
        dangerous = [
            ("unwrap()", "unwrap() usage (peut paniquer en production)"),
            ("todo!()", "todo!() macro (code incomplet)"),
            ("unsafe ", "bloc unsafe"),
            ("TODO", "commentaire TODO (dette technique)"),
            ("FIXME", "commentaire FIXME (dette technique)"),
        ]

        added_lines = [
            line[1:]
            for line in diff.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        ]

        findings = []
        for pattern, label in dangerous:
            matches = [line for line in added_lines if pattern in line]
            if matches:
                count = len(matches)
                findings.append(f"- **{label}** — {count} occurrence(s) détectée(s)")

        if findings:
            return "### Patterns dangereux détectés\n\n" + "\n".join(findings)
        return "_Aucun pattern dangereux détecté._"

    def _should_record_memory(self, expert_findings):
        """Return True when findings meet the selective memory threshold.

        Records only when at least one of the following is true:
        1. Security expert identified a Critical or High severity issue.
        2. Architecture expert identified a major architectural change.
        3. A dangerous pattern appears in the content of >= 2 expert analyses.
        """
        content_by_label = {label: text.lower() for label, text in expert_findings}

        # Condition 1 — Critical or High security issue
        sec = content_by_label.get("security", "")
        if "critical" in sec or "high" in sec:
            return True

        # Condition 2 — Major architectural change
        arch = content_by_label.get("architecture", "")
        arch_signals = ["major", "breaking", "violation", "architectural", "couplage fort"]
        if len(arch) > 80 and any(signal in arch for signal in arch_signals):
            return True

        # Condition 3 — Same dangerous pattern in >= 2 expert analyses
        cross_patterns = [
            "unwrap", "unsafe", "todo", "fixme", "panic",
            "security", "vulnerability", "injection",
        ]
        all_texts = list(content_by_label.values())
        for pattern in cross_patterns:
            hit_count = sum(1 for text in all_texts if pattern in text)
            if hit_count >= 2:
                return True

        return False

    def _build_report(self, commit_hash_short, commit_message, stat, expert_findings, synthesis):
        """Assemble the full Markdown review report.

        Structure:
            # Code Review — <hash_court> : <commit_message>
            ## Résumé des modifications
            ## Analyse Sécurité          (if LLM)
            ## Analyse Architecture      (if LLM)
            ## Analyse Qualité & Patterns (if LLM)
            ## Analyse Statique          (if no LLM)
            ## Synthèse                  (if LLM)
            ---
            *Generated by code-reviewer-moe v1.0.0*
        """
        lines = [f"# Code Review — {commit_hash_short} : {commit_message}", ""]

        lines += [
            "## Résumé des modifications",
            "",
            f"```\n{stat.strip()}\n```",
            "",
        ]

        for label, content in expert_findings:
            if label == "security":
                lines += ["## Analyse Sécurité", "", content, ""]
            elif label == "architecture":
                lines += ["## Analyse Architecture", "", content, ""]
            elif label == "quality":
                lines += ["## Analyse Qualité & Patterns", "", content, ""]
            elif label == "static":
                lines += ["## Analyse Statique", "", content, ""]

        if synthesis:
            lines += ["## Synthèse", "", synthesis, ""]

        lines += ["---", "*Generated by code-reviewer-moe v1.0.0*", ""]
        return "\n".join(lines)

    async def _write_report_hitl(self, ctx, repo_path, review_relpath, report):
        """Write the report via file_io (HITL gate) or stdlib fallback.

        Because file_io is listed in tools_requiring_approval, the runtime
        suspends the task and sends a desktop notification before the write
        proceeds. The human must approve via:
            apollia-os task resume <id> --approve

        If approval is rejected, the write call raises and the report is not
        saved to disk (the task still returns "completed" with the report text).

        Falls back to stdlib write when ctx.tools is None (degraded mode).
        """
        review_abs_dir = os.path.join(repo_path, os.path.dirname(review_relpath))

        # Ensure target directory exists (bash_executor or stdlib)
        if ctx.tools is not None:
            await self._run_command(ctx, f"mkdir -p '{review_abs_dir}'", timeout_seconds=5)
        else:
            os.makedirs(review_abs_dir, exist_ok=True)

        if ctx.tools is not None:
            try:
                # This call triggers the HITL suspension because file_io is in
                # tools_requiring_approval. The runtime waits for human approval.
                await ctx.tools.call("file_io", {
                    "action": "write",
                    "path": review_relpath,
                    "content": report,
                })
                print(f"[code-reviewer-moe] Rapport écrit: {review_relpath}")
                return
            except Exception as write_err:
                print(f"[code-reviewer-moe] file_io write échoué (HITL rejeté ou erreur): {write_err}")
                # Do not fall through to stdlib — HITL rejection must be respected.
                return

        # ctx.tools is None — write directly without HITL (degraded mode)
        try:
            abs_path = os.path.join(repo_path, review_relpath)
            with open(abs_path, "w", encoding="utf-8") as fh:
                fh.write(report)
            print(f"[code-reviewer-moe] Rapport écrit (mode dégradé, sans HITL): {abs_path}")
        except Exception as file_err:
            print(f"[code-reviewer-moe] Écriture échouée: {file_err}")


agent = CodeReviewerMoe()
